package app.pluto.mobile.ui

// The pieces the sheets and pages share (main.css): a sheet's labels, rows,
// choices, segments, notes, errors and actions; a page's scroll, groups,
// setting rows and toggles; and the outcome of a request asked from them.

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pluto.mobile.LocalModel
import app.pluto.mobile.safeBars

// Sheets

/// `.slabel`: a group's label in a sheet.
@Composable
fun SheetLabel(text: String) {
    T(text, 13f, P.muted, FontWeight.Medium, Modifier.padding(start = 24.dp, end = 24.dp, top = 14.dp, bottom = 6.dp))
}

/// `.srow`: an action row of a sheet; `danger` reddens it.
@Composable
fun SRow(
    icon: String?,
    label: String,
    detail: String? = null,
    danger: Boolean = false,
    enabled: Boolean = true,
    trailing: @Composable () -> Unit = {},
    onClick: () -> Unit,
) {
    val interaction = remember { MutableInteractionSource() }
    val color = if (danger) P.error else P.text
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 52.dp)
            .pressRow(interaction)
            .tap(enabled = enabled, interaction = interaction, onClick = onClick)
            .alpha(if (enabled) 1f else .45f)
            .padding(horizontal = 24.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        if (icon != null) Icon(icon, if (danger) P.error else P.muted2)
        Column(Modifier.weight(1f)) {
            T(label, 16f, color)
            if (!detail.isNullOrEmpty()) T(detail, 13f, P.muted)
        }
        trailing()
    }
}

/// A checked row of a choice sheet, with a provider's mark when given.
@Composable
fun Choice(label: String, checked: Boolean, detail: String? = null, mark: String? = null, enabled: Boolean = true, onPick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 52.dp)
            .pressRow(interaction)
            .tap(enabled = enabled, role = Role.RadioButton, interaction = interaction, onClick = onPick)
            .alpha(if (enabled) 1f else .45f)
            .padding(horizontal = 24.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        if (mark != null) Mark(mark, size = 20.dp)
        Column(Modifier.weight(1f)) {
            T(label, 16f)
            if (!detail.isNullOrEmpty()) T(detail, 13f, P.muted)
        }
        Icon("check", if (checked) P.accent else Color.Transparent)
    }
}

/// `.sheet .note`: a quiet line under a sheet's rows.
@Composable
fun SheetNote(text: String) {
    T(text, 13f, P.muted, line = 1.45f, modifier = Modifier.padding(start = 24.dp, end = 24.dp, top = 4.dp, bottom = 8.dp))
}

/// `.sheet .err`: why a sheet's request failed.
@Composable
fun SheetError(text: String) {
    T(text, 14f, P.error, modifier = Modifier.padding(start = 24.dp, end = 24.dp, bottom = 8.dp))
}

/// `.sheet .confirm`: what a confirmation is about.
@Composable
fun SheetConfirm(text: String) {
    T(text, 15f, P.muted2, modifier = Modifier.padding(start = 24.dp, end = 24.dp, bottom = 8.dp))
}

/// `.sheet .actions`: the buttons at the bottom of a sheet, side by side.
@Composable
fun SheetActions(content: @Composable androidx.compose.foundation.layout.RowScope.() -> Unit) {
    Row(
        Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 8.dp, bottom = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        content = content,
    )
}

