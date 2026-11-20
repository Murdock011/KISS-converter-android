//! Audio and video conversion with FFmpeg (through `ffmpeg-next`).
//!
//! The pipeline for each stream is: demux -> decode -> filter -> encode -> mux.
//! We keep at most one video stream and one audio stream.

use ffmpeg_next as ff;
use ff::codec::{self, Capabilities};
use ff::{encoder, filter, format, frame, media, ChannelLayout, Dictionary, Packet, Rational};

use crate::{init_ffmpeg, ConvertError, ConvertOptions, Progress};

/// Encoder choices for each output extension, best first.
/// The first one that this FFmpeg build has is used.
struct Plan {
    video: &'static [&'static str],
    audio: &'static [&'static str],
    /// Output has no audio (GIF).
    video_only: bool,
}

/// Codecs that the output container can hold as they are (stream copy, no new encoding).
fn copy_ok(ext: &str, id: codec::Id, video: bool) -> bool {
    use codec::Id::*;
    let ok: &[codec::Id] = match (ext, video) {
        ("mp4" | "mov", true) => &[H264, HEVC, MPEG4, AV1],
        ("mp4" | "mov", false) => &[AAC, MP3],
        ("mkv", true) => &[H264, HEVC, MPEG4, AV1, VP8, VP9],
        ("mkv", false) => &[AAC, MP3, OPUS, VORBIS, FLAC],
        ("webm", true) => &[VP8, VP9, AV1],
        ("webm", false) => &[OPUS, VORBIS],
        ("avi", true) => &[MPEG4],
        ("avi", false) => &[MP3],
        ("mp3", false) => &[MP3],
        ("m4a", false) => &[AAC],
        ("ogg", false) => &[VORBIS, OPUS, FLAC],
        ("opus", false) => &[OPUS],
        ("flac", false) => &[FLAC],
        _ => &[],
    };
    ok.contains(&id)
}

/// From this quality up, a stream that fits the new container is copied as it is.
/// This keeps the size and the quality, and it is fast.
const COPY_FROM_QUALITY: u8 = 80;

fn plan_for(ext: &str) -> Option<Plan> {
    const H264_ETC: &[&str] = &["libx264", "libopenh264", "mpeg4"];
    let p = match ext {
        "mp4" | "mov" => Plan { video: H264_ETC, audio: &["aac"], video_only: false },
        "mkv" => Plan { video: H264_ETC, audio: &["libopus", "aac", "flac"], video_only: false },
        "webm" => Plan {
            video: &["libvpx-vp9", "libvpx"],
            audio: &["libopus", "libvorbis", "opus"],
            video_only: false,
        },
        "avi" => Plan { video: &["mpeg4"], audio: &["libmp3lame", "ac3"], video_only: false },
        "gif" => Plan { video: &["gif"], audio: &[], video_only: true },
        "mp3" => Plan { video: &[], audio: &["libmp3lame"], video_only: false },
        "m4a" | "aac" => Plan { video: &[], audio: &["aac"], video_only: false },
        "ogg" => Plan { video: &[], audio: &["libvorbis", "libopus", "opus", "flac"], video_only: false },
        "opus" => Plan { video: &[], audio: &["libopus", "opus"], video_only: false },
        "flac" => Plan { video: &[], audio: &["flac"], video_only: false },
        "wav" => Plan { video: &[], audio: &["pcm_s16le"], video_only: false },
        _ => return None,
    };
    Some(p)
}

fn find_encoder(names: &[&str]) -> Option<ff::Codec> {
    names.iter().find_map(|n| encoder::find_by_name(n))
}

fn failed(msg: impl Into<String>) -> ConvertError {
    ConvertError::Failed { msg: msg.into() }
}

/// Options that let FFmpeg use its "experimental" encoders (native opus/vorbis).
fn open_opts() -> Dictionary<'static> {
    let mut d = Dictionary::new();
    d.set("strict", "-2");
    d
}

