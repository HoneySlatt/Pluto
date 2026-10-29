package app.pluto.mobile.ui

// An answer's markdown (`.a` in main.css): the core's blocks
// (`markdown_blocks`) laid out at 16sp on 1.6 lines: paragraphs, headings,
// list items with their marker, quotes, code, rules and tables. Only
// http(s) and mailto links are links; any HTML stays text.

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
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
import app.pluto.core.BlockKind
import app.pluto.core.Span

fun spans(spans: List<Span>, codeSize: Float = 14f): AnnotatedString = buildAnnotatedString {
    for (span in spans) {
        val style = SpanStyle(
            fontWeight = if (span.bold) FontWeight.SemiBold else null,
            fontStyle = if (span.italic) androidx.compose.ui.text.font.FontStyle.Italic else null,
            textDecoration = if (span.strike) TextDecoration.LineThrough else null,
            fontFamily = if (span.code) GeistMono else null,
            fontSize = if (span.code) codeSize.sp else androidx.compose.ui.unit.TextUnit.Unspecified,
            background = if (span.code) P.chip else androidx.compose.ui.graphics.Color.Unspecified,
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
fun Markdown(blocks: List<Block>, modifier: Modifier = Modifier) {
    SelectionContainer(modifier) {
        Column(Modifier.fillMaxWidth()) {
            for ((index, block) in blocks.withIndex()) {
                val next = blocks.getOrNull(index + 1)
                // A list's last item leaves the list's margin under it.
                val listEnd = block.indent > 0u && (next == null || next.indent < block.indent)
                MarkdownBlock(block, listEnd, last = next == null)
            }
        }
    }
}

@Composable
private fun MarkdownBlock(block: Block, listEnd: Boolean, last: Boolean) {
    val indent = block.indent.toInt()
    val inList = indent > 0
    val bottom = when {
        last -> 0.dp
        inList && !listEnd -> 8.dp
        else -> when (block.kind) {
            is BlockKind.Code, is BlockKind.Table -> 14.dp
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
                is BlockKind.Code -> CodeBlock(kind.text)
                is BlockKind.Rule -> Box(Modifier.padding(vertical = 8.dp).fillMaxWidth().height(1.dp).background(P.input))
                is BlockKind.Table -> Table(kind.header, kind.rows)
            }
        }
    }
}

@Composable
private fun Paragraph(content: List<Span>, style: androidx.compose.ui.text.TextStyle) {
    val text = remember(content) { spans(content) }
    BasicText(text, style = style)
}

@Composable
fun CodeBlock(text: String) {
    val shape = RoundedCornerShape(16.dp)
    Box(
        Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(P.chip)
            .border(1.dp, P.border, shape)
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = 14.dp, vertical = 12.dp),
    ) {
        BasicText(text, style = style(13f, color = P.text, line = 1.6f, mono = true), softWrap = false)
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

