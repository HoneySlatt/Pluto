package app.pluto.mobile.ui

// Lucide icons (ISC) from the core's path data (`icon_paths`), stroked at
// 1.75 in their 24px box (`svg.i`), and the providers' marks rendered by
// the core.

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pluto.core.iconPaths
import app.pluto.core.markPng

private val vectors = HashMap<Pair<String, Float>, ImageVector>()

fun iconVector(name: String, stroke: Float = 1.75f): ImageVector = vectors.getOrPut(name to stroke) {
    val builder = ImageVector.Builder(
        name = name,
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f,
    )
    for (data in iconPaths(name)) {
        builder.addPath(
            pathData = PathParser().parsePathString(data).toNodes(),
            fill = null,
            stroke = SolidColor(Color.White),
            strokeLineWidth = stroke,
            strokeLineCap = StrokeCap.Round,
            strokeLineJoin = StrokeJoin.Round,
        )
    }
    builder.build()
}

// A Lucide glyph, `size` square, tinted `tint`.
@Composable
fun Icon(
    name: String,
    tint: Color,
    modifier: Modifier = Modifier,
    size: Dp = 22.dp,
    stroke: Float = 1.75f,
) {
    val painter = rememberVectorPainter(iconVector(name, stroke))
    Image(
        painter = painter,
        contentDescription = null,
        modifier = modifier.size(size),
        colorFilter = androidx.compose.ui.graphics.ColorFilter.tint(tint),
    )
}

private val marks = HashMap<String, ImageBitmap?>()

// A provider's mark (`ComposerView.mark`), drawn `size` square.
@Composable
fun Mark(name: String, modifier: Modifier = Modifier, size: Dp = 18.dp) {
    val bitmap = remember(name) {
        marks.getOrPut(name) {
            markPng(name, 96u)?.let { png ->
                BitmapFactory.decodeByteArray(png, 0, png.size)?.asImageBitmap()
            }
        }
    } ?: return
    Image(bitmap = bitmap, contentDescription = null, modifier = modifier.size(size))
}
