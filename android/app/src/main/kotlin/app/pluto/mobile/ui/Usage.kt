package app.pluto.mobile.ui

// Usage (`#usage`): Cost | Tokens | Limits, Limits first. Cost and Tokens
// show the period's total, a daily (or hourly) bar chart and the split by
// provider; the chart's bars rise in, and a finger on the chart scrubs
// across them with the value of the one under it. Limits are the
// providers' quota windows, one card per provider with a meter per window.
// The core (usage.rs) asks the server and merges what it answers.

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
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
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import app.pluto.core.LimitCard
import app.pluto.core.UsageBar
import app.pluto.core.UsageShare
import app.pluto.core.UsageSummary
import app.pluto.mobile.LocalModel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private const val LIMITS = 2

/// Figures in tabular digits (`font-variant-numeric: tabular-nums`).
@Composable
private fun Num(text: String, size: Float, color: Color = P.text, weight: FontWeight = FontWeight.Normal, line: Float = 1.5f, track: Float = 0f, modifier: Modifier = Modifier) {
    BasicText(text, modifier, style = style(size, weight, color, line, track).copy(fontFeatureSettings = "tnum"), maxLines = 1)
}

@Composable
fun UsagePage() {
    val model = LocalModel.current
    val core = model.core
    // The page's windows start from its opening.
    val periods = remember {
        core.usageOpen()
        core.usagePeriods()
    }
    val zone = remember { java.util.TimeZone.getDefault().id }
    var metric by rememberSaveable { mutableIntStateOf(LIMITS) }
    var period by rememberSaveable { mutableIntStateOf(2) }
    var spinning by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(metric, period) {
        if (metric != LIMITS) core.usageFollow(period.toUInt(), zone)
    }
    PageFrame(
        "Usage",
        action = {
            Disc("Refresh", onClick = {
                spinning = true
                core.usageRefresh(period.toUInt(), metric == LIMITS, zone)
                scope.launch {
                    delay(800)
                    spinning = false
                }
            }) {
                if (spinning) Spinner(P.text, name = "refresh-cw") else Icon("refresh-cw", P.text)
            }
        },
    ) {
        Seg(
            listOf("0" to "Cost", "1" to "Tokens", "2" to "Limits"),
            metric.toString(),
            Modifier.padding(bottom = 12.dp),
        ) { metric = it.toInt() }
        if (metric == LIMITS) {
            Limits()
        } else {
            Periods(periods, period) { period = it }
            val summary = remember(model.revision, period, metric) {
                core.usageSummary(period.toUInt(), metric == 1, zone)
            }
            when (summary) {
                is UsageSummary.Loading -> LoadingNote("Loading usage…")
                is UsageSummary.Failed -> EmptyPage("Couldn’t load usage", summary.error)
                is UsageSummary.Ready -> Summary(summary)
            }
        }
    }
}

/// `.periods`: the period chips.
@Composable
private fun Periods(labels: List<String>, period: Int, onPick: (Int) -> Unit) {
    Row(
        Modifier.fillMaxWidth().padding(bottom = 16.dp).horizontalScroll(rememberScrollState()),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        for ((index, label) in labels.withIndex()) {
            val on = index == period
            val shape = RoundedCornerShape(18.dp)
            val fill by animateColorAsState(if (on) P.rowActive else P.chip, tween(150), label = "period")
            val color by animateColorAsState(if (on) P.text else P.muted2, tween(150), label = "period")
            Box(
                Modifier
                    .height(36.dp)
                    .card(fill, shape, if (on) P.input else P.border)
                    .tap(role = Role.RadioButton) { onPick(index) }
                    .padding(horizontal = 14.dp),
                contentAlignment = Alignment.Center,
            ) { T(label, 13f, color, lines = 1, line = 1.2f) }
        }
    }
}

@Composable
private fun Summary(summary: UsageSummary.Ready) {
    // `.hero-num`
    Column(Modifier.padding(start = 4.dp, end = 4.dp, top = 4.dp, bottom = 12.dp)) {
        Num(summary.total, 40f, weight = FontWeight.SemiBold, line = 1.1f, track = -.03f)
        T(summary.caption, 14f, P.muted)
    }
    Chart(summary.bars, summary.start, summary.end)
    if (summary.providers.isNotEmpty()) {
        T("By provider", 14f, P.muted, FontWeight.Medium, Modifier.padding(start = 8.dp, end = 8.dp, bottom = 8.dp))
        Group {
            for (share in summary.providers) ShareRow(share)
        }
    }
}