/// `.seg`: a segmented choice (value, label).
@Composable
fun Seg(options: List<Pair<String, String>>, value: String, modifier: Modifier = Modifier.padding(start = 20.dp, end = 20.dp, bottom = 4.dp), onPick: (String) -> Unit) {
    val shape = RoundedCornerShape(22.dp)
    Row(
        modifier.fillMaxWidth().card(P.chip, shape).padding(4.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        for ((key, label) in options) {
            val on = key == value
            val fill by animateColorAsState(if (on) P.rowActive else Color.Transparent, tween(150), label = "seg")
            val color by animateColorAsState(if (on) P.text else P.muted, tween(150), label = "seg")
            Box(
                Modifier
                    .weight(1f)
                    .height(40.dp)
                    .clip(RoundedCornerShape(18.dp))
                    .background(fill)
                    .tap(role = Role.RadioButton) { onPick(key) }
                    .padding(horizontal = 6.dp),
                contentAlignment = Alignment.Center,
            ) { T(label, 14f, color, FontWeight.Medium, line = 1.2f, align = TextAlign.Center, lines = 2) }
        }
    }
}

/// `.choices`: options that wrap as chips (value, label).
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun Choices(options: List<Pair<String, String>>, value: String, onPick: (String) -> Unit) {
    FlowRow(
        Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, bottom = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        for ((key, label) in options) {
            val on = key == value
            val shape = RoundedCornerShape(20.dp)
            Box(
                Modifier
                    .height(40.dp)
                    .card(if (on) P.accent.copy(alpha = .16f) else P.chip, shape, if (on) P.accent.copy(alpha = .5f) else P.border)
                    .tap(role = Role.RadioButton) { onPick(key) }
                    .padding(horizontal = 14.dp),
                contentAlignment = Alignment.Center,
            ) { T(label, 14f, if (on) P.accentHover else P.muted, FontWeight.Medium, lines = 1, line = 1.2f) }
        }
    }
}

/// `.sfield`: a search or text field in a sheet.
@Composable
fun SField(
    value: String,
    onValue: (String) -> Unit,
    placeholder: String,
    icon: String? = null,
    modifier: Modifier = Modifier.padding(start = 20.dp, end = 20.dp, top = 4.dp, bottom = 8.dp),
    enabled: Boolean = true,
) {
    val shape = RoundedCornerShape(24.dp)
    Row(
        modifier.fillMaxWidth().height(48.dp).card(P.chip, shape).padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (icon != null) Icon(icon, P.muted, size = 18.dp)
        BasicTextField(
            value,
            onValue,
            Modifier.weight(1f),
            enabled = enabled,
            singleLine = true,
            textStyle = style(16f),
            cursorBrush = SolidColor(P.accent),
            decorationBox = { field ->
                if (value.isEmpty()) T(placeholder, 16f, P.muted, lines = 1)
                field()
            },
        )
    }
}

/// A multi-line field (`textarea.big`, `.instr textarea`).
@Composable
fun Area(
    value: String,
    onValue: (String) -> Unit,
    placeholder: String = "",
    minHeight: Dp = 110.dp,
    size: Float = 16f,
    modifier: Modifier = Modifier,
) {
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(20.dp)
    Box(
        modifier
            .fillMaxWidth()
            .heightIn(min = minHeight)
            .card(P.chip, shape, if (focused) P.accent else P.input)
            .padding(horizontal = 16.dp, vertical = 14.dp),
    ) {
        BasicTextField(
            value,
            onValue,
            Modifier.fillMaxWidth().onFocus { focused = it },
            textStyle = style(size),
            cursorBrush = SolidColor(P.accent),
            decorationBox = { field ->
                if (value.isEmpty() && placeholder.isNotEmpty()) T(placeholder, size, P.muted)
                field()
            },
        )
    }
}

/// A request or tracked command asked from a sheet or a page: set `sent`
/// to its key; `busy` while it waits, `error` once it failed; `onOk` runs
/// once the server accepted it.
class Outcome {
    private var key by mutableStateOf<String?>(null)
    var error by mutableStateOf<String?>(null)
    var busy by mutableStateOf(false)

    // Setting it marks the outcome busy at once: a second tap in the same
    // frame sees it.
    var sent: String?
        get() = key
        set(value) {
            key = value
            if (value != null) busy = true
        }

    // Runs `ask` (which returns the key to follow) unless one waits already.
    fun once(ask: () -> String?) {
        if (busy) return
        sent = ask()
    }
}

@Composable
fun rememberOutcome(onOk: () -> Unit = {}): Outcome {
    val model = LocalModel.current
    val outcome = remember { Outcome() }
    val key = outcome.sent
    LaunchedEffect(model.revision, key) {
        if (key == null) {
            outcome.busy = false
            return@LaunchedEffect
        }
        val state = model.core.replyState(key)
        outcome.busy = state.waiting
        if (state.done) {
            model.core.forget(key)
            outcome.sent = null
            outcome.busy = false
            outcome.error = state.error
            if (state.error == null) onOk()
        }
    }
    return outcome
}

// Pages

/// A page: its header, then its content scrolling (`.pscroll`).
@Composable
fun PageFrame(title: String, action: (@Composable () -> Unit)? = null, content: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxSize().background(P.canvas).safeBars().imePadding()) {
        PageHead(title, action)
        Column(
            Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState()).padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 32.dp),
            content = content,
        )
    }
}

