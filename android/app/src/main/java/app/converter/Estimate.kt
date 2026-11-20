package app.converter

import android.graphics.BitmapFactory
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
    /** Facts about one input file: pixel size and play time. */
    data class Info(val width: Int, val height: Int, val durationMs: Long)

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
                    if (d > 0) Info(w, h, d) else null
                } finally {
                    r.release()
                }
            }
            else -> null
        }
    }.getOrNull()

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
                if (seconds <= 0) return null
                val bitsPerSecond = when (target) {
                    "mp4", "mkv", "webm", "mov", "avi" -> {
                        info ?: return null
                        if (info.width <= 0 || info.height <= 0) return null
                        val (w, h) = fit(info.width, info.height, maxDim)
                        val audio = if (target == "webm") opusBitRate(q) else audioBitRate(q)
                        videoBitRate(w, h, q) + audio
                    }
                    "mp3", "m4a", "ogg" -> audioBitRate(q)
                    "opus" -> opusBitRate(q)
                    "flac" -> 800_000.0
                    "wav" -> 1_411_200.0
                    else -> return null
                }
                (seconds * bitsPerSecond / 8).roundToLong()
            }
            else -> null
        }
    }

    /** The same as video_bit_rate() in the Rust core. */
    private fun videoBitRate(w: Int, h: Int, q: Int): Double {
        val bpp = 0.05 + 0.2 * (q.coerceIn(1, 100) / 100.0)
        return w.toDouble() * h * 30.0 * bpp
    }

    /** The same as audio_bit_rate() in the Rust core, for stereo. */
    private fun audioBitRate(q: Int) = (32_000.0 + q.coerceIn(1, 100) * 1_280) * 2
    private fun opusBitRate(q: Int) = (24_000.0 + q.coerceIn(1, 100) * 800) * 2

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
