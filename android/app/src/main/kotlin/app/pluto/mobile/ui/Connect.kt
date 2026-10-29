package app.pluto.mobile.ui

// The connection screen (`#connect`): the server's address with the
// remembered one as a chip, the token with show and paste, "Remember on
// this phone" (the Android Keystore), errors under the fields, and "Where
// do I get a token?". Plain http is accepted for private addresses only,
// like `pluto_core::protocol::socket_url`. `#unreachable` is the launch
// with a remembered server that does not answer.

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.pluto.core.Net
import app.pluto.core.checkAddress
import app.pluto.mobile.LocalGate
import app.pluto.mobile.LocalModel
import app.pluto.mobile.LocalNetwork
import app.pluto.mobile.safeBars

private const val TOKEN_COMMAND =
    "pluto-server auth session issue --base-dir ~/.pluto --label phone --token-only"

// "192.168.1.20:3774" from "http://192.168.1.20:3774/".
fun urlHost(server: String) = server.substringAfter("://").trimEnd('/')

// A field in its pill (`.inbox`): its border lights when focused, reddens
// when wrong.
@Composable
fun Inbox(
    value: String,
    onValue: (String) -> Unit,
    bad: Boolean = false,
    mono: Boolean = false,
    hidden: Boolean = false,
    keyboard: KeyboardOptions = KeyboardOptions.Default,
    onDone: () -> Unit = {},
    trailing: @Composable () -> Unit = {},
) {
    var focused by remember { mutableStateOf(false) }
    val border = when {
        focused -> P.accent
        bad -> P.error.copy(alpha = .6f)
        else -> P.input
    }
    val shape = RoundedCornerShape(28.dp)
    Row(
        Modifier
            .fillMaxWidth()
            .height(56.dp)
            .clip(shape)
            .background(P.surface)
            .border(1.dp, border, shape)
            .padding(start = 20.dp, end = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        BasicTextField(
            value,
            onValue,
            Modifier.weight(1f).onFocusChanged { focused = it.isFocused },
            singleLine = true,
            textStyle = if (mono) style(15f, mono = true, track = .02f) else style(17f),
            cursorBrush = SolidColor(P.accent),
            visualTransformation = if (hidden) PasswordVisualTransformation() else VisualTransformation.None,
            keyboardOptions = keyboard,
            keyboardActions = KeyboardActions(onDone = { onDone() }, onGo = { onDone() }),
        )
        trailing()
    }
}

// A round 44dp button inside a field.
@Composable
fun FieldButton(icon: String, label: String, onClick: () -> Unit) {
    Box(
        Modifier.size(44.dp).clip(CircleShape).tap(label = label, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { Icon(icon, P.muted2) }
}

@Composable
fun FieldLabel(text: String) {
    T(text, 14f, P.muted2, FontWeight.Medium, Modifier.padding(start = 8.dp, end = 8.dp, bottom = 8.dp))
}

@Composable
fun Connect() {
    val model = LocalModel.current
    val core = model.core
    val shell = remember(model.revision) { core.shell() }
    val remembered = remember { model.secret.server() }
    var server by rememberSaveable { mutableStateOf(remembered ?: "http://") }
    var token by remember { mutableStateOf("") }
    var remember by rememberSaveable { mutableStateOf(true) }
    var reveal by remember { mutableStateOf(false) }
    var localError by remember { mutableStateOf<Pair<String, Boolean>?>(null) }
    var help by remember { mutableStateOf(false) }
    val clipboard = LocalClipboardManager.current
    val gate = LocalGate.current
    val busy = shell.net is Net.Connecting
    // The connection's own failure goes under the address.
    val error = localError ?: (shell.net as? Net.Disconnected)?.error?.let { it to true }
    val submit = submit@{
        val address = server.trim()
        val secret = token.trim()
        val problem = checkAddress(address)
        if ((problem != null && !address.contains("://")) || address == "http://") {
            localError = "Enter the server’s address, like http://192.168.1.20:3774." to true
            return@submit
        }
        if (problem != null) {
            localError = problem to true
            return@submit
        }
        if (secret.isEmpty()) {
            localError = "Paste the session token." to false
            return@submit
        }
        localError = null
        if (remember) {
            model.secret.save(address, secret)?.let { core.toast(it) }
        } else {
            model.secret.forget()
        }
        token = ""
        gate.connect(address) { core.connect(address, secret, false) }
    }
    val badServer = error?.second == true
    val badToken = error?.second == false
    Box(Modifier.fillMaxSize().background(P.canvas)) {
        Glow()
        Column(
            Modifier
                .fillMaxSize()
                .safeBars()
                .imePadding()
                .verticalScroll(rememberScrollState())
                .padding(start = 20.dp, end = 20.dp, bottom = 24.dp),
        ) {
            Column(
                Modifier.fillMaxWidth().padding(start = 8.dp, end = 8.dp, top = 40.dp, bottom = 28.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Planet(Modifier.size(104.dp, 87.dp), moon = false)
                Spacer(height = 20.dp)
                T("Connect to your server", 26f, weight = FontWeight.SemiBold, line = 1.25f, track = -.02f, align = TextAlign.Center)
                Spacer(height = 8.dp)
                T(
                    "The server's address and a session token.",
                    16f,
                    P.muted2,
                    modifier = Modifier.widthIn(max = ch(30, 16f)),
                    align = TextAlign.Center,
                )
            }
            FieldLabel("Server")
            Inbox(
                server,
                { server = it; localError = null },
                bad = badServer,
                mono = true,
                keyboard = KeyboardOptions(
                    capitalization = KeyboardCapitalization.None,
                    autoCorrectEnabled = false,
                    keyboardType = KeyboardType.Uri,
                    imeAction = ImeAction.Next,
                ),
            )
            // The recent server pulls up under the field (`margin: -4px 0 14px`).
            Spacer(height = if (remembered != null) 10.dp else 14.dp)
            if (remembered != null) {
                Row(
                    Modifier.padding(start = 4.dp, end = 4.dp, bottom = 14.dp).horizontalScroll(rememberScrollState()),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    val shape = RoundedCornerShape(18.dp)
                    Box(
                        Modifier
                            .height(36.dp)
                            .card(P.chip, shape)
                            .tap { server = remembered }
                            .padding(horizontal = 14.dp),
                        contentAlignment = Alignment.Center,
                    ) { T(urlHost(remembered), 13f, P.muted2, mono = true, line = 1.2f) }
                }
            }
            FieldLabel("Token")
            Inbox(
                token,
                { token = it; localError = null },
                bad = badToken,
                mono = true,
                hidden = !reveal,
                keyboard = KeyboardOptions(
                    capitalization = KeyboardCapitalization.None,
                    autoCorrectEnabled = false,
                    keyboardType = KeyboardType.Password,
                    imeAction = ImeAction.Go,
                ),
                onDone = submit,
            ) {
                FieldButton(if (reveal) "eye-off" else "eye", if (reveal) "Hide token" else "Show token") { reveal = !reveal }
                FieldButton("clipboard", "Paste token") {
                    val text = clipboard.getText()?.text?.trim().orEmpty()
                    if (text.isEmpty()) core.toast("Nothing to paste") else token = text
                }
            }
            Spacer(height = 14.dp)
            val shape = RoundedCornerShape(24.dp)
            Row(
                Modifier
                    .padding(top = 4.dp, bottom = 8.dp)
                    .fillMaxWidth()
                    .card(P.surface, shape)
                    .tap { remember = !remember }
                    .padding(start = 20.dp, end = 16.dp, top = 14.dp, bottom = 14.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                Column(Modifier.weight(1f)) {
                    T("Remember on this phone")
                    T("Encrypted with the Android Keystore", 13f, P.muted)
                }
                Toggle(remember, "Remember on this phone") { remember = it }
            }
            if (error != null) LocalNetworkNotice(server)
            if (error != null) {
                Row(
                    Modifier.padding(start = 4.dp, end = 4.dp, top = 8.dp),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Icon("circle-alert", P.error, Modifier.padding(top = 1.dp), size = 18.dp)
                    T(error.first, 14f, P.error)
                }
            }
            Btn(
                if (busy) "Connecting…" else "Connect",
                Modifier.fillMaxWidth().padding(top = 16.dp),
                tone = Tone.Primary,
                enabled = !busy,
                height = 56.dp,
                fontSize = 17f,
                icon = if (busy) "loader-circle" else null,
                spin = busy,
                onClick = submit,
            )
            // "Where do I get a token?"
            val helpShape = RoundedCornerShape(20.dp)
            Column(Modifier.padding(top = 24.dp).fillMaxWidth().card(P.surface, helpShape)) {
                val turn by animateFloatAsState(if (help) 180f else 0f, tween(200), label = "chev")
                Row(
                    Modifier.fillMaxWidth().heightIn(min = 52.dp).tap { help = !help }.padding(horizontal = 16.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Icon("key-round", P.muted2, size = 18.dp)
                    T("Where do I get a token?", 15f, P.muted2, modifier = Modifier.weight(1f))
                    Icon("chevron-down", P.muted2, Modifier.rotate(turn), size = 18.dp)
                }
                AnimatedVisibility(help, enter = expandVertically(tween(200)), exit = shrinkVertically(tween(200))) {
                    Column(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 16.dp)) {
                        T(
                            "Run this on the computer that hosts Pluto, then paste the token here. It stays on this phone.",
                            14f,
                            P.muted,
                        )
                        Row(
                            Modifier
                                .padding(top = 10.dp)
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(12.dp))
                                .background(P.chip)
                                .padding(start = 14.dp, end = 6.dp, top = 10.dp, bottom = 10.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            Box(Modifier.weight(1f).horizontalScroll(rememberScrollState())) {
                                T(TOKEN_COMMAND, 12.5f, mono = true, lines = 1)
                            }
                            Box(
                                Modifier.size(36.dp).clip(RoundedCornerShape(10.dp)).tap(label = "Copy command") {
                                    clipboard.setText(AnnotatedString(TOKEN_COMMAND))
                                    core.toast("Command copied")
                                },
                                contentAlignment = Alignment.Center,
                            ) { Icon("copy", P.muted2, size = 18.dp) }
                        }
                    }
                }
            }
        }
    }
}

// A remembered server that did not answer at launch (`#unreachable`).
@Composable
fun Unreachable() {
    val model = LocalModel.current
    val shell = remember(model.revision) { model.core.shell() }
    val busy = shell.net is Net.Connecting
    val gate = LocalGate.current
    Column(
        Modifier
            .fillMaxSize()
            .background(P.canvas)
            .safeBars()
            .verticalScroll(rememberScrollState())
            .padding(start = 20.dp, end = 20.dp, bottom = 24.dp),
    ) {
        Column(
            Modifier.fillMaxWidth().padding(start = 8.dp, end = 8.dp, top = 40.dp, bottom = 28.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Planet(Modifier.size(104.dp, 87.dp), moon = false, dim = true)
            Spacer(height = 20.dp)
            T("Can’t reach your server", 26f, weight = FontWeight.SemiBold, line = 1.25f, track = -.02f, align = TextAlign.Center)
            Spacer(height = 8.dp)
            val text = androidx.compose.ui.text.buildAnnotatedString {
                append("Pluto couldn’t connect to ")
                pushStyle(androidx.compose.ui.text.SpanStyle(fontFamily = GeistMono, color = P.text))
                append(shell.host)
                pop()
                append(".")
            }
            androidx.compose.foundation.text.BasicText(
                text,
                Modifier.widthIn(max = ch(30, 16f)),
                style = style(16f, color = P.muted2).copy(textAlign = TextAlign.Center),
            )
            // What the connection answered, for telling the causes apart.
            if (!busy) shell.error?.let { cause ->
                T(cause, 12.5f, P.muted, mono = true, align = TextAlign.Center, modifier = Modifier.padding(top = 10.dp))
            }
        }
        LocalNetworkNotice(shell.host) { model.core.retry() }
        val shape = RoundedCornerShape(24.dp)
        Column(Modifier.padding(top = 8.dp).fillMaxWidth().card(P.surface, shape).padding(vertical = 4.dp)) {
            Check("server", "The Pluto server runs on the computer", "systemctl --user status pluto-server")
            Check("globe", "This phone is on the same Wi-Fi as the computer")
            Check("shield", "NymVPN lets local network traffic through on both devices")
        }
        Btn(
            if (busy) "Connecting…" else "Try again",
            Modifier.fillMaxWidth().padding(top = 16.dp),
            tone = Tone.Primary,
            enabled = !busy,
            height = 56.dp,
            fontSize = 17f,
            icon = if (busy) "loader-circle" else null,
            spin = busy,
        ) { gate.connect(shell.host) { model.core.retry() } }
        Btn(
            "Change server",
            Modifier.fillMaxWidth().padding(top = 10.dp),
            tone = Tone.Ghost,
            height = 56.dp,
            fontSize = 17f,
        ) { model.nav.disconnect() }
    }
}

@Composable
private fun Check(icon: String, text: String, detail: String? = null) {
    Row(Modifier.padding(horizontal = 18.dp, vertical = 12.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Icon(icon, P.accent, Modifier.padding(top = 1.dp), size = 20.dp)
        Column {
            T(text, 15f, P.muted2)
            if (detail != null) T(detail, 12.5f, P.muted, mono = true, modifier = Modifier.padding(top = 2.dp))
        }
    }
}

// Android 17 keeps the app off the home network until it is allowed
// (LocalNetwork.kt): when that is why, this says so and asks again, or
// points to the app's settings once Android stopped asking.
@Composable
fun LocalNetworkNotice(server: String, onAllowed: () -> Unit = {}) {
    val context = androidx.compose.ui.platform.LocalContext.current
    val gate = LocalGate.current
    // Read again when the app comes back from its settings.
    var resumed by remember { mutableStateOf(0) }
    androidx.lifecycle.compose.LifecycleEventEffect(androidx.lifecycle.Lifecycle.Event.ON_RESUME) { resumed++ }
    val missing = remember(server, resumed) { LocalNetwork.missing(context, server) }
    if (!missing) return
    val shape = RoundedCornerShape(16.dp)
    Column(
        Modifier
            .padding(top = 12.dp)
            .fillMaxWidth()
            .card(P.warning.copy(alpha = .08f), shape, P.warning.copy(alpha = .28f))
            .padding(horizontal = 16.dp, vertical = 14.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Icon("triangle-alert", P.warning, Modifier.padding(top = 1.dp), size = 18.dp)
            T(
                "Android keeps Pluto off your home network until you allow it (Nearby devices).",
                14f,
                P.warning,
            )
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Btn("Allow", height = 40.dp, fontSize = 14f, tone = Tone.Primary) {
                gate.connect(server) {
                    resumed++
                    if (!LocalNetwork.missing(context, server)) onAllowed()
                }
            }
            Btn("Open settings", height = 40.dp, fontSize = 14f) { LocalNetwork.openSettings(context) }
        }
    }
}