fn sink_time_base(graph: &mut filter::Graph) -> Rational {
    let mut sink = graph.get("out").expect("sink exists");
    unsafe { Rational::from(ff::ffi::av_buffersink_get_time_base(sink.as_mut_ptr())) }
}

fn sink_frame_rate(graph: &mut filter::Graph) -> Rational {
    let mut sink = graph.get("out").expect("sink exists");
    unsafe { Rational::from(ff::ffi::av_buffersink_get_frame_rate(sink.as_mut_ptr())) }
}

// ---------------------------------------------------------------- video

struct VideoPipe {
    ist: usize,
    ost: usize,
    decoder: ff::decoder::Video,
    encoder: ff::encoder::Video,
    graph: filter::Graph,
    enc_tb: Rational,
    ost_tb: Rational,
}

fn video_filter_spec(ext: &str, rate: Rational, max_dim: u32) -> String {
    let box_scale = |m: u32| {
        format!("scale=w='if(gt(iw,ih),min({m},iw),-2)':h='if(gt(iw,ih),-2,min({m},ih))'")
    };
    if ext == "gif" {
        let m = if max_dim == 0 { 480 } else { max_dim.min(800) };
        return format!(
            "fps=12,{}:flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer",
            box_scale(m)
        );
    }
    let fps = if rate.numerator() > 0 && rate.denominator() > 0 && f64::from(rate) <= 120.0 {
        format!("{}/{}", rate.numerator(), rate.denominator())
    } else {
        "30".to_owned()
    };
    let scale = if max_dim == 0 {
        "scale=w='trunc(iw/2)*2':h='trunc(ih/2)*2'".to_owned()
    } else {
        // -2 keeps the size even, which 4:2:0 video needs.
        box_scale(max_dim)
    };
    format!("fps={fps},{scale}")
}

