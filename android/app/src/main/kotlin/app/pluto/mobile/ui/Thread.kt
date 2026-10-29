package app.pluto.mobile.ui

// A thread (`#thread`): the header with the title and project · branch (or
// the chat's day · model), the timeline the core builds (user bubbles with
// their context chips and images, work folds, answers in markdown, the
// changed-files card, the live work and "Working for …"), the approval and
// question cards, a subagent's request below this thread, then the
// composer and the workspace line. The timeline opens at its end and stays
// there while it grows, unless the user scrolled up; the jump button brings
// it back.

import android.graphics.BitmapFactory
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.pluto.core.ApprovalCard
import app.pluto.core.Changes
import app.pluto.core.ImageRef
import app.pluto.core.Item
import app.pluto.core.QuestionCard
import app.pluto.core.Target
import app.pluto.core.ThreadView
import app.pluto.core.Work
import app.pluto.core.elapsed
import app.pluto.mobile.LocalModel
import app.pluto.mobile.Page
import app.pluto.mobile.Sheet
import app.pluto.mobile.safeBars
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

@Composable
fun ThreadView(id: String, page: Boolean) {
    val model = LocalModel.current
    val core = model.core
    val nav = model.nav
    val view = remember(model.revision, id) { core.thread(id) }
    // Codex's images land on the server's disk without an event: they are
    // asked for while the thread is on screen, often while it works.
    LaunchedEffect(id, view.running, view.loaded) {
        while (view.loaded) {
            core.watchGenerated(id)
            delay(if (view.running) 3_000 else 20_000)
        }
    }
    Box(Modifier.fillMaxSize().background(P.canvas)) {
        Glow(dim = true)
        Column(Modifier.fillMaxSize().safeBars().imePadding()) {
            Header(id, view, page)
            Timeline(id, view, Modifier.weight(1f))
            Composer(Target.Thread(id), "Send a follow-up")
            WorkspaceLine(view)
        }
    }
}

@Composable
private fun Header(id: String, view: ThreadView, page: Boolean) {
    val nav = LocalModel.current.nav
    Row(
        Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (page) DiscIcon("chevron-left", "Back") { nav.back() } else DiscIcon("menu", "Open threads") { nav.openDrawer() }
        Column(Modifier.weight(1f), horizontalAlignment = Alignment.CenterHorizontally) {
            T(view.title, 16f, weight = FontWeight.SemiBold, lines = 1, line = 1.4f)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                val project = view.project
                if (view.chat) {
                    T("${view.day} · ${view.modelLabel}", 13f, P.muted, lines = 1)
                } else if (project != null) {
                    ProjectBadge(project, BadgeMeta)
                    T(project.title, 13f, P.muted, lines = 1)
                    view.branch?.let { branch ->
                        T("·", 13f, P.muted.copy(alpha = .5f))
                        Icon("git-branch", P.muted, size = 13.dp)
                        T(branch, 13f, P.muted, lines = 1)
                    }
                }
            }
        }
        DiscIcon("ellipsis-vertical", "Thread actions", enabled = view.listed) { nav.openSheet(Sheet.ThreadMenu(id)) }
    }
}

