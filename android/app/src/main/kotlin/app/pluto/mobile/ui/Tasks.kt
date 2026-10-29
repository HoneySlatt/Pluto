package app.pluto.mobile.ui

// Scheduled tasks (`#tasks`, after the desktop's `scheduled_tasks.rs`): the
// live list, one card per task with its schedule line, prompt, last error,
// project and model, its pause switch and its Run now, Edit and Delete
// actions; "+" and Edit open the editor, saved with the desktop's checks
// and messages. The time of day is picked with the phone's own clock.

import android.app.TimePickerDialog
import android.text.format.DateFormat
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pluto.core.TaskCard
import app.pluto.core.TaskMode
import app.pluto.core.TasksState
import app.pluto.mobile.LocalModel
import app.pluto.mobile.Page
import app.pluto.mobile.Sheet

// `WEEKDAY_LABELS`: 0 is Sunday; the chips start on Monday.
private val WEEKDAYS = listOf("Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat")
private val WEEK = listOf(1, 2, 3, 4, 5, 6, 0)

@Composable
fun TasksPage() {
    val model = LocalModel.current
    val core = model.core
    val nav = model.nav
    LaunchedEffect(Unit) { core.watchTasks() }
    val state = remember(model.revision) { core.tasks() }
    PageFrame(
        "Scheduled tasks",
        action = {
            DiscIcon("plus", "New task") {
                core.taskNew()
                nav.openPage(Page.TaskEdit(null))
            }
        },
    ) {
        when (state) {
            TasksState.Loading -> LoadingNote()
            is TasksState.Failed -> EmptyPage("Couldn’t load the tasks", state.error)
            is TasksState.Listed -> if (state.tasks.isEmpty()) {
                EmptyPage("No scheduled tasks", "Tap + to run a prompt on a schedule.")
            } else {
                for (task in state.tasks) {
                    androidx.compose.runtime.key(task.id) { TaskCardView(task) }
                }
            }
        }
    }
}

// `.task`: one task's card; a paused task fades.
@Composable
private fun TaskCardView(task: TaskCard) {
    val model = LocalModel.current
    val core = model.core
    val nav = model.nav
    val shape = RoundedCornerShape(24.dp)
    Column(
        Modifier
            .padding(bottom = 12.dp)
            .fillMaxWidth()
            .card(P.surface, shape)
            .padding(start = 18.dp, end = 12.dp, top = 16.dp, bottom = 8.dp),
    ) {
        val fade = if (task.enabled) 1f else .62f
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Column(Modifier.weight(1f).alpha(fade)) {
                T(task.title, 16f, weight = FontWeight.Medium)
                val line = buildAnnotatedString {
                    append(task.`when`)
                    task.last?.let { last ->
                        append(" · ")
                        withStyle(SpanStyle(color = if (last == "failed") P.error else P.success)) { append(last) }
                    }
                }
                androidx.compose.foundation.text.BasicText(
                    line,
                    Modifier.padding(top = 2.dp),
                    style = style(13f, color = P.muted),
                )
            }
            Toggle(task.enabled, "Enabled") { core.setTaskEnabled(task.id, it) }
        }
        Column(Modifier.alpha(fade)) {
            T(task.prompt, 14f, P.muted2, lines = 2, modifier = Modifier.padding(top = 10.dp))
            task.error?.let { error ->
                Row(Modifier.padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Icon("circle-alert", P.error, Modifier.padding(top = 2.dp), size = 16.dp)
                    T(error, 12.5f, P.error, mono = true)
                }
            }
            Row(
                Modifier.padding(top = 8.dp).fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                task.project?.let { project ->
                    ProjectBadge(project, BadgeMeta)
                    T(project.title, 13f, P.muted, lines = 1, modifier = Modifier.weight(1f, fill = false))
                    T("·", 13f, P.muted.copy(alpha = .5f))
                }
                task.mark?.let { Mark(it, size = 14.dp) }
                T(task.modelLabel, 13f, P.muted, lines = 1, modifier = Modifier.weight(1f, fill = false))
                Grow()
                Box(
                    Modifier.size(44.dp).clip(CircleShape).tap(label = "Actions") { nav.openSheet(Sheet.TaskMenu(task.id)) },
                    contentAlignment = Alignment.Center,
                ) { Icon("ellipsis-vertical", P.muted) }
            }
        }
    }
}

