package app.converter.ui

import android.os.Build
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.InsertDriveFile
import androidx.compose.material.icons.outlined.AudioFile
import androidx.compose.material.icons.outlined.CheckCircle
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material.icons.outlined.FileOpen
import androidx.compose.material.icons.outlined.Image
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.Movie
import androidx.compose.material.icons.outlined.OpenInNew
import androidx.compose.material.icons.outlined.PictureAsPdf
import androidx.compose.material.icons.outlined.Share
import androidx.compose.material.icons.outlined.SwapHoriz
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.converter.AUDIO_ONLY
import app.converter.ConverterViewModel
import app.converter.Files
import app.converter.LOSSY
import app.converter.Outputs
import app.converter.Status
import app.converter.UiState
import app.converter.core.FileKind
import app.converter.core.coreVersion
import java.io.File

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ConverterScreen(vm: ConverterViewModel) {
    val state by vm.state.collectAsStateWithLifecycle()
    var showAbout by remember { mutableStateOf(false) }
    val scroll = TopAppBarDefaults.pinnedScrollBehavior()

    val pick = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        vm.load(uris)
    }
    val pickFiles = { pick.launch(arrayOf("image/*", "audio/*", "video/*", "application/pdf")) }

    Scaffold(
        modifier = Modifier.nestedScroll(scroll.nestedScrollConnection),
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text("Converter") },
                navigationIcon = {
                    if (state.inputs.isNotEmpty() || state.status is Status.Failed) {
                        IconButton(onClick = vm::reset) { Icon(Icons.Outlined.Close, "Start again") }
                    }
                },
                actions = {
                    IconButton(onClick = { showAbout = true }) { Icon(Icons.Outlined.Info, "About") }
                },
                scrollBehavior = scroll,
            )
        },
    ) { padding ->
        Box(
            Modifier
                .fillMaxSize()
                .padding(padding),
        ) {
            AnimatedContent(targetState = screenOf(state), label = "screen") { screen ->
                Column(
                    Modifier
                        .fillMaxSize()
                        .verticalScroll(rememberScrollState())
                        .padding(horizontal = 16.dp, vertical = 8.dp),
                    verticalArrangement = Arrangement.spacedBy(16.dp),
                ) {
                    when (screen) {
                        Screen.Start -> StartScreen(onPick = pickFiles)
                        Screen.Loading -> Box(Modifier.fillMaxWidth().padding(48.dp), Alignment.Center) {
                            CircularProgressIndicator()
                        }
                        Screen.Settings -> SettingsScreen(state, vm, onChange = pickFiles)
                        Screen.Working -> WorkingScreen(state, onCancel = vm::cancel)
                        Screen.Done -> DoneScreen(
                            (state.status as? Status.Done)?.outputs.orEmpty(),
                            onAgain = vm::backToSettings,
                            onNew = { vm.reset(); pickFiles() },
                        )
                        Screen.Failed -> FailedScreen(
                            (state.status as? Status.Failed)?.message.orEmpty(),
                            canRetry = state.inputs.isNotEmpty(),
                            onRetry = vm::backToSettings,
                            onNew = { vm.reset(); pickFiles() },
                        )
                    }
                }
            }
        }
    }

    if (showAbout) AboutDialog(onDismiss = { showAbout = false })
}

private enum class Screen { Start, Loading, Settings, Working, Done, Failed }

private fun screenOf(s: UiState): Screen = when (s.status) {
    Status.Loading -> Screen.Loading
    is Status.Working -> Screen.Working
    is Status.Done -> Screen.Done
    is Status.Failed -> Screen.Failed
    Status.Idle -> if (s.inputs.isEmpty()) Screen.Start else Screen.Settings
}

// ------------------------------------------------------------------ start

