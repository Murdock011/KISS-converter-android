package app.converter

import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.DocumentsContract
import android.provider.MediaStore
import androidx.core.content.FileProvider
import java.io.File

/** Save, share and open the converted files. */
object Outputs {
    private fun uriFor(ctx: Context, f: File): Uri =
        FileProvider.getUriForFile(ctx, "${ctx.packageName}.files", f)

    /** Android 10+: save to Downloads/Converter with no picker. Returns false on older Android. */
    fun saveToDownloads(ctx: Context, files: List<File>): Boolean {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return false
        val resolver = ctx.contentResolver
        for (f in files) {
            val values = ContentValues().apply {
                put(MediaStore.Downloads.DISPLAY_NAME, f.name)
                put(MediaStore.Downloads.MIME_TYPE, Files.mimeOf(f.extension))
                put(MediaStore.Downloads.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS + "/Converter")
                put(MediaStore.Downloads.IS_PENDING, 1)
            }
            val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
                ?: error("Cannot create ${f.name} in Downloads")
            resolver.openOutputStream(uri).use { out ->
                requireNotNull(out)
                f.inputStream().use { it.copyTo(out) }
            }
            values.clear()
            values.put(MediaStore.Downloads.IS_PENDING, 0)
            resolver.update(uri, values, null, null)
        }
        return true
    }

    /** Copy one file to a place the user chose with the "Save as" picker. */
    fun copyTo(ctx: Context, f: File, target: Uri) {
        ctx.contentResolver.openOutputStream(target).use { out ->
            requireNotNull(out)
            f.inputStream().use { it.copyTo(out) }
        }
    }

    /** Copy many files into a folder the user chose with the folder picker. */
    fun copyToTree(ctx: Context, files: List<File>, tree: Uri) {
        val parent = DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))
        for (f in files) {
            val doc = DocumentsContract.createDocument(ctx.contentResolver, parent, Files.mimeOf(f.extension), f.name)
                ?: error("Cannot create ${f.name}")
            copyTo(ctx, f, doc)
        }
    }

    fun share(ctx: Context, files: List<File>) {
        val uris = ArrayList(files.map { uriFor(ctx, it) })
        val mime = files.map { Files.mimeOf(it.extension) }.distinct().singleOrNull() ?: "*/*"
        val send = if (uris.size == 1) {
            Intent(Intent.ACTION_SEND).putExtra(Intent.EXTRA_STREAM, uris[0])
        } else {
            Intent(Intent.ACTION_SEND_MULTIPLE).putParcelableArrayListExtra(Intent.EXTRA_STREAM, uris)
        }.setType(mime).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        ctx.startActivity(Intent.createChooser(send, null).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }

    fun open(ctx: Context, f: File) {
        val view = Intent(Intent.ACTION_VIEW)
            .setDataAndType(uriFor(ctx, f), Files.mimeOf(f.extension))
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        ctx.startActivity(Intent.createChooser(view, null).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }
}
