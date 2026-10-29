package app.pluto.mobile.ui

// An image full screen (a message's attachment, an image Codex made): it
// fades in, pinches and double-taps to zoom, pans when zoomed. Its buttons
// (Save, Close) leave after a moment and come back on a tap; a swipe down
// or Back closes it. Save puts the image as it came in Pictures/Pluto.

import android.Manifest
import android.content.ContentValues
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.provider.MediaStore
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
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
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.unit.dp
import app.pluto.mobile.LocalModel
import app.pluto.mobile.Viewed
import app.pluto.mobile.safeBars
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.abs

// How long the buttons stay after the image opens or a tap shows them.
private const val CHROME_MS = 2500L

@Composable
fun Viewer() {
    val model = LocalModel.current
    val context = LocalContext.current
    val haptic = LocalHapticFeedback.current
    var shown by remember { mutableStateOf<Viewed?>(null) }
    val open = model.viewer != null
    if (open) shown = model.viewer
    val viewed = shown ?: return
    val image = viewed.bitmap
    val fade = remember { Animatable(0f) }
    val scope = rememberCoroutineScope()
    var scale by remember(viewed) { mutableFloatStateOf(1f) }
    var pan by remember(viewed) { mutableStateOf(Offset.Zero) }
    // A swipe down at rest closes it.
    var drop by remember(viewed) { mutableFloatStateOf(0f) }
    // The buttons, and each tap that shows them again.
    var chrome by remember(viewed) { mutableStateOf(true) }
    var shownAt by remember(viewed) { mutableStateOf(0) }
    val chromeAlpha by animateFloatAsState(if (chrome) 1f else 0f, tween(200), label = "chrome")
    LaunchedEffect(open) {
        if (open) fade.animateTo(1f, tween(220, easing = Enter))
        else {
            fade.animateTo(0f, tween(180, easing = Exit))
            shown = null
        }
    }
    LaunchedEffect(chrome, shownAt) {
        if (chrome) {
            delay(CHROME_MS)
            chrome = false
        }
    }
    val close = { model.viewer = null }
    val save = {
        haptic.performHapticFeedback(HapticFeedbackType.Confirm)
        val error = saveToGallery(context, viewed)
        model.core.toast(error ?: "Saved to Pictures/Pluto")
    }
    // Android 9 asks for storage first; later ones need nothing.
    val askStorage = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (granted) save() else model.core.toast("Pluto needs storage access to save the image")
    }
    BackHandler(open) { close() }
    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .graphicsLayer { alpha = fade.value }
            .background(Color.Black.copy(alpha = 1f - minOf(abs(drop) / 900f, .6f)))
            .pointerInput(viewed) {
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
                    onTap = {
                        chrome = !chrome
                        shownAt++
                    },
                )
            }
            .pointerInput(viewed) {
                detectTransformGestures { _, move, zoom, _ ->
                    if (scale == 1f && zoom == 1f) {
                        drop += move.y
                    } else {
                        scale = (scale * zoom).coerceIn(1f, 6f)
                        pan = if (scale == 1f) Offset.Zero else pan + move
                    }
                }
            }
            .pointerInput(viewed) {
                // Let go: far enough down closes, else it settles back.
                awaitPointerEventScope {
                    while (true) {
                        val event = awaitPointerEvent()
                        if (event.changes.none { it.pressed } && drop != 0f) {
                            if (abs(drop) > 160f) close()
                            else scope.launch {
                                Animatable(drop).animateTo(0f, tween(200, easing = Enter)) { drop = value }
                            }
                        }
                    }
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Image(
            image,
            viewed.name,
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
        if (chromeAlpha > 0f) {
            Row(
                Modifier
                    .align(Alignment.TopCenter)
                    .fillMaxWidth()
                    .graphicsLayer { alpha = chromeAlpha }
                    .safeBars()
                    .padding(16.dp),
                horizontalArrangement = Arrangement.spacedBy(10.dp, Alignment.End),
            ) {
                DiscIcon("download", "Save image", enabled = chrome) {
                    shownAt++
                    val needs = Build.VERSION.SDK_INT < Build.VERSION_CODES.Q &&
                        context.checkSelfPermission(Manifest.permission.WRITE_EXTERNAL_STORAGE) != PackageManager.PERMISSION_GRANTED
                    if (needs) askStorage.launch(Manifest.permission.WRITE_EXTERNAL_STORAGE) else save()
                }
                DiscIcon("x", "Close", enabled = chrome) { close() }
            }
        }
    }
}

// Writes the image's bytes as they came into Pictures/Pluto; the error to
// show when it failed.
private fun saveToGallery(context: Context, image: Viewed): String? = runCatching {
    val bytes = image.bytes
    val (mime, extension) = when {
        bytes.size > 3 && bytes[0] == 0x89.toByte() && bytes[1] == 'P'.code.toByte() -> "image/png" to "png"
        bytes.size > 2 && bytes[0] == 0xFF.toByte() && bytes[1] == 0xD8.toByte() -> "image/jpeg" to "jpg"
        bytes.size > 3 && String(bytes, 0, 4, Charsets.US_ASCII) == "GIF8" -> "image/gif" to "gif"
        bytes.size > 12 && String(bytes, 8, 4, Charsets.US_ASCII) == "WEBP" -> "image/webp" to "webp"
        else -> "image/png" to "png"
    }
    val base = image.name.substringBeforeLast('.').ifBlank { "image" }
    val values = ContentValues().apply {
        put(MediaStore.Images.Media.DISPLAY_NAME, "$base.$extension")
        put(MediaStore.Images.Media.MIME_TYPE, mime)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            put(MediaStore.Images.Media.RELATIVE_PATH, "Pictures/Pluto")
            put(MediaStore.Images.Media.IS_PENDING, 1)
        }
    }
    val resolver = context.contentResolver
    val uri = resolver.insert(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, values)
        ?: error("The gallery refused the image")
    resolver.openOutputStream(uri)?.use { it.write(bytes) } ?: error("The gallery refused the image")
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
        resolver.update(uri, ContentValues().apply { put(MediaStore.Images.Media.IS_PENDING, 0) }, null, null)
    }
    null
}.getOrElse { it.message ?: "The image could not be saved" }
