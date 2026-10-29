package app.pluto.mobile.ui

// Filled by its port.

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.unit.dp

@Composable
fun SettingsPage() {
    PageFrame("") { Todo() }
}

@Composable
fun SettingSheet(key: String) {
    Todo()
}

@Composable
fun settingTitle(key: String): String = ""

@Composable
fun LicensesPage() {
    PageFrame("") { Todo() }
}

@Composable
fun LicenseSheet(key: String) {
    Todo()
}

@Composable
fun licenseTitle(key: String): String = ""

@Composable
private fun Todo() {
    T("Not ported yet.", 15f, P.muted, modifier = Modifier.padding(24.dp))
}
