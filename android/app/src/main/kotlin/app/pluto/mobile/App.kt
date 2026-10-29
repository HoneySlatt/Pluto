package app.pluto.mobile

// Pluto on Android: the desktop client's Chat | Code split on a phone,
// after the validated prototype (laid out after Grok's Android app). It
// connects to a Pluto server (its address and a token from
// `pluto-server auth session issue`), opens on a Chat or Code draft, lists
// the threads in a full-screen drawer, shows a thread's timeline (built by
// the Rust core, like the desktop's) with its approvals, questions and
// changed files, and pushes Settings, the diff, Projects, Scheduled tasks
// and Usage as pages.
//
// The layers follow the finger: an edge swipe opens the drawer and a swipe
// back closes it, the view behind moving and dimming with it; a sheet
// pulled down by its top closes. Back closes the top layer.

import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBars
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.max
import app.pluto.core.Net
import app.pluto.mobile.ui.Connect
import app.pluto.mobile.ui.Drawer
import app.pluto.mobile.ui.Enter
import app.pluto.mobile.ui.Exit
import app.pluto.mobile.ui.Glow
import app.pluto.mobile.ui.Home
import app.pluto.mobile.ui.Icon
import app.pluto.mobile.ui.P
import app.pluto.mobile.ui.PageView
import app.pluto.mobile.ui.Planet
import app.pluto.mobile.ui.SheetView
import app.pluto.mobile.ui.Spinner
import app.pluto.mobile.ui.T
import app.pluto.mobile.ui.ThreadView
import app.pluto.mobile.ui.Btn
import app.pluto.mobile.ui.Tone
import app.pluto.mobile.ui.Unreachable
import app.pluto.mobile.ui.tap
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.abs

val LocalModel = staticCompositionLocalOf<Model> { error("No model") }

// `padding-top: max(safe-area, 12px)`, `padding-bottom: max(…, 8px)`.
fun Modifier.safeBars(top: Dp = 12.dp, bottom: Dp = 8.dp): Modifier = composed {
    val bars = WindowInsets.systemBars.asPaddingValues()
    padding(top = max(bars.calculateTopPadding(), top), bottom = max(bars.calculateBottomPadding(), bottom))
}

@Composable
fun PlutoApp(model: Model) {
    // Selected text and its handles take Pluto's accent.
    val selection = androidx.compose.foundation.text.selection.TextSelectionColors(
        handleColor = P.accent,
        backgroundColor = P.accent.copy(alpha = .35f),
    )
    // Connections to the home network ask Android first (LocalNetwork.kt).
    val gate = rememberLocalNetworkGate()
    CompositionLocalProvider(
        LocalModel provides model,
        androidx.compose.foundation.text.selection.LocalTextSelectionColors provides selection,
        LocalGate provides gate,
        app.pluto.mobile.ui.LocalReduceMotion provides app.pluto.mobile.ui.reduceMotion(androidx.compose.ui.platform.LocalContext.current),
    ) {
        // A remembered server connects at once.
        LaunchedEffect(Unit) {
            model.secret.load()?.let { (server, token) ->
                gate.connect(server) { model.core.connect(server, token, true) }
            }
        }
        val shell = remember(model.revision) { model.core.shell() }
        Box(Modifier.fillMaxSize().background(P.canvas)) {
            when (val net = shell.net) {
                // The remembered server's first answer: a quiet splash.
                is Net.Connecting -> when (shell.launch) {
                    false -> Splash()
                    true -> Unreachable()
                    null -> Connect()
                }
                is Net.Disconnected -> Connect()
                is Net.Unreachable -> Unreachable()
                else -> Session(net)
            }
            app.pluto.mobile.ui.Viewer()
            Toast(shell.toast)
        }
    }
}