fn video_pipe(
    ictx: &format::context::Input,
    octx: &mut format::context::Output,
    ist_index: usize,
    codec: ff::Codec,
    ext: &str,
    options: &ConvertOptions,
    source_bps: Option<u64>,
) -> Result<VideoPipe, ConvertError> {
    let ist = ictx.stream(ist_index).ok_or_else(|| failed("video stream missing"))?;
    let mut dec_ctx = codec::context::Context::from_parameters(ist.parameters())?;
    // Let the decoder use more than one CPU core.
    dec_ctx.set_threading(codec::threading::Config { kind: codec::threading::Type::Frame, count: 0 });
    let decoder = dec_ctx.decoder().video()?;

    let vcodec = codec.video()?;
    let pix_fmts: Vec<format::Pixel> = vcodec
        .formats()
        .map(|f| f.collect())
        .unwrap_or_else(|| vec![format::Pixel::YUV420P]);
    let out_fmt = if pix_fmts.contains(&format::Pixel::YUV420P) {
        format::Pixel::YUV420P
    } else {
        pix_fmts[0]
    };

    // Build the filter graph.
    let in_tb = ist.time_base();
    let sar = decoder.aspect_ratio();
    let sar = if sar.numerator() <= 0 { Rational(1, 1) } else { sar };
    let args = format!(
        "video_size={}x{}:pix_fmt={}:time_base={}/{}:pixel_aspect={}/{}",
        decoder.width(),
        decoder.height(),
        ff::ffi::AVPixelFormat::from(decoder.format()) as i32,
        in_tb.numerator(),
        in_tb.denominator(),
        sar.numerator(),
        sar.denominator()
    );
    let mut graph = filter::Graph::new();
    graph.add(&filter::find("buffer").ok_or_else(|| failed("no buffer filter"))?, "in", &args)?;
    graph.add(&filter::find("buffersink").ok_or_else(|| failed("no buffersink filter"))?, "out", "")?;
    {
        let mut out = graph.get("out").unwrap();
        out.set_pixel_format(if ext == "gif" { format::Pixel::PAL8 } else { out_fmt });
    }
    let spec = video_filter_spec(ext, ist.avg_frame_rate(), options.max_dimension);
    graph.output("in", 0)?.input("out", 0)?.parse(&spec)?;
    graph.validate()?;

    let enc_tb = sink_time_base(&mut graph);
    let frame_rate = sink_frame_rate(&mut graph);
    let (w, h) = {
        let mut sink = graph.get("out").unwrap();
        unsafe {
            let p = sink.as_mut_ptr();
            (ff::ffi::av_buffersink_get_w(p) as u32, ff::ffi::av_buffersink_get_h(p) as u32)
        }
    };

    // Set up the encoder.
    let global = octx.format().flags().contains(format::Flags::GLOBAL_HEADER);
    let mut ost = octx.add_stream(codec)?;
    let mut enc = codec::context::Context::new_with_codec(codec).encoder().video()?;
    enc.set_width(w);
    enc.set_height(h);
    enc.set_aspect_ratio(sar);
    enc.set_format(if ext == "gif" { format::Pixel::PAL8 } else { out_fmt });
    enc.set_time_base(enc_tb);
    if frame_rate.numerator() > 0 {
        enc.set_frame_rate(Some(frame_rate));
    }
    if ext != "gif" {
        enc.set_gop(frame_rate_gop(frame_rate));
        let fps = if frame_rate.numerator() > 0 && frame_rate.denominator() > 0 { f64::from(frame_rate) } else { 30.0 };
        let cap = video_cap(
            source_bps,
            options.quality,
            decoder.width() as f64 * decoder.height() as f64,
            w as f64 * h as f64,
        );
        enc.set_bit_rate(video_bit_rate(w, h, fps, options.quality, cap));
        enc.set_max_b_frames(0);
    }
    if global {
        enc.set_flags(codec::Flags::GLOBAL_HEADER);
    }
    let mut opts = open_opts();
    if codec.name().starts_with("libvpx") {
        // Fast settings, so phones finish in a fair time.
        opts.set("deadline", "realtime");
        opts.set("cpu-used", "8");
        opts.set("row-mt", "1");
    }
    if codec.name() == "libx264" {
        opts.set("preset", "veryfast");
    }
    let encoder = enc.open_with(opts)?;
    ost.set_parameters(&encoder);
    unsafe {
        (*ost.parameters().as_mut_ptr()).codec_tag = 0;
    }
    ost.set_time_base(enc_tb);
    let ost_index = ost.index();

    Ok(VideoPipe {
        ist: ist_index,
        ost: ost_index,
        decoder,
        encoder,
        graph,
        enc_tb,
        ost_tb: enc_tb,
    })
}

fn frame_rate_gop(rate: Rational) -> u32 {
    let fps = if rate.denominator() > 0 { f64::from(rate) } else { 30.0 };
    (fps * 2.0).round().clamp(1.0, 600.0) as u32
}

/// The bit rate for new video: from the size, the frame rate and the quality,
/// but never more than `cap` (from the bit rate of the original file).
pub(crate) fn video_bit_rate(w: u32, h: u32, fps: f64, quality: u8, cap: Option<u64>) -> usize {
    let fps = if fps.is_finite() && fps > 0.0 { fps.clamp(1.0, 120.0) } else { 30.0 };
    // About 0.03 to 0.13 bits per pixel per frame.
    let bpp = 0.03 + 0.1 * (quality.clamp(1, 100) as f64 / 100.0);
    let mut rate = (w as f64) * (h as f64) * fps * bpp;
    if let Some(c) = cap {
        rate = rate.min(c as f64);
    }
    rate.max(100_000.0) as usize
}

/// The most bits per second the new video may use: the original rate, times the quality
/// (Best: 95%, Balanced: 85%, Small: 60%), and smaller again for a smaller picture.
pub(crate) fn video_cap(source_bps: Option<u64>, quality: u8, in_px: f64, out_px: f64) -> Option<u64> {
    let src = source_bps? as f64;
    let q = quality.clamp(1, 100) as f64 / 100.0;
    let scale = if in_px > 0.0 && out_px > 0.0 { (out_px / in_px).min(1.0) } else { 1.0 };
    Some((src * q * scale) as u64)
}

