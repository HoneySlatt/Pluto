package app.pluto.mobile.ui

// The shared pieces of main.css: the round discs, the buttons, the toggle,
// the glow, the pixel planet with its stars, the avatar and the project
// badges. Touch answers like the WebView's `:active`: discs, chips and
// buttons shrink a little, rows light up at once.

import android.graphics.BitmapFactory
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.keyframes
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.scale
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.pluto.core.Badge
import app.pluto.mobile.LocalModel

// A click without ripple; `pressed` reports the finger on it.
@Composable
fun Modifier.tap(
    enabled: Boolean = true,
    role: Role = Role.Button,
    label: String? = null,
    interaction: MutableInteractionSource = remember { MutableInteractionSource() },
    onClick: () -> Unit,
): Modifier = clickable(
    interactionSource = interaction,
    indication = null,
    enabled = enabled,
    role = role,
    onClickLabel = label,
    onClick = onClick,
)

// Shrinks to `to` while pressed (`:active { transform: scale(…) }`).
@Composable
fun Modifier.pressScale(interaction: MutableInteractionSource, to: Float): Modifier {
    val pressed by interaction.collectIsPressedAsState()
    val scale by animateFloatAsState(if (pressed) to else 1f, tween(150), label = "press")
    return scale(scale)
}

// A row lit while pressed (`:active { background: var(--row-active) }`).
@Composable
fun Modifier.pressRow(interaction: MutableInteractionSource, base: Color = Color.Transparent): Modifier {
    val pressed by interaction.collectIsPressedAsState()
    return background(if (pressed) P.rowActive else base)
}

// Grok's round buttons: 48dp on the overlay, the accent when on.
@Composable
fun Disc(
    label: String,
    modifier: Modifier = Modifier,
    size: Dp = 48.dp,
    on: Boolean = false,
    enabled: Boolean = true,
    onClick: () -> Unit,
    content: @Composable BoxScope.() -> Unit,
) {
    val interaction = remember { MutableInteractionSource() }
    val fill by animateColorAsState(if (on) P.accent else P.overlay, tween(150), label = "disc")
    Box(
        modifier
            .size(size)
            .pressScale(interaction, .94f)
            .clip(CircleShape)
            .background(fill)
            .border(1.dp, if (on) Color.Transparent else P.border, CircleShape)
            .tap(enabled = enabled, label = label, interaction = interaction, onClick = onClick)
            .alpha(if (enabled) 1f else .45f),
        contentAlignment = Alignment.Center,
        content = content,
    )
}

@Composable
fun DiscIcon(name: String, label: String, modifier: Modifier = Modifier, on: Boolean = false, enabled: Boolean = true, size: Dp = 48.dp, onClick: () -> Unit) {
    Disc(label, modifier, size = size, on = on, enabled = enabled, onClick = onClick) {
        Icon(name, if (on) P.accentFg else P.text)
    }
}

enum class Tone { Plain, Primary, Danger, Ghost }

// `.btn`: a 48dp pill; Primary is the gradient one.
@Composable
fun Btn(
    text: String,
    modifier: Modifier = Modifier,
    tone: Tone = Tone.Plain,
    enabled: Boolean = true,
    height: Dp = 48.dp,
    fontSize: Float = 16f,
    icon: String? = null,
    spin: Boolean = false,
    onClick: () -> Unit,
) {
    val interaction = remember { MutableInteractionSource() }
    val shape = RoundedCornerShape(50)
    val base = modifier
        .height(height)
        .pressScale(interaction, .98f)
        .clip(shape)
    val filled = when (tone) {
        Tone.Primary -> base.background(Brush.verticalGradient(listOf(P.gradientTop, P.gradientBottom)))
        Tone.Danger -> base.background(P.error.copy(alpha = .14f)).border(1.dp, P.error.copy(alpha = .3f), shape)
        Tone.Ghost -> base.border(1.dp, P.border, shape)
        Tone.Plain -> base.background(P.overlay).border(1.dp, P.border, shape)
    }
    val color = when (tone) {
        Tone.Primary -> P.accentFg
        Tone.Danger -> P.error
        else -> P.text
    }
    Row(
        filled
            .tap(enabled = enabled, interaction = interaction, onClick = onClick)
            .alpha(
                when {
                    enabled -> 1f
                    tone == Tone.Primary -> .7f
                    else -> .45f
                },
            )
            .padding(horizontal = 18.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            if (spin) Spinner(color, 22.dp) else Icon(icon, color)
        }
        BasicText(
            text,
            style = style(fontSize, if (tone == Tone.Primary) FontWeight.SemiBold else FontWeight.Medium, color, line = 1.2f),
            maxLines = 1,
        )
    }
}