@Composable
private fun ColumnScope.StartScreen(onPick: () -> Unit) {
    Spacer(Modifier.height(24.dp))
    Surface(
        shape = CircleShape,
        color = MaterialTheme.colorScheme.primaryContainer,
        modifier = Modifier
            .size(96.dp)
            .align(Alignment.CenterHorizontally),
    ) {
        Box(contentAlignment = Alignment.Center) {
            Icon(
                Icons.Outlined.SwapHoriz,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onPrimaryContainer,
                modifier = Modifier.size(48.dp),
            )
        }
    }
    Text(
        "Convert your files",
        style = MaterialTheme.typography.headlineMedium,
        textAlign = TextAlign.Center,
        modifier = Modifier.fillMaxWidth(),
    )
    Text(
        "Images, audio, video and PDF. All work stays on your phone. No upload.",
        style = MaterialTheme.typography.bodyLarge,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        textAlign = TextAlign.Center,
        modifier = Modifier.fillMaxWidth(),
    )
    Button(
        onClick = onPick,
        modifier = Modifier
            .fillMaxWidth()
            .height(56.dp),
    ) {
        Icon(Icons.Outlined.FileOpen, null)
        Spacer(Modifier.width(8.dp))
        Text("Choose files")
    }
    Text(
        "Tip: choose many images to make one PDF.",
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        textAlign = TextAlign.Center,
        modifier = Modifier.fillMaxWidth(),
    )

    FormatGroup(Icons.Outlined.Image, "Images", "JPG, PNG, WebP, GIF, BMP, TIFF, ICO, to PDF")
    FormatGroup(Icons.Outlined.AudioFile, "Audio", "MP3, M4A, OGG, OPUS, FLAC, WAV")
    FormatGroup(Icons.Outlined.Movie, "Video", "MP4, MKV, WebM, MOV, AVI, GIF, or audio only")
    FormatGroup(Icons.Outlined.PictureAsPdf, "PDF", "PDF pages to PNG or JPG")
}

