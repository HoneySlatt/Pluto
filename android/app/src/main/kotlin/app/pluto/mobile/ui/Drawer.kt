package app.pluto.mobile.ui

// The drawer (`#drawer`, Grok's second screen): the connection header, the
// menu (Projects or Memory, Scheduled tasks, Usage), then Code's "Needs
// you" (threads waiting on an approval or an answer), Threads and Settled,
// or Chat's chats by day; Search, Settings and New at the bottom. The list
// pulls down to refresh.

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Velocity
import androidx.compose.ui.unit.dp
import app.pluto.core.Dot
import app.pluto.core.DrawerView
import app.pluto.core.ThreadRow
import app.pluto.mobile.LocalModel
import app.pluto.mobile.Model
import app.pluto.mobile.Page
import app.pluto.mobile.Sheet
import app.pluto.mobile.safeBars
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

// The pull to refresh: the shell is live, so a connected list is current
// once the server answers a probe; an offline one reconnects.
private suspend fun refresh(model: Model) {
    val core = model.core
    val online = core.online()
    if (online) core.probe() else core.retry()
    // The server answers the probe, or misses its six-second deadline; a
    // reconnection tries three times.
    for (tick in 0 until 60) {
        delay(200)
        val now = core.online()
        val settled = if (online) !now || core.probed() else now || core.shell().net is app.pluto.core.Net.Offline
        if (settled) break
    }
    done(model)
}

private fun done(model: Model) {
    model.core.toast(if (model.core.online()) "Threads up to date" else "Can’t reach the server")
}

private const val LIMIT = 64f

@Composable
fun Drawer(open: Boolean) {
    val model = LocalModel.current
    val nav = model.nav
    val core = model.core
    var query by rememberSaveable { mutableStateOf("") }
    var attentionOpen by rememberSaveable { mutableStateOf(true) }
    var threadsOpen by rememberSaveable { mutableStateOf(true) }
    var settledOpen by rememberSaveable { mutableStateOf(false) }
    // The times move on while it is open.
    var minute by remember { mutableIntStateOf(0) }
    LaunchedEffect(open) {
        while (open) {
            delay(30_000)
            minute++
        }
    }
    // Closed, it does not follow every event.
    val revision = if (open) model.revision else -1
    val view = remember(revision, query, minute) { core.drawer(query) }
    val scope = rememberCoroutineScope()
    val density = LocalDensity.current
    val pull = remember { Animatable(0f) }
    var busy by remember { mutableStateOf(false) }
    val list = rememberLazyListState()
    val connection = remember {
        object : NestedScrollConnection {
            override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
                // Pushing back up takes the pull first.
                if (pull.value > 0f && available.y < 0f && !busy) {
                    val px = pull.value * density.density
                    val used = maxOf(available.y, -px)
                    scope.launch { pull.snapTo((px + used) / density.density) }
                    return Offset(0f, used)
                }
                return Offset.Zero
            }

            override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
                if (available.y > 0f && source == NestedScrollSource.UserInput && !busy) {
                    scope.launch { pull.snapTo(minOf(96f, pull.value + available.y * .5f / density.density)) }
                    return Offset(0f, available.y)
                }
                return Offset.Zero
            }

            override suspend fun onPreFling(available: Velocity): Velocity {
                if (pull.value <= 0f || busy) return Velocity.Zero
                if (pull.value < LIMIT) {
                    pull.animateTo(0f, tween(240, easing = Ease))
                } else {
                    busy = true
                    pull.animateTo(LIMIT, tween(240, easing = Ease))
                    refresh(model)
                    pull.animateTo(0f, tween(240, easing = Ease))
                    busy = false
                }
                return available
            }
        }
    }
    Box(Modifier.fillMaxSize().background(P.sidebar)) {
        Column(Modifier.fillMaxSize().safeBars().imePadding()) {
            Header(view)
            Box(Modifier.weight(1f)) {
                val ready = pull.value >= LIMIT
                // The pull's indicator.
                val turn by animateFloatAsState(if (busy) 0f else pull.value * 4f, tween(0), label = "ptr")
                LazyColumn(
                    Modifier
                        .fillMaxSize()
                        .nestedScroll(connection)
                        .graphicsLayer {
                            translationY = pull.value * density.density
                            compositingStrategy = CompositingStrategy.Offscreen
                        }
                        .drawWithContent {
                            drawContent()
                            // The list fades out over its last 28dp.
                            val fade = 28.dp.toPx()
                            drawRect(
                                Brush.verticalGradient(
                                    0f to Color.Black,
                                    1f to Color.Transparent,
                                    startY = size.height - fade,
                                    endY = size.height,
                                ),
                                blendMode = BlendMode.DstIn,
                            )
                        },
                    state = list,
                    contentPadding = androidx.compose.foundation.layout.PaddingValues(top = 8.dp, bottom = 24.dp),
                ) {
                    if (query.isBlank()) item(key = "menu") { Menu(view) }
                    content(
                        view,
                        query,
                        attentionOpen to { attentionOpen = !attentionOpen },
                        threadsOpen to { threadsOpen = !threadsOpen },
                        settledOpen to { settledOpen = !settledOpen },
                    )
                }
                if (pull.value > 0f || busy) {
                    Box(
                        Modifier
                            .align(Alignment.TopCenter)
                            .offset(y = (minOf(pull.value, LIMIT) - 12f).dp + 20.dp)
                            .graphicsLayer { alpha = minOf(pull.value / LIMIT, 1f) }
                            .size(40.dp)
                            .clip(CircleShape)
                            .background(P.overlay)
                            .border(1.dp, P.input, CircleShape),
                        contentAlignment = Alignment.Center,
                    ) {
                        if (busy) {
                            Spinner(P.accent, 20.dp, "refresh-cw")
                        } else {
                            Icon("refresh-cw", if (ready) P.accentHover else P.accent, Modifier.rotate(turn), size = 20.dp)
                        }
                    }
                }
            }
            Footer(view, query, { query = it })
        }
    }
}