/// The bar chart (`.chart`): the bars rise when the data comes, and the
/// finger scrubs across them, the one under it lit with its value above.
@Composable
private fun Chart(bars: List<UsageBar>, start: String, end: String) {
    val haptics = LocalHapticFeedback.current
    var tip by remember(bars) { mutableStateOf<Int?>(null) }
    val rise = remember(bars) { Animatable(0f) }
    LaunchedEffect(bars) { rise.animateTo(1f, tween(520, easing = Enter)) }
    val focus by animateFloatAsState(if (tip != null) 1f else 0f, tween(120), label = "focus")
    val shape = RoundedCornerShape(24.dp)
    Column(
        Modifier
            .padding(bottom = 20.dp)
            .fillMaxWidth()
            .card(P.surface, shape)
            .padding(start = 12.dp, end = 12.dp, top = 16.dp, bottom = 8.dp),
    ) {
        BoxWithConstraints(Modifier.fillMaxWidth().height(168.dp)) {
            val count = bars.size.coerceAtLeast(1)
            val width = constraints.maxWidth.toFloat()
            val index = { x: Float -> ((x / width) * count).toInt().coerceIn(0, count - 1) }
            Canvas(
                Modifier
                    .fillMaxWidth()
                    .height(168.dp)
                    .pointerInput(bars) {
                        awaitEachGesture {
                            val down = awaitFirstDown()
                            val first = index(down.position.x)
                            // A tap on the lit bar puts it out; a drag scrubs.
                            val wasLit = tip == first
                            if (!wasLit) haptics.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                            tip = first
                            var moved = false
                            while (true) {
                                val event = awaitPointerEvent()
                                val change = event.changes.firstOrNull { it.id == down.id } ?: break
                                if (!change.pressed) break
                                val at = index(change.position.x)
                                if (at != first || moved) {
                                    moved = true
                                    change.consume()
                                }
                                if (at != tip) {
                                    tip = at
                                    haptics.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                                }
                            }
                            if (wasLit && !moved) tip = null
                        }
                    },
            ) {
                val h = size.height
                val top = 8.dp.toPx()
                val line = 1.dp.toPx()
                drawLine(P.border, Offset(0f, h - (h - top) * .5f), Offset(size.width, h - (h - top) * .5f), line)
                drawLine(P.border, Offset(0f, top), Offset(size.width, top), line)
                drawLine(P.input, Offset(0f, h), Offset(size.width, h), line)
                val max = (bars.maxOfOrNull { it.value } ?: 0.0) * 1.1
                val n = bars.size.coerceAtLeast(1).toFloat()
                val gap = (if (n > 40f) 1f else 2f) * size.width / 334f
                val bw = (size.width - gap * (n - 1f)) / n
                val r = minOf(bw / 2f, 4.dp.toPx())
                for ((i, bar) in bars.withIndex()) {
                    val full = if (max > 0.0) ((h - top) * bar.value / max).toFloat().coerceAtLeast(2f) else 2f
                    val bh = (full * rise.value).coerceAtLeast(2f)
                    val x = i * (bw + gap)
                    val alpha = when {
                        tip == i -> .85f + .15f * focus
                        else -> .85f - .40f * focus
                    }
                    val path = Path().apply {
                        addRoundRect(
                            RoundRect(
                                left = x,
                                top = h - bh,
                                right = x + bw,
                                bottom = h,
                                topLeftCornerRadius = CornerRadius(r, r),
                                topRightCornerRadius = CornerRadius(r, r),
                                bottomLeftCornerRadius = CornerRadius.Zero,
                                bottomRightCornerRadius = CornerRadius.Zero,
                            ),
                        )
                    }
                    drawPath(path, P.accent.copy(alpha = alpha))
                }
            }
            // The value of the bar under the finger, kept inside the chart.
            val shown = tip?.let { bars.getOrNull(it) }
            if (shown != null || focus > 0f) {
                val lit = tip ?: 0
                val center = ((lit + .5f) / count).coerceIn(.18f, .82f) * width
                val tipShape = RoundedCornerShape(10.dp)
                Box(
                    Modifier
                        .layout { measurable, constraints ->
                            val placeable = measurable.measure(constraints.copy(minWidth = 0))
                            layout(placeable.width, placeable.height) {
                                placeable.place((center - placeable.width / 2f).toInt(), 0)
                            }
                        }
                        .graphicsLayer { alpha = focus }
                        .card(P.overlay, tipShape, P.input)
                        .padding(horizontal = 10.dp, vertical = 6.dp),
                ) {
                    val bar = bars.getOrNull(lit)
                    if (bar != null) {
                        BasicText(
                            buildAnnotatedString {
                                withStyle(SpanStyle(color = P.muted)) { append(bar.label) }
                                append(" ")
                                withStyle(SpanStyle(fontWeight = FontWeight.SemiBold)) { append(bar.shown) }
                            },
                            style = style(12f, line = 1.4f).copy(fontFeatureSettings = "tnum"),
                            maxLines = 1,
                        )
                    }
                }
            }
        }
        Row(Modifier.fillMaxWidth().padding(start = 2.dp, end = 2.dp, top = 6.dp)) {
            Num(start, 11f, P.muted)
            Grow()
            Num(end, 11f, P.muted)
        }
    }
}

