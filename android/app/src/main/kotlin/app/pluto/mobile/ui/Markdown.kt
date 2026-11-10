package app.pluto.mobile.ui

// An answer's markdown (`.a` in main.css): the core's blocks
// (`markdown_blocks`) laid out at 16sp on 1.6 lines: paragraphs, headings,
// list items with their marker, quotes, code, rules and tables. Only
// http(s) and mailto links are links; any HTML stays text.

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.pluto.core.Block
import app.pluto.core.TokenKind
import app.pluto.core.CodeToken
import app.pluto.core.BlockKind
import app.pluto.core.Span
import app.pluto.mobile.LocalModel

// `boxed`: the inline code's box is drawn around it (`Paragraph`), else
// its fill marks it.
fun spans(spans: List<Span>, codeSize: Float = 14f, boxed: Boolean = false): AnnotatedString = buildAnnotatedString {
    for (span in spans) {
        val style = SpanStyle(
            fontWeight = if (span.bold) FontWeight.SemiBold else null,
            fontStyle = if (span.italic) androidx.compose.ui.text.font.FontStyle.Italic else null,
            textDecoration = if (span.strike) TextDecoration.LineThrough else null,
            fontFamily = if (span.code) GeistMono else null,
            fontSize = if (span.code) codeSize.sp else androidx.compose.ui.unit.TextUnit.Unspecified,
            background = if (span.code && !boxed) P.chip else androidx.compose.ui.graphics.Color.Unspecified,
        )
        val link = span.link
        if (link != null) {
            withLink(LinkAnnotation.Url(link, TextLinkStyles(SpanStyle(color = P.accentHover)))) {
                withStyle(style) { append(span.text) }
            }
        } else {
            withStyle(style) { append(span.text) }
        }
    }
}

// Browser heading sizes, `.a h3` set to 18.
private fun headingSize(level: Int) = when (level) {
    1 -> 32f
    2 -> 24f
    3 -> 18f
    4 -> 16f
    5 -> 13.3f
    else -> 10.7f
}

@Composable
fun Markdown(blocks: List<Block>, answer: String, modifier: Modifier = Modifier) {
    SelectionContainer(modifier) {
        Column(Modifier.fillMaxWidth()) {
            for ((index, block) in blocks.withIndex()) {
                val next = blocks.getOrNull(index + 1)
                // A list's last item leaves the list's margin under it.
                val listEnd = block.indent > 0u && (next == null || next.indent < block.indent)
                MarkdownBlock(block, answer, listEnd, last = next == null)
            }
        }
    }
}

@Composable
private fun MarkdownBlock(block: Block, answer: String, listEnd: Boolean, last: Boolean) {
    val indent = block.indent.toInt()
    val inList = indent > 0
    val bottom = when {
        last -> 0.dp
        inList && !listEnd -> 8.dp
        else -> when (block.kind) {
            is BlockKind.Code, is BlockKind.Table, is BlockKind.Image -> 14.dp
            is BlockKind.Heading -> 8.dp
            else -> 12.dp
        }
    }
    val top = if (block.kind is BlockKind.Heading) 20.dp else 0.dp
    Row(Modifier.fillMaxWidth().padding(start = (22 * maxOf(0, indent - 1)).dp, top = top, bottom = bottom).height(IntrinsicSize.Min)) {
        repeat(block.quote.toInt()) {
            Box(Modifier.padding(end = 12.dp).width(2.dp).fillMaxHeight().background(P.input))
        }
        if (inList) {
            Box(Modifier.width(22.dp)) {
                block.marker?.let { T(it, 16f, P.muted, line = 1.6f) }
            }
        }
        Box(Modifier.weight(1f)) {
            val color = if (block.quote > 0u) P.muted2 else P.text
            when (val kind = block.kind) {
                is BlockKind.Paragraph -> Paragraph(block.spans, style(16f, color = color, line = 1.6f))
                is BlockKind.Heading -> Paragraph(
                    block.spans,
                    style(headingSize(kind.level.toInt()), FontWeight.SemiBold, color, line = 1.35f, track = if (kind.level.toInt() == 3) -.01f else 0f),
                )
                is BlockKind.Code -> CodeBlock(kind.language, kind.text, kind.tokens)
                is BlockKind.Rule -> Box(Modifier.padding(vertical = 8.dp).fillMaxWidth().height(1.dp).background(P.input))
                is BlockKind.Table -> Table(kind.header, kind.rows)
                is BlockKind.Image -> MarkdownImage(kind.src, kind.alt, kind.loadsByItself, answer)
            }
        }
    }
}

