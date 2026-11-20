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

/** A file that did not convert, and why. */
data class FailedFile(val input: InputFile, val message: String)

sealed interface Status {
    data object Idle : Status
    data object Loading : Status
    data class Working(val progress: Float, val step: String) : Status
    data class Done(val outputs: List<File>, val failures: List<FailedFile> = emptyList()) : Status
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

    private val store = Store(app)
    private val _presets = MutableStateFlow(store.presets())
    val presets: StateFlow<List<Preset>> = _presets
    private val _recent = MutableStateFlow(store.recent())
    val recent: StateFlow<List<RecentItem>> = _recent

    private val cancelled = AtomicBoolean(false)

    /** A preset the user tapped. It is used when the files are loaded. */
    private var pendingPreset: Preset? = null

    init {
        ConvertService.onStopRequest = { cancel() }
    }

    override fun onCleared() {
        cancelled.set(true)
        ConvertService.onStopRequest = null
        Work.stop(getApplication(), null)
    }

    /** Load files. With [append], add them to the files already chosen. */
    fun load(uris: List<Uri>, append: Boolean = false) {
        if (uris.isEmpty()) {
            pendingPreset = null
            return
        }
        val before = _state.value
        val keep = if (append && before.status == Status.Idle) before.inputs else emptyList()
        _state.value = UiState(status = Status.Loading)
        viewModelScope.launch {
            try {
                val ctx = getApplication<Application>()
                val added = withContext(Dispatchers.IO) {
                    if (keep.isEmpty()) Files.clean(ctx)
                    uris.mapIndexed { i, uri -> Files.copyToCache(ctx, uri, keep.size + i) }
                }
                val inputs = keep + added
                val kinds = inputs.map { kindOf(it.extension) }.toSet()
                if (kinds.size != 1 || kinds.first() == FileKind.UNKNOWN) {
                    pendingPreset = null
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
                var next = UiState(
                    inputs = inputs,
                    kind = kind,
                    targets = targets,
                    target = before.target?.takeIf { append && it in targets } ?: targets.firstOrNull(),
                    quality = if (append) before.quality else 85,
                    maxDimension = if (append) before.maxDimension else 0,
                )
                val p = pendingPreset
                pendingPreset = null
                if (p != null && p.kind == kind && p.target in targets) {
                    next = next.copy(target = p.target, quality = p.quality, maxDimension = p.maxDimension)
                }
                _state.value = next
            } catch (e: Exception) {
                pendingPreset = null
                _state.value = UiState(status = Status.Failed(e.message ?: "Cannot read the file."))
            }
        }
    }

    /** Take one file out of the list. */
    fun removeInput(index: Int) {
        val s = _state.value
        if (index !in s.inputs.indices) return
        val inputs = s.inputs.toMutableList().apply { removeAt(index) }
        if (inputs.isEmpty()) {
            _state.value = UiState()
            return
        }
        val targets = targetsFor(inputs, s.kind)
        _state.value = s.copy(
            inputs = inputs,
            targets = targets,
            target = s.target?.takeIf { it in targets } ?: targets.firstOrNull(),
        )
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
        pendingPreset = null
        _state.value = UiState()
    }

    /** Go back from "done" or "failed" to the settings, with the same files. */
    fun backToSettings() = _state.update {
        if (it.inputs.isEmpty()) UiState() else it.copy(status = Status.Idle)
    }

    fun cancel() = cancelled.set(true)

    // ------------------------------------------------------------ presets

    /** The screen opens the file picker after this; the preset is used for the files. */
    fun usePreset(p: Preset) {
        pendingPreset = p
    }

    /** Save the current settings as a preset. Returns its name. */
    fun saveCurrentAsPreset(): String? {
        val s = _state.value
        val target = s.target ?: return null
        val name = presetName(s.kind, target, s.quality)
        val existing = _presets.value
        if (existing.none { it.name == name }) {
            val p = Preset(System.currentTimeMillis(), name, s.kind, target, s.quality, s.maxDimension)
            _presets.value = existing + p
            store.savePresets(_presets.value)
        }
        return name
    }

    fun deletePreset(id: Long) {
        _presets.value = _presets.value.filter { it.id != id }
        store.savePresets(_presets.value)
    }

    private fun presetName(kind: FileKind, target: String, quality: Int): String {
        val what = when (kind) {
            FileKind.IMAGE -> "Images"
            FileKind.AUDIO -> "Audio"
            FileKind.VIDEO -> "Video"
            FileKind.PDF -> "PDF"
            FileKind.DOCUMENT -> "Documents"
            FileKind.UNKNOWN -> "Files"
        }
        val level = when {
            target !in LOSSY || kind == FileKind.DOCUMENT -> null
            quality <= 70 -> "small"
            quality >= 92 -> "best"
            else -> null
        }
        return if (level == null) "$what to ${target.uppercase()}" else "$what to $level ${target.uppercase()}"
    }

    // ------------------------------------------------------------ recent

    fun deleteRecent(item: RecentItem) {
        item.file.delete()
        _recent.value = _recent.value.filter { it.path != item.path }
        store.saveRecent(_recent.value)
    }

    fun clearRecent() {
        _recent.value.forEach { it.file.delete() }
        _recent.value = emptyList()
        store.saveRecent(emptyList())
    }

    private fun addRecent(items: List<RecentItem>) {
        val all = items + _recent.value
        val keep = all.take(Store.MAX_RECENT)
        all.drop(Store.MAX_RECENT).forEach { it.file.delete() }
        _recent.value = keep
        store.saveRecent(keep)
    }

    // ------------------------------------------------------------ convert

    /** Convert only the files that failed, with another format (for example "mp3"). */
    fun retryFailed(target: String) {
        val done = _state.value.status as? Status.Done ?: return
        val inputs = done.failures.map { it.input }
        if (inputs.isEmpty()) return
        _state.update { it.copy(inputs = inputs, target = target) }
        convert()
    }

    fun convert() {
        val s = _state.value
        val target = s.target ?: return
        if (s.inputs.isEmpty()) return
        cancelled.set(false)
        _state.update { it.copy(status = Status.Working(0f, "")) }

        val ctx = getApplication<Application>()
        val title = "Converting to ${target.uppercase()}"
        Work.start(ctx, title)

        viewModelScope.launch {
            var lastPercent = -1
            val result = withContext(Dispatchers.Default) {
                runCatching {
                    val outDir = File(Files.outputDir(ctx), "run-${System.currentTimeMillis()}").apply { mkdirs() }
                    val opts = ConvertOptions(s.quality.toUByte(), s.maxDimension.toUInt())
                    val n = s.inputs.size
                    val outputs = mutableListOf<File>()
                    val recent = mutableListOf<RecentItem>()
                    val failures = mutableListOf<FailedFile>()

                    fun listener(index: Int, count: Int, step: String) = object : ProgressListener {
                        override fun onProgress(fraction: Float): Boolean {
                            val p = (index + fraction) / count
                            _state.update { st ->
                                if (st.status is Status.Working) st.copy(status = Status.Working(p, step)) else st
                            }
                            val percent = (p * 100).toInt()
                            if (percent != lastPercent) {
                                lastPercent = percent
                                Work.update(ctx, title, percent)
                            }
                            return !cancelled.get()
                        }
                    }

                    fun record(input: InputFile, files: List<File>, inBytes: Long) {
                        files.forEach {
                            recent += RecentItem(
                                it.absolutePath, it.name, input.extension, target,
                                inBytes / files.size, it.length(), System.currentTimeMillis(),
                            )
                        }
                    }

                    /** Convert one file. A failure is kept, so the other files still convert. */
                    fun each(i: Int, f: InputFile, block: () -> List<File>) {
                        try {
                            val files = block()
                            outputs += files
                            record(f, files, f.size)
                        } catch (e: Throwable) {
                            if (e is ConvertException.Cancelled || e is InterruptedException || cancelled.get()) throw e
                            failures += FailedFile(f, messageOf(e))
                        }
                    }

                    when {
                        s.kind == FileKind.PDF -> s.inputs.forEachIndexed { i, f ->
                            val l = listener(i, n, f.name)
                            each(i, f) {
                                PdfPages.render(
                                    File(f.localPath), outDir, baseName(f.name), target,
                                    s.quality, s.maxDimension,
                                ) { l.onProgress(it) }
                            }
                        }
                        // Many images into one PDF.
                        target == "pdf" && n > 1 -> {
                            val out = File(outDir, "${baseName(s.inputs.first().name)}-and-${n - 1}-more.pdf")
                            rustImagesToPdf(s.inputs.map { it.localPath }, out.absolutePath, opts, listener(0, 1, "$n images"))
                            outputs += out
                            record(s.inputs.first(), listOf(out), s.inputs.sumOf { it.size })
                        }
                        else -> s.inputs.forEachIndexed { i, f ->
                            each(i, f) {
                                val out = uniqueFile(outDir, baseName(f.name), target)
                                rustConvert(f.localPath, out.absolutePath, opts, listener(i, n, f.name))
                                listOf(out)
                            }
                        }
                    }
                    Triple(outputs.toList(), failures.toList(), recent.toList())
                }
            }
            result.onSuccess { (outputs, _, recent) ->
                if (outputs.isNotEmpty()) addRecent(recent)
            }
            _state.update { st ->
                result.fold(
                    onSuccess = { (outputs, failures, _) ->
                        when {
                            outputs.isEmpty() && failures.size == 1 ->
                                st.copy(status = Status.Failed(failures.first().message))
                            outputs.isEmpty() ->
                                st.copy(status = Status.Done(outputs, failures))
                            else -> st.copy(status = Status.Done(outputs, failures))
                        }
                    },
                    onFailure = { e ->
                        when {
                            e is ConvertException.Cancelled || e is InterruptedException || cancelled.get() ->
                                st.copy(status = Status.Idle)
                            else -> st.copy(status = Status.Failed(messageOf(e)))
                        }
                    },
                )
            }
            val doneMessage = when (val st = _state.value.status) {
                is Status.Done -> when {
                    st.failures.isEmpty() && st.outputs.size == 1 -> "File converted"
                    st.failures.isEmpty() -> "${st.outputs.size} files converted"
                    else -> "${st.outputs.size} converted, ${st.failures.size} not converted"
                }
                is Status.Failed -> "The file did not convert"
                else -> null
            }
            Work.stop(ctx, doneMessage)
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