impl VideoPipe {
    fn send_packet(&mut self, p: &Packet, octx: &mut format::context::Output) -> Result<(), ConvertError> {
        self.decoder.send_packet(p)?;
        self.drain_decoder(octx)
    }

    fn drain_decoder(&mut self, octx: &mut format::context::Output) -> Result<(), ConvertError> {
        let mut decoded = frame::Video::empty();
        while self.decoder.receive_frame(&mut decoded).is_ok() {
            let ts = decoded.timestamp();
            decoded.set_pts(ts);
            self.graph.get("in").unwrap().source().add(&decoded)?;
            self.drain_filter(octx)?;
        }
        Ok(())
    }

    fn drain_filter(&mut self, octx: &mut format::context::Output) -> Result<(), ConvertError> {
        let mut filtered = frame::Video::empty();
        while self.graph.get("out").unwrap().sink().frame(&mut filtered).is_ok() {
            filtered.set_kind(ff::picture::Type::None);
            self.encoder.send_frame(&filtered).map_err(|e| failed(format!("audio encode: {e}")))?;
            self.drain_encoder(octx)?;
        }
        Ok(())
    }

    fn drain_encoder(&mut self, octx: &mut format::context::Output) -> Result<(), ConvertError> {
        let mut pkt = Packet::empty();
        while self.encoder.receive_packet(&mut pkt).is_ok() {
            pkt.set_stream(self.ost);
            pkt.rescale_ts(self.enc_tb, self.ost_tb);
            // FLAC sends an empty last packet that only holds new side data.
            // Some muxers reject empty packets, so skip it.
            if pkt.size() == 0 {
                continue;
            }
            pkt.write_interleaved(octx).map_err(|e| failed(format!("audio write: {e}")))?;
        }
        Ok(())
    }

    fn finish(&mut self, octx: &mut format::context::Output) -> Result<(), ConvertError> {
        self.decoder.send_eof()?;
        self.drain_decoder(octx)?;
        self.graph.get("in").unwrap().source().flush()?;
        self.drain_filter(octx)?;
        self.encoder.send_eof()?;
        self.drain_encoder(octx)
    }
}

// ---------------------------------------------------------------- audio

struct AudioPipe {
    ist: usize,
    ost: usize,
    decoder: ff::decoder::Audio,
    encoder: ff::encoder::Audio,
    graph: filter::Graph,
    enc_tb: Rational,
    ost_tb: Rational,
}

fn pick_rate(codec: &ff::codec::Audio, want: u32) -> u32 {
    match codec.rates() {
        None => want,
        Some(rates) => {
            let rates: Vec<i32> = rates.collect();
            if rates.contains(&(want as i32)) {
                want
            } else if rates.contains(&48000) {
                48000
            } else if rates.contains(&44100) {
                44100
            } else {
                *rates.iter().max().unwrap_or(&48000) as u32
            }
        }
    }
}

/// The bit rate for new audio, but never more than the original rate (`source_bps`).
pub(crate) fn audio_bit_rate(name: &str, quality: u8, channels: u32, source_bps: Option<u64>) -> usize {
    let q = quality.clamp(1, 100) as usize;
    let ch = channels.clamp(1, 2) as usize;
    let per_channel = match name {
        "libopus" | "opus" => 12_000 + q * 600, // ~12k..72k per channel
        _ => 16_000 + q * 800,                  // ~16k..96k per channel
    };
    let rate = per_channel * ch;
    match source_bps {
        // Not below 32k per channel: under that, the sound gets bad fast.
        Some(src) if src > 0 => rate.min((src as usize).max(32_000 * ch)),
        _ => rate,
    }
}