// The timeline, newest at the bottom: a reversed list keeps its end in
// place while it grows, and the rows in view while earlier turns load.
@Composable
private fun Timeline(id: String, view: ThreadView, modifier: Modifier) {
    val model = LocalModel.current
    val core = model.core
    val nav = model.nav
    val list = rememberLazyListState()
    val scope = rememberCoroutineScope()
    val away by remember { derivedStateOf { list.firstVisibleItemIndex > 0 || list.firstVisibleItemScrollOffset > 320 } }
    // The clock ticks only while the thread works ("Working for …").
    var now by remember { mutableLongStateOf(System.currentTimeMillis() / 1000) }
    LaunchedEffect(view.running) {
        while (view.running) {
            now = System.currentTimeMillis() / 1000
            delay(1000)
        }
    }
    Box(modifier.fillMaxWidth()) {
        LazyColumn(
            Modifier.fillMaxSize(),
            state = list,
            reverseLayout = true,
            contentPadding = androidx.compose.foundation.layout.PaddingValues(start = 20.dp, end = 20.dp, top = 8.dp, bottom = 24.dp),
        ) {
            view.subagent?.let { child ->
                item(key = "subagent") { SubagentCard(child) }
            }
            for (input in view.questions.reversed()) {
                item(key = "q:${input.requestId}") {
                    Box(Modifier.animateItem(fadeInSpec = tween(260), placementSpec = tween(260, easing = Enter), fadeOutSpec = tween(160))) {
                        QuestionCardView(id, input, view.online)
                    }
                }
            }
            for (approval in view.approvals.reversed()) {
                item(key = "p:${approval.requestId}") {
                    Box(Modifier.animateItem(fadeInSpec = tween(260), placementSpec = tween(260, easing = Enter), fadeOutSpec = tween(160))) {
                        ApprovalCardView(id, approval, view.online)
                    }
                }
            }
            for (item in view.items.asReversed()) {
                item(key = itemKey(item)) {
                    Box(Modifier.animateItem(fadeInSpec = tween(220), placementSpec = tween(260, easing = Enter), fadeOutSpec = tween(160))) {
                        TimelineItem(id, item, now)
                    }
                }
            }
            if (view.partial) {
                item(key = "earlier") {
                    Column(Modifier.fillMaxWidth().padding(top = 4.dp, bottom = 8.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                        TextBtn(
                            when {
                                view.loadingHistory -> "Loading earlier turns…"
                                view.historyError != null -> "Try loading earlier turns again"
                                else -> "Load earlier turns"
                            },
                            enabled = !view.loadingHistory && view.online,
                        ) { core.loadEarlier(id) }
                        view.historyError?.let { T(it, 13f, P.error, align = TextAlign.Center, modifier = Modifier.padding(top = 4.dp)) }
                    }
                }
            }
            val failure = view.failure
            when {
                view.gone -> item(key = "state") {
                    ThreadState("This thread is not available", "It was deleted, or it belongs to another server.", "Back to the draft", true) { nav.goHome() }
                }
                failure != null && !view.loaded -> item(key = "state") {
                    ThreadState("Could not open this thread", failure, "Retry", view.online) { core.followAgain(id) }
                }
                !view.loaded -> item(key = "state") {
                    T(
                        if (view.launching) "Starting…" else "Loading…",
                        15f,
                        P.muted,
                        align = TextAlign.Center,
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 22.dp, vertical = 40.dp),
                    )
                }
            }
        }
        // The jump button: back to the end, following it again.
        val shown by animateFloatAsState(if (away) 1f else 0f, tween(180), label = "tolatest")
        if (shown > 0f) {
            Box(
                Modifier
                    .align(Alignment.BottomCenter)
                    .graphicsLayer {
                        alpha = shown
                        translationY = (12f - 24f * shown) * density
                    }
                    .size(44.dp)
                    .clip(CircleShape)
                    .background(P.overlay)
                    .border(1.dp, P.input, CircleShape)
                    .tap(label = "Jump to the latest message") { scope.launch { list.animateScrollToItem(0) } },
                contentAlignment = Alignment.Center,
            ) { Icon("arrow-down", P.text) }
        }
    }
}

private fun itemKey(item: Item): String = when (item) {
    is Item.User -> item.key
    is Item.Answer -> item.key
    is Item.Fold -> item.key
    is Item.Works -> item.key
    is Item.Working -> item.key
    is Item.Thinking -> item.key
    is Item.Note -> item.key
    is Item.Image -> item.key
}

@Composable
private fun ThreadState(title: String, text: String, action: String, enabled: Boolean, onClick: () -> Unit) {
    Column(
        Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        T(title, 17f, weight = FontWeight.Medium, align = TextAlign.Center)
        T(text, 15f, P.muted, align = TextAlign.Center)
        Btn(action, enabled = enabled, onClick = onClick)
    }
}