/// `.glabel`: a group's label on a page.
@Composable
fun GroupLabel(text: String) {
    T(text, 14f, P.muted, FontWeight.Medium, Modifier.padding(start = 8.dp, end = 8.dp, top = 24.dp, bottom = 8.dp))
}

/// `.group`: rows on one rounded card, 2dp apart.
@Composable
fun Group(content: @Composable ColumnScope.() -> Unit) {
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(24.dp)).background(P.surface),
        verticalArrangement = Arrangement.spacedBy(2.dp),
        content = content,
    )
}

/// `.note`: a quiet paragraph under a group.
@Composable
fun PageNote(text: String) {
    T(text, 13f, P.muted, line = 1.45f, modifier = Modifier.padding(start = 8.dp, end = 8.dp, top = 8.dp))
}

/// `.set`: a row of a group; `value` shows at the end (or under the label
/// when `stacked`), with a chevron when it opens something.
@Composable
fun SetRow(
    icon: String?,
    label: String,
    value: String? = null,
    detail: String? = null,
    stacked: Boolean = false,
    mark: String? = null,
    danger: Boolean = false,
    chevron: Boolean = true,
    enabled: Boolean = true,
    onClick: (() -> Unit)? = null,
) {
    val interaction = remember { MutableInteractionSource() }
    val color = if (danger) P.error else P.text
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .then(if (onClick != null) Modifier.pressRow(interaction, P.overlay).tap(enabled = enabled, interaction = interaction, onClick = onClick) else Modifier.background(P.overlay))
            .alpha(if (enabled) 1f else .45f)
            .padding(start = 18.dp, end = 16.dp, top = 12.dp, bottom = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        if (icon != null) Icon(icon, if (danger) P.error else P.muted2)
        Column(Modifier.weight(1f)) {
            T(label, 16f, color)
            val under = if (stacked) value else detail
            if (!under.isNullOrEmpty()) T(under, 13f, P.muted, line = 1.4f, modifier = Modifier.padding(top = 2.dp))
        }
        if (!stacked && value != null) {
            Row(
                Modifier.widthIn(max = 200.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                if (mark != null) Mark(mark, size = 16.dp)
                T(value, 15f, P.muted, lines = 1)
            }
        }
        if (onClick != null && chevron) Icon("chevron-right", P.icon, size = 18.dp)
    }
}

/// A row with a toggle (`label.set` with `.toggle`).
@Composable
fun SetToggle(icon: String?, label: String, on: Boolean, detail: String? = null, enabled: Boolean = true, onChange: (Boolean) -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .pressRow(interaction, P.overlay)
            .tap(enabled = enabled, role = Role.Switch, interaction = interaction) { onChange(!on) }
            .alpha(if (enabled) 1f else .45f)
            .padding(start = 18.dp, end = 16.dp, top = 12.dp, bottom = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        if (icon != null) Icon(icon, P.muted2)
        Column(Modifier.weight(1f)) {
            T(label, 16f)
            if (!detail.isNullOrEmpty()) T(detail, 13f, P.muted, line = 1.4f, modifier = Modifier.padding(top = 2.dp))
        }
        Toggle(on, label, enabled) { onChange(it) }
    }
}

/// `.empty-page`: a page with nothing to list yet.
@Composable
fun EmptyPage(title: String, text: String) {
    Column(Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 64.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        T(title, 17f, weight = FontWeight.Medium, align = TextAlign.Center, modifier = Modifier.padding(bottom = 6.dp))
        T(text, 15f, P.muted, align = TextAlign.Center)
    }
}

/// `.loading-note`.
@Composable
fun LoadingNote(text: String = "Loading…") {
    T(text, 15f, P.muted, align = TextAlign.Center, modifier = Modifier.fillMaxWidth().padding(horizontal = 22.dp, vertical = 40.dp))
}