fn audio_pipe(
    ictx: &format::context::Input,
    octx: &mut format::context::Output,
    ist_index: usize,
    codec: ff::Codec,
    options: &ConvertOptions,
    source_bps: Option<u64>,
) -> Result<AudioPipe, ConvertError> {
    let ist = ictx.stream(ist_index).ok_or_else(|| failed("audio stream missing"))?;
    let decoder = codec::context::Context::from_parameters(ist.parameters())?.decoder().audio()?;
    let acodec = codec.audio()?;

    let in_layout = {
        let l = decoder.channel_layout();
        if l.bits() == 0 { ChannelLayout::default(decoder.channels() as i32) } else { l }
    };
    let out_layout = acodec
        .channel_layouts()
        .map(|cls| cls.best(in_layout.channels().min(2)))
        .unwrap_or(if in_layout.channels() >= 2 { ChannelLayout::STEREO } else { ChannelLayout::MONO });
    let out_fmt = acodec
        .formats()
        .and_then(|mut f| f.next())
        .ok_or_else(|| failed("encoder has no sample format"))?;
    let out_rate = pick_rate(&acodec, decoder.rate());

    // Filter graph: resample, change format and channels.
    let in_tb = ist.time_base();
    let args = format!(
        "time_base={}/{}:sample_rate={}:sample_fmt={}:channel_layout=0x{:x}",
        in_tb.numerator(),
        in_tb.denominator(),
        decoder.rate(),
        decoder.format().name(),
        in_layout.bits()
    );
    let mut graph = filter::Graph::new();
    graph.add(&filter::find("abuffer").ok_or_else(|| failed("no abuffer filter"))?, "in", &args)?;
    graph.add(&filter::find("abuffersink").ok_or_else(|| failed("no abuffersink filter"))?, "out", "")?;
    {
        let mut out = graph.get("out").unwrap();
        out.set_sample_format(out_fmt);
        out.set_channel_layout(out_layout);
        out.set_sample_rate(out_rate);
    }
    graph.output("in", 0)?.input("out", 0)?.parse("aresample=async=1000")?;
    graph.validate()?;
    let enc_tb = sink_time_base(&mut graph);

    let global = octx.format().flags().contains(format::Flags::GLOBAL_HEADER);
    let mut ost = octx.add_stream(codec)?;
    let mut enc = codec::context::Context::new_with_codec(codec).encoder().audio()?;
    enc.set_rate(out_rate as i32);
    enc.set_channel_layout(out_layout);
    #[cfg(not(feature = "ffmpeg_7_0"))]
    enc.set_channels(out_layout.channels());
    enc.set_format(out_fmt);
    enc.set_time_base(enc_tb);
    let lossless = matches!(codec.name(), "flac" | "pcm_s16le");
    if !lossless {
        enc.set_bit_rate(audio_bit_rate(codec.name(), options.quality, out_layout.channels() as u32, source_bps));
    }
    if global {
        enc.set_flags(codec::Flags::GLOBAL_HEADER);
    }
    let encoder = enc.open_with(open_opts())?;
    ost.set_parameters(&encoder);
    unsafe {
        (*ost.parameters().as_mut_ptr()).codec_tag = 0;
    }
    ost.set_time_base(enc_tb);

    if !acodec.capabilities().contains(Capabilities::VARIABLE_FRAME_SIZE) && encoder.frame_size() > 0 {
        graph.get("out").unwrap().sink().set_frame_size(encoder.frame_size());
    }

    Ok(AudioPipe {
        ist: ist_index,
        ost: ost.index(),
        decoder,
        encoder,
        graph,
        enc_tb,
        ost_tb: enc_tb,
    })
}

impl AudioPipe {
    fn send_packet(&mut self, p: &Packet, octx: &mut format::context::Output) -> Result<(), ConvertError> {
        self.decoder.send_packet(p)?;
        self.drain_decoder(octx)
    }

