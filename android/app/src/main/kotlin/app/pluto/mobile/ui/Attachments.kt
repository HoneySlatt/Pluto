package app.pluto.mobile.ui

// A composer's attachments (the core's `attachments.rs`): picked from the
// photos, the camera or the files, read off the main thread, and shown
// above the composer's field as thumbnails and file chips, each with its
// upload state and a remove button. A photo the providers do not take
// (HEIC) or larger than 10 MiB is sent as a JPEG of at most 3072 px.

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.provider.OpenableColumns
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.pluto.core.AttachmentView
import app.pluto.core.Core
import app.pluto.core.UploadState
import app.pluto.mobile.LocalModel
import java.io.ByteArrayOutputStream

// The size a file may have to be read at all (the core refuses beyond).
private const val MAX_READ = 50L * 1024 * 1024
private const val MAX_IMAGE = 10L * 1024 * 1024
private val PROVIDER_IMAGES = setOf("image/png", "image/jpeg", "image/gif", "image/webp")

// Thumbnails by attachment id, decoded once.
private val thumbnails = mutableStateMapOf<String, ImageBitmap>()

// Reads `uris` and attaches them to `composer`; runs off the main thread.
fun attachUris(context: Context, core: Core, composer: String, uris: List<Uri>) {
    val resolver = context.contentResolver
    for (uri in uris) {
        val error = runCatching {
            var name = "attachment"
            var size = -1L
            resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { cursor ->
                if (cursor.moveToFirst()) {
                    cursor.getString(0)?.let { name = it }
                    if (!cursor.isNull(1)) size = cursor.getLong(1)
                }
            }
            if (size > MAX_READ && resolver.getType(uri)?.startsWith("image/") != true) {
                return@runCatching "'$name' is larger than 50 MB."
            }
            var mime = resolver.getType(uri) ?: "application/octet-stream"
            var bytes = resolver.openInputStream(uri)?.use { it.readBytes() } ?: return@runCatching "'$name' could not be read."
            if (mime.startsWith("image/") && (mime !in PROVIDER_IMAGES || bytes.size > MAX_IMAGE)) {
                jpeg(bytes)?.let { converted ->
                    bytes = converted
                    mime = "image/jpeg"
                    name = name.substringBeforeLast('.') + ".jpg"
                }
            }
            core.attach(composer, name, mime, bytes)
        }.getOrElse { it.message ?: "The file could not be read." }
        if (error != null) core.toast(error)
    }
}

// A photo as a JPEG of at most 3072 px on its longer side.
private fun jpeg(bytes: ByteArray): ByteArray? {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
    val longer = maxOf(bounds.outWidth, bounds.outHeight)
    if (longer <= 0) return null
    var sample = 1
    while (longer / (sample * 2) >= 3072) sample *= 2
    val decoded = BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = sample })
        ?: return null
    val scale = minOf(1f, 3072f / maxOf(decoded.width, decoded.height))
    val bitmap = if (scale < 1f) {
        Bitmap.createScaledBitmap(decoded, (decoded.width * scale).toInt(), (decoded.height * scale).toInt(), true)
    } else decoded
    return ByteArrayOutputStream().use { out ->
        bitmap.compress(Bitmap.CompressFormat.JPEG, 88, out)
        out.toByteArray()
    }
}

// The attachments above a composer's field.
@Composable
fun AttachmentsRow(composer: String, list: List<AttachmentView>) {
    if (list.isEmpty()) return
    Row(
        Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(start = 12.dp, end = 12.dp, top = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        for (attachment in list) {
            androidx.compose.runtime.key(attachment.id) {
                if (attachment.image) ImageTile(composer, attachment) else FileChip(composer, attachment)
            }
        }
    }
}

@Composable
private fun ImageTile(composer: String, attachment: AttachmentView) {
    val core = LocalModel.current.core
    val thumbnail = thumbnails[attachment.id] ?: remember(attachment.id) {
        core.attachmentPreview(composer, attachment.id)?.let { bytes ->
            val options = BitmapFactory.Options().apply { inSampleSize = 4 }
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)?.asImageBitmap()
        }?.also { thumbnails[attachment.id] = it }
    }
    val shape = RoundedCornerShape(14.dp)
    Box(Modifier.size(64.dp)) {
        Box(Modifier.fillMaxSize().card(P.chip, shape), contentAlignment = Alignment.Center) {
            if (thumbnail != null) {
                Image(thumbnail, attachment.name, Modifier.fillMaxSize().clip(shape), contentScale = ContentScale.Crop)
            } else {
                Icon("image", P.muted)
            }
        }
        Remove(composer, attachment, Modifier.align(Alignment.TopEnd).offset(x = 4.dp, y = (-4).dp))
    }
}

@Composable
private fun FileChip(composer: String, attachment: AttachmentView) {
    val core = LocalModel.current.core
    val failed = attachment.state as? UploadState.Failed
    val shape = RoundedCornerShape(14.dp)
    Box {
        Row(
            Modifier
                .height(64.dp)
                .widthIn(max = 220.dp)
                .card(P.chip, shape, if (failed != null) P.error.copy(alpha = .5f) else P.border)
                .then(if (failed != null) Modifier.tap { core.toast(failed.error) } else Modifier)
                .padding(start = 12.dp, end = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Box(
                Modifier.size(36.dp).clip(RoundedCornerShape(10.dp)).background(P.overlay),
                contentAlignment = Alignment.Center,
            ) {
                when (attachment.state) {
                    is UploadState.Uploading -> Spinner(P.accent, 18.dp)
                    is UploadState.Failed -> Icon("circle-alert", P.error, size = 18.dp)
                    is UploadState.Ready -> Icon("file", P.muted2, size = 18.dp)
                }
            }
            Column {
                T(attachment.name, 14f, weight = FontWeight.Medium, lines = 1, line = 1.3f)
                T(
                    when (attachment.state) {
                        is UploadState.Uploading -> "Uploading…"
                        is UploadState.Failed -> "Upload failed"
                        is UploadState.Ready -> attachment.size
                    },
                    12f,
                    if (failed != null) P.error else P.muted,
                    lines = 1,
                    line = 1.3f,
                )
            }
        }
        Remove(composer, attachment, Modifier.align(Alignment.TopEnd).offset(x = 4.dp, y = (-4).dp))
    }
}

// The round remove button on an attachment's corner.
@Composable
private fun Remove(composer: String, attachment: AttachmentView, modifier: Modifier) {
    val core = LocalModel.current.core
    Box(
        modifier
            .size(22.dp)
            .clip(CircleShape)
            .background(P.overlay)
            .border(1.dp, P.input, CircleShape)
            .tap(label = "Remove ${attachment.name}") {
                thumbnails.remove(attachment.id)
                core.detach(composer, attachment.id)
            },
        contentAlignment = Alignment.Center,
    ) { Icon("x", P.text, size = 12.dp, stroke = 2.25f) }
}