@Composable
private fun FormatGroup(icon: ImageVector, title: String, formats: String) {
    Card(
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(icon, null, tint = MaterialTheme.colorScheme.primary)
            Spacer(Modifier.width(16.dp))
            Column {
                Text(title, style = MaterialTheme.typography.titleMedium)
                Text(formats, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
}

// ------------------------------------------------------------------ settings

private fun iconFor(kind: FileKind): ImageVector = when (kind) {
    FileKind.IMAGE -> Icons.Outlined.Image
    FileKind.AUDIO -> Icons.Outlined.AudioFile
    FileKind.VIDEO -> Icons.Outlined.Movie
    FileKind.PDF -> Icons.Outlined.PictureAsPdf
    FileKind.UNKNOWN -> Icons.AutoMirrored.Outlined.InsertDriveFile
}

@Composable
private fun SettingsScreen(state: UiState, vm: ConverterViewModel, onChange: () -> Unit) {
    InputCard(state, onChange)

    SectionTitle("Convert to")
    if (state.kind == FileKind.VIDEO) {
        TargetChips(state.targets.filter { it !in AUDIO_ONLY }, state.target, vm::selectTarget, "Video")
        TargetChips(state.targets.filter { it in AUDIO_ONLY }, state.target, vm::selectTarget, "Audio only")
    } else {
        TargetChips(state.targets, state.target, vm::selectTarget, null)
    }

    val target = state.target
    if (target != null) {
        val showQuality = target in LOSSY
        val showSize = target !in AUDIO_ONLY
        if (showQuality || showSize) {
            ElevatedCard(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    if (showQuality) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text("Quality", style = MaterialTheme.typography.titleSmall, modifier = Modifier.weight(1f))
                            Text("${state.quality}%", style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
                        }
                        Slider(
                            value = state.quality.toFloat(),
                            onValueChange = { vm.setQuality(it.toInt()) },
                            valueRange = 10f..100f,
                        )
                        Text(
                            "Lower quality gives a smaller file.",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    if (showSize) {
                        Text("Largest side", style = MaterialTheme.typography.titleSmall)
                        SizeSelector(state.maxDimension, vm::setMaxDimension)
                    }
                }
            }
        }

        Button(
            onClick = vm::convert,
            modifier = Modifier
                .fillMaxWidth()
                .height(56.dp),
        ) {
            Text("Convert to ${target.uppercase()}", style = MaterialTheme.typography.titleMedium)
        }
        if (target == "pdf" && state.inputs.size > 1) {
            Text(
                "All ${state.inputs.size} images go into one PDF, one image on each page.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun InputCard(state: UiState, onChange: () -> Unit) {
    Card(
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.secondaryContainer),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(iconFor(state.kind), null, tint = MaterialTheme.colorScheme.onSecondaryContainer, modifier = Modifier.size(32.dp))
            Spacer(Modifier.width(16.dp))
            Column(Modifier.weight(1f)) {
                val first = state.inputs.first()
                Text(
                    if (state.inputs.size == 1) first.name else "${state.inputs.size} files",
                    style = MaterialTheme.typography.titleMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    if (state.inputs.size == 1) {
                        "${first.extension.uppercase()} · ${Files.humanSize(first.size)}"
                    } else {
                        state.inputs.joinToString { it.name }
                    },
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            TextButton(onClick = onChange) { Text("Change") }
        }
    }
}

@Composable
private fun SectionTitle(text: String) {
    Text(text, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun TargetChips(targets: List<String>, selected: String?, onSelect: (String) -> Unit, label: String?) {
    if (targets.isEmpty()) return
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        if (label != null) {
            Text(label, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            targets.forEach { t ->
                FilterChip(
                    selected = t == selected,
                    onClick = { onSelect(t) },
                    label = { Text(t.uppercase()) },
                )
            }
        }
    }
}

private val SIZES = listOf(0 to "Original", 1920 to "1920", 1280 to "1280", 640 to "640")

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SizeSelector(value: Int, onChange: (Int) -> Unit) {
    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
        SIZES.forEachIndexed { i, (px, label) ->
            SegmentedButton(
                selected = value == px,
                onClick = { onChange(px) },
                shape = SegmentedButtonDefaults.itemShape(i, SIZES.size),
            ) { Text(label, maxLines = 1) }
        }
    }
}

// ------------------------------------------------------------------ working

@Composable
private fun WorkingScreen(state: UiState, onCancel: () -> Unit) {
    val w = state.status as? Status.Working ?: return
    Spacer(Modifier.height(32.dp))
    ElevatedCard(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Text(
                "Converting to ${state.target?.uppercase()}",
                style = MaterialTheme.typography.titleLarge,
            )
            if (w.step.isNotBlank() && state.inputs.size > 1) {
                Text(w.step, style = MaterialTheme.typography.bodyMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            LinearProgressIndicator(progress = { w.progress }, modifier = Modifier.fillMaxWidth())
            Text(
                "${(w.progress * 100).toInt()}%",
                style = MaterialTheme.typography.headlineSmall,
                color = MaterialTheme.colorScheme.primary,
            )
            Text(
                "Keep the app open until the work is done.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            OutlinedButton(onClick = onCancel, modifier = Modifier.align(Alignment.End)) { Text("Cancel") }
        }
    }
}

// ------------------------------------------------------------------ done

@Composable
private fun DoneScreen(outputs: List<File>, onAgain: () -> Unit, onNew: () -> Unit) {
    val ctx = LocalContext.current
    fun toast(msg: String) = Toast.makeText(ctx, msg, Toast.LENGTH_SHORT).show()

    val saveOne = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("*/*")) { uri ->
        if (uri != null) runCatching { Outputs.copyTo(ctx, outputs.first(), uri) }
            .onSuccess { toast("Saved") }.onFailure { toast("Not saved: ${it.message}") }
    }
    val saveMany = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { tree ->
        if (tree != null) runCatching { Outputs.copyToTree(ctx, outputs, tree) }
            .onSuccess { toast("Saved ${outputs.size} files") }.onFailure { toast("Not saved: ${it.message}") }
    }

    Spacer(Modifier.height(16.dp))
    Card(
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.primaryContainer),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Icon(Icons.Outlined.CheckCircle, null, tint = MaterialTheme.colorScheme.onPrimaryContainer, modifier = Modifier.size(40.dp))
            Text("Done", style = MaterialTheme.typography.headlineSmall, color = MaterialTheme.colorScheme.onPrimaryContainer)
            outputs.take(6).forEach {
                Text(
                    "${it.name} · ${Files.humanSize(it.length())}",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onPrimaryContainer,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            if (outputs.size > 6) {
                Text("and ${outputs.size - 6} more", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onPrimaryContainer)
            }
        }
    }

    Button(
        onClick = {
            val saved = runCatching { Outputs.saveToDownloads(ctx, outputs) }
            when {
                saved.isFailure -> toast("Not saved: ${saved.exceptionOrNull()?.message}")
                saved.getOrDefault(false) -> toast("Saved to Downloads/Converter")
                outputs.size == 1 -> saveOne.launch(outputs.first().name)
                else -> saveMany.launch(null)
            }
        },
        modifier = Modifier
            .fillMaxWidth()
            .height(56.dp),
    ) {
        Icon(Icons.Outlined.Download, null)
        Spacer(Modifier.width(8.dp))
        Text(if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) "Save to Downloads" else "Save")
    }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
        FilledTonalButton(onClick = { Outputs.share(ctx, outputs) }, modifier = Modifier.weight(1f)) {
            Icon(Icons.Outlined.Share, null)
            Spacer(Modifier.width(8.dp))
            Text("Share")
        }
        if (outputs.size == 1) {
            FilledTonalButton(
                onClick = { runCatching { Outputs.open(ctx, outputs.first()) }.onFailure { toast("No app can open this file") } },
                modifier = Modifier.weight(1f),
            ) {
                Icon(Icons.Outlined.OpenInNew, null)
                Spacer(Modifier.width(8.dp))
                Text("Open")
            }
        }
    }
    TextButton(
        onClick = {
            if (outputs.size == 1) saveOne.launch(outputs.first().name) else saveMany.launch(null)
        },
        modifier = Modifier.fillMaxWidth(),
    ) { Text("Save as…") }

    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
        OutlinedButton(onClick = onAgain, modifier = Modifier.weight(1f)) { Text("Other format") }
        OutlinedButton(onClick = onNew, modifier = Modifier.weight(1f)) { Text("New file") }
    }
}

// ------------------------------------------------------------------ failed

@Composable
private fun FailedScreen(message: String, canRetry: Boolean, onRetry: () -> Unit, onNew: () -> Unit) {
    Spacer(Modifier.height(16.dp))
    Card(
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.errorContainer),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Icon(Icons.Outlined.ErrorOutline, null, tint = MaterialTheme.colorScheme.onErrorContainer, modifier = Modifier.size(40.dp))
            Text("Something went wrong", style = MaterialTheme.typography.titleLarge, color = MaterialTheme.colorScheme.onErrorContainer)
            Text(message, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onErrorContainer)
        }
    }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
        if (canRetry) OutlinedButton(onClick = onRetry, modifier = Modifier.weight(1f)) { Text("Try again") }
        Button(onClick = onNew, modifier = Modifier.weight(1f)) { Text("Choose a file") }
    }
}

// ------------------------------------------------------------------ about

@Composable
private fun AboutDialog(onDismiss: () -> Unit) {
    val version = remember { runCatching { coreVersion() }.getOrElse { "engine not loaded: ${it.message}" } }
    AlertDialog(
        onDismissRequest = onDismiss,
        confirmButton = { TextButton(onClick = onDismiss) { Text("OK") } },
        icon = { Icon(Icons.Outlined.Info, null) },
        title = { Text("About Converter") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("All conversions run on your phone. The app does not use the internet.")
                Text("Engine: Rust, with FFmpeg (LGPL), LAME, Opus and libvpx.")
                Text(version, style = MaterialTheme.typography.bodySmall)
            }
        },
    )
}