/// A provider's row of "By provider" (`.brk .prow`), its share as a bar.
@Composable
private fun ShareRow(share: UsageShare) {
    val grow = remember(share.share) { Animatable(0f) }
    LaunchedEffect(share.share) { grow.animateTo((share.share / 100.0).toFloat().coerceIn(0f, 1f), tween(520, easing = Enter)) }
    Row(
        Modifier.fillMaxWidth().heightIn(min = 60.dp).background(P.overlay).padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        share.mark?.let { Mark(it, size = 22.dp) }
        Column(Modifier.weight(1f)) {
            T(share.name, 16f, weight = FontWeight.Medium, lines = 1)
            Box(
                Modifier
                    .padding(top = 6.dp)
                    .fillMaxWidth()
                    .height(4.dp)
                    .clip(RoundedCornerShape(2.dp))
                    .background(P.input),
            ) {
                Box(Modifier.fillMaxWidth(grow.value).height(4.dp).clip(RoundedCornerShape(2.dp)).background(P.accent))
            }
        }
        Num(share.shown, 15f)
    }
}

@Composable
private fun Limits() {
    val model = LocalModel.current
    val cards = remember(model.revision) { model.core.usageLimits() }
    if (cards.isEmpty()) {
        EmptyPage("No limits reported", "Providers report their quota windows after a refresh.")
    }
    for (card in cards) LimitCardView(card)
    PageNote("Updated when you refresh.")
}

/// `.limit`: a provider's quota windows, a meter each.
@Composable
private fun LimitCardView(card: LimitCard) {
    val shape = RoundedCornerShape(24.dp)
    Column(
        Modifier.padding(bottom = 12.dp).fillMaxWidth().card(P.surface, shape).padding(horizontal = 18.dp, vertical = 16.dp),
    ) {
        Row(
            Modifier.padding(bottom = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            card.mark?.let { Mark(it, size = 22.dp) }
            T(card.name, 16f, weight = FontWeight.Medium, lines = 1, modifier = Modifier.weight(1f))
            card.plan?.let { T(it, 13f, P.muted, lines = 1) }
        }
        if (card.windows.isEmpty()) {
            T("No limits reported.", 14f, P.muted, modifier = Modifier.padding(top = 8.dp))
        }
        for (window in card.windows) {
            Column(Modifier.padding(top = 14.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    T(window.label, 14f, P.muted2, lines = 1, modifier = Modifier.weight(1f))
                    if (window.low) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                            Icon("circle-alert", P.warning, size = 14.dp)
                            T("Low", 12f, P.warning, FontWeight.Medium, line = 1.2f)
                        }
                    }
                    Num("${window.left}% left", 14f, weight = FontWeight.SemiBold)
                }
                val fill = remember(window.left) { Animatable(0f) }
                LaunchedEffect(window.left) { fill.animateTo(window.left / 100f, tween(620, easing = Enter)) }
                Box(
                    Modifier
                        .padding(top = 8.dp, bottom = 6.dp)
                        .fillMaxWidth()
                        .height(8.dp)
                        .clip(RoundedCornerShape(4.dp))
                        .background(P.input),
                ) {
                    Box(
                        Modifier
                            .fillMaxWidth(fill.value.coerceIn(0f, 1f))
                            .height(8.dp)
                            .clip(RoundedCornerShape(4.dp))
                            .background(if (window.low) P.warning else P.accent),
                    )
                }
                if (window.resets.isNotEmpty()) Num(window.resets, 13f, P.muted)
            }
        }
    }
}
