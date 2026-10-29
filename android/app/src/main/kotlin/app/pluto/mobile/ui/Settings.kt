package app.pluto.mobile.ui

// Settings (the Dioxus app's `pages.rs`, the desktop's `settings.rs` labels
// kept to what a phone needs): the server card, Chat's default model and
// personalization, New threads' model, permissions and workspace, the
// display choices, the confirmations, the server's address and token,
// Disconnect, and About. Each choice opens its sheet (`SettingSheet`); the
// core reads and writes them (crates/pluto-ffi/src/settings.rs).
//
// Open source licenses (Settings → About): Pluto (MIT), the fonts and
// icons (Geist under OFL-1.1, Lucide under ISC, the provider marks as their
// owners' trademarks), the Rust core's direct crates with the licenses
// cargo-about found in them, the Android libraries, and the full notice of
// every crate the app links, all generated with the APK
// (android/app/notice.gradle.kts). A row opens its text.

import android.content.Context
import androidx.compose.foundation.background
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.pluto.core.Dot
import app.pluto.core.coreVersion
import app.pluto.mobile.BuildConfig
import app.pluto.mobile.LocalModel
import app.pluto.mobile.Page
import app.pluto.mobile.Sheet
import org.json.JSONArray
import org.json.JSONObject

// Settings

@Composable
fun SettingsPage() {
    val model = LocalModel.current
    val core = model.core
    val nav = model.nav
    val view = remember(model.revision) { core.settingsView() }
    // The address stays remembered after the token is forgotten.
    val remembered = remember(model.revision) { model.secret.server() != null }
    PageFrame("Settings") {
        ServerCard(view.host, view.state, view.dot)
        GroupLabel("Chat")
        Group {
            SetRow("message-square", "Default model", view.chatModel.label, mark = view.chatModel.mark) {
                nav.openSheet(Sheet.Setting("chatModel"))
            }
            SetRow("brain", "Personalization", "Memory") { nav.openPage(Page.Memory) }
        }
        GroupLabel("New threads")
        Group {
            SetRow("bot", "Model", view.codeModel.label, mark = view.codeModel.mark) {
                nav.openSheet(Sheet.Setting("codeModel"))
            }
            SetRow("shield", "Permissions", view.access) { nav.openSheet(Sheet.Setting("codeAccess")) }
            SetRow("folder", "Workspace", view.workspace) { nav.openSheet(Sheet.Setting("workspace")) }
        }
        GroupLabel("Display")
        Group {
            SetRow("clock", "Time format", view.timeFormat) { nav.openSheet(Sheet.Setting("timeFormat")) }
            SetRow("text-wrap", "Response streaming", view.streaming, stacked = true) {
                nav.openSheet(Sheet.Setting("streaming"))
            }
            SetToggle("file-diff", "Hide whitespace changes", view.hideWhitespace) {
                core.setSettingFlag("diffIgnoreWhitespace", it)
            }
            SetRow("files", "Default diff file state", view.diffState) { nav.openSheet(Sheet.Setting("diffState")) }
        }
        GroupLabel("Confirmations")
        Group {
            SetToggle(
                "trash-2",
                "Delete confirmation",
                view.confirmDelete,
                detail = "Ask before deleting a thread and its chat history.",
            ) { core.setSettingFlag("confirmThreadDelete", it) }
        }
        GroupLabel("Server")
        Group {
            SetRow("link-2", "Address", view.host)
            SetRow(
                "key-round",
                "Token",
                detail = if (remembered) "Stored encrypted on this phone" else "Kept in memory until Pluto closes",
            )
            if (remembered) {
                SetRow("eye-off", "Forget token", detail = "Ask for it on the next launch", chevron = false) {
                    model.secret.forgetToken()
                    core.toast("Token forgotten. Pluto will ask for it next time")
                }
            }
            SetRow("log-out", "Disconnect", danger = true, chevron = false) {
                // Leaving the server: the next launch asks for a token, with
                // the address kept on the connection screen.
                model.secret.forgetToken()
                nav.disconnect()
            }
        }
        PageNote("Plain http is accepted on your home network only. There the token crosses the Wi-Fi unencrypted.")
        GroupLabel("About")
        Group {
            SetRow("server", "Server", view.version)
            SetRow("smartphone", "App", "${BuildConfig.VERSION_NAME} · core ${coreVersion()}")
            SetRow("info", "Open source licenses") { nav.openPage(Page.Licenses) }
        }
        PageNote("Providers, appearance, keybindings and storage are set in Pluto on the computer.")
    }
}

