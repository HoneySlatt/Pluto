package app.pluto.mobile.ui

// The changed files of a thread (`#diff`): a turn's checkpoint diff, or
// every turn of the thread, parsed by the core. Unified with one
// line-number gutter, a chip per file to jump to it, files collapsed or
// expanded after "Default diff file state", and a wrap-lines toggle. Each
// file's header stays at the top while its lines scroll under it, and the
// words a changed line changed are marked.

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.SnapshotStateList
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.pluto.core.DiffFile
import app.pluto.core.DiffHunk
import app.pluto.core.DiffLine
import app.pluto.core.DiffLineKind
import app.pluto.core.DiffState
import app.pluto.mobile.LocalModel
import app.pluto.mobile.safeBars
import kotlinx.coroutines.launch

private val addTint = Color(0x1700D492)
private val delTint = Color(0x17FF6467)
private val addMark = Color(0x4000D492)
private val delMark = Color(0x40FF6467)
private val hunkTint = Color(0x0F51A2FF)

@OptIn(ExperimentalFoundationApi::class)
@Composable
fun DiffPage(thread: String, run: String?, focus: String?) {
    val model = LocalModel.current
    val core = model.core
    val nav = model.nav
    val view = remember(model.revision, thread, run) { core.diff(thread, run) }
    var wrap by rememberSaveable { mutableStateOf(false) }
    // The files open; set once from the setting when they arrive.
    val open: SnapshotStateList<String> = remember { mutableStateListOf() }
    var opened by remember { mutableStateOf(false) }
    val list = rememberLazyListState()
    val scope = rememberCoroutineScope()
    // The turn range is asked for once the thread's projection has its
    // checkpoints.
    LaunchedEffect(view.key) { core.askDiff(thread, run) }
    // The files, asked once per reply (the view carries their count).
    val count = (view.state as? DiffState.Files)?.count ?: 0u
    val files = remember(view.key, count) {
        val key = view.key
        if (count > 0u && key != null) core.diffFiles(key) else emptyList()
    }
    // Where each file's header sits in the list.
    val headers = headerIndices(files, open)
    LaunchedEffect(files.isNotEmpty()) {
        if (files.isEmpty() || opened) return@LaunchedEffect
        opened = true
        if (!view.collapsed) open.addAll(files.map { it.path })
        if (focus != null && files.any { it.path == focus }) {
            if (focus !in open) open.add(focus)
            headerIndices(files, open)[focus]?.let { list.scrollToItem(it) }
        }
    }
    Column(Modifier.fillMaxSize().background(P.canvas).safeBars().imePadding()) {
        Row(
            Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            DiscIcon("chevron-left", "Back") { nav.back() }
            Column(Modifier.weight(1f), horizontalAlignment = Alignment.CenterHorizontally) {
                T("${files.size} changed files", 16f, weight = FontWeight.SemiBold, lines = 1, line = 1.4f)
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    T("+${view.additions}", 13f, P.add)
                    T("−${view.deletions}", 13f, P.del)
                    view.branch?.let { branch ->
                        T("·", 13f, P.muted.copy(alpha = .5f))
                        Icon("git-branch", P.muted, size = 13.dp)
                        T(branch, 13f, P.muted, lines = 1)
                    }
                }
            }
            DiscIcon("text-wrap", "Wrap lines", on = wrap) { wrap = !wrap }
        }
        when (val state = view.state) {
            DiffState.NoChanges -> EmptyPage("", "No changes in this thread yet.")
            DiffState.Loading -> LoadingNote("Loading the diff…")
            is DiffState.Failed -> EmptyPage("Couldn’t load the diff", state.error)
            DiffState.Empty -> EmptyPage("", "No changes in this turn.")
            is DiffState.Files -> {
                Row(
                    Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 12.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    for (file in files) {
                        val shape = RoundedCornerShape(20.dp)
                        Row(
                            Modifier
                                .height(40.dp)
                                .card(P.chip, shape)
                                .tap {
                                    if (file.path !in open) open.add(file.path)
                                    scope.launch {
                                        // The list takes the opened file's lines first.
                                        kotlinx.coroutines.yield()
                                        headers[file.path]?.let { list.animateScrollToItem(it) }
                                    }
                                }
                                .padding(horizontal = 14.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            T(file.name, 13f, P.muted2, mono = true, lines = 1, line = 1.2f)
                            T("+${file.additions}", 12f, P.add, mono = true, line = 1.2f)
                            T("−${file.deletions}", 12f, P.del, mono = true, line = 1.2f)
                        }
                    }
                }
                LazyColumn(
                    Modifier.weight(1f).fillMaxWidth(),
                    state = list,
                    contentPadding = androidx.compose.foundation.layout.PaddingValues(bottom = 32.dp),
                ) {
                    for (file in files) {
                        val expanded = file.path in open
                        stickyHeader(key = "h:${file.path}") {
                            FileHeader(file, expanded) {
                                if (!open.remove(file.path)) open.add(file.path)
                            }
                        }
                        if (expanded) {
                            if (file.binary && file.hunks.isEmpty()) {
                                item(key = "b:${file.path}") {
                                    T("Binary file", 12f, P.muted, mono = true, modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp))
                                }
                            } else if (file.hunks.isEmpty()) {
                                item(key = "e:${file.path}") { Box(Modifier.height(8.dp)) }
                            }
                            for ((index, hunk) in file.hunks.withIndex()) {
                                item(key = "k:${file.path}:$index") { HunkView(hunk, wrap) }
                            }
                        }
                    }
                }
            }
        }
    }
}