@Composable
private fun Splash() {
    Box(Modifier.fillMaxSize().background(P.canvas)) {
        Glow()
        Column(
            Modifier.fillMaxSize().safeBars().padding(top = 40.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Planet(Modifier.size(104.dp, 87.dp))
        }
    }
}

// The drawer's opening, 0 to 1, following the finger or settling.
@Composable
private fun Session(net: Net) {
    val model = LocalModel.current
    val nav = model.nav
    val shell = remember(model.revision) { model.core.shell() }
    val scope = rememberCoroutineScope()
    val progress = remember { Animatable(if (nav.drawer) 1f else 0f) }
    var dragging by remember { mutableStateOf(false) }
    LaunchedEffect(nav.drawer) {
        if (!dragging) {
            val open = nav.drawer
            progress.animateTo(if (open) 1f else 0f, tween(if (open) 420 else 260, easing = if (open) Enter else Exit))
        }
    }
    // Predictive back: the top layer follows the back gesture, and goes
    // once it is let go (or comes back when cancelled).
    PredictiveBackHandler(nav.canGoBack) { events ->
        try {
            events.collect { event -> nav.backProgress = event.progress }
            nav.back()
        } finally {
            nav.backProgress = 0f
        }
    }
    // The drawer follows the back gesture too.
    LaunchedEffect(nav.backProgress) {
        if (nav.top == "drawer" && nav.backProgress > 0f) progress.snapTo(1f - nav.backProgress)
        else if (nav.top == "drawer" && nav.drawer && !dragging) progress.animateTo(1f, tween(200, easing = Enter))
    }
    val pages = nav.pages.toList()
    val leaving = nav.leavingPages.toList()
    val threadPage = (pages + leaving.map { it.page }).any { it is Page.Thread }
    val density = LocalDensity.current
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val width = with(density) { maxWidth.toPx() }
        val edge = with(density) { 28.dp.toPx() }
        val gestures = Modifier.pointerInput(Unit) {
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                if (nav.pages.isNotEmpty() || nav.sheet != null) return@awaitEachGesture
                val open = nav.drawer
                if (!open && down.position.x > edge) return@awaitEachGesture
                val tracker = VelocityTracker()
                var started = false
                while (true) {
                    val event = awaitPointerEvent(PointerEventPass.Initial)
                    val change = event.changes.firstOrNull { it.id == down.id } ?: break
                    tracker.addPosition(change.uptimeMillis, change.position)
                    val dx = change.position.x - down.position.x
                    val dy = change.position.y - down.position.y
                    if (!change.pressed) {
                        if (started) {
                            // A flick decides, else where the drawer is.
                            val speed = tracker.calculateVelocity().x
                            val at = progress.value
                            val toOpen = if (abs(speed) > 350f * density.density) speed > 0 else at > .5f
                            dragging = false
                            scope.launch {
                                progress.animateTo(
                                    if (toOpen) 1f else 0f,
                                    tween(if (toOpen) 420 else 260, easing = if (toOpen) Enter else Exit),
                                )
                            }
                            if (toOpen && !open) nav.openDrawer()
                            if (!toOpen && open) nav.back()
                        }
                        break
                    }
                    if (!started) {
                        if (abs(dy) > 12 * density.density && abs(dy) > abs(dx)) break
                        if (abs(dx) < 10 * density.density) continue
                        started = true
                        dragging = true
                    }
                    change.consume()
                    val at = ((if (open) 1f else 0f) + dx / width).coerceIn(0f, 1f)
                    scope.launch { progress.snapTo(at) }
                }
            }
        }
        Box(Modifier.fillMaxSize().then(gestures)) {
            // The base view moves and dims with the drawer.
            Box(
                Modifier.fillMaxSize().graphicsLayer {
                    translationX = progress.value * .18f * width
                    alpha = 1f - progress.value * .6f
                },
            ) {
                val selected = shell.selected?.takeIf { !threadPage }
                if (selected != null) {
                    androidx.compose.runtime.key(selected) { ThreadView(selected, page = false) }
                } else {
                    Home()
                }
            }
            for ((index, page) in pages.withIndex()) {
                androidx.compose.runtime.key(index, page) { PageLayer(page, leaving = false, top = index == pages.lastIndex) }
            }
            for (out in leaving) {
                androidx.compose.runtime.key(out.index, out.page) { PageLayer(out.page, leaving = true, top = false) }
            }
            NetBar(net, shell.host, shell.attempts.toInt())
            Box(
                Modifier.fillMaxSize().graphicsLayer {
                    translationX = (progress.value - 1f) * width
                },
            ) {
                // Painted beside the screen while closed: its first opening
                // has nothing left to draw.
                Drawer(open = nav.drawer || progress.value > 0f)
            }
            Sheets()
        }
    }
}

// A page sliding in from the right (380ms), out quicker (240ms).
@Composable
private fun PageLayer(page: Page, leaving: Boolean, top: Boolean) {
    val nav = LocalModel.current.nav
    val shift = remember { Animatable(1f) }
    LaunchedEffect(leaving) {
        if (leaving) shift.animateTo(1f, tween(EXIT_MS.toInt(), easing = Exit))
        else shift.animateTo(0f, tween(380, easing = Enter))
    }
    // The top page follows the back gesture, and settles back if it is
    // cancelled.
    val back = if (top && nav.top == "page") nav.backProgress else 0f
    LaunchedEffect(back) {
        if (leaving) return@LaunchedEffect
        if (back > 0f) shift.snapTo(back * .9f)
        else if (shift.value > 0f && shift.targetValue != 0f) shift.animateTo(0f, tween(200, easing = Enter))
    }
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val width = constraints.maxWidth.toFloat()
        Box(
            Modifier
                .fillMaxSize()
                .graphicsLayer { translationX = shift.value * width }
                .shadow(if (shift.value < 1f) 24.dp else 0.dp)
                .background(P.canvas)
                .then(if (leaving) Modifier.pointerInput(Unit) {} else Modifier),
        ) {
            PageView(page)
        }
    }
}

