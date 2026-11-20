package app.converter.ui

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.automirrored.outlined.InsertDriveFile
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.AudioFile
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material.icons.outlined.CheckCircle
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material.icons.outlined.ExpandLess
import androidx.compose.material.icons.outlined.ExpandMore
import androidx.compose.material.icons.outlined.FileOpen
import androidx.compose.material.icons.outlined.History
import androidx.compose.material.icons.outlined.Image
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material.icons.outlined.Movie
import androidx.compose.material.icons.outlined.Notifications
import androidx.compose.material.icons.outlined.OpenInNew
import androidx.compose.material.icons.outlined.PictureAsPdf
import androidx.compose.material.icons.outlined.RadioButtonUnchecked
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.Share
import androidx.compose.material.icons.outlined.SwapHoriz
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.converter.AUDIO_ONLY
import app.converter.BuildConfig
import app.converter.ConverterViewModel
import app.converter.Estimate
import app.converter.FailedFile
import app.converter.Files
import app.converter.InputFile
import app.converter.LOSSY
import app.converter.Outputs
import app.converter.Preset
import app.converter.RecentItem
import app.converter.Status
import app.converter.UiState
import app.converter.core.FileKind
import app.converter.core.coreVersion
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlin.math.abs
import kotlin.math.roundToInt