@Composable
private fun Header(view: DrawerView) {
    val nav = LocalModel.current.nav
    Row(
        Modifier.fillMaxWidth().height(72.dp).padding(start = 20.dp, end = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Avatar()
        Column(Modifier.weight(1f)) {
            T("Pluto", 20f, weight = FontWeight.SemiBold, line = 1.3f, track = -.01f)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                val dot = when (view.dot) {
                    Dot.OK -> P.success
                    Dot.WARN -> P.warning
                    Dot.BAD -> P.error
                }
                Box(
                    Modifier
                        .size(13.dp)
                        .clip(CircleShape)
                        .background(dot.copy(alpha = .15f)),
                    contentAlignment = Alignment.Center,
                ) { Box(Modifier.size(7.dp).clip(CircleShape).background(dot)) }
                T("${view.host} · ${view.state}", 14f, P.muted, lines = 1)
            }
        }
        DiscIcon("chevrons-right", "Close") { nav.back() }
    }
}

@Composable
private fun Menu(view: DrawerView) {
    val model = LocalModel.current
    val nav = model.nav
    val shape = RoundedCornerShape(24.dp)
    Column(
        Modifier.padding(start = 16.dp, end = 16.dp, top = 8.dp).fillMaxWidth().clip(shape).background(P.surface),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        if (view.chat) {
            MenuRow("brain", "Memory") { nav.openPage(Page.Memory) }
        } else {
            MenuRow("folder-closed", "Projects", view.projects.toString()) { nav.openPage(Page.Projects) }
        }
        MenuRow("calendar-clock", "Scheduled tasks", view.tasks?.toString()) {
            model.core.watchTasks()
            nav.openPage(Page.Tasks)
        }
        MenuRow("chart-no-axes-column", "Usage") { nav.openPage(Page.Usage) }
    }
}