// `.textbtn`: a quiet 40dp button.
@Composable
fun TextBtn(text: String, icon: String? = null, enabled: Boolean = true, onClick: () -> Unit) {
    Row(
        Modifier
            .graphicsLayer { alpha = if (enabled) 1f else .45f }
            .height(40.dp)
            .clip(RoundedCornerShape(12.dp))
            .tap(enabled = enabled, onClick = onClick)
            .padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        if (icon != null) Icon(icon, P.muted2, size = 16.dp)
        T(text, 14f, P.muted2, lines = 1)
    }
}

@Composable
private fun TimelineItem(thread: String, item: Item, now: Long) {
    val model = LocalModel.current
    when (item) {
        is Item.User -> UserMessage(item)
        is Item.Answer -> Column {
            // Asked again only when the answer changed (it streams).
            val blocks = remember(item.key, item.hash) { model.core.answerBlocks(item.key) }
            Markdown(blocks)
            item.changes?.let { ChangesCard(thread, it) }
        }
        is Item.Fold -> {
            val turn by animateFloatAsState(if (item.expanded) 90f else 0f, tween(200), label = "fold")
            val color = if (item.failure) P.error else P.muted
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(vertical = 4.dp)
                    .heightIn(min = 44.dp)
                    .tap { model.core.toggle(item.toggle, item.turn) },
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                T(item.label, 15f, color, modifier = Modifier.weight(1f, fill = false))
                Icon("chevron-right", color, Modifier.rotate(turn), size = 16.dp)
            }
        }
        is Item.Works -> Row(
            Modifier
                .padding(start = 4.dp, top = if (item.live) 8.dp else 0.dp, bottom = 10.dp)
                .height(IntrinsicSize.Min),
        ) {
            Box(Modifier.width(1.dp).fillMaxHeight().background(P.input))
            Column(Modifier.padding(start = 14.dp)) {
                item.thought?.let { WorkLine(Work("thought", it, "brain", null, false, false)) }
                for (entry in item.entries) WorkLine(entry)
            }
        }
        is Item.Working -> {
            val started = item.startedAt
            WorkingRow(if (started != null) "Working for " to elapsed(now - started) else "Working" to null)
        }
        is Item.Thinking -> WorkingRow("Thinking" to null)
        is Item.Image -> GeneratedImage(thread, item.path)
        is Item.Note -> Row(
            Modifier.padding(vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon("info", P.success, size = 16.dp)
            T(item.text, 14f, P.muted)
        }
    }
}