@Composable
private fun Paragraph(content: List<Span>, style: androidx.compose.ui.text.TextStyle) {
    val text = remember(content) { spans(content, boxed = true) }
    // `.a code`: each inline code sits in a rounded box, its fill and a
    // 1px border, drawn behind its text line by line.
    val codes = remember(content) {
        var at = 0
        content.mapNotNull { span ->
            val range = at until at + span.text.length
            at += span.text.length
            range.takeIf { span.code && !it.isEmpty() }
        }
    }
    var layout by remember { androidx.compose.runtime.mutableStateOf<androidx.compose.ui.text.TextLayoutResult?>(null) }
    BasicText(
        text,
        Modifier.drawBehind {
            val laid = layout ?: return@drawBehind
            val pad = 3.dp.toPx()
            val height = 14.sp.toPx() * 1.45f
            val radius = androidx.compose.ui.geometry.CornerRadius(6.dp.toPx())
            for (range in codes) {
                val end = minOf(range.last + 1, laid.layoutInput.text.length)
                if (range.first >= end) continue
                for (line in laid.getLineForOffset(range.first)..laid.getLineForOffset(end - 1)) {
                    val from = maxOf(range.first, laid.getLineStart(line))
                    val to = minOf(end, laid.getLineEnd(line, visibleEnd = true))
                    if (from >= to) continue
                    val left = laid.getHorizontalPosition(from, true) - pad
                    val right = laid.getHorizontalPosition(to, true) + pad
                    val middle = (laid.getLineTop(line) + laid.getLineBottom(line)) / 2f
                    val topLeft = androidx.compose.ui.geometry.Offset(left, middle - height / 2f)
                    val size = androidx.compose.ui.geometry.Size(right - left, height)
                    drawRoundRect(P.chip, topLeft, size, radius)
                    drawRoundRect(P.border, topLeft, size, radius, style = androidx.compose.ui.graphics.drawscope.Stroke(1.dp.toPx()))
                }
            }
        },
        style = style,
        onTextLayout = { layout = it },
    )
}

// The spec's code colors: `.k`, `.s`, `.f` and `.c`.
private fun tokenColor(kind: TokenKind) = when (kind) {
    TokenKind.KEYWORD -> P.accentHover
    TokenKind.STRING -> androidx.compose.ui.graphics.Color(0xFF5EE9B5)
    TokenKind.FUNCTION -> P.info
    TokenKind.COMMENT -> P.icon
}

// A code block (`.code`): its language and a copy button over the code,
// colored by the core, scrolling sideways.
@Composable
fun CodeBlock(language: String, text: String, tokens: List<CodeToken>) {
    val model = app.pluto.mobile.LocalModel.current
    val clipboard = androidx.compose.ui.platform.LocalClipboardManager.current
    val haptic = androidx.compose.ui.platform.LocalHapticFeedback.current
    var copied by remember { androidx.compose.runtime.mutableStateOf(false) }
    androidx.compose.runtime.LaunchedEffect(copied) {
        if (copied) {
            kotlinx.coroutines.delay(1500)
            copied = false
        }
    }
    val colored = remember(text, tokens) {
        buildAnnotatedString {
            append(text)
            for (token in tokens) {
                val end = minOf(token.end.toInt(), text.length)
                if (token.start.toInt() < end) addStyle(SpanStyle(color = tokenColor(token.kind)), token.start.toInt(), end)
            }
        }
    }
    val shape = RoundedCornerShape(16.dp)
    Column(Modifier.fillMaxWidth().clip(shape).background(P.chip).border(1.dp, P.border, shape)) {
        androidx.compose.foundation.layout.Row(
            Modifier.fillMaxWidth().height(40.dp).padding(start = 14.dp, end = 8.dp),
            verticalAlignment = androidx.compose.ui.Alignment.CenterVertically,
        ) {
            T(language.ifEmpty { "code" }, 13f, P.muted, lines = 1, modifier = Modifier.weight(1f))
            Box(
                Modifier
                    .size(36.dp)
                    .clip(RoundedCornerShape(10.dp))
                    .tap(label = "Copy code") {
                        clipboard.setText(AnnotatedString(text))
                        haptic.performHapticFeedback(androidx.compose.ui.hapticfeedback.HapticFeedbackType.Confirm)
                        copied = true
                        model.core.toast("Copied")
                    },
                contentAlignment = androidx.compose.ui.Alignment.Center,
            ) {
                androidx.compose.animation.Crossfade(copied, label = "copy") { done ->
                    Icon(if (done) "check" else "copy", if (done) P.success else P.muted, size = 18.dp)
                }
            }
        }
        Box(
            Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState())
                .padding(start = 14.dp, end = 14.dp, bottom = 14.dp),
        ) {
            BasicText(colored, style = style(13f, color = P.text, line = 1.6f, mono = true), softWrap = false)
        }
    }
}

