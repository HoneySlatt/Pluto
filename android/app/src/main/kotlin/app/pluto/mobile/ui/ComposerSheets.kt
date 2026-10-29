package app.pluto.mobile.ui

// Filled by its port.

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.unit.dp

@Composable
fun ModelSheet() {
    Todo()
}

@Composable
fun ThreadMenuSheet(id: String) {
    Todo()
}

@Composable
fun threadMenuTitle(id: String): String = ""

@Composable
fun AttachSheet() {
    Todo()
}

@Composable
fun ProjectSheet() {
    Todo()
}

@Composable
fun WorkspaceSheet() {
    Todo()
}

@Composable
fun BranchSheet() {
    Todo()
}

@Composable
fun RenameSheet(id: String) {
    Todo()
}

@Composable
fun DeleteThreadSheet(id: String) {
    Todo()
}

@Composable
private fun Todo() {
    T("Not ported yet.", 15f, P.muted, modifier = Modifier.padding(24.dp))
}