@Composable
private fun WorkLine(work: Work) {
    val color = when {
        work.failed -> P.error
        work.live -> P.text
        else -> P.muted
    }
    Row(
        Modifier.heightIn(min = 34.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (work.live) Spinner(P.accent, 16.dp) else Icon(work.icon, if (work.failed) P.error else P.icon, size = 16.dp)
        T(work.label, 14f, color, lines = 1)
        work.code?.let { T(it, 13f, if (work.failed) P.error else P.muted2, mono = true, lines = 1) }
    }
}

// "Working for 6m 40s" or "Thinking…", with the pulsing orb.
@Composable
private fun WorkingRow(text: Pair<String, String?>) {
    val pulse = rememberInfiniteTransition(label = "orb")
    val phase by pulse.animateFloat(0f, 1f, infiniteRepeatable(tween(600), RepeatMode.Reverse), label = "orb")
    Row(
        Modifier.padding(vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(
            Modifier
                .size(8.dp)
                .graphicsLayer {
                    alpha = 1f - .7f * phase
                    scaleX = 1f - .3f * phase
                    scaleY = 1f - .3f * phase
                }
                .clip(CircleShape)
                .background(P.accent),
        )
        val (label, time) = text
        if (time != null) {
            T(label + time, 15f, P.muted2)
        } else {
            // `.dots`: the ellipsis types itself out.
            val dots by rememberInfiniteTransition(label = "dots").animateFloat(
                0f,
                4f,
                infiniteRepeatable(tween(1400, easing = androidx.compose.animation.core.LinearEasing)),
                label = "dots",
            )
            val shown = dots.toInt().coerceIn(0, 3)
            val line = androidx.compose.ui.text.buildAnnotatedString {
                append(label)
                append(".".repeat(shown))
                // The dots still to come hold their place.
                pushStyle(androidx.compose.ui.text.SpanStyle(color = Color.Transparent))
                append(".".repeat(3 - shown))
                pop()
            }
            androidx.compose.foundation.text.BasicText(line, style = style(15f, color = P.muted2))
        }
    }
}

// The images shown, bounded by their memory (the bitmap and the bytes kept
// for saving), the least recently shown going first.
private val images = object : android.util.LruCache<String, app.pluto.mobile.Viewed>(64 * 1024 * 1024) {
    override fun sizeOf(key: String, value: app.pluto.mobile.Viewed) =
        value.bitmap.width * value.bitmap.height * 4 + value.bytes.size
}

// Leaving the server forgets its images.
fun clearImages() {
    images.evictAll()
}

// Decodes an image at most 2048 px on its longer side (the viewer zooms
// it; the timeline shows it smaller).
fun decodeImage(bytes: ByteArray): androidx.compose.ui.graphics.ImageBitmap? {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
    var sample = 1
    while (maxOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= 2048) sample *= 2
    val options = BitmapFactory.Options().apply { inSampleSize = sample }
    return BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)?.asImageBitmap()
}

// An image the core fetches (`fetch` returns its bytes once they came),
// decoded off the main thread and kept in `images`; asked again with each
// change until it is there.
@Composable
private fun rememberImage(key: String, name: String, fetch: () -> ByteArray?): app.pluto.mobile.Viewed? {
    val model = LocalModel.current
    val cached = images[key]
    val tick = if (cached == null) model.revision else -1
    val image by androidx.compose.runtime.produceState(cached, key, tick) {
        if (value == null) {
            value = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) {
                fetch()?.let { bytes ->
                    decodeImage(bytes)?.let { app.pluto.mobile.Viewed(it, bytes, name).also { v -> images.put(key, v) } }
                }
            }
        }
    }
    return image
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun UserMessage(item: Item.User) {
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.End) {
        if (item.images.isNotEmpty()) {
            val one = item.images.size == 1
            FlowRow(
                Modifier.padding(top = 20.dp, bottom = 0.dp).width(if (one) 160.dp else 210.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
                maxItemsInEachRow = if (one) 1 else 2,
            ) {
                for (image in item.images) AttachmentImage(image, Modifier.weight(1f))
            }
        }
        if (item.chips.isNotEmpty()) {
            FlowRow(
                Modifier.padding(top = 20.dp),
                horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.End),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                for (chip in item.chips) {
                    Row(
                        Modifier
                            .height(28.dp)
                            .clip(RoundedCornerShape(14.dp))
                            .background(P.accent.copy(alpha = .12f))
                            .padding(horizontal = 10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Icon(chip.icon, P.accentHover, size = 14.dp)
                        T(chip.label, 13f, P.accentHover, lines = 1, line = 1.2f)
                    }
                }
            }
        }
        val nothing = item.text.isEmpty() && item.chips.isEmpty() && item.images.isEmpty()
        if (item.text.isNotEmpty() || nothing) {
            val shape = RoundedCornerShape(24.dp, 24.dp, 8.dp, 24.dp)
            // The chips sit closer to their bubble (`margin: 20px 0 -10px`).
            val top = if (item.chips.isNotEmpty() || item.images.isNotEmpty()) 10.dp else 20.dp
            androidx.compose.foundation.layout.BoxWithConstraints(
                Modifier.fillMaxWidth().padding(top = top, bottom = 16.dp),
                contentAlignment = Alignment.TopEnd,
            ) {
                SelectionContainer {
                    Box(Modifier.widthIn(max = maxWidth * .86f).clip(shape).background(P.message).padding(horizontal = 18.dp, vertical = 12.dp)) {
                        if (nothing) T("Message without text", 16f, P.muted) else T(item.text, 16f)
                    }
                }
            }
        } else {
            Spacer(height = 16.dp)
        }
    }
}

// A message's image (`attachments.rs`): fetched through the core, a
// placeholder while it comes or when the server no longer has it.
@Composable
private fun AttachmentImage(image: ImageRef, modifier: Modifier) {
    val model = LocalModel.current
    val viewed = rememberImage(image.id, image.name) { model.core.attachment(image.id) }
    val bitmap = viewed?.bitmap
    val shape = RoundedCornerShape(14.dp)
    Box(
        modifier
            .aspectRatio(4f / 3f)
            .card(P.chip, shape)
            .then(if (viewed != null) Modifier.tap(label = "Open ${image.name}") { model.viewer = viewed } else Modifier),
        contentAlignment = Alignment.Center,
    ) {
        if (bitmap != null) {
            Image(bitmap, image.name, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
        } else {
            Icon("image", P.muted)
        }
    }
}

/// An image Codex generated: as wide as the answers, at its own ratio, a
/// tap opens it full screen.
@Composable
private fun GeneratedImage(thread: String, path: String) {
    val model = LocalModel.current
    val key = "generated:$path"
    val name = path.substringAfterLast('/')
    val viewed = rememberImage(key, name) { model.core.generatedImage(thread, path) }
    val bitmap = viewed?.bitmap
    val shape = RoundedCornerShape(16.dp)
    Box(
        Modifier
            .padding(top = 4.dp, bottom = 16.dp)
            .fillMaxWidth()
            .then(if (bitmap != null) Modifier.aspectRatio(bitmap.width.toFloat() / bitmap.height) else Modifier.aspectRatio(1f))
            .card(P.chip, shape)
            .then(if (viewed != null) Modifier.tap(label = "Open $name") { model.viewer = viewed } else Modifier),
        contentAlignment = Alignment.Center,
    ) {
        if (bitmap != null) {
            Image(bitmap, name, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
        } else {
            Spinner(P.muted)
        }
    }
}

@Composable
private fun ChangesCard(thread: String, changes: Changes) {
    val nav = LocalModel.current.nav
    val shape = RoundedCornerShape(20.dp)
    Column(Modifier.padding(top = 12.dp, bottom = 16.dp).fillMaxWidth().card(P.surface, shape)) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = 52.dp).padding(start = 16.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            T("${changes.files.size} changed files", 15f, weight = FontWeight.Medium, lines = 1)
            T("+${changes.additions}", 13f, P.add, mono = true)
            T("−${changes.deletions}", 13f, P.del, mono = true)
            Grow()
            TextBtn("Open diff", "file-diff") { nav.openPage(Page.Diff(thread, changes.run, null)) }
        }
        for (file in changes.files) {
            Box(Modifier.fillMaxWidth().height(1.dp).background(P.border))
            Row(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = 44.dp)
                    .tap { nav.openPage(Page.Diff(thread, changes.run, file.path)) }
                    .padding(horizontal = 16.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                // A long path keeps its end (`direction: rtl`).
                T(startEllipsis(file.path), 13f, P.muted2, mono = true, lines = 1, modifier = Modifier.weight(1f))
                T("+${file.additions}", 13f, P.add, mono = true)
                T("−${file.deletions}", 13f, P.del, mono = true)
            }
        }
    }
}

// The end of a path that does not fit: Compose ellipsizes at the end, so
// a long path is cut from its start here.
private fun startEllipsis(path: String, max: Int = 38) =
    if (path.length > max) "…" + path.takeLast(max - 1) else path

// The card of a pending request (`.ask-card`).
@Composable
private fun AskCard(tone: Color, icon: String, title: String, content: @Composable () -> Unit) {
    val shape = RoundedCornerShape(20.dp)
    Column(Modifier.padding(top = 12.dp, bottom = 16.dp).fillMaxWidth().card(P.surface, shape, tone.copy(alpha = .28f))) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = 52.dp).padding(start = 16.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Icon(icon, tone)
            T(title, 15f, tone, FontWeight.Medium, modifier = Modifier.padding(vertical = 12.dp))
        }
        content()
    }
}