    fn drain_decoder(&mut self, octx: &mut format::context::Output) -> Result<(), ConvertError> {
        let mut decoded = frame::Audio::empty();
        while self.decoder.receive_frame(&mut decoded).is_ok() {
            let ts = decoded.timestamp();
            decoded.set_pts(ts);
            self.graph.get("in").unwrap().source().add(&decoded)?;
            self.drain_filter(octx)?;
        }
        Ok(())
    }

    fn drain_filter(&mut self, octx: &mut format::context::Output) -> Result<(), ConvertError> {
        let mut filtered = frame::Audio::empty();
        while self.graph.get("out").unwrap().sink().frame(&mut filtered).is_ok() {
            self.encoder.send_frame(&filtered).map_err(|e| failed(format!("audio encode: {e}")))?;
            self.drain_encoder(octx)?;
        }
        Ok(())
    }

    fn drain_encoder(&mut self, octx: &mut format::context::Output) -> Result<(), ConvertError> {
        let mut pkt = Packet::empty();
        while self.encoder.receive_packet(&mut pkt).is_ok() {
            pkt.set_stream(self.ost);
            pkt.rescale_ts(self.enc_tb, self.ost_tb);
            // FLAC sends an empty last packet that only holds new side data.
            // Some muxers reject empty packets, so skip it.
            if pkt.size() == 0 {
                continue;
            }
            pkt.write_interleaved(octx).map_err(|e| failed(format!("audio write: {e}")))?;
        }
        Ok(())
    }

    fn finish(&mut self, octx: &mut format::context::Output) -> Result<(), ConvertError> {
        self.decoder.send_eof()?;
        self.drain_decoder(octx)?;
        self.graph.get("in").unwrap().source().flush()?;
        self.drain_filter(octx)?;
        self.encoder.send_eof()?;
        self.drain_encoder(octx)
    }
}

// ---------------------------------------------------------------- stream copy

/// A stream that goes into the new file as it is (no decode, no encode).
struct CopyPipe {
    ist: usize,
    ost: usize,
    ist_tb: Rational,
    ost_tb: Rational,
}

fn copy_pipe(
    ictx: &format::context::Input,
    octx: &mut format::context::Output,
    ist_index: usize,
) -> Result<CopyPipe, ConvertError> {
    let ist = ictx.stream(ist_index).ok_or_else(|| failed("stream missing"))?;
    let mut ost = octx.add_stream(encoder::find(codec::Id::None))?;
    ost.set_parameters(ist.parameters());
    unsafe {
        // Let the muxer choose the tag that fits the new container.
        (*ost.parameters().as_mut_ptr()).codec_tag = 0;
    }
    Ok(CopyPipe { ist: ist_index, ost: ost.index(), ist_tb: ist.time_base(), ost_tb: ist.time_base() })
}

impl CopyPipe {
    fn write(&self, mut p: Packet, octx: &mut format::context::Output) -> Result<(), ConvertError> {
        p.rescale_ts(self.ist_tb, self.ost_tb);
        p.set_position(-1);
        p.set_stream(self.ost);
        p.write_interleaved(octx).map_err(|e| failed(format!("write: {e}")))
    }
}

/// Bits per second of one stream, as the file says. None when the file does not say.
fn stream_bps(ictx: &format::context::Input, index: usize) -> Option<u64> {
    let st = ictx.stream(index)?;
    let rate = unsafe { (*st.parameters().as_ptr()).bit_rate };
    (rate > 0).then_some(rate as u64)
}

/// Bits per second of the original video and audio streams.
/// When the file does not give the video rate, it is the file rate minus the audio rate.
pub(crate) fn source_rates(
    file_bytes: u64,
    duration_us: i64,
    video_bps: Option<u64>,
    audio_bps: Option<u64>,
    has_audio: bool,
) -> (Option<u64>, Option<u64>) {
    let total = if duration_us > 0 && file_bytes > 0 {
        Some((file_bytes as f64 * 8.0 / (duration_us as f64 / 1_000_000.0)) as u64)
    } else {
        None
    };
    let video = video_bps.or_else(|| {
        let audio = if has_audio { audio_bps.unwrap_or(128_000) } else { 0 };
        total.map(|t| t.saturating_sub(audio)).filter(|v| *v > 0)
    });
    (video, audio_bps)
}