// `.srv`: the server card, the planet beside its address and state.
@Composable
private fun ServerCard(host: String, state: String, dot: Dot) {
    val shape = RoundedCornerShape(24.dp)
    Row(
        Modifier.fillMaxWidth().card(P.surface, shape).padding(16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Avatar()
        Column(Modifier.weight(1f)) {
            T("Pluto server", 17f, weight = FontWeight.SemiBold, line = 1.3f, track = -.01f, lines = 1)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                StatusDot(dot)
                T("$host · $state", 14f, P.muted, lines = 1)
            }
        }
    }
}

// `.online`: the connection's dot in its soft ring.
@Composable
private fun StatusDot(dot: Dot) {
    val color = when (dot) {
        Dot.OK -> P.success
        Dot.WARN -> P.warning
        Dot.BAD -> P.error
    }
    Box(Modifier.size(13.dp).clip(CircleShape).background(color.copy(alpha = .15f)), contentAlignment = Alignment.Center) {
        Box(Modifier.size(7.dp).clip(CircleShape).background(color))
    }
}

// A Settings choice (`Sheet.Setting`): its options, or the models by
// provider; picking one applies it and closes the sheet.
@Composable
fun SettingSheet(key: String) {
    val model = LocalModel.current
    val core = model.core
    val nav = model.nav
    val view = remember(model.revision, key) { core.settingSheet(key) }
    view.note?.let { SheetNote(it) }
    for (provider in view.providers) {
        SheetLabel(provider.name)
        for (option in provider.models) {
            Choice(option.label, option.checked, mark = provider.mark) {
                core.pickSettingModel(key, provider.instanceId, option.slug)
                nav.back()
            }
        }
    }
    for (choice in view.choices) {
        Choice(choice.label, choice.checked, detail = choice.detail) {
            core.pickSetting(key, choice.value)
            nav.back()
        }
    }
}

@Composable
fun settingTitle(key: String): String = LocalModel.current.core.settingTitle(key)

// Licenses

private const val COPYRIGHT = "Copyright (c) 2026 T3 Tools Inc."
private const val TRADEMARKS = "The logos only identify each provider in the model picker and on the " +
    "Usage page. They belong to their owners: Anthropic (Claude), OpenAI (Codex), Moonshot AI " +
    "(Kimi Code), Mistral AI (Mistral Vibe), Zhipu AI (Z Code), Ollama, and the other providers " +
    "shown. Pluto is not affiliated with these companies."
private const val JNA = "JNA is offered under the Apache License 2.0 or the LGPL 2.1; Pluto uses it " +
    "under the Apache License 2.0."

// An Android library family on the Licenses page: its key, name and the
// group prefixes it takes.
private val FAMILIES = listOf(
    Triple("androidx", "AndroidX and Jetpack Compose", listOf("androidx.")),
    Triple("kotlin", "Kotlin and kotlinx", listOf("org.jetbrains")),
    Triple("jna", "JNA", listOf("net.java.dev.jna")),
    Triple("other", "Other libraries", listOf("")),
)

// The notice generated with the APK (`notice/` in its assets).
private class Notice(context: Context) {
    private val assets = context.assets

    private fun read(path: String): String? = runCatching {
        assets.open("notice/$path").bufferedReader().use { it.readText() }
    }.getOrNull()

    val rust: JSONObject? = read("rust.json")?.let { runCatching { JSONObject(it) }.getOrNull() }
    val android: JSONArray? = read("android.json")?.let { runCatching { JSONArray(it) }.getOrNull() }
    val license: String = read("texts/LICENSE").orEmpty()
    val ofl: String = read("texts/OFL.txt").orEmpty()
    val lucide: String = read("texts/LUCIDE-LICENSE").orEmpty()
    val complete = rust != null && android != null