// `.spin`: a loader turning once a second.
@Composable
fun Spinner(tint: Color, size: Dp = 22.dp, name: String = "loader-circle") {
    val turn by rememberInfiniteTransition(label = "spin").animateFloat(
        0f,
        360f,
        infiniteRepeatable(tween(1000, easing = androidx.compose.animation.core.LinearEasing)),
        label = "spin",
    )
    Icon(name, tint, Modifier.graphicsLayer { rotationZ = turn }, size = size)
}

// `.toggle`: 52×32, the knob sliding 20dp.
@Composable
fun Toggle(checked: Boolean, label: String? = null, enabled: Boolean = true, onChange: (Boolean) -> Unit) {
    val x by animateFloatAsState(if (checked) 20f else 0f, tween(180, easing = Ease), label = "knob")
    val track by animateColorAsState(if (checked) P.accent else P.input, tween(180), label = "track")
    val knob by animateColorAsState(if (checked) P.accentFg else P.muted2, tween(180), label = "knob")
    val haptic = androidx.compose.ui.platform.LocalHapticFeedback.current
    Box(
        Modifier
            .size(52.dp, 32.dp)
            .clip(RoundedCornerShape(16.dp))
            .background(track)
            .tap(enabled = enabled, role = Role.Switch, label = label) {
                haptic.performHapticFeedback(
                    if (checked) androidx.compose.ui.hapticfeedback.HapticFeedbackType.ToggleOff
                    else androidx.compose.ui.hapticfeedback.HapticFeedbackType.ToggleOn,
                )
                onChange(!checked)
            },
    ) {
        Box(
            Modifier
                .offset(x = (4 + x).dp, y = 4.dp)
                .size(24.dp)
                .clip(CircleShape)
                .background(knob),
        )
    }
}

// Pluto stock's glow, falling from the top center (backdrop.rs):
// `radial-gradient(55% 100% at 50% 0%, …)` over the top 70%.
@Composable
fun Glow(modifier: Modifier = Modifier, dim: Boolean = false) {
    Canvas(modifier.fillMaxSize().alpha(if (dim) .35f else 1f)) {
        val box = Size(size.width, size.height * .7f)
        val rx = box.width * .55f
        val ry = box.height
        scale(1f, ry / rx, pivot = Offset(box.width / 2, 0f)) {
            drawRect(
                Brush.radialGradient(
                    0f to P.glow.copy(alpha = .30f),
                    .45f to P.glow.copy(alpha = .10f),
                    1f to Color.Transparent,
                    center = Offset(box.width / 2, 0f),
                    radius = rx,
                ),
                topLeft = Offset(0f, 0f),
                size = Size(box.width, rx),
            )
        }
    }
}

private val planetPaths by lazy {
    val parse = { d: String -> PathParser().parsePathString(d).toPath() }
    listOf(
        // Outline ring, then body, top, shading, spots, highlights, rim.
        parse("M88 256v-24h24v-24h48v-24h48v-24h80v24h48v24h48v24h24v48h-24v24h-48v24h-48v24h-80v-24h-48v-24h-48v-24H88v-48Z"),
        parse("M192 120h128v24h24v24h24v24h24v24h24v80h-24v24h-24v24h-24v24h-24v24H192v-24h-24v-24h-24v-24h-24v-24h-24v-80h24v-24h24v-24h24v-24h24v-24Z"),
        parse("M208 80h40v24h-40ZM192 104h80v24h-80Z"),
        parse("M120 288h24v24h24v24h24v24h128v-24h24v-24h24v-24h24v-24H120Z"),
        parse("M184 192h48v24h-48Zm128 88h48v24h-48Z"),
        parse("M168 160h48v16h-48Zm-24 24h24v24h-24Zm72-88h16v16h-16Z"),
        parse("M88 256v48h24v24h48v24h80v-24h48v-24h48v-24h24v-48"),
    )
}