// Whether the card's last answer still waits, read now (a second tap in
// the same frame sees the first).
@Composable
private fun waitingCheck(): (String?) -> Boolean {
    val core = LocalModel.current.core
    return { key -> key != null && core.replyState(key).waiting }
}

// The reply to a card's last answer: waiting, or why it failed.
@Composable
private fun answerState(key: String?): Pair<Boolean, String?> {
    val model = LocalModel.current
    if (key == null) return false to null
    val state = remember(model.revision, key) { model.core.replyState(key) }
    return state.waiting to state.error
}

@Composable
private fun ApprovalCardView(thread: String, approval: ApprovalCard, online: Boolean) {
    val core = LocalModel.current.core
    val waiting = waitingCheck()
    val haptic = LocalHapticFeedback.current
    var sent by remember { mutableStateOf<String?>(null) }
    val (busy, error) = answerState(sent)
    val enabled = online && !busy
    AskCard(P.warning, "shield", approval.label) {
        approval.app?.let { T(it, 13f, P.muted, modifier = Modifier.padding(start = 16.dp, end = 16.dp, bottom = 10.dp)) }
        approval.detail?.let { detail ->
            Box(
                Modifier
                    .padding(start = 16.dp, end = 16.dp, bottom = 14.dp)
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(12.dp))
                    .background(P.chip)
                    .horizontalScroll(rememberScrollState())
                    .padding(horizontal = 14.dp, vertical = 12.dp),
            ) { androidx.compose.foundation.text.BasicText(detail, style = style(13f, line = 1.5f, mono = true), softWrap = false) }
        }
        for (warning in approval.warnings) {
            Row(
                Modifier
                    .padding(start = 16.dp, end = 16.dp, bottom = 12.dp)
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(12.dp))
                    .background(P.warning.copy(alpha = .08f))
                    .padding(horizontal = 12.dp, vertical = 10.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Icon("triangle-alert", P.warning, size = 18.dp)
                T(warning, 14f, P.warning)
            }
        }
        error?.let { T(it, 14f, P.error, modifier = Modifier.padding(start = 16.dp, end = 16.dp, bottom = 12.dp)) }
        val dim = if (online) 1f else .45f
        if (approval.pair.isNotEmpty()) {
            Row(
                Modifier.padding(start = 12.dp, end = 12.dp, bottom = 12.dp).fillMaxWidth().alpha(dim),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                for (choice in approval.pair) {
                    Btn(
                        choice.label,
                        Modifier.weight(1f),
                        tone = if (choice.decision == "accept") Tone.Primary else Tone.Plain,
                        enabled = enabled,
                    ) {
                        haptic.performHapticFeedback(HapticFeedbackType.Confirm)
                        if (!waiting(sent)) sent = core.respond(thread, approval.requestId, choice.decision)
                    }
                }
            }
        }
        if (approval.rest.isNotEmpty()) {
            Column(
                Modifier.padding(start = 12.dp, end = 12.dp, bottom = 12.dp).fillMaxWidth().alpha(dim),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                for (choice in approval.rest) {
                    Btn(choice.label, Modifier.fillMaxWidth(), enabled = enabled) {
                        if (!waiting(sent)) sent = core.respond(thread, approval.requestId, choice.decision)
                    }
                }
            }
        }
    }
}

