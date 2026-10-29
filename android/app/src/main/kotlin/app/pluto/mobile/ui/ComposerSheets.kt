package app.pluto.mobile.ui

// The composer's sheets (the Dioxus app's `sheets.rs`): the model (effort,
// Code's mode and access, the models by provider), a thread's actions,
// attach, the draft's project and workspace, the ref picker ("Search
// refs...", `vcs.listRefs`), renaming and deleting a thread. The core
// (`sheets.rs`) decides; these draw.

import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import app.pluto.core.Badge
import app.pluto.core.RefSwitch
import app.pluto.mobile.LocalModel
import kotlinx.coroutines.launch
import app.pluto.mobile.Page
import app.pluto.mobile.Sheet
import kotlinx.coroutines.delay

@Composable
fun ModelSheet() {
    val model = LocalModel.current
    val core = model.core
    val view = remember(model.revision) { core.modelSheet() }
    view.effort?.let { effort ->
        SheetLabel("Effort")
        Choices(effort.options.map { it.value to it.label }, effort.chosen) { core.pickEffort(effort.id, it) }
    }
    if (!view.chat) {
        SheetLabel("Mode")
        Seg(listOf("build" to "Build", "plan" to "Plan"), if (view.plan) "plan" else "build") { core.pickPlan(it == "plan") }
        SheetLabel("Access")
        Choices(view.accesses.map { it.value to it.label }, view.access) { core.pickAccess(it) }
        // `.seg-note`: what the access lets the agent do.
        T(
            view.accessNote,
            13f,
            P.muted,
            line = 1.45f,
            modifier = Modifier.heightIn(min = 20.dp).padding(start = 24.dp, end = 24.dp, top = 4.dp, bottom = 8.dp),
        )
    }
    if (view.providers.isEmpty()) {
        SheetNote("No provider is ready on the server.")
        return
    }
    for (provider in view.providers) {
        SheetLabel(provider.name)
        for (choice in provider.models) {
            Choice(choice.label, choice.checked, mark = provider.mark) {
                core.pickModel(provider.instanceId, choice.slug)
                model.nav.back()
            }
        }
    }
}

@Composable
fun threadMenuTitle(id: String): String {
    val model = LocalModel.current
    return remember(model.revision, id) { model.core.threadTitle(id) }
}

@Composable
fun ThreadMenuSheet(id: String) {
    val model = LocalModel.current
    val core = model.core
    val nav = model.nav
    val clipboard = LocalClipboardManager.current
    val view = remember(model.revision, id) { core.threadMenu(id) }
    var done by remember { mutableStateOf("") }
    val outcome = rememberOutcome {
        if (done == "Deleted") core.leaveDeleted(id)
        nav.back()
        core.toast(done)
    }
    if (view == null) {
        SheetNote("This thread is no longer listed.")
        return
    }
    val command = { kind: String ->
        core.threadCommand(id, kind)
        nav.back()
    }
    SRow("pencil", "Rename") { nav.openSheet(Sheet.Rename(id)) }
    SRow("pin", if (view.pinned) "Unpin" else "Pin") { command(if (view.pinned) "thread.unpin" else "thread.pin") }
    if (!view.chat) {
        if (view.canFork) {
            SRow("git-fork", "Fork thread", enabled = !outcome.busy) {
                done = "Forked"
                outcome.error = null
                outcome.sent = core.forkThread(id)
            }
        }
        SRow("file-diff", "Changed files") {
            // The diff reads the thread's checkpoints: it opens first.
            if (core.shell().selected != id) core.select(id)
            nav.openPage(Page.Diff(id, null, null))
        }
    }
    SRow("copy", "Copy thread ID") {
        clipboard.setText(AnnotatedString(id))
        nav.back()
        core.toast("Thread ID copied")
    }
    SRow("archive", if (view.settled) "Unsettle" else "Settle") {
        command(if (view.settled) "thread.unsettle" else "thread.settle")
    }
    SRow("trash-2", "Delete", danger = true, enabled = !outcome.busy) {
        if (view.confirmDelete) {
            nav.openSheet(Sheet.DeleteThread(id))
        } else {
            done = "Deleted"
            outcome.error = null
            outcome.sent = core.deleteThread(id)
        }
    }
    if (outcome.busy) SheetNote("Waiting for the server…")
    outcome.error?.let { SheetError(it) }
}

