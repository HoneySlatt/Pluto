package app.pluto.mobile.ui

// Projects (`#projects`): one row per project with its threads and state.
// A project (`#project`, after the desktop's `projects_page.rs`): its
// identity, New thread, its threads, its New threads defaults with their
// choice sheets, and its removal behind a confirmation.

import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.pluto.core.Badge
import app.pluto.core.DefaultModelGroup
import app.pluto.mobile.LocalModel
import app.pluto.mobile.Page
import app.pluto.mobile.Sheet

// `.prow`: a 72dp row of a page's group, its badge first.
@Composable
private fun PRow(
    badge: Badge?,
    title: String,
    line: String,
    titleLines: Int = 1,
    trailing: @Composable () -> Unit = {},
    onClick: () -> Unit,
) {
    val interaction = remember { MutableInteractionSource() }
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 72.dp)
            .pressRow(interaction, P.overlay)
            .tap(interaction = interaction, onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        if (badge != null) ProjectBadge(badge, BadgeRow)
        Column(Modifier.weight(1f)) {
            T(title, 16f, weight = if (titleLines == 1) FontWeight.Medium else FontWeight.Normal, lines = titleLines, line = 1.4f)
            T(line, 13f, P.muted, lines = 1)
        }
        trailing()
        Icon("chevron-right", P.icon, size = 18.dp)
    }
}

@Composable
fun ProjectsPage() {
    val model = LocalModel.current
    val rows = remember(model.revision) { model.core.projects() }
    PageFrame("Projects") {
        if (rows.isEmpty()) {
            EmptyPage("No projects", "Projects are added in Pluto on the computer.")
        } else {
            Group {
                for (row in rows) {
                    val count = row.threads.toInt()
                    PRow(
                        row.badge,
                        row.badge.title,
                        "${row.root} · $count thread${if (count == 1) "" else "s"}",
                        trailing = {
                            when {
                                row.waiting > 0u -> T("${row.waiting} waiting", 13f, P.warning, FontWeight.Medium, lines = 1)
                                row.working > 0u -> Row(
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                                ) {
                                    Spinner(P.accent, 14.dp)
                                    T("Working", 13f, P.accent, FontWeight.Medium, lines = 1)
                                }
                            }
                        },
                    ) { model.nav.openPage(Page.Project(row.badge.projectId)) }
                }
            }
            PageNote("Projects are added in Pluto on the computer.")
        }
    }
}

@Composable
fun ProjectPage(id: String) {
    val model = LocalModel.current
    val nav = model.nav
    val core = model.core
    val view = remember(model.revision, id) { core.project(id) }
    if (view == null) {
        PageFrame("Project") {
            T("This project was removed.", 15f, P.muted, align = TextAlign.Center, modifier = Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 64.dp))
        }
        return
    }
    PageFrame(view.badge.title) {
        Column(
            Modifier.fillMaxWidth().padding(top = 12.dp, bottom = 20.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            ProjectBadge(view.badge, BadgeHead)
            T(view.badge.title, 22f, weight = FontWeight.SemiBold, track = -.015f, line = 1.3f, align = TextAlign.Center)
            T(view.root, 13f, P.muted, mono = true, align = TextAlign.Center)
        }
        Btn("New thread", Modifier.fillMaxWidth(), tone = Tone.Primary, height = 56.dp, fontSize = 17f, icon = "square-pen") {
            nav.reset()
            core.projectNewThread(id)
        }
        GroupLabel("Threads")
        if (view.threads.isEmpty()) {
            PageNote("No threads yet.")
        } else {
            Group {
                for (thread in view.threads) {
                    PRow(null, thread.title, thread.line, titleLines = 2) { nav.openPage(Page.Thread(thread.id)) }
                }
            }
        }
        GroupLabel("New threads")
        Group {
            SetRow("bot", "Model", view.model, mark = view.modelMark) { nav.openSheet(Sheet.ProjectSetting(id, "model")) }
            SetRow("shield", "Permissions", view.access) { nav.openSheet(Sheet.ProjectSetting(id, "access")) }
            SetRow("folder", "Workspace", view.workspace) { nav.openSheet(Sheet.ProjectSetting(id, "workspace")) }
        }
        PageNote("Overrides the defaults from Settings for this project only.")
        GroupLabel("Danger")
        Group {
            SetRow(
                "trash-2",
                "Remove project",
                detail = "This removes only the project entries, not the files on disk.",
                danger = true,
                chevron = false,
            ) { nav.openSheet(Sheet.RemoveProject(id)) }
        }
    }
}

@Composable
fun projectSettingTitle(field: String): String = when (field) {
    "model" -> "Model"
    "access" -> "Permissions"
    else -> "Workspace"
}

// A provider's models as checked rows (`ModelList`), or why there are none.
@Composable
fun DefaultModels(groups: List<DefaultModelGroup>, onPick: (instance: String, model: String) -> Unit) {
    if (groups.isEmpty()) {
        SheetNote("No provider is ready on the server.")
        return
    }
    for (group in groups) {
        SheetLabel(group.name)
        for (choice in group.models) {
            Choice(choice.label, choice.checked, mark = group.mark) { onPick(choice.instanceId, choice.model) }
        }
    }
}

@Composable
fun ProjectSettingSheet(project: String, field: String) {
    val model = LocalModel.current
    val core = model.core
    val nav = model.nav
    val view = remember(model.revision, project, field) { core.projectSetting(project, field) }
    Choice("Default", view.default, detail = "Use the default from Settings") {
        core.setProjectSetting(project, field, null)
        nav.back()
    }
    if (field == "model") {
        DefaultModels(view.groups) { instance, slug ->
            core.setProjectModel(project, instance, slug)
            nav.back()
        }
    } else {
        for (option in view.options) {
            Choice(option.label, option.checked, detail = option.detail) {
                core.setProjectSetting(project, field, option.value)
                nav.back()
            }
        }
    }
}

@Composable
fun RemoveProjectSheet(project: String) {
    val model = LocalModel.current
    val core = model.core
    val nav = model.nav
    // `removeMembers`'s confirmation (`projects_page.rs`).
    val view = remember(project) { core.removeProjectView(project) }
    val outcome = rememberOutcome {
        core.toast("${view.title} removed")
        nav.reset()
    }
    for ((index, line) in view.lines.withIndex()) {
        T(
            line,
            15f,
            if (index == 0) P.text else P.muted2,
            modifier = Modifier.padding(start = 24.dp, end = 24.dp, bottom = 8.dp),
        )
    }
    outcome.error?.let { SheetError(it) }
    SheetActions {
        Btn("Cancel", Modifier.weight(1f)) { nav.back() }
        Btn(
            if (outcome.busy) "Removing…" else "Remove",
            Modifier.weight(1f),
            tone = Tone.Danger,
            enabled = !outcome.busy,
        ) { outcome.once { core.removeProject(project) } }
    }
}