@Composable
private fun QuestionCardView(thread: String, input: QuestionCard, online: Boolean) {
    val core = LocalModel.current.core
    val waiting = waitingCheck()
    var sent by remember { mutableStateOf<String?>(null) }
    val (busy, error) = answerState(sent)
    val enabled = online && !busy
    val dim = if (online) 1f else .45f
    Column {
        for (question in input.questions) {
            AskCard(P.info, "message-circle-question-mark", question.title) {
                for (option in question.options) {
                    val shape = RoundedCornerShape(16.dp)
                    Row(
                        Modifier
                            .padding(start = 12.dp, end = 12.dp, bottom = 8.dp)
                            .fillMaxWidth()
                            .alpha(dim)
                            .heightIn(min = 52.dp)
                            .card(P.overlay, shape, if (option.checked) P.info.copy(alpha = .5f) else P.border)
                            .tap(enabled = enabled) { core.pickAnswer(thread, input.requestId, question.id, option.value) }
                            .padding(horizontal = 16.dp, vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        Box(
                            Modifier.size(20.dp).border(2.dp, if (option.checked) P.info else P.icon, CircleShape),
                            contentAlignment = Alignment.Center,
                        ) {
                            if (option.checked) Box(Modifier.size(10.dp).clip(CircleShape).background(P.info))
                        }
                        Column(Modifier.weight(1f)) {
                            T(option.label, 16f)
                            option.description?.let { T(it, 13f, P.muted) }
                        }
                    }
                }
                if (question.customAllowed) {
                    val shape = RoundedCornerShape(24.dp)
                    Box(
                        Modifier
                            .padding(start = 20.dp, end = 20.dp, top = 4.dp, bottom = 8.dp)
                            .fillMaxWidth()
                            .height(48.dp)
                            .card(P.chip, shape)
                            .padding(horizontal = 16.dp),
                        contentAlignment = Alignment.CenterStart,
                    ) {
                        // The field keeps its text; the core follows it, and
                        // a picked option (which clears it) comes back here.
                        var custom by remember(question.id) { mutableStateOf(question.custom) }
                        LaunchedEffect(question.custom) { if (question.custom != custom) custom = question.custom }
                        BasicTextField(
                            custom,
                            {
                                custom = it
                                core.typeAnswer(input.requestId, question.id, it)
                            },
                            Modifier.fillMaxWidth(),
                            enabled = enabled,
                            singleLine = true,
                            textStyle = style(16f),
                            cursorBrush = SolidColor(P.accent),
                            decorationBox = { field ->
                                if (custom.isEmpty()) T("Your answer", 16f, P.muted, lines = 1)
                                field()
                            },
                        )
                    }
                }
            }
        }
        error?.let { T(it, 14f, P.error, modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp)) }
        Row(
            Modifier.padding(start = 12.dp, end = 12.dp, bottom = 12.dp).fillMaxWidth().alpha(dim),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            if (input.dismissible) {
                Btn("Dismiss", Modifier.weight(1f), enabled = enabled) { if (!waiting(sent)) sent = core.dismissQuestion(thread, input.requestId) }
            }
            Btn(
                if (busy) "Sending…" else "Submit",
                Modifier.weight(1f),
                tone = Tone.Primary,
                enabled = enabled && input.ready,
            ) { if (!waiting(sent)) core.submitAnswers(thread, input.requestId)?.let { sent = it } }
        }
    }
}