@Composable
fun AttachSheet() {
    val model = LocalModel.current
    val core = model.core
    val context = androidx.compose.ui.platform.LocalContext.current
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    // The composer the sheet acts for: the shown thread, else the draft.
    val composer = remember {
        val shell = core.shell()
        shell.selected ?: if (shell.chat) "chat" else "code"
    }
    val take = { uris: List<android.net.Uri> ->
        if (uris.isNotEmpty()) {
            model.nav.back()
            scope.launch(kotlinx.coroutines.Dispatchers.IO) { attachUris(context, core, composer, uris) }
        }
    }
    val photos = androidx.activity.compose.rememberLauncherForActivityResult(
        androidx.activity.result.contract.ActivityResultContracts.PickMultipleVisualMedia(20),
    ) { take(it) }
    val files = androidx.activity.compose.rememberLauncherForActivityResult(
        androidx.activity.result.contract.ActivityResultContracts.OpenMultipleDocuments(),
    ) { take(it) }
    // The camera writes its photo where the app's file provider points.
    val shot = remember { arrayOfNulls<android.net.Uri>(1) }
    val camera = androidx.activity.compose.rememberLauncherForActivityResult(
        androidx.activity.result.contract.ActivityResultContracts.TakePicture(),
    ) { saved -> shot[0]?.takeIf { saved }?.let { take(listOf(it)) } }
    // `.tiles`: three tiles in a row.
    Row(
        Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 4.dp, bottom = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        for ((icon, label) in listOf("image" to "Photos", "camera" to "Camera", "file" to "Files")) {
            val interaction = remember { MutableInteractionSource() }
            val shape = RoundedCornerShape(20.dp)
            Column(
                Modifier
                    .weight(1f)
                    .pressScale(interaction, .97f)
                    .card(P.overlay, shape)
                    .pressRow(interaction)
                    .tap(interaction = interaction) {
                        when (label) {
                            "Photos" -> photos.launch(
                                androidx.activity.result.PickVisualMediaRequest(
                                    androidx.activity.result.contract.ActivityResultContracts.PickVisualMedia.ImageOnly,
                                ),
                            )
                            "Files" -> files.launch(arrayOf("*/*"))
                            else -> {
                                val dir = java.io.File(context.cacheDir, "camera").apply { mkdirs() }
                                val file = java.io.File(dir, "photo-${System.currentTimeMillis()}.jpg")
                                val uri = androidx.core.content.FileProvider.getUriForFile(
                                    context,
                                    "${context.packageName}.files",
                                    file,
                                )
                                shot[0] = uri
                                runCatching { camera.launch(uri) }.onFailure { core.toast("No camera app is available") }
                            }
                        }
                    }
                    .padding(horizontal = 8.dp, vertical = 18.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Icon(icon, P.accent, size = 26.dp)
                T(label, 14f, lines = 1)
            }
        }
    }
}

/// A checked row with a leading icon or project badge and a detail line.
@Composable
private fun CheckRow(
    label: String,
    checked: Boolean,
    icon: String? = null,
    badge: Badge? = null,
    detail: String? = null,
    onPick: () -> Unit,
) {
    val interaction = remember { MutableInteractionSource() }
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 52.dp)
            .pressRow(interaction)
            .tap(role = Role.RadioButton, interaction = interaction, onClick = onPick)
            .padding(horizontal = 24.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        if (badge != null) ProjectBadge(badge)
        if (icon != null) Icon(icon, P.muted2)
        Column(Modifier.weight(1f)) {
            T(label, 16f, lines = 1)
            if (detail != null) T(detail, 13f, P.muted)
        }
        Icon("check", if (checked) P.accent else Color.Transparent)
    }
}

@Composable
fun ProjectSheet() {
    val model = LocalModel.current
    val core = model.core
    val choices = remember(model.revision) { core.projectChoices() }
    for (choice in choices) {
        CheckRow(choice.badge.title, choice.checked, badge = choice.badge) {
            core.pickProject(choice.badge.projectId)
            model.nav.back()
        }
    }
}