private enum class Tab(val label: String, val icon: ImageVector) {
    Convert("Convert", Icons.Outlined.SwapHoriz),
    History("History", Icons.Outlined.History),
    Settings("Settings", Icons.Outlined.Settings),
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ConverterScreen(vm: ConverterViewModel) {
    val state by vm.state.collectAsStateWithLifecycle()
    val presets by vm.presets.collectAsStateWithLifecycle()
    val recent by vm.recent.collectAsStateWithLifecycle()
    var tab by rememberSaveable { mutableStateOf(Tab.Convert) }
    val ctx = LocalContext.current
    val scroll = TopAppBarDefaults.pinnedScrollBehavior()

    val pick = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        vm.load(uris)
    }
    val pickMore = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        vm.load(uris, append = true)
    }
    val pickFiles = { pick.launch(PICK_TYPES) }
    val pickOfType = { types: Array<String> -> pick.launch(types) }
    val usePreset = { p: Preset -> vm.usePreset(p); pick.launch(typesFor(p.kind)) }

    // Android 13+: ask one time for the progress notification. Convert in all cases.
    var askedNotify by rememberSaveable { mutableStateOf(false) }
    val notifyPermission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        vm.convert()
    }
    val convert = {
        val need = Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(ctx, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        if (need && !askedNotify) {
            askedNotify = true
            notifyPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        } else {
            vm.convert()
        }
    }

    val screen = screenOf(state)
    val home = screen == Screen.Start

    // Pixel size and play time of the chosen files, for the size estimate.
    val infos by produceState<List<Estimate.Info?>>(emptyList(), state.inputs, state.kind) {
        val inputs = state.inputs
        val kind = state.kind
        value = withContext(Dispatchers.IO) { inputs.map { Estimate.probe(it, kind) } }
    }
    val estimate = state.target?.let {
        Estimate.total(state.inputs, infos, state.kind, it, state.quality, state.maxDimension)
    }

    // A file opened from another app goes straight to the format screen.
    LaunchedEffect(screen) { if (!home) tab = Tab.Convert }
    BackHandler(enabled = screen == Screen.Settings || screen == Screen.Done || screen == Screen.Failed) { vm.reset() }
    BackHandler(enabled = home && tab != Tab.Convert) { tab = Tab.Convert }

    Scaffold(
        modifier = Modifier.nestedScroll(scroll.nestedScrollConnection),
        topBar = {
            TopAppBar(
                title = {
                    Text(
                        if (home) tab.label else titleOf(screen, state),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                },
                navigationIcon = {
                    when (screen) {
                        Screen.Settings -> IconButton(onClick = vm::reset) {
                            Icon(Icons.AutoMirrored.Outlined.ArrowBack, "Back")
                        }
                        Screen.Done, Screen.Failed -> IconButton(onClick = vm::reset) {
                            Icon(Icons.Outlined.Close, "Start again")
                        }
                        else -> {}
                    }
                },
                scrollBehavior = scroll,
            )
        },
        bottomBar = {
            when {
                home -> NavigationBar {
                    Tab.entries.forEach { t ->
                        NavigationBarItem(
                            selected = tab == t,
                            onClick = { tab = t },
                            icon = { Icon(t.icon, null) },
                            label = { Text(t.label) },
                        )
                    }
                }
                // The main action stays at the bottom, so it is always easy to reach.
                screen == Screen.Settings && state.target != null -> ConvertBar(state, estimate, onConvert = convert)
            }
        },
    ) { padding ->
        Box(
            Modifier
                .fillMaxSize()
                .padding(padding),
        ) {
            val key = if (home) "tab-${tab.name}" else screen.name
            AnimatedContent(targetState = key, label = "screen") { k ->
                Column(
                    Modifier
                        .fillMaxSize()
                        .verticalScroll(rememberScrollState())
                        .padding(horizontal = 16.dp, vertical = 8.dp),
                    verticalArrangement = Arrangement.spacedBy(20.dp),
                ) {
                    when (k) {
                        "tab-Convert" -> StartScreen(
                            presets = presets,
                            recent = recent,
                            onPick = pickFiles,
                            onPickType = pickOfType,
                            onPreset = usePreset,
                            onSeeAll = { tab = Tab.History },
                            onEditPresets = { tab = Tab.Settings },
                        )
                        "tab-History" -> HistoryScreen(recent, vm::deleteRecent, vm::clearRecent)
                        "tab-Settings" -> SettingsTab(presets, vm::deletePreset)
                        Screen.Loading.name -> Box(Modifier.fillMaxWidth().padding(48.dp), Alignment.Center) {
                            CircularProgressIndicator()
                        }
                        Screen.Settings.name -> SettingsScreen(state, vm, onAdd = { pickMore.launch(PICK_TYPES) })
                        Screen.Working.name -> WorkingScreen(state, onCancel = vm::cancel)
                        Screen.Done.name -> {
                            val done = state.status as? Status.Done
                            DoneScreen(
                                state = state,
                                outputs = done?.outputs.orEmpty(),
                                failures = done?.failures.orEmpty(),
                                onAgain = vm::backToSettings,
                                onNew = { vm.reset(); pickFiles() },
                                onRetryAs = vm::retryFailed,
                                onSavePreset = {
                                    val name = vm.saveCurrentAsPreset()
                                    if (name != null) Toast.makeText(ctx, "Preset saved: $name", Toast.LENGTH_SHORT).show()
                                },
                                presetExists = presets.any { it.kind == state.kind && it.target == state.target },
                            )
                        }
                        Screen.Failed.name -> FailedScreen(
                            (state.status as? Status.Failed)?.message.orEmpty(),
                            canRetry = state.inputs.isNotEmpty(),
                            onRetry = vm::backToSettings,
                            onNew = { vm.reset(); pickFiles() },
                        )
                    }
                    Spacer(Modifier.height(8.dp))
                }
            }
        }
    }
}

/** File types shown in the file picker. */
private val DOC_TYPES = arrayOf(
    "text/*",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "application/vnd.oasis.opendocument.text",
    "application/rtf", "application/xhtml+xml",
)
private val PICK_TYPES = arrayOf("image/*", "audio/*", "video/*", "application/pdf") + DOC_TYPES

private fun typesFor(kind: FileKind): Array<String> = when (kind) {
    FileKind.IMAGE -> arrayOf("image/*")
    FileKind.AUDIO -> arrayOf("audio/*")
    FileKind.VIDEO -> arrayOf("video/*")
    FileKind.PDF -> arrayOf("application/pdf")
    FileKind.DOCUMENT -> DOC_TYPES
    FileKind.UNKNOWN -> PICK_TYPES
}

private enum class Screen { Start, Loading, Settings, Working, Done, Failed }

private fun screenOf(s: UiState): Screen = when (s.status) {
    Status.Loading -> Screen.Loading
    is Status.Working -> Screen.Working
    is Status.Done -> Screen.Done
    is Status.Failed -> Screen.Failed
    Status.Idle -> if (s.inputs.isEmpty()) Screen.Start else Screen.Settings
}

private fun titleOf(screen: Screen, s: UiState): String = when (screen) {
    Screen.Start, Screen.Loading -> "Convert"
    Screen.Settings -> if (s.inputs.size == 1) "Convert 1 file" else "Convert ${s.inputs.size} files"
    Screen.Working -> "Converting"
    Screen.Done -> {
        val d = s.status as? Status.Done
        if (d != null && d.failures.isNotEmpty()) "${d.outputs.size} of ${d.outputs.size + d.failures.size} converted" else ""
    }
    Screen.Failed -> "Not converted"
}

private fun kindName(kind: FileKind, count: Int): String {
    val one = when (kind) {
        FileKind.IMAGE -> "image"
        FileKind.AUDIO -> "audio file"
        FileKind.VIDEO -> "video"
        FileKind.PDF -> "PDF"
        FileKind.DOCUMENT -> "document"
        FileKind.UNKNOWN -> "file"
    }
    return if (count == 1) "1 $one" else "$count ${one}s"
}

@Composable
private fun SectionTitle(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.semantics { heading() },
    )
}