// The scrim and the open (or leaving) sheet.
@Composable
private fun Sheets() {
    val nav = LocalModel.current.nav
    val sheet = nav.sheet ?: nav.leavingSheet ?: return
    val open = nav.sheet != null
    val scrim by animateFloatAsState(if (open) 1f else 0f, tween(if (open) 300 else 220, easing = if (open) Enter else Exit), label = "scrim")
    Box(
        Modifier
            .fillMaxSize()
            .graphicsLayer { alpha = scrim }
            .background(Color.Black.copy(alpha = .55f))
            .then(if (open) Modifier.tap(label = "Close") { nav.back() } else Modifier),
    )
    androidx.compose.runtime.key(sheet::class) { SheetView(sheet, open) }
}

// The connection's floating bar (`#lost`).
@Composable
private fun NetBar(net: Net, host: String, attempts: Int) {
    val model = LocalModel.current
    val on = net is Net.Reconnecting || net is Net.Offline || net is Net.Back
    // The last trouble stays while the bar fades out.
    var shown by remember { mutableStateOf(net) }
    if (on) shown = net
    val fade by animateFloatAsState(if (on) 1f else 0f, tween(200), label = "netbar")
    if (fade == 0f) return
    val shape = RoundedCornerShape(24.dp)
    val bad = shown is Net.Offline
    Box(Modifier.fillMaxWidth().safeBars().padding(top = 70.dp, start = 16.dp, end = 16.dp)) {
        Row(
            Modifier
                .fillMaxWidth()
                .graphicsLayer {
                    alpha = fade
                    translationY = (1f - fade) * -8.dp.toPx()
                }
                .shadow(24.dp, shape, ambientColor = Color.Black.copy(alpha = .4f), spotColor = Color.Black.copy(alpha = .4f))
                .clip(shape)
                .background(P.overlay)
                .border(1.dp, if (bad) P.error.copy(alpha = .35f) else P.input, shape)
                .heightIn(min = 48.dp)
                .padding(start = 16.dp, end = 6.dp, top = 6.dp, bottom = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            when (val n = shown) {
                is Net.Reconnecting -> {
                    Spinner(P.warning, 18.dp)
                    Column(Modifier.weight(1f)) {
                        T("Connection lost. Reconnecting…", 14f)
                        T("Attempt ${n.attempt} of $attempts · $host", 12f, P.muted)
                    }
                }
                is Net.Offline -> {
                    Icon("circle-alert", P.error, size = 18.dp)
                    Column(Modifier.weight(1f)) {
                        T("Can’t reach the server", 14f)
                        T(host, 12f, P.muted)
                    }
                    Btn("Retry", tone = Tone.Primary, height = 36.dp, fontSize = 14f) { model.core.retry() }
                }
                else -> {
                    Icon("check", P.success, size = 18.dp)
                    T("Reconnected", 14f, modifier = Modifier.weight(1f))
                }
            }
        }
    }
}

// The toast at the top: 1.8 s for a confirmation, longer for a longer
// message (an error), up to 7 s; a tap dismisses it.
@Composable
private fun Toast(toast: app.pluto.core.Toast?) {
    val model = LocalModel.current
    var shown by remember { mutableStateOf(toast) }
    if (toast != null) shown = toast
    LaunchedEffect(toast?.serial) {
        val current = toast ?: return@LaunchedEffect
        delay(minOf(1800L + 45L * current.text.length, 7000L))
        model.core.dismissToast(current.serial)
    }
    val on = toast != null
    val fade by animateFloatAsState(if (on) 1f else 0f, tween(220, easing = Enter), label = "toast")
    val drop by animateFloatAsState(if (on) 0f else -12f, tween(320, easing = Enter), label = "toast")
    val text = shown?.text ?: return
    if (fade == 0f && !on) return
    val shape = RoundedCornerShape(20.dp)
    Box(
        Modifier.fillMaxWidth().safeBars(top = 0.dp).padding(top = 84.dp, start = 16.dp, end = 16.dp),
        contentAlignment = Alignment.TopCenter,
    ) {
        Box(
            Modifier
                .graphicsLayer {
                    alpha = fade
                    translationY = drop.dp.toPx()
                }
                .widthIn(max = 448.dp)
                .clip(shape)
                .background(P.overlay)
                .border(1.dp, P.input, shape)
                .then(if (on) Modifier.tap(label = "Dismiss") { toast?.let { model.core.dismissToast(it.serial) } } else Modifier)
                .padding(horizontal = 16.dp, vertical = 10.dp),
        ) {
            T(text, 14f, line = 1.4f, align = androidx.compose.ui.text.style.TextAlign.Center)
        }
    }
}