    // Pluto and the assets: name, license, one-line notice, text.
    val own = listOf(
        listOf("Pluto", "MIT", COPYRIGHT, license),
        listOf("pluto-core", "MIT", COPYRIGHT, license),
    )
    val assetsList = listOf(
        listOf("Geist, Geist Mono", "OFL-1.1", "Copyright 2024 The Geist Project Authors", ofl),
        listOf(
            "Lucide icons",
            "ISC",
            "Copyright (c) Lucide Contributors; portions (c) Cole Bemis as part of Feather (MIT)",
            lucide,
        ),
        listOf("Provider logos", "Trademarks", "Marks of their owners", TRADEMARKS),
    )

    // The direct crates: name and license expression.
    val crates: List<JSONObject> = rust?.optJSONArray("crates")?.let { list ->
        (0 until list.length()).map { list.getJSONObject(it) }
    }.orEmpty()

    val libraries: List<JSONObject> = android?.let { list ->
        (0 until list.length()).map { list.getJSONObject(it) }
    }.orEmpty()

    // The family a library belongs to (the first whose prefix matches).
    fun family(library: JSONObject): String {
        val group = library.optString("group")
        return FAMILIES.first { (_, _, prefixes) -> prefixes.any { group.startsWith(it) } }.first
    }

    fun inFamily(key: String) = libraries.filter { family(it) == key }

    companion object {
        @Volatile private var loaded: Notice? = null

        fun of(context: Context): Notice = loaded ?: Notice(context.applicationContext).also { loaded = it }
    }
}

@Composable
private fun notice(): Notice {
    val context = LocalContext.current
    return remember { Notice.of(context) }
}

@Composable
fun LicensesPage() {
    val nav = LocalModel.current.nav
    val notice = notice()
    PageFrame("Open source licenses") {
        T(
            "Notices for dependencies, assets, and optional tools used by Pluto.",
            13f,
            P.muted,
            line = 1.45f,
            modifier = Modifier.padding(horizontal = 8.dp),
        )
        GroupLabel("Pluto")
        Group {
            notice.own.forEachIndexed { index, (name, license) ->
                LicenseRow(name, license) { nav.openSheet(Sheet.License("own:$index")) }
            }
        }
        GroupLabel("Fonts and icons")
        Group {
            notice.assetsList.forEachIndexed { index, (name, license) ->
                LicenseRow(name, license) { nav.openSheet(Sheet.License("asset:$index")) }
            }
        }
        GroupLabel("Rust crates")
        Group {
            for (entry in notice.crates) {
                val name = entry.optString("name")
                val license = entry.optString("license").ifEmpty { "See notice" }
                LicenseRow(name, license) { nav.openSheet(Sheet.License("crate:$name")) }
            }
            if (notice.rust != null) {
                SetRow(
                    null,
                    "Full notice",
                    detail = "Every crate in the app, ${notice.rust.optInt("count")} in all",
                ) { nav.openSheet(Sheet.License("notice")) }
            }
        }
        if (notice.rust != null) {
            PageNote("Direct dependencies only. Their own dependencies are in the full notice, generated with the app.")
        }
        GroupLabel("Android libraries")
        Group {
            for ((key, name) in FAMILIES) {
                val count = notice.inFamily(key).size
                if (count == 0) continue
                LicenseRow(name, "Apache-2.0", detail = if (count == 1) "1 library" else "$count libraries") {
                    nav.openSheet(Sheet.License("android:$key"))
                }
            }
        }
        if (!notice.complete) {
            PageNote("This build was made without its notice: android/app/notice.gradle.kts generates it.")
        }
    }
}

// `.set.lic`: a name, its license in mono, a chevron.
@Composable
private fun LicenseRow(name: String, license: String, detail: String? = null, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .pressRow(interaction, P.overlay)
            .tap(interaction = interaction, onClick = onClick)
            .padding(start = 18.dp, end = 16.dp, top = 12.dp, bottom = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Column(Modifier.weight(1f)) {
            T(name, 16f)
            if (detail != null) T(detail, 13f, P.muted, line = 1.4f, modifier = Modifier.padding(top = 2.dp))
        }
        T(license, 12f, P.muted, mono = true, lines = 1, modifier = Modifier.widthIn(max = 180.dp))
        Icon("chevron-right", P.icon, size = 18.dp)
    }
}