// The pixel planet (`pixel_logo.rs`, viewBox 72 72 384 320), with its
// orbiting moon on the hero (`steps(4)` over 6s, back and forth).
@Composable
fun Planet(modifier: Modifier = Modifier, moon: Boolean = true, glow: Boolean = true, dim: Boolean = false) {
    // `.dim-planet`: faded and partly grey, its glow with it (an alpha
    // layer would clip the glow to the planet's box).
    val fade = if (dim) .45f else 1f
    val grey = if (dim) {
        androidx.compose.ui.graphics.ColorFilter.colorMatrix(
            androidx.compose.ui.graphics.ColorMatrix().apply { setToSaturation(.6f) },
        )
    } else null
    val step = if (moon) {
        val t by rememberInfiniteTransition(label = "moon").animateFloat(
            0f,
            1f,
            infiniteRepeatable(tween(6000, easing = androidx.compose.animation.core.LinearEasing), RepeatMode.Reverse),
            label = "moon",
        )
        (kotlin.math.floor(t * 4f).coerceAtMost(4f)) / 4f
    } else 0f
    Canvas(
        modifier.drawBehind {
            if (glow) {
                // `drop-shadow(0 8px 40px rgb(var(--glow) / .45))`.
                drawCircle(
                    Brush.radialGradient(
                        listOf(P.glow.copy(alpha = .30f), Color.Transparent),
                        center = Offset(size.width / 2, size.height / 2 + 8.dp.toPx()),
                        radius = size.width * .75f,
                    ),
                    radius = size.width * .75f,
                    center = Offset(size.width / 2, size.height / 2 + 8.dp.toPx()),
                    alpha = fade,
                    colorFilter = grey,
                )
            }
        },
    ) {
        val s = size.width / 384f
        scale(s, s, pivot = Offset.Zero) {
            translate(-72f, -72f) {
                val p = planetPaths
                drawPath(p[0], P.accent, alpha = fade, style = Stroke(18f), colorFilter = grey)
                drawPath(p[1], P.star, alpha = fade, colorFilter = grey)
                drawPath(p[2], P.star, alpha = fade, colorFilter = grey)
                drawPath(p[3], P.accent, alpha = fade, colorFilter = grey)
                drawPath(p[4], P.accent, alpha = fade, colorFilter = grey)
                drawPath(p[5], Color.White, alpha = fade, colorFilter = grey)
                drawPath(p[6], Color(0xFF6D28D9), alpha = fade, style = Stroke(20f), colorFilter = grey)
                drawRect(Color(0xFFF5F3FF), topLeft = Offset(392f, 144f + step * 24f), size = Size(28f, 28f), alpha = fade, colorFilter = grey)
            }
        }
    }
}

// Pixel stars around the hero (`backdrop.rs`'s sky), from a fixed seed.
@Composable
fun Stars(modifier: Modifier = Modifier) {
    val stars = remember {
        var seed = 7L
        val rnd = {
            seed = (seed * 16807) % 2_147_483_647
            seed.toDouble() / 2_147_483_647.0
        }
        val out = ArrayList<FloatArray>()
        while (out.size < 46) {
            val x = rnd()
            val y = rnd()
            val dx = (x - .5) / .32
            val dy = (y - .45) / .3
            if (dx * dx + dy * dy < 1.0) continue
            val size = if (rnd() < .3) 3f else 2f
            val opacity = .15 + rnd() * .45
            out.add(floatArrayOf(x.toFloat(), y.toFloat(), size, opacity.toFloat()))
        }
        out
    }
    Canvas(modifier.fillMaxSize()) {
        for ((x, y, px, opacity) in stars.map { listOf(it[0], it[1], it[2], it[3]) }) {
            drawRect(
                P.star.copy(alpha = opacity),
                topLeft = Offset(x * size.width, y * size.height),
                size = Size(px.dp.toPx(), px.dp.toPx()),
            )
        }
    }
}