@Composable
private fun Table(header: List<List<Span>>, rows: List<List<List<Span>>>) {
    val shape = RoundedCornerShape(14.dp)
    Box(
        Modifier
            .clip(shape)
            .border(1.dp, P.border, shape)
            .horizontalScroll(rememberScrollState()),
    ) {
        // Columns as wide as their widest cell.
        Row {
            val columns = maxOf(header.size, rows.maxOfOrNull { it.size } ?: 0)
            for (column in 0 until columns) {
                Column(Modifier.width(IntrinsicSize.Max)) {
                    Cell(header.getOrNull(column).orEmpty(), head = true, bottom = rows.isNotEmpty())
                    for ((index, row) in rows.withIndex()) {
                        Cell(row.getOrNull(column).orEmpty(), head = false, bottom = index < rows.lastIndex)
                    }
                }
            }
        }
    }
}

@Composable
private fun Cell(content: List<Span>, head: Boolean, bottom: Boolean) {
    Column(Modifier.fillMaxWidth().background(if (head) P.chip else androidx.compose.ui.graphics.Color.Transparent)) {
        Box(Modifier.padding(horizontal = 12.dp, vertical = 10.dp)) {
            BasicText(
                remember(content) { spans(content, 13f) },
                style = style(14f, if (head) FontWeight.SemiBold else FontWeight.Normal, if (head) P.muted2 else P.text, line = 1.5f),
                softWrap = false,
            )
        }
        if (bottom) Box(Modifier.fillMaxWidth().height(1.dp).background(P.border))
    }
}


// An answer's image (`![alt](src)`): as wide as the answer at its own
// ratio once loaded, a tap opens it full screen; until then (or when it
// cannot load) its alt text beside an image icon. A remote image outside the
// answer's sources waits for a tap on that row (ADR-0022), and so does one the
// rule refused while loading it. The tap is kept (`rememberSaveable`) when the
// row scrolls away and comes back.
@Composable
private fun MarkdownImage(src: String, alt: String, loadsByItself: Boolean, answer: String) {
    val model = LocalModel.current
    val name = alt.ifBlank { src.substringAfterLast('/') }
    var tapped by rememberSaveable(src) { mutableStateOf(false) }
    // Keyed by the tap, so that a tap after a refusal starts the fetch at once
    // rather than at the core's next change.
    val viewed = if (loadsByItself || tapped) {
        androidx.compose.runtime.key(tapped) {
            rememberImage("markdown:$src", name) { model.core.markdownImage(src, answer, tapped) }
        }
    } else {
        null
    }
    val bitmap = viewed?.bitmap
    val shape = RoundedCornerShape(16.dp)
    if (bitmap != null) {
        Box(
            Modifier
                .fillMaxWidth()
                .aspectRatio(bitmap.width.toFloat() / bitmap.height)
                .clip(shape)
                .tap(label = "Open $name") { model.viewer = viewed },
        ) {
            Image(bitmap, name, Modifier.fillMaxWidth(), contentScale = ContentScale.FillWidth)
        }
    } else {
        val refused = loadsByItself && !tapped &&
            remember(model.revision, src) { model.core.markdownImageRefused(src) }
        val waiting = (!loadsByItself && !tapped) || refused
        val host = android.net.Uri.parse(if (src.startsWith("//")) "https:$src" else src).host
        val label = if (waiting && host != null) "Load image from $host · $name" else name
        Row(
            Modifier
                .heightIn(min = 40.dp)
                .card(P.chip, shape)
                .tap(enabled = waiting, label = "Load $name") { tapped = true }
                .padding(horizontal = 14.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Icon("image", P.muted, size = 16.dp)
            T(label, 14f, P.muted2, lines = 2)
        }
    }
}
