package app.converter

import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import java.io.File

/**
 * PDF to images. Android's own PdfRenderer draws the pages,
 * so the app does not need a PDF engine in Rust.
 */
object PdfPages {
    fun render(
        pdf: File,
        outDir: File,
        baseName: String,
        format: String,
        quality: Int,
        maxDimension: Int,
        onProgress: (Float) -> Boolean,
    ): List<File> {
        val result = mutableListOf<File>()
        ParcelFileDescriptor.open(pdf, ParcelFileDescriptor.MODE_READ_ONLY).use { fd ->
            PdfRenderer(fd).use { renderer ->
                val count = renderer.pageCount
                for (i in 0 until count) {
                    if (!onProgress(i.toFloat() / count)) throw InterruptedException("cancelled")
                    renderer.openPage(i).use { page ->
                        // 150 DPI: the page size is in points (1/72 inch).
                        var scale = 150f / 72f
                        val limit = if (maxDimension > 0) maxDimension else 4000
                        val longest = maxOf(page.width, page.height) * scale
                        if (longest > limit) scale *= limit / longest
                        val w = (page.width * scale).toInt().coerceAtLeast(1)
                        val h = (page.height * scale).toInt().coerceAtLeast(1)
                        val bmp = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
                        bmp.eraseColor(Color.WHITE)
                        page.render(bmp, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                        val out = File(outDir, "%s-page%03d.%s".format(baseName, i + 1, format))
                        out.outputStream().use {
                            val fmt = if (format == "png") Bitmap.CompressFormat.PNG else Bitmap.CompressFormat.JPEG
                            bmp.compress(fmt, quality, it)
                        }
                        bmp.recycle()
                        result += out
                    }
                }
            }
        }
        onProgress(1f)
        return result
    }
}