@Composable
private fun SectionHeader(text: String, action: String?, onAction: () -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.weight(1f)) { SectionTitle(text) }
        if (action != null) TextButton(onClick = onAction) { Text(action) }
    }
}

// ------------------------------------------------------------------ start

private data class TypeTile(val icon: ImageVector, val label: String, val types: Array<String>)

private val TILES = listOf(
    TypeTile(Icons.Outlined.Image, "Image", arrayOf("image/*")),
    TypeTile(Icons.Outlined.Movie, "Video", arrayOf("video/*")),
    TypeTile(Icons.Outlined.AudioFile, "Audio", arrayOf("audio/*")),
    TypeTile(Icons.Outlined.Description, "Doc", DOC_TYPES),
    TypeTile(Icons.Outlined.PictureAsPdf, "PDF", arrayOf("application/pdf")),
)

@Composable
private fun StartScreen(
    presets: List<Preset>,
    recent: List<RecentItem>,
    onPick: () -> Unit,
    onPickType: (Array<String>) -> Unit,
    onPreset: (Preset) -> Unit,
    onSeeAll: () -> Unit,
    onEditPresets: () -> Unit,
) {
    Card(
        shape = RoundedCornerShape(28.dp),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.primaryContainer,
            contentColor = MaterialTheme.colorScheme.onPrimaryContainer,
        ),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Surface(
                    shape = RoundedCornerShape(16.dp),
                    color = MaterialTheme.colorScheme.primary,
                    contentColor = MaterialTheme.colorScheme.onPrimary,
                    modifier = Modifier.size(48.dp),
                ) {
                    Box(contentAlignment = Alignment.Center) { Icon(Icons.Outlined.SwapHoriz, null) }
                }
                Spacer(Modifier.width(12.dp))
                Column {
                    Text(
                        "Convert files",
                        style = MaterialTheme.typography.titleLarge,
                        modifier = Modifier.semantics { heading() },
                    )
                    Text("Choose one file or many. All work stays on your phone.", style = MaterialTheme.typography.bodyMedium)
                }
            }
            Button(
                onClick = onPick,
                modifier = Modifier
                    .fillMaxWidth()
                    .height(56.dp),
            ) {
                Icon(Icons.Outlined.FileOpen, null)
                Spacer(Modifier.width(8.dp))
                Text("Choose files", style = MaterialTheme.typography.titleMedium)
            }
            Text(
                "Tip: in other apps, use \"Open with\" or \"Share\" and choose Convert.",
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }

    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SectionTitle("Start from a type")
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            TILES.forEach { t ->
                Surface(
                    onClick = { onPickType(t.types) },
                    shape = RoundedCornerShape(16.dp),
                    color = MaterialTheme.colorScheme.surfaceContainer,
                    modifier = Modifier
                        .weight(1f)
                        .heightIn(min = 76.dp),
                ) {
                    Column(
                        Modifier.padding(vertical = 12.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Icon(t.icon, null, tint = MaterialTheme.colorScheme.primary)
                        Text(t.label, style = MaterialTheme.typography.labelLarge, maxLines = 1)
                    }
                }
            }
        }
    }

    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        SectionHeader("My presets", "Edit", onEditPresets)
        if (presets.isEmpty()) {
            Hint("No presets. After a conversion, tap \"Save as a preset\".")
        } else {
            Row(
                Modifier.horizontalScroll(rememberScrollState()),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                presets.forEach { p ->
                    AssistChip(onClick = { onPreset(p) }, label = { Text(p.name) })
                }
            }
        }
    }

    Column {
        SectionHeader("Recent", if (recent.size > 3) "See all" else null, onSeeAll)
        if (recent.isEmpty()) {
            Hint("Your converted files show here.")
        } else {
            recent.take(3).forEach { RecentRow(it, onDelete = null) }
        }
    }
}

// ------------------------------------------------------------------ history and settings tabs