@Composable
fun TaskMenuSheet(id: String) {
    val model = LocalModel.current
    val core = model.core
    val nav = model.nav
    // Run now and Delete wait for the server's answer.
    var done by remember { mutableStateOf("") }
    val outcome = rememberOutcome {
        nav.back()
        if (done.isNotEmpty()) core.toast(done)
    }
    val view = remember(model.revision, id) { core.taskMenu(id) } ?: return
    if (!view.webhook) {
        SRow("play", "Run now", enabled = !outcome.busy) {
            done = "Running ${view.title}"
            outcome.once { core.taskAction(id, true) }
        }
    }
    SRow("pencil", "Edit", enabled = !outcome.busy) {
        if (core.taskEdit(id)) nav.openPage(Page.TaskEdit(id))
    }
    SRow("trash-2", "Delete", danger = true, enabled = !outcome.busy) {
        done = "Task deleted"
        outcome.once { core.taskAction(id, false) }
    }
    if (outcome.busy) SheetNote("Waiting for the server…")
    outcome.error?.let { SheetError(it) }
}

@Composable
fun taskMenuTitle(id: String): String {
    val model = LocalModel.current
    return remember(model.revision, id) { model.core.taskMenu(id)?.title }.orEmpty()
}

@Composable
fun taskFieldTitle(field: String): String = when (field) {
    "project" -> "Project"
    "workspace" -> "Workspace"
    else -> "Model"
}