@Composable
private fun SubagentCard(child: app.pluto.core.SubagentCard) {
    val nav = LocalModel.current.nav
    AskCard(
        P.info,
        if (child.question) "message-circle-question-mark" else "shield",
        if (child.question) "A subagent needs your answer" else "A subagent needs your approval",
    ) {
        T(child.title, 15f, P.muted2, modifier = Modifier.padding(start = 16.dp, end = 16.dp, bottom = 12.dp))
        Btn(
            "Open the subagent",
            Modifier.padding(start = 12.dp, end = 12.dp, bottom = 12.dp).fillMaxWidth(),
            tone = Tone.Primary,
        ) { nav.openPage(Page.Thread(child.id)) }
    }
}

@Composable
private fun WorkspaceLine(view: ThreadView) {
    val model = LocalModel.current
    Row(
        Modifier.padding(horizontal = 28.dp).fillMaxWidth().padding(start = 4.dp, end = 4.dp, top = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        if (!view.chat) {
            Row(
                Modifier.heightIn(min = 32.dp).clip(RoundedCornerShape(8.dp)).tap {
                    model.core.toast("The workspace of a started thread is set on the computer")
                }.padding(horizontal = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Icon(if (view.worktree) "folder-git-2" else "folder", P.muted, size = 15.dp)
                T(if (view.worktree) "Worktree" else "Local checkout", 13f, P.muted, lines = 1)
            }
            Grow()
            view.branch?.let { branch ->
                Row(
                    Modifier.heightIn(min = 32.dp).clip(RoundedCornerShape(8.dp)).tap { model.nav.openSheet(Sheet.Branch) }.padding(horizontal = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Icon("git-branch", P.muted, size = 15.dp)
                    T(branch, 13f, P.muted, lines = 1, modifier = Modifier.widthIn(max = 180.dp))
                    Icon("chevron-down", P.muted, size = 15.dp)
                }
            }
        }
    }
}