// ---------------------------------------------------------------- driver

pub fn convert(
    input: &str,
    output: &str,
    out_ext: &str,
    options: &ConvertOptions,
    progress: &Progress,
) -> Result<(), ConvertError> {
    init_ffmpeg();
    let plan = plan_for(out_ext).ok_or_else(|| ConvertError::Unsupported { msg: out_ext.to_owned() })?;

    let mut ictx = format::input(&input).map_err(|e| ConvertError::Input { msg: e.to_string() })?;
    let mut octx = format::output(&output)?;

    let best_video = ictx
        .streams()
        .best(media::Type::Video)
        // Skip cover art pictures in music files.
        .filter(|s| s.disposition() & format::stream::Disposition::ATTACHED_PIC != format::stream::Disposition::ATTACHED_PIC)
        .map(|s| s.index());
    let best_audio = ictx.streams().best(media::Type::Audio).map(|s| s.index());

    let file_bytes = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
    let (src_video_bps, src_audio_bps) = source_rates(
        file_bytes,
        ictx.duration(),
        best_video.and_then(|i| stream_bps(&ictx, i)),
        best_audio.and_then(|i| stream_bps(&ictx, i)),
        best_audio.is_some(),
    );
    // Raw AAC (ADTS) needs a bitstream filter to go into MP4, so it is not copied.
    let in_format = ictx.format().name().to_owned();
    let adts = in_format.contains("mpegts") || in_format == "aac";
    let may_copy = options.quality >= COPY_FROM_QUALITY;

    let mut video = None;
    let mut video_copy = None;
    if let (Some(i), false) = (best_video, plan.video.is_empty()) {
        let st = ictx.stream(i).ok_or_else(|| failed("video stream missing"))?;
        let par = st.parameters();
        let (w, h) = unsafe { ((*par.as_ptr()).width as u32, (*par.as_ptr()).height as u32) };
        let fits = options.max_dimension == 0 || w.max(h) <= options.max_dimension;
        if may_copy && fits && !plan.video_only && copy_ok(out_ext, par.id(), true) {
            video_copy = Some(copy_pipe(&ictx, &mut octx, i)?);
        } else {
            let codec = find_encoder(plan.video)
                .ok_or_else(|| failed(format!("no video encoder for {out_ext} in this build")))?;
            video = Some(video_pipe(&ictx, &mut octx, i, codec, out_ext, options, src_video_bps)?);
        }
    }
    let mut audio = None;
    let mut audio_copy = None;
    if let (Some(i), false) = (best_audio, plan.audio.is_empty() || plan.video_only) {
        let id = ictx.stream(i).ok_or_else(|| failed("audio stream missing"))?.parameters().id();
        if may_copy && copy_ok(out_ext, id, false) && !(id == codec::Id::AAC && adts) {
            audio_copy = Some(copy_pipe(&ictx, &mut octx, i)?);
        } else {
            let codec = find_encoder(plan.audio)
                .ok_or_else(|| failed(format!("no audio encoder for {out_ext} in this build")))?;
            // The file often does not give the rate of lossy audio (for example Opus in WebM).
            // Then use 128k, so the new audio is not bigger than usual music files.
            let lossy = matches!(id, codec::Id::AAC | codec::Id::MP3 | codec::Id::OPUS | codec::Id::VORBIS);
            let src = src_audio_bps.or(if lossy { Some(128_000) } else { None });
            audio = Some(audio_pipe(&ictx, &mut octx, i, codec, options, src)?);
        }
    }
    let has_video = video.is_some() || video_copy.is_some();
    let has_audio = audio.is_some() || audio_copy.is_some();

    if !has_video && !has_audio {
        return Err(ConvertError::Input {
            msg: if plan.video.is_empty() { "the file has no audio".into() } else { "the file has no video".into() },
        });
    }
    if !plan.video.is_empty() && !plan.video_only && !has_video {
        return Err(ConvertError::Input { msg: "the file has no video".into() });
    }

    octx.set_metadata(ictx.metadata().to_owned());
    octx.write_header().map_err(|e| failed(format!("write header: {e}")))?;
    // The muxer can change the stream time bases in write_header.
    if let Some(v) = video.as_mut() {
        v.ost_tb = octx.stream(v.ost).unwrap().time_base();
    }
    if let Some(a) = audio.as_mut() {
        a.ost_tb = octx.stream(a.ost).unwrap().time_base();
    }
    for c in [video_copy.as_mut(), audio_copy.as_mut()].into_iter().flatten() {
        c.ost_tb = octx.stream(c.ost).unwrap().time_base();
    }

    let duration = ictx.duration(); // in AV_TIME_BASE (microseconds)
    let mut last_report = -1.0f32;
    for (stream, packet) in ictx.packets() {
        let idx = stream.index();
        let pts = packet.pts();
        if let Some(v) = video.as_mut().filter(|v| v.ist == idx) {
            v.send_packet(&packet, &mut octx)?;
        } else if let Some(a) = audio.as_mut().filter(|a| a.ist == idx) {
            a.send_packet(&packet, &mut octx)?;
        } else if let Some(c) = [video_copy.as_ref(), audio_copy.as_ref()].into_iter().flatten().find(|c| c.ist == idx) {
            c.write(packet, &mut octx)?;
        } else {
            continue;
        }
        if duration > 0 {
            if let Some(pts) = pts {
                let tb = stream.time_base();
                let secs = pts as f64 * f64::from(tb);
                let frac = (secs * 1_000_000.0 / duration as f64) as f32 * 0.98;
                if frac - last_report >= 0.01 {
                    last_report = frac;
                    progress.report(frac)?;
                }
            }
        }
    }

    if let Some(v) = video.as_mut() {
        v.finish(&mut octx)?;
    }
    if let Some(a) = audio.as_mut() {
        a.finish(&mut octx)?;
    }
    octx.write_trailer()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_rate_never_above_source() {
        // 720p at 30 fps, original 1 Mbit/s: the new rate is at most 85% of it.
        let cap = video_cap(Some(1_000_000), 85, 1280.0 * 720.0, 1280.0 * 720.0);
        assert_eq!(cap, Some(850_000));
        assert!(video_bit_rate(1280, 720, 30.0, 85, cap) <= 850_000);
        // Half the pixels: half the cap.
        assert_eq!(video_cap(Some(1_000_000), 100, 100.0, 50.0), Some(500_000));
        // No source rate: the formula only.
        assert!(video_bit_rate(1280, 720, 30.0, 85, None) > 1_000_000);
    }

    #[test]
    fn audio_rate_never_above_source() {
        assert_eq!(audio_bit_rate("aac", 85, 2, Some(96_000)), 96_000);
        assert_eq!(audio_bit_rate("aac", 85, 2, Some(32_000)), 64_000);
        assert_eq!(audio_bit_rate("aac", 85, 2, None), (16_000 + 85 * 800) * 2);
    }

    #[test]
    fn source_video_rate_from_file_size() {
        // 20 MB over 100 s = 1.6 Mbit/s; minus 128k audio.
        let (v, a) = source_rates(20_000_000, 100_000_000, None, None, true);
        assert_eq!(v, Some(1_600_000 - 128_000));
        assert_eq!(a, None);
    }

    #[test]
    fn copy_rules() {
        assert!(copy_ok("mp4", codec::Id::H264, true));
        assert!(!copy_ok("mp4", codec::Id::VP9, true));
        assert!(copy_ok("webm", codec::Id::VP9, true));
        assert!(copy_ok("m4a", codec::Id::AAC, false));
        assert!(!copy_ok("wav", codec::Id::PCM_S16LE, false));
    }
}
