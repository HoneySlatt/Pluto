package app.pluto.mobile.ui

// Filled by its port.

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.unit.dp

@Composable
fun TasksPage() {
    PageFrame("") { Todo() }
}

@Composable
fun TaskEditPage(id: String?) {
    PageFrame("") { Todo() }
}

@Composable
fun TaskMenuSheet(id: String) {
    Todo()
}

@Composable
fun taskMenuTitle(id: String): String = ""

@Composable
fun TaskFieldSheet(field: String) {
    Todo()
}

@Composable
fun taskFieldTitle(field: String): String = ""

@Composable
private fun Todo() {
    T("Not ported yet.", 15f, P.muted, modifier = Modifier.padding(24.dp))
}
