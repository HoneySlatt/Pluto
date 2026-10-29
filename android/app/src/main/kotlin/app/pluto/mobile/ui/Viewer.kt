package app.pluto.mobile.ui

// An image full screen (a message's attachment, an image Codex made): it
// opens from where it was tapped with a fade, pinches and double-taps to
// zoom, pans when zoomed, and a swipe down or Back closes it.

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp
import app.pluto.mobile.LocalModel
import app.pluto.mobile.safeBars
import kotlinx.coroutines.launch
import kotlin.math.abs

@Composable
fun Viewer() {
    val model = LocalModel.current
    var shown by remember { mutableStateOf<ImageBitmap?>(null) }
    val open = model.viewer != null
    if (open) shown = model.viewer
    val image = shown ?: return
    val fade = remember { Animatable(0f) }
    val scope = rememberCoroutineScope()
    var scale by remember(image) { mutableFloatStateOf(1f) }
    var pan by remember(image) { mutableStateOf(Offset.Zero) }
    // A swipe down at rest closes it.
    var drop by remember(image) { mutableFloatStateOf(0f) }
    LaunchedEffect(open) {
        if (open) fade.animateTo(1f, tween(220, easing = Enter))
        else {
            fade.animateTo(0f, tween(180, easing = Exit))
            shown = null
        }
    }
    val close = { model.viewer = null }
    BackHandler(open) { close() }
    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .graphicsLayer { alpha = fade.value }
            .background(Color.Black.copy(alpha = .94f * (1f - minOf(abs(drop) / 900f, .6f))))
            .pointerInput(image) {
                detectTapGestures(
                    onDoubleTap = { at ->
                        if (scale > 1f) {
                            scale = 1f
                            pan = Offset.Zero
                        } else {
                            scale = 2.5f
                            pan = (Offset(size.width / 2f, size.height / 2f) - at) * 1.5f
                        }
                    },
                    onTap = { if (scale == 1f) close() },
                )
            }
            .pointerInput(image) {
                detectTransformGestures { _, move, zoom, _ ->
                    if (scale == 1f && zoom == 1f) {
                        drop += move.y
                    } else {
                        scale = (scale * zoom).coerceIn(1f, 6f)
                        pan = if (scale == 1f) Offset.Zero else pan + move
                    }
                }
            }
            .pointerInput(image) {
                // Let go: far enough down closes, else it settles back.
                awaitPointerEventScope {
                    while (true) {
                        val event = awaitPointerEvent()
                        if (event.changes.none { it.pressed } && drop != 0f) {
                            if (abs(drop) > 160f) close()
                            else scope.launch {
                                val from = drop
                                Animatable(from).animateTo(0f, tween(200, easing = Enter)) { drop = value }
                            }
                        }
                    }
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Image(
            image,
            null,
            Modifier
                .fillMaxSize()
                .graphicsLayer {
                    scaleX = scale
                    scaleY = scale
                    translationX = pan.x
                    translationY = pan.y + drop
                },
            contentScale = ContentScale.Fit,
        )
        Box(Modifier.align(Alignment.TopEnd).safeBars().padding(16.dp)) {
            DiscIcon("x", "Close") { close() }
        }
    }
}
