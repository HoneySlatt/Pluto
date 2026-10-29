package app.pluto.mobile.ui

// The pages pushed over the current view: Settings, the diff, Projects, a
// project, Scheduled tasks and their editor, Usage, Personalization and
// Licenses, each under its header (`PageHead`).

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.pluto.mobile.LocalModel
import app.pluto.mobile.Page
import app.pluto.mobile.safeBars

@Composable
fun PageView(page: Page) {
    when (page) {
        is Page.Thread -> ThreadView(page.id, page = true)
        else -> Column(Modifier.fillMaxSize().background(P.canvas).safeBars()) {
            PageHead(pageTitle(page))
            T("Not ported yet.", 15f, P.muted, modifier = Modifier.padding(24.dp))
        }
    }
}

private fun pageTitle(page: Page) = when (page) {
    Page.Settings -> "Settings"
    is Page.Diff -> "Changes"
    Page.Projects -> "Projects"
    is Page.Project -> "Project"
    Page.Tasks -> "Scheduled tasks"
    is Page.TaskEdit -> if (page.id == null) "New task" else "Edit task"
    Page.Usage -> "Usage"
    Page.Memory -> "Personalization"
    Page.Licenses -> "Licenses"
    is Page.Thread -> ""
}

// A page's header: Back, the title, and an action or the space it keeps.
@Composable
fun PageHead(title: String, action: (@Composable () -> Unit)? = null) {
    val nav = LocalModel.current.nav
    Row(
        Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        DiscIcon("chevron-left", "Back") { nav.back() }
        T(title, 16f, weight = FontWeight.SemiBold, lines = 1, align = TextAlign.Center, modifier = Modifier.weight(1f))
        if (action != null) action() else Box(Modifier.width(48.dp))
    }
}
