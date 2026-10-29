package app.pluto.mobile.ui

// The home draft (`#home`): round buttons around the Chat | Code tabs, the
// hero (planet, glow, stars, the draft's title), the chips row and the
// composer. Chat's chips are Research and Image (one at a time) and
// Continue; its right disc turns a temporary chat on. Code's are the
// workspace, the branch and Continue; its right disc picks the project.

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
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
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import app.pluto.core.Target
import app.pluto.mobile.LocalModel
import app.pluto.mobile.Sheet
import app.pluto.mobile.safeBars

@Composable
fun Home() {
    val model = LocalModel.current
    val core = model.core
    val nav = model.nav
    val home = remember(model.revision) { core.home() }
    val chat = home.chat
    Box(Modifier.fillMaxSize().background(P.canvas)) {
        Glow()
        Column(Modifier.fillMaxSize().safeBars().imePadding()) {
            // Top bar
            Row(
                Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 16.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                DiscIcon("menu", "Open threads") { nav.openDrawer() }
                Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(28.dp, Alignment.CenterHorizontally)) {
                    Tab("Chat", chat) { core.setChat(true) }
                    Tab("Code", !chat) { core.setChat(false) }
                }
                if (chat) {
                    DiscIcon("message-square-dashed", "Temporary chat", on = home.temporary) { core.setTemporary(!home.temporary) }
                } else {
                    Disc("Project", onClick = { nav.openSheet(Sheet.Project) }) {
                        val project = home.project
                        if (project != null) ProjectBadge(project, BadgeDisc) else Icon("folder", P.text)
                    }
                }
            }
            // Hero
            Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                Stars()
                Column(
                    Modifier.padding(horizontal = 24.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(20.dp),
                ) {
                    Planet(Modifier.size(132.dp, 110.dp))
                    val title = buildAnnotatedString {
                        val project = home.project
                        when {
                            chat && home.temporary -> {
                                append("Temporary chat\n")
                                withStyle(SpanStyle(fontSize = style(15f).fontSize, color = P.muted)) {
                                    append("No memory, deleted from the server once you leave it.")
                                }
                            }
                            chat -> append("How can I help?")
                            project != null -> {
                                append("What should we build in ")
                                withStyle(SpanStyle(color = P.text, fontWeight = FontWeight.SemiBold)) { append(project.title) }
                                append("?")
                            }
                            else -> append("Add a project in Pluto on the computer to start coding here.")
                        }
                    }
                    BasicText(
                        title,
                        style = style(22f, FontWeight.Medium, P.muted2, line = 1.3f, track = -.015f).copy(textAlign = TextAlign.Center),
                    )
                }
            }
            // Chips
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(start = 16.dp, end = 16.dp, bottom = 10.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                if (chat) {
                    if (!home.temporary) {
                        Chip("globe", "Research", on = home.research) { core.setResearch(!home.research) }
                        Chip("image", "Image", on = home.image) { core.setImage(!home.image) }
                    }
                } else if (home.project != null) {
                    Chip(if (home.worktree) "folder-git-2" else "folder", if (home.worktree) "New worktree" else "Local checkout") {
                        nav.openSheet(Sheet.Workspace)
                    }
                    if (home.worktree) Chip("git-branch", "From ${home.base}") { nav.openSheet(Sheet.Branch) }
                }
                home.latest?.let { latest ->
                    Chip("corner", short(latest.title), sub = "Continue") { nav.openThread(latest.id) }
                }
            }
            Composer(if (chat) Target.Chat else Target.Code, placeholder(home))
            Spacer(height = 8.dp)
        }
    }
}

private fun placeholder(home: app.pluto.core.HomeView) = when {
    !home.chat -> "Ask anything, @ files, / commands"
    home.research -> "What should I research?"
    home.image -> "Describe an image"
    else -> "Ask anything"
}

private fun short(title: String) = if (title.codePointCount(0, title.length) > 26) {
    title.substring(0, title.offsetByCodePoints(0, 25)) + "…"
} else title

// A space tab: its underline grows in when selected.
@Composable
private fun Tab(label: String, selected: Boolean, onClick: () -> Unit) {
    val color by animateColorAsState(if (selected) P.text else P.icon, tween(200, easing = Enter), label = "tab")
    val line by animateFloatAsState(if (selected) 1f else 0f, tween(200, easing = Ease), label = "tab")
    Box(
        Modifier.height(48.dp).tap(role = Role.Tab, onClick = onClick).padding(horizontal = 2.dp),
        contentAlignment = Alignment.Center,
    ) {
        T(label, 19f, color, FontWeight.Medium, line = 1.2f, track = -.01f)
        Box(
            Modifier
                .align(Alignment.BottomCenter)
                .padding(bottom = 2.dp)
                .size(16.dp, 3.dp)
                .graphicsLayer {
                    scaleX = line
                    transformOrigin = TransformOrigin.Center
                }
                .clip(RoundedCornerShape(2.dp))
                .background(P.accent),
        )
    }
}

