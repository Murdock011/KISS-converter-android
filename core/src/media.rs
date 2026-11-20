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
        enc.set_bit_rate(video_bit_rate(w, h, options.quality));
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

fn video_bit_rate(w: u32, h: u32, quality: u8) -> usize {
    // About 0.05 to 0.25 bits per pixel per frame at 30 fps.
    let bpp = 0.05 + 0.2 * (quality.clamp(1, 100) as f64 / 100.0);
    ((w as f64) * (h as f64) * 30.0 * bpp) as usize
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

fn audio_bit_rate(name: &str, quality: u8, channels: u32) -> usize {
    let q = quality.clamp(1, 100) as usize;
    let per_channel = match name {
        "libopus" | "opus" => 24_000 + q * 800,  // ~32k..104k per channel
        _ => 32_000 + q * 1_280,                  // ~32k..160k per channel
    };
    per_channel * channels.clamp(1, 2) as usize
}

fn audio_pipe(
    ictx: &format::context::Input,
    octx: &mut format::context::Output,
    ist_index: usize,
    codec: ff::Codec,
    options: &ConvertOptions,
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
        enc.set_bit_rate(audio_bit_rate(codec.name(), options.quality, out_layout.channels() as u32));
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

    let mut video = match (best_video, plan.video.is_empty()) {
        (Some(i), false) => {
            let codec = find_encoder(plan.video)
                .ok_or_else(|| failed(format!("no video encoder for {out_ext} in this build")))?;
            Some(video_pipe(&ictx, &mut octx, i, codec, out_ext, options)?)
        }
        _ => None,
    };
    let mut audio = match (best_audio, plan.audio.is_empty() || plan.video_only) {
        (Some(i), false) => {
            let codec = find_encoder(plan.audio)
                .ok_or_else(|| failed(format!("no audio encoder for {out_ext} in this build")))?;
            Some(audio_pipe(&ictx, &mut octx, i, codec, options)?)
        }
        _ => None,
    };

    if video.is_none() && audio.is_none() {
        return Err(ConvertError::Input {
            msg: if plan.video.is_empty() { "the file has no audio".into() } else { "the file has no video".into() },
        });
    }
    if !plan.video.is_empty() && !plan.video_only && video.is_none() {
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

    let duration = ictx.duration(); // in AV_TIME_BASE (microseconds)
    let mut last_report = -1.0f32;
    for (stream, packet) in ictx.packets() {
        let idx = stream.index();
        if let Some(v) = video.as_mut().filter(|v| v.ist == idx) {
            v.send_packet(&packet, &mut octx)?;
        } else if let Some(a) = audio.as_mut().filter(|a| a.ist == idx) {
            a.send_packet(&packet, &mut octx)?;
        } else {
            continue;
        }
        if duration > 0 {
            if let Some(pts) = packet.pts() {
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