@Composable
private fun MenuRow(icon: String, label: String, count: String? = null, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    Row(
        Modifier
            .fillMaxWidth()
            .height(56.dp)
            .pressRow(interaction, P.overlay)
            .tap(interaction = interaction, onClick = onClick)
            .padding(horizontal = 20.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Icon(icon, P.muted2)
        T(label, 17f, weight = FontWeight.Medium, modifier = Modifier.weight(1f), lines = 1)
        if (count != null) T(count, 14f, P.muted)
    }
}

private fun LazyListScope.content(
    view: DrawerView,
    query: String,
    attention: Pair<Boolean, () -> Unit>,
    threads: Pair<Boolean, () -> Unit>,
    settled: Pair<Boolean, () -> Unit>,
) {
    when {
        !view.listed -> item(key = "loading") { Note("Loading…") }
        view.chat -> for (day in view.days) {
            item(key = "day:${day.label}") { Section(day.label, null, null) }
            for (row in day.rows) item(key = row.id) { TRow(row, view) }
        }
        else -> {
            if (view.attention.isNotEmpty()) {
                item(key = "s:attention") { Section("Needs you", attention.first, attention.second) }
                if (attention.first) for (row in view.attention) item(key = "a:${row.id}") { AttentionRow(row) }
            }
            item(key = "s:threads") { Section("Threads", threads.first, threads.second) }
            if (threads.first) for (row in view.active) item(key = row.id) { TRow(row, view) }
            if (view.settled.isNotEmpty()) {
                item(key = "s:settled") { Section("Settled (${view.settled.size})", settled.first, settled.second) }
                if (settled.first) for (row in view.settled) item(key = row.id) { TRow(row, view) }
            }
        }
    }
    if (view.listed && view.empty) {
        item(key = "empty") {
            Note(
                when {
                    query.isNotBlank() -> "No threads match “$query”."
                    view.chat -> "No chats yet."
                    else -> "No threads yet."
                },
            )
        }
    }
}

@Composable
private fun Note(text: String) {
    T(text, 15f, P.muted, modifier = Modifier.padding(horizontal = 22.dp, vertical = 24.dp))
}

// A section's header; `open == null` for Chat's static days.
@Composable
private fun Section(label: String, open: Boolean?, toggle: (() -> Unit)?) {
    val turn by animateFloatAsState(if (open == false) 180f else 0f, tween(200), label = "section")
    Row(
        Modifier
            .padding(top = 16.dp)
            .fillMaxWidth()
            .height(48.dp)
            .then(if (toggle != null) Modifier.tap(onClick = toggle) else Modifier)
            .padding(horizontal = 22.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        T(label, 15f, P.muted, FontWeight.Medium, Modifier.weight(1f))
        if (open != null) Icon("chevron-up", P.muted, Modifier.rotate(turn), size = 20.dp)
    }
}

@Composable
private fun Pill(status: String) {
    val (icon, color, label) = when (status) {
        "working" -> Triple("loader-circle", P.accent, "Working")
        "approval" -> Triple("shield", P.warning, "Approval")
        else -> Triple("message-circle-question-mark", P.info, "Input")
    }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        if (status == "working") Spinner(color, 14.dp) else Icon(icon, color, size = 14.dp)
        T(label, 13f, color, FontWeight.Medium, lines = 1)
    }
}

@Composable
private fun TRow(row: ThreadRow, view: DrawerView) {
    val model = LocalModel.current
    val nav = model.nav
    val current = view.selected == row.id
    val interaction = remember { MutableInteractionSource() }
    Row(
        Modifier
            .fillMaxWidth()
            .pressRow(interaction, if (current) P.rowActive else Color.Transparent)
            .tap(interaction = interaction) { nav.openThread(row.id) }
            .padding(start = 22.dp, end = 8.dp, top = 12.dp, bottom = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Column(Modifier.weight(1f)) {
            T(row.title, 17f, line = 1.35f, lines = 2)
            Row(
                Modifier.padding(top = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                if (!view.chat) {
                    row.project?.let { project ->
                        ProjectBadge(project, BadgeMeta)
                        T(project.title, 14f, P.muted, lines = 1)
                    }
                    row.branch?.let { branch ->
                        T("·", 14f, P.muted.copy(alpha = .5f))
                        Row(
                            Modifier.weight(1f, fill = false),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(4.dp),
                        ) {
                            Icon("git-branch", P.muted, size = 13.dp)
                            T(branch, 14f, P.muted, lines = 1)
                        }
                    }
                    T("·", 14f, P.muted.copy(alpha = .5f))
                }
                val status = row.status
                if (status != null) Pill(status) else T(row.`when`, 14f, P.muted, lines = 1)
            }
        }
        Box(
            Modifier.offset(y = (-4).dp).size(44.dp).clip(CircleShape).tap(label = "Actions for ${row.title}") {
                nav.openSheet(Sheet.ThreadMenu(row.id))
            },
            contentAlignment = Alignment.Center,
        ) { Icon("ellipsis-vertical", P.muted) }
    }
}

// A "Needs you" row (Grok's "Bots" block): the project disc with its pip,
// the title, the status and what is asked.
@Composable
private fun AttentionRow(row: ThreadRow) {
    val nav = LocalModel.current.nav
    val approval = row.status == "approval"
    val tone = if (approval) P.warning else P.info
    val interaction = remember { MutableInteractionSource() }
    Row(
        Modifier
            .fillMaxWidth()
            .pressRow(interaction)
            .tap(interaction = interaction) { nav.openThread(row.id) }
            .padding(start = 22.dp, end = 22.dp, top = 12.dp, bottom = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(Modifier.size(48.dp)) {
            Box(
                Modifier.size(48.dp).clip(CircleShape).background(tone.copy(alpha = .1f)),
                contentAlignment = Alignment.Center,
            ) { row.project?.let { ProjectBadge(it, BadgeAttention) } }
            Box(
                Modifier
                    .align(Alignment.BottomEnd)
                    .size(18.dp)
                    .clip(CircleShape)
                    .background(P.sidebar)
                    .padding(2.dp)
                    .clip(CircleShape)
                    .background(tone),
                contentAlignment = Alignment.Center,
            ) { T(if (approval) "!" else "?", 11f, P.accentFg, FontWeight.Bold, line = 1f, align = TextAlign.Center) }
        }
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                T(row.title, 17f, weight = FontWeight.Medium, line = 1.35f, lines = 1, modifier = Modifier.weight(1f))
                T(if (approval) "Approval" else "Input", 13f, tone, FontWeight.Medium, lines = 1)
            }
            T(
                if (approval) "Waiting for your approval" else "Waiting for your answer",
                15f,
                P.muted2,
                lines = 1,
                modifier = Modifier.padding(top = 2.dp),
            )
        }
    }
}

@Composable
private fun Footer(view: DrawerView, query: String, onQuery: (String) -> Unit) {
    val nav = LocalModel.current.nav
    Row(
        Modifier.fillMaxWidth().background(P.sidebar).padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        var focused by remember { mutableStateOf(false) }
        val shape = RoundedCornerShape(26.dp)
        Row(
            Modifier
                .weight(1f)
                .height(52.dp)
                .clip(shape)
                .background(P.overlay)
                .border(1.dp, if (focused) P.focus else P.border, shape)
                .padding(horizontal = 18.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Icon("search", if (focused) P.text else P.muted)
            BasicTextField(
                query,
                onQuery,
                Modifier.weight(1f).onFocus { focused = it },
                singleLine = true,
                textStyle = style(17f),
                cursorBrush = androidx.compose.ui.graphics.SolidColor(P.accent),
                decorationBox = { field ->
                    if (query.isEmpty()) T("Search", 17f, P.muted, lines = 1)
                    field()
                },
            )
        }
        DiscIcon("settings", "Settings", size = 52.dp) { nav.openPage(Page.Settings) }
        DiscIcon("square-pen", if (view.chat) "New chat" else "New thread", size = 52.dp) { nav.goHome() }
    }
}

fun Modifier.onFocus(focused: (Boolean) -> Unit) =
    this.onFocusChanged { focused(it.isFocused) }