// `.chip`: a 52dp pill; on, it takes the accent.
@Composable
fun Chip(icon: String, label: String, sub: String? = null, on: Boolean = false, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val shape = RoundedCornerShape(26.dp)
    val fill by animateColorAsState(if (on) P.accent.copy(alpha = .12f) else P.chip, tween(150), label = "chip")
    val border by animateColorAsState(if (on) P.accent.copy(alpha = .5f) else P.border, tween(150), label = "chip")
    val color by animateColorAsState(if (on) P.accentHover else P.text, tween(150), label = "chip")
    Row(
        Modifier
            .height(52.dp)
            .pressScale(interaction, .97f)
            .clip(shape)
            .background(fill)
            .border(1.dp, border, shape)
            .tap(interaction = interaction, onClick = onClick)
            .padding(start = 16.dp, end = 20.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(icon, if (on) P.accent else P.muted2)
        if (sub != null) T(sub, 16f, P.muted, lines = 1)
        T(label, 16f, color, FontWeight.Medium, lines = 1)
    }
}

// The composer card (Grok's pill, Pluto's model picker and gradient send):
// attach, the model, Code's access and mode, then Send, or Stop while the
// thread works and the field is empty.
@Composable
fun Composer(target: Target, placeholder: String) {
    val model = LocalModel.current
    val core = model.core
    val nav = model.nav
    val key = when (target) {
        is Target.Chat -> "chat"
        is Target.Code -> "code"
        is Target.Thread -> target.id
    }
    val view = remember(model.revision, target) { core.composer(target) }
    val text = model.texts[key].orEmpty()
    // A message the server refused comes back here, unless something new
    // was typed meanwhile.
    LaunchedEffect(model.revision, key) {
        core.takeUnsent(key)?.let { back ->
            if (model.texts[key].isNullOrBlank()) model.texts[key] = back
        }
    }
    var focused by remember { mutableStateOf(false) }
    val haptic = LocalHapticFeedback.current
    val border by animateColorAsState(if (focused) P.focus else P.input, tween(150), label = "composer")
    val empty = text.isBlank()
    val shape = RoundedCornerShape(28.dp)
    Column(
        Modifier
            .padding(horizontal = 12.dp)
            .fillMaxWidth()
            .clip(shape)
            .background(P.surface)
            .border(1.dp, border, shape),
    ) {
        Box(Modifier.fillMaxWidth().heightIn(min = 56.dp, max = 180.dp).padding(start = 20.dp, end = 20.dp, top = 16.dp, bottom = 6.dp)) {
            BasicTextField(
                text,
                { model.texts[key] = it },
                Modifier.fillMaxWidth().onFocusChanged { focused = it.isFocused },
                textStyle = style(17f, line = 1.45f),
                cursorBrush = SolidColor(P.accent),
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                decorationBox = { field ->
                    if (text.isEmpty()) T(placeholder, 17f, P.muted, line = 1.45f, lines = 1)
                    field()
                },
            )
        }
        Row(
            Modifier.fillMaxWidth().padding(start = 8.dp, end = 8.dp, top = 6.dp, bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            // The model shortens instead of pushing Send out.
            Row(
                Modifier.weight(1f),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Tool(label = "Attach", round = true, onClick = { nav.openSheet(Sheet.Attach) }) { Icon("plus", P.text) }
                Tool(
                    label = "Model: ${view.modelLabel}",
                    modifier = Modifier.weight(1f, fill = false),
                    onClick = { nav.openSheet(Sheet.Model) },
                ) {
                    view.mark?.let { Mark(it) }
                    T(view.modelLabel, 15f, weight = FontWeight.Medium, lines = 1, line = 1.2f, modifier = Modifier.weight(1f, fill = false))
                    Icon("chevron-down", P.muted, size = 16.dp)
                }
                if (view.code) {
                    Tool(label = "Access and mode", quiet = true, onClick = { nav.openSheet(Sheet.Model) }) {
                        Icon(view.accessIcon, P.muted2)
                        if (view.plan) T("Plan", 15f, P.muted2, FontWeight.Medium, line = 1.2f)
                    }
                }
            }
            if (view.running && empty) {
                SendButton(stop = true, enabled = view.online, label = "Stop") {
                    if (target is Target.Thread) core.stop(target.id)
                }
            } else {
                SendButton(stop = false, enabled = !empty && view.online, label = "Send") {
                    haptic.performHapticFeedback(HapticFeedbackType.Confirm)
                    val outcome = core.send(target, text)
                    val error = outcome.error
                    if (error == null) {
                        model.texts.remove(key)
                        outcome.created?.let { nav.openThread(it) }
                    } else if (error.isNotEmpty()) {
                        core.toast(error)
                    }
                }
            }
        }
    }
}

// `.tool`: a 44dp pill on the overlay; `quiet` has no fill.
@Composable
private fun Tool(
    label: String,
    modifier: Modifier = Modifier,
    round: Boolean = false,
    quiet: Boolean = false,
    onClick: () -> Unit,
    content: @Composable () -> Unit,
) {
    val shape = RoundedCornerShape(22.dp)
    Row(
        modifier
            .height(44.dp)
            .then(if (round) Modifier.width(44.dp) else Modifier.widthIn(min = 44.dp))
            .clip(shape)
            .then(if (quiet) Modifier else Modifier.background(P.overlay))
            .tap(label = label, onClick = onClick)
            .padding(horizontal = if (round) 0.dp else if (quiet) 10.dp else 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
    ) { content() }
}

// Send's gradient disc, Stop's light one.
@Composable
private fun SendButton(stop: Boolean, enabled: Boolean, label: String, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val fill = when {
        stop -> Brush.linearGradient(listOf(P.text, P.text))
        enabled -> Brush.verticalGradient(listOf(P.gradientTop, P.gradientBottom))
        else -> Brush.linearGradient(listOf(P.overlay, P.overlay))
    }
    val tint = when {
        stop -> P.canvas
        enabled -> P.accentFg
        else -> P.icon
    }
    Box(
        Modifier
            .size(44.dp)
            .then(if (enabled) Modifier.pressScale(interaction, .92f) else Modifier)
            .clip(CircleShape)
            .background(fill)
            .tap(enabled = enabled, label = label, interaction = interaction, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(if (stop) "stop" else "arrow-up", tint, stroke = 2.25f)
    }
}

