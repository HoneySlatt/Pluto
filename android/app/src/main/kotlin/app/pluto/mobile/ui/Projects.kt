package app.pluto.mobile.ui

// Filled by its port.

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.unit.dp

@Composable
fun ProjectsPage() {
    PageFrame("") { Todo() }
}

@Composable
fun ProjectPage(id: String) {
    PageFrame("") { Todo() }
}

@Composable
fun ProjectSettingSheet(project: String, field: String) {
    Todo()
}

@Composable
fun projectSettingTitle(field: String): String = ""

@Composable
fun RemoveProjectSheet(project: String) {
    Todo()
}

@Composable
private fun Todo() {
    T("Not ported yet.", 15f, P.muted, modifier = Modifier.padding(24.dp))
}
