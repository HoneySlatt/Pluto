package app.pluto.mobile.ui

// Filled by its port.

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.unit.dp

@Composable
fun MemoryPage() {
    PageFrame("") { Todo() }
}

@Composable
fun MemoryEditSheet(id: String?) {
    Todo()
}

@Composable
fun MemoryMenuSheet(id: String) {
    Todo()
}

@Composable
fun ClearMemoriesSheet() {
    Todo()
}

@Composable
private fun Todo() {
    T("Not ported yet.", 15f, P.muted, modifier = Modifier.padding(24.dp))
}
