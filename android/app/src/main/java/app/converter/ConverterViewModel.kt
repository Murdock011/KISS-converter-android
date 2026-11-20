package app.converter

import android.app.Application
import android.net.Uri
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import app.converter.core.ConvertException
import app.converter.core.ConvertOptions
import app.converter.core.FileKind
import app.converter.core.ProgressListener
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean
import app.converter.core.convert as rustConvert
import app.converter.core.detectKind as rustDetectKind
import app.converter.core.imagesToPdf as rustImagesToPdf
import app.converter.core.targetFormats as rustTargetFormats

sealed interface Status {
    data object Idle : Status
    data object Loading : Status
    data class Working(val progress: Float, val step: String) : Status
    data class Done(val outputs: List<File>) : Status
    data class Failed(val message: String) : Status
}

data class UiState(
    val inputs: List<InputFile> = emptyList(),
    val kind: FileKind = FileKind.UNKNOWN,
    val targets: List<String> = emptyList(),
    val target: String? = null,
    val quality: Int = 85,
    val maxDimension: Int = 0,
    val status: Status = Status.Idle,
)

/** Output formats where the quality setting has an effect. */
val LOSSY = setOf("jpg", "mp3", "m4a", "ogg", "opus", "mp4", "mkv", "webm", "mov", "avi", "pdf")
val AUDIO_ONLY = setOf("mp3", "m4a", "ogg", "opus", "flac", "wav")

class ConverterViewModel(app: Application) : AndroidViewModel(app) {
    private val _state = MutableStateFlow(UiState())
    val state: StateFlow<UiState> = _state

    private val cancelled = AtomicBoolean(false)

    fun load(uris: List<Uri>) {
        if (uris.isEmpty()) return
        _state.value = UiState(status = Status.Loading)
        viewModelScope.launch {
            try {
                val ctx = getApplication<Application>()
                val inputs = withContext(Dispatchers.IO) {
                    Files.clean(ctx)
                    uris.mapIndexed { i, uri -> Files.copyToCache(ctx, uri, i) }
                }
                val kinds = inputs.map { kindOf(it.extension) }.toSet()
                if (kinds.size != 1 || kinds.first() == FileKind.UNKNOWN) {
                    _state.value = UiState(
                        status = Status.Failed(
                            if (kinds.size > 1) "Choose files of one type only (for example, only images)."
                            else "This file type is not supported: .${inputs.first().extension}",
                        ),
                    )
                    return@launch
                }
                val kind = kinds.first()
                val targets = targetsFor(inputs, kind)
                _state.value = UiState(
                    inputs = inputs,
                    kind = kind,
                    targets = targets,
                    target = targets.firstOrNull(),
                )
            } catch (e: Exception) {
                _state.value = UiState(status = Status.Failed(e.message ?: "Cannot read the file."))
            }
        }
    }

    private fun kindOf(ext: String): FileKind = try {
        rustDetectKind(ext)
    } catch (e: Throwable) {
        FileKind.UNKNOWN
    }

    private fun targetsFor(inputs: List<InputFile>, kind: FileKind): List<String> {
        if (kind == FileKind.PDF) return listOf("png", "jpg")
        // Keep only the formats that work for every chosen file.
        val sets = inputs.map { rustTargetFormats(it.extension) }
        var common = sets.first()
        for (s in sets.drop(1)) common = common.filter { it in s }
        return common
    }

    fun selectTarget(t: String) = _state.update { it.copy(target = t) }
    fun setQuality(q: Int) = _state.update { it.copy(quality = q.coerceIn(1, 100)) }
    fun setMaxDimension(d: Int) = _state.update { it.copy(maxDimension = d) }

    fun reset() {
        cancelled.set(true)
        _state.value = UiState()
    }

    /** Go back from "done" or "failed" to the settings, with the same files. */
    fun backToSettings() = _state.update {
        if (it.inputs.isEmpty()) UiState() else it.copy(status = Status.Idle)
    }

    fun cancel() = cancelled.set(true)

    fun convert() {
        val s = _state.value
        val target = s.target ?: return
        if (s.inputs.isEmpty()) return
        cancelled.set(false)
        _state.update { it.copy(status = Status.Working(0f, "")) }

        viewModelScope.launch {
            val ctx = getApplication<Application>()
            val result = withContext(Dispatchers.Default) {
                runCatching {
                    val outDir = File(Files.outputDir(ctx), "run-${System.currentTimeMillis()}").apply { mkdirs() }
                    val opts = ConvertOptions(s.quality.toUByte(), s.maxDimension.toUInt())
                    val n = s.inputs.size
                    val outputs = mutableListOf<File>()

                    fun listener(index: Int, count: Int, step: String) = object : ProgressListener {
                        override fun onProgress(fraction: Float): Boolean {
                            _state.update { st ->
                                if (st.status is Status.Working)
                                    st.copy(status = Status.Working((index + fraction) / count, step))
                                else st
                            }
                            return !cancelled.get()
                        }
                    }

                    when {
                        s.kind == FileKind.PDF -> s.inputs.forEachIndexed { i, f ->
                            val l = listener(i, n, f.name)
                            outputs += PdfPages.render(
                                File(f.localPath), outDir, baseName(f.name), target,
                                s.quality, s.maxDimension,
                            ) { l.onProgress(it) }
                        }
                        // Many images into one PDF.
                        target == "pdf" && n > 1 -> {
                            val out = File(outDir, "${baseName(s.inputs.first().name)}-and-${n - 1}-more.pdf")
                            rustImagesToPdf(s.inputs.map { it.localPath }, out.absolutePath, opts, listener(0, 1, "$n images"))
                            outputs += out
                        }
                        else -> s.inputs.forEachIndexed { i, f ->
                            val out = uniqueFile(outDir, baseName(f.name), target)
                            rustConvert(f.localPath, out.absolutePath, opts, listener(i, n, f.name))
                            outputs += out
                        }
                    }
                    outputs.toList()
                }
            }
            _state.update { st ->
                result.fold(
                    onSuccess = { st.copy(status = Status.Done(it)) },
                    onFailure = { e ->
                        when {
                            e is ConvertException.Cancelled || e is InterruptedException ->
                                st.copy(status = Status.Idle)
                            else -> st.copy(status = Status.Failed(messageOf(e)))
                        }
                    },
                )
            }
        }
    }

    private fun messageOf(e: Throwable): String = when (e) {
        is ConvertException.Unsupported -> "This conversion is not supported: ${e.msg}"
        is ConvertException.Input -> "Cannot read the file: ${e.msg}"
        is ConvertException.Failed -> "Conversion failed: ${e.msg}"
        is UnsatisfiedLinkError -> "The conversion engine did not load on this phone (${e.message})."
        else -> e.message ?: e.toString()
    }

    private fun baseName(name: String) =
        name.substringBeforeLast('.').ifBlank { "converted" }.replace(Regex("[/\\\\:*?\"<>|]"), "_")

    private fun uniqueFile(dir: File, base: String, ext: String): File {
        var f = File(dir, "$base.$ext")
        var i = 2
        while (f.exists()) f = File(dir, "$base ($i).$ext").also { i++ }
        return f
    }
}
