package app.pluto.mobile.ui

// Pluto stock (desktop `theme::pluto_stock()`), laid out for a phone after
// Grok's Android app: the Dioxus app's main.css tokens, in Compose. CSS
// pixels are dp; font sizes are sp.

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.PlatformTextStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import app.pluto.mobile.R

object P {
    val canvas = Color(0xFF0B0A0F)
    val sidebar = Color(0xFF07060A)
    val surface = Color(0xFF121018)
    val overlay = Color(0xFF16141D)
    val text = Color(0xFFF3F1F8)
    val muted = Color(0xFF8F8A9C)
    val muted2 = Color(0xFFA6A1B3)
    val icon = Color(0xFF625D70)
    val border = Color(0xFF1B1A20)
    val input = Color(0xFF26252C)
    val chip = Color(0xFF141319)
    val message = Color(0xFF19171D)
    val rowActive = Color(0xFF1F1D29)
    val accent = Color(0xFFA78BFA)
    val accentHover = Color(0xFFC4B5FD)
    val accentFg = Color(0xFF1A1426)
    // `--glow`: rgb(109 40 217).
    val glow = Color(0xFF6D28D9)
    val warning = Color(0xFFFFB900)
    val info = Color(0xFF51A2FF)
    val success = Color(0xFF00D492)
    val error = Color(0xFFFF6467)
    val add = Color(0xFF00D492)
    val del = Color(0xFFFF6467)
    // A focused field's border.
    val focus = Color(0xFF3A3645)
    // The gradient of Send and the primary buttons.
    val gradientTop = Color(0xFFC4B5FD)
    val gradientBottom = Color(0xFF8B5CF6)
    val star = Color(0xFFEDE9FE)
}

// `prefers-reduced-motion`: Android's "Remove animations" (an animator
// duration scale of 0) stills the endless animations (moon, orb, dots,
// spinners).
val LocalReduceMotion = androidx.compose.runtime.staticCompositionLocalOf { false }

fun reduceMotion(context: android.content.Context): Boolean =
    android.provider.Settings.Global.getFloat(
        context.contentResolver,
        android.provider.Settings.Global.ANIMATOR_DURATION_SCALE,
        1f,
    ) == 0f

// `--ease`, `--enter` and `--exit`.
val Ease = CubicBezierEasing(.2f, .8f, .2f, 1f)
val Enter = CubicBezierEasing(.32f, .72f, 0f, 1f)
val Exit = CubicBezierEasing(.4f, 0f, .7f, .2f)

val Geist = FontFamily(
    Font(R.font.geist_regular, FontWeight.Normal),
    Font(R.font.geist_medium, FontWeight.Medium),
    Font(R.font.geist_semibold, FontWeight.SemiBold),
    // Bold text (`<b>`, pips) uses SemiBold's face, like the WebView did
    // with the faces it had.
    Font(R.font.geist_semibold, FontWeight.Bold),
)
val GeistMono = FontFamily(Font(R.font.geist_mono_regular, FontWeight.Normal))

// A text style: `size` sp, `line` its line height as a factor (CSS
// `line-height`), `track` its letter spacing in em.
fun style(
    size: Float,
    weight: FontWeight = FontWeight.Normal,
    color: Color = P.text,
    line: Float = 1.5f,
    track: Float = 0f,
    mono: Boolean = false,
): TextStyle = styles.getOrPut(StyleKey(size, weight, color, line, track, mono)) {
    makeStyle(size, weight, color, line, track, mono)
}

// The styles made so far: a screen asks for the same few again and again
// (built on the main thread only).
private data class StyleKey(
    val size: Float,
    val weight: FontWeight,
    val color: Color,
    val line: Float,
    val track: Float,
    val mono: Boolean,
)

private val styles = HashMap<StyleKey, TextStyle>()

private fun makeStyle(
    size: Float,
    weight: FontWeight,
    color: Color,
    line: Float,
    track: Float,
    mono: Boolean,
) = TextStyle(
    fontFamily = if (mono) GeistMono else Geist,
    fontSize = size.sp,
    fontWeight = weight,
    color = color,
    lineHeight = (size * line).sp,
    letterSpacing = if (track == 0f) TextUnit.Unspecified else track.em,
    // CSS spreads the line height evenly above and below, first and last
    // lines included.
    lineHeightStyle = LineHeightStyle(LineHeightStyle.Alignment.Center, LineHeightStyle.Trim.None),
    platformStyle = PlatformTextStyle(includeFontPadding = false),
)
