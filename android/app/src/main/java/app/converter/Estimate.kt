package app.converter

import android.graphics.BitmapFactory
import android.media.MediaExtractor
import android.media.MediaFormat
import android.media.MediaMetadataRetriever
import app.converter.core.FileKind
import kotlin.math.max
import kotlin.math.roundToLong

/**
 * A rough guess of the size after conversion, shown before the user converts.
 * It uses the same bit rates as the Rust core (core/src/media.rs), and typical
 * bytes per pixel for images. Returns null when there is no good guess.
 */
object Estimate {
    /** Facts about one input file: pixel size, play time, frame rate and codecs. */
    data class Info(
        val width: Int,
        val height: Int,
        val durationMs: Long,
        val fps: Double = 30.0,
        val videoCodec: String? = null,
        val audioCodec: String? = null,
        val audioBps: Long = 0,
    )

    /** Read the facts from the file. Call this off the main thread. */
    fun probe(f: InputFile, kind: FileKind): Info? = runCatching {
        when (kind) {
            FileKind.IMAGE -> {
                val o = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                BitmapFactory.decodeFile(f.localPath, o)
                if (o.outWidth > 0 && o.outHeight > 0) Info(o.outWidth, o.outHeight, 0) else null
            }
            FileKind.VIDEO, FileKind.AUDIO -> {
                val r = MediaMetadataRetriever()
                try {
                    r.setDataSource(f.localPath)
                    val d = r.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)?.toLongOrNull() ?: 0L
                    val w = r.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_WIDTH)?.toIntOrNull() ?: 0
                    val h = r.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_HEIGHT)?.toIntOrNull() ?: 0
                    if (d <= 0) return@runCatching null
                    val codecs = codecs(f.localPath)
                    Info(w, h, d, codecs.fps, codecs.video, codecs.audio, codecs.audioBps)
                } finally {
                    r.release()
                }
            }
            else -> null
        }
    }.getOrNull()

    private data class Codecs(val video: String?, val audio: String?, val fps: Double, val audioBps: Long)

    /** Codec names as the Rust core knows them (h264, vp9, aac, opus, ...). */
    private fun codecs(path: String): Codecs {
        val x = MediaExtractor()
        try {
            x.setDataSource(path)
            var video: String? = null
            var audio: String? = null
            var fps = 30.0
            var audioBps = 0L
            for (i in 0 until x.trackCount) {
                val f = x.getTrackFormat(i)
                val mime = f.getString(MediaFormat.KEY_MIME) ?: continue
                if (mime.startsWith("video/") && video == null) {
                    video = CODECS[mime]
                    if (f.containsKey(MediaFormat.KEY_FRAME_RATE)) {
                        fps = runCatching { f.getInteger(MediaFormat.KEY_FRAME_RATE).toDouble() }
                            .recoverCatching { f.getFloat(MediaFormat.KEY_FRAME_RATE).toDouble() }
                            .getOrDefault(30.0)
                    }
                } else if (mime.startsWith("audio/") && audio == null) {
                    audio = CODECS[mime]
                    if (f.containsKey(MediaFormat.KEY_BIT_RATE)) audioBps = f.getInteger(MediaFormat.KEY_BIT_RATE).toLong()
                }
            }
            return Codecs(video, audio, fps, audioBps)
        } catch (e: Exception) {
            return Codecs(null, null, 30.0, 0)
        } finally {
            x.release()
        }
    }

    private val CODECS = mapOf(
        "video/avc" to "h264", "video/hevc" to "hevc", "video/mp4v-es" to "mpeg4", "video/av01" to "av1",
        "video/x-vnd.on2.vp8" to "vp8", "video/x-vnd.on2.vp9" to "vp9",
        "audio/mp4a-latm" to "aac", "audio/mpeg" to "mp3", "audio/opus" to "opus",
        "audio/vorbis" to "vorbis", "audio/flac" to "flac",
    )

    /** The same as copy_ok() in the Rust core (core/src/media.rs). */
    private fun copyOk(ext: String, codec: String?, video: Boolean): Boolean {
        codec ?: return false
        val ok = when (ext) {
            "mp4", "mov" -> if (video) setOf("h264", "hevc", "mpeg4", "av1") else setOf("aac", "mp3")
            "mkv" -> if (video) setOf("h264", "hevc", "mpeg4", "av1", "vp8", "vp9") else setOf("aac", "mp3", "opus", "vorbis", "flac")
            "webm" -> if (video) setOf("vp8", "vp9", "av1") else setOf("opus", "vorbis")
            "avi" -> if (video) setOf("mpeg4") else setOf("mp3")
            "mp3" -> if (video) emptySet() else setOf("mp3")
            "m4a" -> if (video) emptySet() else setOf("aac")
            "ogg" -> if (video) emptySet() else setOf("vorbis", "opus", "flac")
            "opus" -> if (video) emptySet() else setOf("opus")
            "flac" -> if (video) emptySet() else setOf("flac")
            else -> emptySet()
        }
        return codec in ok
    }

    private const val COPY_FROM_QUALITY = 80

    /** Total bytes after conversion for all files, or null. */
    fun total(
        inputs: List<InputFile>,
        infos: List<Info?>,
        kind: FileKind,
        target: String,
        quality: Int,
        maxDimension: Int,
    ): Long? {
        if (inputs.isEmpty() || infos.size != inputs.size) return null
        var sum = 0L
        for ((i, f) in inputs.withIndex()) {
            sum += one(f, infos[i], kind, target, quality, maxDimension) ?: return null
        }
        return sum
    }

    private fun one(f: InputFile, info: Info?, kind: FileKind, target: String, q: Int, maxDim: Int): Long? {
        val seconds = (info?.durationMs ?: 0L) / 1000.0
        return when (kind) {
            FileKind.IMAGE -> {
                info ?: return null
                val (w, h) = fit(info.width, info.height, if (target == "ico") 256 else maxDim)
                val px = w.toDouble() * h
                val bytesPerPixel = when (target) {
                    "jpg", "jpeg", "pdf" -> jpegBytesPerPixel(q)
                    "webp" -> 0.9
                    "png" -> 1.4
                    "bmp" -> 4.0
                    "tiff", "tif" -> 3.0
                    "gif" -> 0.6
                    "ico" -> 2.0
                    else -> return null
                }
                (px * bytesPerPixel).roundToLong() + if (target == "pdf") 2_000 else 0
            }
            FileKind.VIDEO, FileKind.AUDIO -> {
                info ?: return null
                if (seconds <= 0) return null
                val copy = q >= COPY_FROM_QUALITY
                // Bits per second of the original, from the file size and play time.
                val total = f.size * 8 / seconds
                val lossy = info.audioCodec in setOf("aac", "mp3", "opus", "vorbis")
                val srcAudio = when {
                    info.audioBps > 0 -> info.audioBps.toDouble()
                    lossy -> 128_000.0
                    info.audioCodec != null -> null
                    else -> 0.0
                }
                val hasAudio = info.audioCodec != null
                val srcVideo = (total - (srcAudio ?: if (hasAudio) 128_000.0 else 0.0)).coerceAtLeast(0.0)

                fun audio(ext: String): Double {
                    if (!hasAudio) return 0.0
                    if (copy && copyOk(ext, info.audioCodec, false)) return srcAudio ?: 128_000.0
                    return when (ext) {
                        "flac" -> 800_000.0
                        "wav" -> 1_411_200.0
                        else -> {
                            val rate = if (ext == "webm" || ext == "opus") opusBitRate(q) else audioBitRate(q)
                            if (srcAudio != null && srcAudio > 0) minOf(rate, maxOf(srcAudio, 64_000.0)) else rate
                        }
                    }
                }

                val bitsPerSecond = when (target) {
                    "mp4", "mkv", "webm", "mov", "avi" -> {
                        if (info.width <= 0 || info.height <= 0) return null
                        val (w, h) = fit(info.width, info.height, maxDim)
                        val fits = maxDim <= 0 || max(info.width, info.height) <= maxDim
                        val video = if (copy && fits && copyOk(target, info.videoCodec, true)) {
                            srcVideo
                        } else {
                            val scale = (w.toDouble() * h / (info.width.toDouble() * info.height)).coerceAtMost(1.0)
                            val cap = if (srcVideo > 0) srcVideo * q.coerceIn(1, 100) / 100.0 * scale else Double.MAX_VALUE
                            minOf(videoBitRate(w, h, info.fps, q), cap).coerceAtLeast(100_000.0)
                        }
                        video + audio(target)
                    }
                    "mp3", "m4a", "ogg", "opus", "flac", "wav" -> audio(target)
                    else -> return null
                }
                (seconds * bitsPerSecond / 8).roundToLong()
            }
            else -> null
        }
    }

    /** The same as video_bit_rate() in the Rust core, without the cap. */
    private fun videoBitRate(w: Int, h: Int, fps: Double, q: Int): Double {
        val f = if (fps > 0) fps.coerceIn(1.0, 120.0) else 30.0
        val bpp = 0.03 + 0.1 * (q.coerceIn(1, 100) / 100.0)
        return w.toDouble() * h * f * bpp
    }

    /** The same as audio_bit_rate() in the Rust core, for stereo, without the cap. */
    private fun audioBitRate(q: Int) = (16_000.0 + q.coerceIn(1, 100) * 800) * 2
    private fun opusBitRate(q: Int) = (12_000.0 + q.coerceIn(1, 100) * 600) * 2

    /** Typical JPEG size for photos, in bytes per pixel. */
    private fun jpegBytesPerPixel(q: Int): Double {
        val points = listOf(1 to 0.03, 50 to 0.12, 75 to 0.18, 85 to 0.25, 95 to 0.45, 100 to 0.9)
        val x = q.coerceIn(1, 100)
        for (i in 1 until points.size) {
            val (x1, y1) = points[i]
            if (x <= x1) {
                val (x0, y0) = points[i - 1]
                return y0 + (y1 - y0) * (x - x0) / (x1 - x0).toDouble()
            }
        }
        return points.last().second
    }

    private fun fit(w: Int, h: Int, maxDim: Int): Pair<Int, Int> {
        val big = max(w, h)
        if (maxDim <= 0 || big <= maxDim) return w to h
        val s = maxDim.toDouble() / big
        return (w * s).toInt().coerceAtLeast(1) to (h * s).toInt().coerceAtLeast(1)
    }
}