@Composable
fun WorkspaceSheet() {
    val model = LocalModel.current
    val core = model.core
    val worktree = remember(model.revision) { core.draftWorktree() }
    for ((value, row) in listOf(
        false to Triple("Local checkout", "folder", "Work in the project’s own folder"),
        true to Triple("New worktree", "folder-git-2", "A fresh branch in its own folder"),
    )) {
        CheckRow(row.first, worktree == value, icon = row.second, detail = row.third) {
            core.pickWorktree(value)
            model.nav.back()
        }
    }
}

/// The ref picker: the draft's base for a new worktree, or the thread's
/// branch (`vcs.switchRef`, or `vcs.createRef` for a new one; the thread
/// records it once the checkout succeeded).
@Composable
fun BranchSheet() {
    val model = LocalModel.current
    val core = model.core
    val nav = model.nav
    var query by remember { mutableStateOf("") }
    // The query the list shows, a moment after the typing stops.
    var searched by remember { mutableStateOf("") }
    var switch by remember { mutableStateOf<RefSwitch?>(null) }
    var failure by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(query) {
        delay(220)
        searched = query.trim()
    }
    // The picker's searches leave with it.
    DisposableEffect(Unit) { onDispose { core.forget("refs:") } }
    // The switch's answer: the thread records the branch, or the sheet says
    // why the checkout failed.
    LaunchedEffect(model.revision, switch) {
        val pending = switch ?: return@LaunchedEffect
        if (!core.replyState(pending.key).done) return@LaunchedEffect
        switch = null
        val error = core.finishRefSwitch(pending)
        if (error == null) nav.back() else failure = error
    }
    val refs = remember(model.revision, searched) { core.refs(searched) }
    val busy = switch != null
    val pick = { name: String, create: Boolean ->
        failure = null
        val picked = core.pickRef(name, create)
        if (picked.done) nav.back() else switch = picked.switch
    }
    SField(query, { query = it }, "Search refs...", icon = "search")
    if (busy) SheetNote("Switching…")
    failure?.let { SheetError(it) }
    val list = refs.refs
    when {
        refs.error != null -> SheetNote(refs.error!!)
        list == null -> SheetNote("Loading refs...")
        else -> {
            if (list.isEmpty() && refs.create == null) SheetNote("No refs found.")
            for (ref in list) {
                SRow(
                    "git-branch",
                    ref.name,
                    enabled = !busy,
                    trailing = { ref.tag?.let { Tag(it) } },
                ) { pick(ref.name, false) }
            }
            refs.create?.let { name ->
                SRow("plus", "Create new ref “$name”", enabled = !busy) { pick(name, true) }
            }
        }
    }
}

/// `.tag`: a ref's state beside its name.
@Composable
private fun Tag(text: String) {
    Box(
        Modifier
            .card(P.overlay, RoundedCornerShape(10.dp), Color.Transparent)
            .padding(horizontal = 8.dp, vertical = 2.dp),
    ) { T(text, 12f, P.muted2, lines = 1, line = 1.4f) }
}

@Composable
fun RenameSheet(id: String) {
    val model = LocalModel.current
    val core = model.core
    val nav = model.nav
    var title by remember(id) { mutableStateOf(core.threadTitle(id)) }
    val outcome = rememberOutcome { nav.back() }
    SField(title, { title = it }, "Title")
    outcome.error?.let { SheetError(it) }
    SheetActions {
        Btn("Cancel", Modifier.weight(1f)) { nav.back() }
        Btn(
            if (outcome.busy) "Saving…" else "Save",
            Modifier.weight(1f),
            tone = Tone.Primary,
            enabled = !outcome.busy && title.isNotBlank(),
        ) {
            outcome.error = null
            outcome.sent = core.renameThread(id, title)
        }
    }
}

@Composable
fun DeleteThreadSheet(id: String) {
    val model = LocalModel.current
    val core = model.core
    val nav = model.nav
    val outcome = rememberOutcome {
        core.leaveDeleted(id)
        nav.back()
        core.toast("Deleted")
    }
    SheetConfirm("This deletes the thread and its chat history.")
    outcome.error?.let { SheetError(it) }
    SheetActions {
        Btn("Cancel", Modifier.weight(1f)) { nav.back() }
        Btn(
            if (outcome.busy) "Deleting…" else "Delete",
            Modifier.weight(1f),
            tone = Tone.Danger,
            enabled = !outcome.busy,
        ) {
            outcome.error = null
            outcome.sent = core.deleteThread(id)
        }
    }
}