// The list index of each file's header: a file takes its header, then a
// row per hunk (at least one) while open.
private fun headerIndices(files: List<DiffFile>, open: List<String>): Map<String, Int> {
    val out = HashMap<String, Int>()
    var index = 0
    for (file in files) {
        out[file.path] = index
        index += 1 + if (file.path in open) maxOf(1, file.hunks.size) else 0
    }
    return out
}

// `.dfh`: the file's header, its folder dimmed, folding its lines.
@Composable
private fun FileHeader(file: DiffFile, expanded: Boolean, onToggle: () -> Unit) {
    val turn by animateFloatAsState(if (expanded) 0f else -90f, tween(200), label = "dfh")
    Column(Modifier.fillMaxWidth().background(P.canvas)) {
        Box(Modifier.fillMaxWidth().height(1.dp).background(P.border))
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = 52.dp)
                .tap(onClick = onToggle)
                .padding(horizontal = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Icon("chevron-down", P.muted, Modifier.rotate(turn), size = 16.dp)
            val path = buildAnnotatedString {
                pushStyle(SpanStyle(color = P.muted))
                append(file.dir)
                pop()
                append(file.name)
            }
            BasicText(
                path,
                Modifier.weight(1f),
                style = style(13f, color = P.text, mono = true, line = 1.4f),
                maxLines = 1,
                overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis,
            )
            T("+${file.additions}", 13f, P.add, mono = true)
            T("−${file.deletions}", 13f, P.del, mono = true)
        }
    }
}

// A hunk: its `@@` line, then its lines; unwrapped, it scrolls sideways
// as one block, every line as wide as the widest.
@Composable
private fun HunkView(hunk: DiffHunk, wrap: Boolean) {
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val lines = @Composable {
            Box(Modifier.fillMaxWidth().background(hunkTint).padding(horizontal = 16.dp, vertical = 8.dp)) {
                T(hunk.header, 12f, P.info, mono = true, lines = 1, line = 1.4f)
            }
            for (line in hunk.lines) LineView(line, wrap)
        }
        val viewport = maxWidth
        if (wrap) {
            Column(Modifier.fillMaxWidth().padding(bottom = 8.dp)) { lines() }
        } else {
            Box(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState())) {
                Column(Modifier.widthIn(min = viewport).width(IntrinsicSize.Max).padding(bottom = 8.dp)) { lines() }
            }
        }
    }
}

@Composable
private fun LineView(line: DiffLine, wrap: Boolean) {
    val look = when (line.kind) {
        DiffLineKind.ADDITION -> LineLook(addTint, addMark, "+", P.add, P.text)
        DiffLineKind.DELETION -> LineLook(delTint, delMark, "−", P.del, P.text)
        DiffLineKind.CONTEXT -> LineLook(Color.Transparent, Color.Transparent, "", P.icon, P.muted)
    }
    val code = remember(line) {
        buildAnnotatedString {
            append(line.text.ifEmpty { " " })
            for (span in line.emphasis) {
                val end = minOf(span.end.toInt(), line.text.length)
                if (span.start.toInt() < end) {
                    addStyle(SpanStyle(background = look.mark), span.start.toInt(), end)
                }
            }
        }
    }
    val lineStyle = style(12.5f, color = look.text, line = 1.65f, mono = true)
    Row(Modifier.fillMaxWidth().background(look.fill)) {
        T(
            line.number.toString(),
            12.5f,
            look.gutter,
            mono = true,
            line = 1.65f,
            align = TextAlign.End,
            modifier = Modifier.width(44.dp).padding(end = 10.dp),
        )
        T(look.sign, 12.5f, look.gutter, mono = true, line = 1.65f, modifier = Modifier.width(16.dp))
        BasicText(
            code,
            if (wrap) Modifier.weight(1f).padding(end = 16.dp) else Modifier.padding(end = 16.dp),
            style = lineStyle,
            softWrap = wrap,
        )
    }
}

// A line's colors by its kind: its fill, its changed words' mark, its
// sign, its gutter and its text.
private data class LineLook(val fill: Color, val mark: Color, val sign: String, val gutter: Color, val text: Color)
