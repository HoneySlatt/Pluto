package app.pluto.mobile.ui

// The bottom sheets: the model (effort, Code's mode and access, the models
// by provider), a thread's actions, attach, the draft's project, workspace
// and branch, the Settings and project choices, the confirmations and the
// task actions. A sheet rises on a long deceleration and leaves quicker; it
// follows the finger pulled down by its grab or title, and closes past
// 90dp or on a flick down.

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBars
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.max
import app.pluto.mobile.EXIT_MS
import app.pluto.mobile.LocalModel
import app.pluto.mobile.Sheet
import kotlinx.coroutines.launch

// A sheet's title (empty: none).
@Composable
private fun title(sheet: Sheet): String = when (sheet) {
    Sheet.Model -> "Model"
    Sheet.Attach -> "Attach"
    Sheet.Project -> "Project"
    Sheet.Workspace -> "Workspace"
    Sheet.Branch -> ""
    is Sheet.RemoveProject -> "Remove project"
    is Sheet.DeleteThread -> "Delete thread"
    is Sheet.Rename -> "Rename thread"
    is Sheet.MemoryEdit -> if (sheet.id != null) "Edit memory" else "Add a memory"
    is Sheet.MemoryMenu -> "Memory"
    Sheet.ClearMemories -> "Delete all memories"
    is Sheet.ThreadMenu -> threadMenuTitle(sheet.id)
    is Sheet.Setting -> settingTitle(sheet.key)
    is Sheet.ProjectSetting -> projectSettingTitle(sheet.field)
    is Sheet.TaskMenu -> taskMenuTitle(sheet.id)
    is Sheet.TaskField -> taskFieldTitle(sheet.field)
    is Sheet.License -> licenseTitle(sheet.key)
}

@Composable
fun SheetView(sheet: Sheet, open: Boolean) {
    val model = LocalModel.current
    val nav = model.nav
    val density = LocalDensity.current
    val scope = rememberCoroutineScope()
    // Its offset in pixels below its place; it starts below the screen.
    val offset = remember { Animatable(Float.NaN) }
    BoxWithConstraints(Modifier.fillMaxSize(), contentAlignment = Alignment.BottomCenter) {
        val height = constraints.maxHeight.toFloat()
        LaunchedEffect(open) {
            if (offset.value.isNaN()) offset.snapTo(height)
            if (open) offset.animateTo(0f, tween(380, easing = Enter))
            else offset.animateTo(height, tween(EXIT_MS.toInt() - 20, easing = Exit))
        }
        val bars = WindowInsets.systemBars.asPaddingValues()
        val shape = RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp)
        val drag = Modifier.pointerInput(open) {
            if (!open) return@pointerInput
            val tracker = VelocityTracker()
            detectVerticalDragGestures(
                onDragStart = { tracker.resetTracking() },
                onDragEnd = {
                    val speed = tracker.calculateVelocity().y / density.density
                    val dy = offset.value / density.density
                    if (dy > 90f || (dy > 20f && speed > 500f)) {
                        nav.back()
                    } else {
                        scope.launch { offset.animateTo(0f, tween(300, easing = Enter)) }
                    }
                },
                onDragCancel = { scope.launch { offset.animateTo(0f, tween(300, easing = Enter)) } },
            ) { change, amount ->
                tracker.addPosition(change.uptimeMillis, change.position)
                change.consume()
                val next = offset.value + amount
                // Up, it resists instead of leaving its place.
                scope.launch { offset.snapTo(if (next > 0f) next else offset.value + amount / 4f) }
            }
        }
        Column(
            Modifier
                .fillMaxWidth()
                .heightIn(max = maxHeight * .86f)
                .graphicsLayer { translationY = if (offset.value.isNaN()) height else offset.value }
                .clip(shape)
                .background(P.surface)
                .drawBehind { drawLine(P.input, Offset(0f, 0f), Offset(size.width, 0f), 1.dp.toPx()) }
                .imePadding()
                .padding(bottom = max(bars.calculateBottomPadding(), 12.dp))
                .then(if (open) Modifier else Modifier.pointerInput(Unit) {}),
        ) {
            val title = title(sheet)
            Column(drag.fillMaxWidth()) {
                // The grab is a 4dp line: the finger gets a wider strip.
                Box(Modifier.fillMaxWidth().padding(top = 10.dp, bottom = 4.dp), contentAlignment = Alignment.Center) {
                    Box(Modifier.size(36.dp, 4.dp).clip(RoundedCornerShape(2.dp)).background(P.input))
                }
                if (title.isNotEmpty()) {
                    T(title, 18f, weight = FontWeight.SemiBold, track = -.01f, lines = 1, modifier = Modifier.padding(start = 24.dp, end = 24.dp, top = 6.dp, bottom = 8.dp))
                }
            }
            Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(bottom = 8.dp)) {
                SheetBody(sheet)
            }
        }
    }
}

@Composable
private fun SheetBody(sheet: Sheet) {
    when (sheet) {
        Sheet.Model -> ModelSheet()
        is Sheet.ThreadMenu -> ThreadMenuSheet(sheet.id)
        Sheet.Attach -> AttachSheet()
        Sheet.Project -> ProjectSheet()
        Sheet.Workspace -> WorkspaceSheet()
        Sheet.Branch -> BranchSheet()
        is Sheet.Setting -> SettingSheet(sheet.key)
        is Sheet.ProjectSetting -> ProjectSettingSheet(sheet.project, sheet.field)
        is Sheet.RemoveProject -> RemoveProjectSheet(sheet.project)
        is Sheet.DeleteThread -> DeleteThreadSheet(sheet.id)
        is Sheet.Rename -> RenameSheet(sheet.id)
        is Sheet.TaskMenu -> TaskMenuSheet(sheet.id)
        is Sheet.TaskField -> TaskFieldSheet(sheet.field)
        is Sheet.MemoryEdit -> MemoryEditSheet(sheet.id)
        is Sheet.MemoryMenu -> MemoryMenuSheet(sheet.id)
        Sheet.ClearMemories -> ClearMemoriesSheet()
        is Sheet.License -> LicenseSheet(sheet.key)
    }
}