@Composable
private fun RecentRow(item: RecentItem, onDelete: ((RecentItem) -> Unit)?) {
    val ctx = LocalContext.current
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 64.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Surface(
            shape = RoundedCornerShape(12.dp),
            color = MaterialTheme.colorScheme.surfaceContainer,
            modifier = Modifier.size(44.dp),
        ) {
            Box(contentAlignment = Alignment.Center) {
                Text(
                    item.toExt.uppercase(),
                    style = MaterialTheme.typography.labelSmall,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.primary,
                    maxLines = 1,
                )
            }
        }
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            Text(item.name, style = MaterialTheme.typography.bodyLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                "${item.fromExt.uppercase()} to ${item.toExt.uppercase()} · " +
                    if (item.inBytes > 0) "${Files.humanSize(item.inBytes)} to ${Files.humanSize(item.outBytes)}"
                    else Files.humanSize(item.outBytes),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        IconButton(onClick = {
            runCatching { Outputs.open(ctx, item.file) }
                .onFailure { Toast.makeText(ctx, "No app can open this file", Toast.LENGTH_SHORT).show() }
        }) { Icon(Icons.Outlined.OpenInNew, "Open ${item.name}") }
        IconButton(onClick = { runCatching { Outputs.share(ctx, listOf(item.file)) } }) {
            Icon(Icons.Outlined.Share, "Share ${item.name}")
        }
        if (onDelete != null) {
            IconButton(onClick = { onDelete(item) }) { Icon(Icons.Outlined.Delete, "Delete ${item.name}") }
        }
    }
}

@Composable
private fun HistoryScreen(recent: List<RecentItem>, onDelete: (RecentItem) -> Unit, onClear: () -> Unit) {
    if (recent.isEmpty()) {
        EmptyState(Icons.Outlined.History, "No converted files yet", "Files that you convert show here, so you can open or share them again.")
        return
    }
    Column {
        val total = recent.sumOf { it.outBytes }
        SectionHeader("${recent.size} files · ${Files.humanSize(total)}", "Delete all", onClear)
        recent.forEach { RecentRow(it, onDelete) }
        Hint("The app keeps the last 30 files. Save a file to keep it longer.")
    }
}