// The planet in a round avatar (the drawer's and Settings' server card).
@Composable
fun Avatar(size: Dp = 52.dp) {
    Box(
        Modifier
            .size(size)
            .clip(CircleShape)
            .background(
                Brush.radialGradient(
                    0f to P.glow.copy(alpha = .55f),
                    .75f to P.surface,
                    center = Offset.Unspecified,
                ),
            )
            .border(1.dp, P.border, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Planet(Modifier.size(34.dp, 29.dp), moon = false, glow = false)
    }
}

// A badge's size: the tile, the monogram's font and the corner radius.
data class BadgeSize(val size: Dp, val font: Float, val radius: Dp, val emoji: Float = 15f)

val BadgeDefault = BadgeSize(22.dp, 9.5f, 6.dp)
val BadgeDisc = BadgeSize(26.dp, 11f, 7.dp, 22f)
val BadgeMeta = BadgeSize(18.dp, 8f, 5.dp)
val BadgeAttention = BadgeSize(30.dp, 11f, 9.dp)
val BadgeRow = BadgeSize(36.dp, 12f, 10.dp)
val BadgeHead = BadgeSize(56.dp, 18f, 16.dp, 40f)

private val favicons = HashMap<String, ImageBitmap>()

// A project's badge (`ProjectFavicon.tsx`): its Lucide icon, emoji or
// favicon, else the monogram on its color.
@Composable
fun ProjectBadge(badge: Badge, size: BadgeSize = BadgeDefault) {
    val model = LocalModel.current
    val shape = RoundedCornerShape(size.radius)
    val tile = Modifier.size(size.size)
    val color = Color(0xFF000000 or badge.color.toLong())
    when {
        badge.lucide != null -> Box(tile, contentAlignment = Alignment.Center) {
            Icon(badge.lucide!!, color, size = size.size)
        }
        badge.emoji -> Box(tile, contentAlignment = Alignment.Center) {
            BasicText(badge.text, style = style(size.emoji, line = 1f))
        }
        else -> {
            val favicon = if (badge.favicon) {
                @Suppress("UNUSED_EXPRESSION") model.revision
                favicons[badge.projectId] ?: model.core.favicon(badge.projectId)?.let { bytes ->
                    BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.asImageBitmap()?.also {
                        favicons[badge.projectId] = it
                    }
                }
            } else null
            if (favicon != null) {
                Image(favicon, null, tile.clip(shape), contentScale = ContentScale.Fit)
            } else {
                Box(tile.clip(shape).background(color), contentAlignment = Alignment.Center) {
                    BasicText(
                        badge.text,
                        style = style(size.font, FontWeight.SemiBold, P.accentFg, line = 1f, track = .02f, mono = true)
                            .copy(textAlign = TextAlign.Center),
                    )
                }
            }
        }
    }
}

// Text, one style.
@Composable
fun T(
    text: String,
    size: Float = 16f,
    color: Color = P.text,
    weight: FontWeight = FontWeight.Normal,
    modifier: Modifier = Modifier,
    line: Float = 1.5f,
    lines: Int = Int.MAX_VALUE,
    mono: Boolean = false,
    track: Float = 0f,
    align: TextAlign? = null,
) {
    BasicText(
        text,
        modifier,
        style = style(size, weight, color, line, track, mono).let { if (align != null) it.copy(textAlign = align) else it },
        maxLines = lines,
        overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis,
    )
}

@Composable
fun Spacer(width: Dp = 0.dp, height: Dp = 0.dp) {
    androidx.compose.foundation.layout.Spacer(Modifier.size(width, height))
}

@Composable
fun RowScope.Grow() {
    androidx.compose.foundation.layout.Spacer(Modifier.weight(1f))
}

// A shape's fill and border, the cards' usual pair.
fun Modifier.card(fill: Color, shape: Shape, border: Color = P.border) =
    this.clip(shape).background(fill).border(1.dp, border, shape)

/// The width of `n` zeros (CSS `ch`) at `size` sp.
@Composable
fun ch(n: Int, size: Float): Dp {
    val measurer = androidx.compose.ui.text.rememberTextMeasurer()
    val density = LocalDensity.current
    return remember(n, size) {
        with(density) { measurer.measure("0".repeat(n), style(size)).size.width.toDp() }
    }
}