@Composable
fun licenseTitle(key: String): String {
    val notice = notice()
    val (kind, rest) = key.substringBefore(':') to key.substringAfter(':', "")
    return when (kind) {
        "own" -> notice.own.getOrNull(rest.toIntOrNull() ?: 0)?.get(0).orEmpty()
        "asset" -> notice.assetsList.getOrNull(rest.toIntOrNull() ?: 0)?.get(0).orEmpty()
        "crate" -> rest
        "android" -> FAMILIES.firstOrNull { it.first == rest }?.second.orEmpty()
        else -> "Full notice"
    }
}

// A license's text in its sheet: a line on what it covers, then its
// texts in mono boxes.
@Composable
fun LicenseSheet(key: String) {
    val notice = notice()
    val kind = key.substringBefore(':')
    val rest = key.substringAfter(':', "")
    var meta = ""
    val blocks = ArrayList<Pair<String, String>>()
    when (kind) {
        "own" -> {
            val own = notice.own[(rest.toIntOrNull() ?: 0).coerceIn(0, notice.own.lastIndex)]
            meta = "${own[1]} · ${own[2]}"
            blocks += "" to own[3]
        }
        "asset" -> {
            val asset = notice.assetsList[(rest.toIntOrNull() ?: 0).coerceIn(0, notice.assetsList.lastIndex)]
            meta = "${asset[1]} · ${asset[2]}"
            blocks += "" to asset[3]
        }
        "crate" -> {
            val entry = notice.crates.firstOrNull { it.optString("name") == rest }
            val versions = entry?.optJSONArray("versions")?.let { list -> (0 until list.length()).map { list.getString(it) } }.orEmpty()
            meta = "${entry?.optString("license")?.ifEmpty { null } ?: "Not in the notice"} · ${versions.joinToString(", ")}"
            entry?.optJSONArray("texts")?.let { texts ->
                for (index in 0 until texts.length()) {
                    val text = texts.getJSONObject(index)
                    blocks += text.optString("name") to text.optString("text").trim()
                }
            }
        }
        "android" -> {
            val libraries = notice.inFamily(rest)
            meta = "Apache-2.0 · " + if (libraries.size == 1) "1 library" else "${libraries.size} libraries"
            blocks += "Libraries" to libraries.joinToString("\n") {
                "${it.optString("group")}:${it.optString("name")} ${it.optString("version")}"
            }
            if (rest == "jna") blocks += "" to JNA
            for (library in libraries) {
                val notices = library.optJSONArray("notices") ?: continue
                for (index in 0 until notices.length()) {
                    blocks += "NOTICE · ${library.optString("name")}" to notices.getString(index)
                }
            }
            blocks += "Apache License 2.0" to notice.rust?.optString("apache").orEmpty()
        }
        else -> {
            blocks += "" to notice.rust?.optString("notice").orEmpty()
        }
    }
    if (meta.isNotEmpty()) {
        T(meta, 13f, P.muted, modifier = Modifier.padding(start = 24.dp, end = 24.dp, bottom = 10.dp))
    }
    if (blocks.all { it.second.isBlank() }) {
        SheetNote(if (kind == "crate") "No license text was found for this crate." else "This build was made without its notice.")
        return
    }
    for ((head, text) in blocks) {
        if (text.isBlank()) continue
        if (head.isNotEmpty()) {
            T(head, 13f, P.muted2, FontWeight.Medium, modifier = Modifier.padding(start = 20.dp, end = 20.dp, bottom = 6.dp))
        }
        LicenseText(text)
    }
}

// `.lictext`: a license in mono, at most half the screen tall, scrolling
// inside; long texts (the full notice) lay out in chunks as they scroll.
@Composable
private fun LicenseText(text: String) {
    val chunks = remember(text) { text.lines().chunked(40).map { it.joinToString("\n") } }
    val shape = RoundedCornerShape(16.dp)
    val tall = (LocalConfiguration.current.screenHeightDp * .52f).dp
    val style = style(12f, color = P.muted2, line = 1.6f, mono = true)
    Box(
        Modifier
            .padding(start = 20.dp, end = 20.dp, bottom = 8.dp)
            .fillMaxWidth()
            .heightIn(max = tall)
            .card(P.chip, shape),
    ) {
        SelectionContainer {
            LazyColumn(Modifier.fillMaxWidth().padding(horizontal = 16.dp), contentPadding = androidx.compose.foundation.layout.PaddingValues(vertical = 14.dp)) {
                items(chunks) { chunk -> BasicText(chunk, style = style) }
            }
        }
    }
}