@Composable
private fun EmptyState(icon: ImageVector, title: String, text: String) {
    Column(
        Modifier
            .fillMaxWidth()
            .padding(top = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(icon, null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(48.dp))
        Text(title, style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
        Text(
            text,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
    }
}

@Composable
private fun SettingsTab(presets: List<Preset>, onDeletePreset: (Long) -> Unit) {
    val version = remember { runCatching { coreVersion() }.getOrElse { "engine not loaded: ${it.message}" } }
    Column {
        SectionTitle("My presets")
        if (presets.isEmpty()) Hint("No presets.")
        presets.forEach { p ->
            Row(Modifier.heightIn(min = 56.dp), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(p.name, style = MaterialTheme.typography.bodyLarge)
                    Hint(
                        listOfNotNull(
                            p.target.uppercase(),
                            if (p.target in LOSSY && p.kind != FileKind.DOCUMENT) "quality ${p.quality}%" else null,
                            if (p.maxDimension > 0) "largest side ${p.maxDimension}" else null,
                        ).joinToString(" · "),
                    )
                }
                IconButton(onClick = { onDeletePreset(p.id) }) { Icon(Icons.Outlined.Delete, "Delete preset ${p.name}") }
            }
        }
    }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SectionTitle("About")
        Text("All conversions run on your phone. The app does not use the internet.", style = MaterialTheme.typography.bodyMedium)
        Text("Engine: Rust, with FFmpeg (LGPL), LAME, Opus and libvpx.", style = MaterialTheme.typography.bodyMedium)
        Text(
            "Documents keep text, headings, bold, italic, lists, links and tables. Pictures in documents are not kept.",
            style = MaterialTheme.typography.bodyMedium,
        )
        Hint("App ${BuildConfig.VERSION_NAME} · $version")
    }
}

// ------------------------------------------------------------------ settings

private fun iconFor(kind: FileKind): ImageVector = when (kind) {
    FileKind.IMAGE -> Icons.Outlined.Image
    FileKind.AUDIO -> Icons.Outlined.AudioFile
    FileKind.VIDEO -> Icons.Outlined.Movie
    FileKind.PDF -> Icons.Outlined.PictureAsPdf
    FileKind.DOCUMENT -> Icons.Outlined.Description
    FileKind.UNKNOWN -> Icons.AutoMirrored.Outlined.InsertDriveFile
}

/** Simple quality choices. "More options" has the exact value. */
private val QUALITY_LEVELS = listOf(60 to "Small file", 85 to "Balanced", 95 to "Best")

/** Number of formats shown before "More formats". */
private const val SUGGESTED = 4

@Composable
private fun SettingsScreen(state: UiState, vm: ConverterViewModel, onAdd: () -> Unit) {
    var showAll by rememberSaveable { mutableStateOf(false) }
    var showMore by rememberSaveable { mutableStateOf(false) }

    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        val total = state.inputs.sumOf { it.size.coerceAtLeast(0) }
        SectionTitle("${kindName(state.kind, state.inputs.size)} · ${Files.humanSize(total)}")
        FileStrip(state.inputs, state.kind, onAdd, vm::removeInput)
    }

    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SectionTitle("Convert to")
        val target = state.target
        val all = state.targets
        val compact = !showAll && all.size > SUGGESTED + 1
        if (compact) {
            val shown = all.take(SUGGESTED).let { if (target != null && target !in it) it + target else it }
            TargetChips(shown, target, vm::selectTarget, null, onMore = { showAll = true })
        } else if (state.kind == FileKind.VIDEO) {
            TargetChips(all.filter { it !in AUDIO_ONLY }, target, vm::selectTarget, "Video", null)
            TargetChips(all.filter { it in AUDIO_ONLY }, target, vm::selectTarget, "Sound only", null)
        } else {
            TargetChips(all, target, vm::selectTarget, null, null)
        }
        if (target == "pdf" && state.inputs.size > 1 && state.kind == FileKind.IMAGE) {
            Hint("All ${state.inputs.size} images go into one PDF, one image on each page.")
        }
    }

    val target = state.target ?: return
    // Documents have no quality or size settings.
    val isDoc = state.kind == FileKind.DOCUMENT
    val showQuality = target in LOSSY && !isDoc
    val showSize = target !in AUDIO_ONLY && !isDoc

    if (showQuality) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SectionTitle("Quality")
            QualitySelector(state.quality, vm::setQuality)
        }
    }

    if (showQuality || showSize) {
        Card(
            onClick = { showMore = !showMore },
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Row(Modifier.padding(start = 16.dp, top = 12.dp, bottom = 12.dp, end = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("More options", style = MaterialTheme.typography.bodyLarge)
                    Text(
                        listOfNotNull(if (showQuality) "Exact quality" else null, if (showSize) "Size" else null).joinToString(", "),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Icon(
                    if (showMore) Icons.Outlined.ExpandLess else Icons.Outlined.ExpandMore,
                    contentDescription = if (showMore) "Hide options" else "Show options",
                )
            }
            AnimatedVisibility(
                visible = showMore,
                enter = expandVertically() + fadeIn(),
                exit = shrinkVertically() + fadeOut(),
            ) {
                Column(
                    Modifier.padding(start = 16.dp, end = 16.dp, bottom = 16.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    if (showQuality) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text("Quality", style = MaterialTheme.typography.titleSmall, modifier = Modifier.weight(1f))
                            Text("${state.quality}%", style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
                        }
                        Slider(
                            value = state.quality.toFloat(),
                            onValueChange = { vm.setQuality(it.roundToInt()) },
                            valueRange = 10f..100f,
                        )
                        Hint("Lower quality gives a smaller file.")
                    }
                    if (showSize) {
                        Text("Largest side (pixels)", style = MaterialTheme.typography.titleSmall)
                        SizeSelector(state.maxDimension, vm::setMaxDimension)
                    }
                }
            }
        }
    }
}

@Composable
private fun Hint(text: String) {
    Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

@Composable
private fun FileStrip(inputs: List<InputFile>, kind: FileKind, onAdd: () -> Unit, onRemove: (Int) -> Unit) {
    LazyRow(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        itemsIndexed(inputs) { i, f ->
            Column(Modifier.width(104.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Surface(
                    shape = RoundedCornerShape(12.dp),
                    color = MaterialTheme.colorScheme.secondaryContainer,
                    contentColor = MaterialTheme.colorScheme.onSecondaryContainer,
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(84.dp),
                ) {
                    Box {
                        Icon(iconFor(kind), null, modifier = Modifier.align(Alignment.Center).size(28.dp))
                        Text(
                            f.extension.uppercase(),
                            style = MaterialTheme.typography.labelSmall,
                            fontWeight = FontWeight.SemiBold,
                            modifier = Modifier
                                .align(Alignment.BottomStart)
                                .padding(8.dp),
                        )
                        if (inputs.size > 1) {
                            IconButton(onClick = { onRemove(i) }, modifier = Modifier.align(Alignment.TopEnd)) {
                                Icon(Icons.Outlined.Close, "Remove ${f.name}", Modifier.size(18.dp))
                            }
                        }
                    }
                }
                Text(f.name, style = MaterialTheme.typography.bodySmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(
                    Files.humanSize(f.size),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        item {
            OutlinedButton(
                onClick = onAdd,
                shape = RoundedCornerShape(12.dp),
                contentPadding = PaddingValues(4.dp),
                modifier = Modifier
                    .width(88.dp)
                    .height(84.dp),
            ) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Icon(Icons.Outlined.Add, null)
                    Text("Add files", style = MaterialTheme.typography.labelMedium)
                }
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun TargetChips(
    targets: List<String>,
    selected: String?,
    onSelect: (String) -> Unit,
    label: String?,
    onMore: (() -> Unit)?,
) {
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
                    leadingIcon = if (t == selected) {
                        { Icon(Icons.Outlined.Check, null, Modifier.size(FilterChipDefaults.IconSize)) }
                    } else null,
                )
            }
            if (onMore != null) {
                TextButton(onClick = onMore) { Text("More formats") }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun QualitySelector(value: Int, onChange: (Int) -> Unit) {
    // The level closest to the current value is selected.
    val closest = QUALITY_LEVELS.minByOrNull { abs(it.first - value) }?.first
    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
        QUALITY_LEVELS.forEachIndexed { i, (q, label) ->
            SegmentedButton(
                selected = q == closest,
                onClick = { onChange(q) },
                shape = SegmentedButtonDefaults.itemShape(i, QUALITY_LEVELS.size),
            ) { Text(label, maxLines = 1) }
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

@Composable
private fun ConvertBar(state: UiState, estimate: Long?, onConvert: () -> Unit) {
    val target = state.target ?: return
    val now = state.inputs.sumOf { it.size.coerceAtLeast(0) }
    Surface(color = MaterialTheme.colorScheme.surface) {
        Column {
            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
            Column(
                Modifier
                    .navigationBarsPadding()
                    .padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                if (estimate != null && estimate > 0) {
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            "Size after",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.weight(1f),
                        )
                        Text(
                            "about ${Files.humanSize(estimate)}" + if (now > 0) " (now ${Files.humanSize(now)})" else "",
                            style = MaterialTheme.typography.bodyMedium,
                            fontWeight = FontWeight.Medium,
                        )
                    }
                }
                Button(
                    onClick = onConvert,
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(56.dp),
                ) {
                    val n = state.inputs.size
                    Text(
                        if (n == 1) "Convert to ${target.uppercase()}" else "Convert $n files to ${target.uppercase()}",
                        style = MaterialTheme.typography.titleMedium,
                    )
                }
                Row(
                    horizontalArrangement = Arrangement.Center,
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Icon(
                        Icons.Outlined.Lock,
                        null,
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.size(14.dp),
                    )
                    Spacer(Modifier.width(6.dp))
                    Hint("Files stay on your phone. Nothing is uploaded.")
                }
            }
        }
    }
}


// ------------------------------------------------------------------ working

@Composable
private fun ColumnScope.WorkingScreen(state: UiState, onCancel: () -> Unit) {
    val w = state.status as? Status.Working ?: return
    val n = state.inputs.size
    // Many images into one PDF is one step, not one step for each file.
    val oneStep = state.target == "pdf" && n > 1 && state.kind == FileKind.IMAGE
    val perFile = n > 1 && !oneStep
    val done = if (perFile) (w.progress * n).toInt().coerceIn(0, n) else 0
    val percent = (w.progress * 100).roundToInt().coerceIn(0, 100)

    // Time left: from the time used so far and the progress.
    val started = remember { mutableLongStateOf(System.currentTimeMillis()) }
    val elapsed = System.currentTimeMillis() - started.longValue
    val left = if (w.progress > 0.05f && elapsed > 2000) ((elapsed / w.progress) - elapsed).toLong() else -1L

    Spacer(Modifier.height(8.dp))
    Box(
        contentAlignment = Alignment.Center,
        modifier = Modifier
            .align(Alignment.CenterHorizontally)
            .size(176.dp),
    ) {
        CircularProgressIndicator(
            progress = { w.progress },
            strokeWidth = 12.dp,
            trackColor = MaterialTheme.colorScheme.secondaryContainer,
            modifier = Modifier.fillMaxSize(),
        )
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            Text("$percent%", style = MaterialTheme.typography.displaySmall)
            Text(
                if (left >= 0) "about ${timeText(left)} left" else "to ${state.target?.uppercase()}",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
    Text(
        if (perFile) "$done of $n files done" else "Converting ${kindName(state.kind, n)} to ${state.target?.uppercase()}",
        style = MaterialTheme.typography.titleMedium,
        textAlign = TextAlign.Center,
        modifier = Modifier.fillMaxWidth(),
    )

    if (perFile) {
        Column {
            state.inputs.forEachIndexed { i, f ->
                FileProgressRow(
                    name = f.name,
                    state = when {
                        i < done -> RowState.Done
                        i == done -> RowState.Working
                        else -> RowState.Waiting
                    },
                    progress = (w.progress * n - i).coerceIn(0f, 1f),
                )
            }
        }
    }

    Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer)) {
        Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Outlined.Notifications, null, tint = MaterialTheme.colorScheme.primary)
            Spacer(Modifier.width(12.dp))
            Text(
                "You can leave the app. Progress shows in a notification, and we tell you when it is done.",
                style = MaterialTheme.typography.bodyMedium,
            )
        }
    }

    OutlinedButton(
        onClick = onCancel,
        modifier = Modifier
            .fillMaxWidth()
            .height(56.dp),
    ) { Text("Stop", style = MaterialTheme.typography.titleMedium) }
}

private fun timeText(ms: Long): String {
    val s = (ms / 1000).coerceAtLeast(1)
    return when {
        s < 60 -> "$s s"
        s < 3600 -> "${(s + 30) / 60} min"
        else -> "${s / 3600} h ${(s % 3600) / 60} min"
    }
}

private enum class RowState { Done, Working, Waiting }

@Composable
private fun FileProgressRow(name: String, state: RowState, progress: Float) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                name,
                style = MaterialTheme.typography.bodyLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                color = if (state == RowState.Waiting) MaterialTheme.colorScheme.onSurfaceVariant
                else MaterialTheme.colorScheme.onSurface,
            )
            if (state == RowState.Working) {
                Spacer(Modifier.height(6.dp))
                LinearProgressIndicator(progress = { progress }, modifier = Modifier.fillMaxWidth())
            }
        }
        Spacer(Modifier.width(12.dp))
        when (state) {
            RowState.Done -> Icon(Icons.Outlined.CheckCircle, "Done", tint = MaterialTheme.colorScheme.primary)
            RowState.Working -> Text("${(progress * 100).roundToInt()}%", style = MaterialTheme.typography.labelLarge)
            RowState.Waiting -> Icon(Icons.Outlined.RadioButtonUnchecked, "Waiting", tint = MaterialTheme.colorScheme.outline)
        }
    }
}

// ------------------------------------------------------------------ done

@Composable
private fun ColumnScope.DoneScreen(
    state: UiState,
    outputs: List<File>,
    failures: List<FailedFile>,
    onAgain: () -> Unit,
    onNew: () -> Unit,
    onRetryAs: (String) -> Unit,
    onSavePreset: () -> Unit,
    presetExists: Boolean,
) {
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

    val failedNames = failures.map { it.input.name }.toSet()
    val before = state.inputs.filter { it.name !in failedNames }.sumOf { it.size.coerceAtLeast(0) }
    val after = remember(outputs) { outputs.sumOf { it.length() } }

    if (failures.isNotEmpty()) {
        FailuresCard(state, failures, onRetryAs, onAgain)
    }

    if (outputs.isNotEmpty()) {
        if (failures.isEmpty()) {
            Spacer(Modifier.height(8.dp))
            Surface(
                shape = RoundedCornerShape(28.dp),
                color = MaterialTheme.colorScheme.primaryContainer,
                contentColor = MaterialTheme.colorScheme.onPrimaryContainer,
                modifier = Modifier
                    .align(Alignment.CenterHorizontally)
                    .size(80.dp),
            ) {
                Box(contentAlignment = Alignment.Center) { Icon(Icons.Outlined.Check, null, Modifier.size(40.dp)) }
            }
            Column(
                Modifier.fillMaxWidth(),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Text(
                    if (outputs.size == 1) "File converted" else "${outputs.size} files converted",
                    style = MaterialTheme.typography.headlineSmall,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.semantics { heading() },
                )
                if (before > 0 && after > 0) {
                    Text(
                        "${Files.humanSize(before)} to ${Files.humanSize(after)}. ${sizeChange(before, after)}",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        textAlign = TextAlign.Center,
                    )
                }
            }
        }

        Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
            Column(Modifier.padding(start = 16.dp, end = 4.dp, top = 4.dp, bottom = 4.dp)) {
                outputs.take(8).forEach { f ->
                    Row(Modifier.heightIn(min = 56.dp), verticalAlignment = Alignment.CenterVertically) {
                        Icon(Icons.Outlined.CheckCircle, "Done", tint = MaterialTheme.colorScheme.primary)
                        Spacer(Modifier.width(12.dp))
                        Column(Modifier.weight(1f)) {
                            Text(f.name, style = MaterialTheme.typography.bodyLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text(
                                Files.humanSize(f.length()),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        IconButton(onClick = {
                            runCatching { Outputs.open(ctx, f) }.onFailure { toast("No app can open this file") }
                        }) { Icon(Icons.Outlined.OpenInNew, "Open ${f.name}") }
                    }
                }
                if (outputs.size > 8) {
                    Text(
                        "and ${outputs.size - 8} more",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(vertical = 8.dp),
                    )
                }
            }
        }

        if (!presetExists && state.target != null) {
            Card(
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
                border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
            ) {
                Row(
                    Modifier.padding(start = 16.dp, end = 8.dp, top = 8.dp, bottom = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text("Save as a preset?", style = MaterialTheme.typography.titleSmall)
                        Hint("Use these settings again with one tap.")
                    }
                    TextButton(onClick = onSavePreset) { Text("Save") }
                }
            }
        }

        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
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
                Text(
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) "Save to Downloads" else "Save",
                    style = MaterialTheme.typography.titleMedium,
                )
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                FilledTonalButton(
                    onClick = { Outputs.share(ctx, outputs) },
                    modifier = Modifier
                        .weight(1f)
                        .height(48.dp),
                ) {
                    Icon(Icons.Outlined.Share, null)
                    Spacer(Modifier.width(8.dp))
                    Text(if (outputs.size == 1) "Share" else "Share all")
                }
                FilledTonalButton(
                    onClick = { if (outputs.size == 1) saveOne.launch(outputs.first().name) else saveMany.launch(null) },
                    modifier = Modifier
                        .weight(1f)
                        .height(48.dp),
                ) { Text("Save as…") }
            }
        }
    }

    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
        TextButton(onClick = onAgain, modifier = Modifier.weight(1f)) { Text("Other format") }
        TextButton(onClick = onNew, modifier = Modifier.weight(1f)) { Text("Convert more files") }
    }
}

/** Some files did not convert: say why, and give a fix. */
@Composable
private fun FailuresCard(state: UiState, failures: List<FailedFile>, onRetryAs: (String) -> Unit, onAgain: () -> Unit) {
    // For a video that fails, the sound often still converts.
    val soundOnly = if (state.kind == FileKind.VIDEO && "mp3" in state.targets && state.target != "mp3") "mp3" else null
    Card(
        shape = RoundedCornerShape(20.dp),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.errorContainer,
            contentColor = MaterialTheme.colorScheme.onErrorContainer,
        ),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(Modifier.padding(16.dp)) {
            Icon(Icons.Outlined.ErrorOutline, null, tint = MaterialTheme.colorScheme.error)
            Spacer(Modifier.width(12.dp))
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(
                    if (failures.size == 1) "${failures.first().input.name} did not convert"
                    else "${failures.size} files did not convert",
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.semantics { heading() },
                )
                failures.take(5).forEach {
                    Text(
                        if (failures.size == 1) it.message else "${it.input.name}: ${it.message}",
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (soundOnly != null) {
                        Button(
                            onClick = { onRetryAs(soundOnly) },
                            colors = ButtonDefaults.buttonColors(
                                containerColor = MaterialTheme.colorScheme.error,
                                contentColor = MaterialTheme.colorScheme.onError,
                            ),
                        ) { Text("Get sound only (MP3)") }
                    } else {
                        TextButton(onClick = onAgain) { Text("Try other settings") }
                    }
                }
            }
        }
    }
}