// A task editor field's choices.
@Composable
fun TaskFieldSheet(field: String) {
    val model = LocalModel.current
    val core = model.core
    val nav = model.nav
    val draft = remember(model.revision) { core.taskDraft() } ?: return
    when (field) {
        "project" -> {
            val projects = remember(model.revision) { core.codeProjects() }
            for (project in projects) {
                val interaction = remember { MutableInteractionSource() }
                Row(
                    Modifier
                        .fillMaxWidth()
                        .heightIn(min = 52.dp)
                        .pressRow(interaction)
                        .tap(role = Role.RadioButton, interaction = interaction) {
                            core.taskSetProject(project.projectId)
                            nav.back()
                        }
                        .padding(horizontal = 24.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(14.dp),
                ) {
                    ProjectBadge(project)
                    T(project.title, 16f, modifier = Modifier.weight(1f))
                    Icon("check", if (draft.projectId == project.projectId) P.accent else Color.Transparent)
                }
            }
        }
        "workspace" -> {
            for ((value, label) in listOf("worktree" to "Create a new worktree", "root" to "Use the project checkout")) {
                Choice(label, draft.workspaceValue == value) {
                    core.taskSetWorkspace(value)
                    nav.back()
                }
            }
        }
        else -> {
            val groups = remember(model.revision) { core.taskModels() }
            DefaultModels(groups) { instance, slug ->
                core.taskSetModel(instance, slug)
                nav.back()
            }
        }
    }
}

// A field of the editor (`.inbox`, `.inbox.area` for several lines).
@Composable
private fun FormField(
    value: String,
    onValue: (String) -> Unit,
    placeholder: String = "",
    area: Boolean = false,
    number: Boolean = false,
    center: Boolean = false,
    width: Dp? = null,
) {
    var focused by remember { mutableStateOf(false) }
    val border by animateColorAsState(if (focused) P.accent else P.input, tween(150), label = "field")
    val shape = RoundedCornerShape(if (area) 22.dp else 28.dp)
    val size = if (area) 16f else 17f
    Box(
        Modifier
            .then(if (width != null) Modifier.width(width) else Modifier.fillMaxWidth())
            .heightIn(min = if (area) 0.dp else if (width != null) 48.dp else 56.dp)
            .card(P.surface, shape, border)
            .padding(horizontal = if (area) 18.dp else if (width != null) 14.dp else 20.dp, vertical = if (area) 14.dp else 0.dp),
        contentAlignment = if (area) Alignment.TopStart else Alignment.CenterStart,
    ) {
        BasicTextField(
            value,
            onValue,
            Modifier
                .fillMaxWidth()
                .then(if (area) Modifier.heightIn(min = 96.dp) else Modifier)
                .onFocus { focused = it },
            singleLine = !area,
            textStyle = style(size).let { if (center) it.copy(textAlign = TextAlign.Center) else it },
            cursorBrush = SolidColor(P.accent),
            keyboardOptions = if (number) {
                KeyboardOptions(keyboardType = KeyboardType.Number)
            } else {
                KeyboardOptions(capitalization = KeyboardCapitalization.Sentences)
            },
            decorationBox = { field ->
                if (value.isEmpty() && placeholder.isNotEmpty()) T(placeholder, size, P.muted, lines = if (area) Int.MAX_VALUE else 1)
                field()
            },
        )
    }
}

// `.inline`: a sentence around a small field.
@Composable
private fun Inline(before: String, after: String?, field: @Composable () -> Unit) {
    Row(
        Modifier.padding(start = 8.dp, end = 8.dp, top = 4.dp, bottom = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        T(before, 16f, P.muted2)
        field()
        if (after != null) T(after, 16f, P.muted2)
    }
}

// `.quick` and `.days` buttons: a pill, lit when pressed on.
@Composable
private fun DayPill(label: String, on: Boolean, modifier: Modifier, height: Dp = 44.dp, size: Float = 14f, onClick: () -> Unit) {
    val fill by animateColorAsState(if (on) P.accent.copy(alpha = .16f) else P.chip, tween(150), label = "day")
    val border by animateColorAsState(if (on) P.accent.copy(alpha = .5f) else P.border, tween(150), label = "day")
    val color by animateColorAsState(if (on) P.accentHover else if (height < 44.dp) P.muted2 else P.muted, tween(150), label = "day")
    val interaction = remember { MutableInteractionSource() }
    Box(
        modifier
            .height(height)
            .pressScale(interaction, .96f)
            .card(fill, RoundedCornerShape(height / 2), border)
            .tap(interaction = interaction, onClick = onClick)
            .padding(horizontal = 14.dp),
        contentAlignment = Alignment.Center,
    ) { T(label, size, color, if (height < 44.dp) FontWeight.Normal else FontWeight.Medium, lines = 1, line = 1.2f) }
}

@Composable
fun TaskEditPage(id: String?) {
    val model = LocalModel.current
    val core = model.core
    val nav = model.nav
    val draft = remember(model.revision) { core.taskDraft() }
    // Saved, the editor closes; a failure shows on the draft.
    LaunchedEffect(model.revision) {
        if (core.taskSaved()) nav.back()
    }
    if (draft == null) {
        PageFrame("Task") {}
        return
    }
    // The fields type into local state (fast typing outran a round trip
    // through the core) and hand every change over.
    var title by remember(id) { mutableStateOf(draft.title) }
    var prompt by remember(id) { mutableStateOf(draft.prompt) }
    var interval by remember(id) { mutableStateOf(draft.interval) }
    var maxAge by remember(id) { mutableStateOf(draft.maxAge) }
    val context = LocalContext.current
    PageFrame(if (draft.editing) "Edit task" else "New task") {
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Column(Modifier.padding(bottom = 14.dp)) {
                FieldLabel("Name")
                FormField(title, { title = it; core.taskSetText("title", it) }, "Morning review")
            }
            Column(Modifier.padding(bottom = 14.dp)) {
                FieldLabel("Prompt")
                FormField(prompt, { prompt = it; core.taskSetText("prompt", it) }, "What the agent should do on each run", area = true)
            }
            Box(Modifier.padding(top = 6.dp, bottom = 4.dp)) {
                Group {
                    SetRow("folder-closed", "Project", draft.project) { nav.openSheet(Sheet.TaskField("project")) }
                    SetRow("folder-git-2", "Workspace", draft.workspace) { nav.openSheet(Sheet.TaskField("workspace")) }
                    SetRow("bot", "Model", draft.model, mark = draft.mark) { nav.openSheet(Sheet.TaskField("model")) }
                }
            }
            GroupLabel("Schedule")
            Seg(
                listOf("fixed" to "At a time", "interval" to "Every interval", "webhook" to "On webhook"),
                when (draft.mode) {
                    TaskMode.FIXED -> "fixed"
                    TaskMode.INTERVAL -> "interval"
                    TaskMode.WEBHOOK -> "webhook"
                },
                Modifier.padding(bottom = 12.dp),
            ) {
                core.taskSetMode(
                    when (it) {
                        "interval" -> TaskMode.INTERVAL
                        "webhook" -> TaskMode.WEBHOOK
                        else -> TaskMode.FIXED
                    },
                )
            }
            when (draft.mode) {
                TaskMode.FIXED -> {
                    Inline("Run at ", null) {
                        val shape = RoundedCornerShape(24.dp)
                        Box(
                            Modifier
                                .width(150.dp)
                                .height(48.dp)
                                .card(P.surface, shape, P.input)
                                .tap(label = "Run at") {
                                    val (hour, minute) = draft.time.split(":").map { it.toIntOrNull() ?: 0 }.let {
                                        (it.getOrNull(0) ?: 9) to (it.getOrNull(1) ?: 0)
                                    }
                                    TimePickerDialog(
                                        context,
                                        { _, h, m -> core.taskSetText("time", "%02d:%02d".format(h, m)) },
                                        hour,
                                        minute,
                                        DateFormat.is24HourFormat(context),
                                    ).show()
                                },
                            contentAlignment = Alignment.Center,
                        ) { T(draft.time, 17f, lines = 1, line = 1.2f) }
                    }
                    Row(
                        Modifier.fillMaxWidth().padding(top = 4.dp, bottom = 8.dp),
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        for (day in WEEK) {
                            DayPill(
                                WEEKDAYS[day].take(1),
                                day.toUInt() in draft.weekdays,
                                Modifier.weight(1f),
                            ) { core.taskToggleDay(day.toUInt()) }
                        }
                    }
                    Row(Modifier.padding(bottom = 12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        DayPill("Weekdays", false, Modifier, height = 36.dp, size = 13f) {
                            core.taskSetDays((1..5).map { it.toUInt() })
                        }
                        DayPill("Daily", false, Modifier, height = 36.dp, size = 13f) {
                            core.taskSetDays((0..6).map { it.toUInt() })
                        }
                    }
                }
                TaskMode.INTERVAL -> Inline("Run every ", " min") {
                    FormField(interval, { interval = it; core.taskSetText("interval", it) }, number = true, center = true, width = 110.dp)
                }
                TaskMode.WEBHOOK -> {
                    Inline("Skip requests older than ", " min") {
                        FormField(maxAge, { maxAge = it; core.taskSetText("max_age", it) }, number = true, center = true, width = 110.dp)
                    }
                    Box(Modifier.offset(y = (-4).dp)) {
                        PageNote("The webhook URL appears in Pluto on the computer once the task is saved.")
                    }
                }
            }
            val shape = RoundedCornerShape(24.dp)
            Row(
                Modifier
                    .padding(top = 12.dp, bottom = 8.dp)
                    .fillMaxWidth()
                    .card(P.surface, shape)
                    .tap(role = Role.Switch) { core.taskSetEnabled(!draft.enabled) }
                    .padding(start = 20.dp, end = 16.dp, top = 14.dp, bottom = 14.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                Column(Modifier.weight(1f)) {
                    T("Enabled")
                    T("Disabled tasks stay saved but do not run.", 13f, P.muted)
                }
                Toggle(draft.enabled, "Enabled") { core.taskSetEnabled(it) }
            }
            Box(Modifier.padding(start = 8.dp, end = 8.dp, top = 4.dp).heightIn(min = 20.dp)) {
                draft.error?.let { T(it, 14f, P.error) }
            }
            Btn(
                when {
                    draft.saving -> "Saving…"
                    draft.editing -> "Save task"
                    else -> "Create task"
                },
                Modifier.fillMaxWidth().padding(top = 4.dp),
                tone = Tone.Primary,
                enabled = !draft.saving,
                height = 56.dp,
                fontSize = 17f,
            ) { core.taskSave() }
        }
    }
}
