package app.converter

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import android.webkit.MimeTypeMap
import java.io.File

/** A file the user picked, copied into the app cache so Rust can read it by path. */
data class InputFile(
    val uri: Uri,
    val name: String,
    val size: Long,
    val extension: String,
    val localPath: String,
)

object Files {
    fun inputDir(ctx: Context) = File(ctx.cacheDir, "input").apply { mkdirs() }
    fun outputDir(ctx: Context) = File(ctx.cacheDir, "output").apply { mkdirs() }

    /** Delete old work files from the last run. */
    fun clean(ctx: Context) {
        inputDir(ctx).deleteRecursively()
        outputDir(ctx).deleteRecursively()
    }

    fun displayName(ctx: Context, uri: Uri): Pair<String, Long> {
        var name = uri.lastPathSegment ?: "file"
        var size = -1L
        ctx.contentResolver.query(uri, null, null, null, null)?.use { c ->
            if (c.moveToFirst()) {
                val n = c.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                val s = c.getColumnIndex(OpenableColumns.SIZE)
                if (n >= 0 && !c.isNull(n)) name = c.getString(n)
                if (s >= 0 && !c.isNull(s)) size = c.getLong(s)
            }
        }
        return name to size
    }

    /** Find the extension from the name, or from the MIME type if the name has none. */
    fun extensionOf(ctx: Context, uri: Uri, name: String): String {
        val fromName = name.substringAfterLast('.', "").lowercase()
        if (fromName.isNotEmpty() && fromName.length <= 5) return fromName
        val mime = ctx.contentResolver.getType(uri) ?: return ""
        return MimeTypeMap.getSingleton().getExtensionFromMimeType(mime)?.lowercase() ?: ""
    }

    fun copyToCache(ctx: Context, uri: Uri, index: Int = 0): InputFile {
        val (name, size) = displayName(ctx, uri)
        val ext = extensionOf(ctx, uri, name)
        val target = File(inputDir(ctx), "in$index.$ext")
        ctx.contentResolver.openInputStream(uri).use { input ->
            requireNotNull(input) { "Cannot open $name" }
            target.outputStream().use { input.copyTo(it) }
        }
        return InputFile(uri, name, if (size >= 0) size else target.length(), ext, target.absolutePath)
    }

    fun mimeOf(ext: String): String =
        MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext)
            ?: when (ext) {
                "opus" -> "audio/ogg"
                "m4a" -> "audio/mp4"
                "mkv" -> "video/x-matroska"
                else -> "application/octet-stream"
            }

    fun humanSize(bytes: Long): String {
        if (bytes < 1024) return "$bytes B"
        val units = listOf("KB", "MB", "GB")
        var v = bytes / 1024.0
        var i = 0
        while (v >= 1024 && i < units.lastIndex) { v /= 1024; i++ }
        return String.format(java.util.Locale.ROOT, "%.1f %s", v, units[i])
    }
}