private fun sizeChange(before: Long, after: Long): String {
    val pct = ((before - after) * 100.0 / before).roundToInt()
    return when {
        pct > 0 -> "That is $pct% smaller."
        pct < 0 -> "That is ${-pct}% bigger."
        else -> "The size is the same."
    }
}

// ------------------------------------------------------------------ failed

@Composable
private fun FailedScreen(message: String, canRetry: Boolean, onRetry: () -> Unit, onNew: () -> Unit) {
    Card(
        shape = RoundedCornerShape(20.dp),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.errorContainer,
            contentColor = MaterialTheme.colorScheme.onErrorContainer,
        ),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(Modifier.padding(16.dp)) {
            Icon(Icons.Outlined.ErrorOutline, null, tint = MaterialTheme.colorScheme.error)
            Spacer(Modifier.width(12.dp))
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(
                    "The file did not convert",
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.semantics { heading() },
                )
                Text(message, style = MaterialTheme.typography.bodyMedium)
            }
        }
    }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (canRetry) {
            Button(
                onClick = onRetry,
                modifier = Modifier
                    .fillMaxWidth()
                    .height(56.dp),
            ) { Text("Try other settings", style = MaterialTheme.typography.titleMedium) }
        }
        OutlinedButton(
            onClick = onNew,
            border = BorderStroke(1.dp, MaterialTheme.colorScheme.outline),
            modifier = Modifier
                .fillMaxWidth()
                .height(48.dp),
        ) { Text("Choose other files") }
    }
}
