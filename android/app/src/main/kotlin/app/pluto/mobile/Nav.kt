package app.pluto.mobile

// What is on screen, layer over layer (the Dioxus app's `nav.rs`): the home
// draft or a thread, the pages pushed from the right, the drawer and a
// bottom sheet. Back closes the top layer. A closed page or sheet stays on
// screen for its exit animation.

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

sealed interface Page {
    data object Settings : Page
    /** A thread opened from a page (a project's threads). */
    data class Thread(val id: String) : Page
    /** The changed files of a thread, from `focus` when a file was picked. */
    data class Diff(val thread: String, val run: String?, val focus: String?) : Page
    data object Projects : Page
    data class Project(val id: String) : Page
    data object Tasks : Page
    /** The task editor: an existing task's id, or a new task. */
    data class TaskEdit(val id: String?) : Page
    data object Usage : Page
    /** Personalization: custom instructions and memory. */
    data object Memory : Page
    data object Licenses : Page
}

sealed interface Sheet {
    data object Model : Sheet
    data class ThreadMenu(val id: String) : Sheet
    data object Attach : Sheet
    data object Project : Sheet
    data object Workspace : Sheet
    data object Branch : Sheet
    /** A Settings choice by key. */
    data class Setting(val key: String) : Sheet
    /** A project's New threads default (`model`, `access`, `workspace`). */
    data class ProjectSetting(val project: String, val field: String) : Sheet
    data class RemoveProject(val project: String) : Sheet
    data class DeleteThread(val id: String) : Sheet
    data class Rename(val id: String) : Sheet
    data class TaskMenu(val id: String) : Sheet
    /** A task editor field (`project`, `workspace`, `model`). */
    data class TaskField(val field: String) : Sheet
    /** Add a memory (`null`) or edit one. */
    data class MemoryEdit(val id: String?) : Sheet
    data class MemoryMenu(val id: String) : Sheet
    data object ClearMemories : Sheet
    /** A license's text by key. */
    data class License(val key: String) : Sheet
}

private sealed interface Layer {
    data object Drawer : Layer
    /** The base view changed; going back opens `prev` again. */
    data class View(val prev: String?) : Layer
    /** A page; a thread page restores the selection it replaced. */
    data class Page(val restores: Boolean, val prev: String?) : Layer
    data object Sheet : Layer
}

// How long a closed page or sheet stays for its exit.
const val EXIT_MS = 240L

class Nav(private val model: Model) {
    var drawer by mutableStateOf(false)
        private set
    val pages = mutableStateListOf<Page>()
    var sheet by mutableStateOf<Sheet?>(null)
        private set
    // The page and the sheet on their way out.
    var leavingPage by mutableStateOf<Page?>(null)
        private set
    var leavingSheet by mutableStateOf<Sheet?>(null)
        private set
    private var exits = 0L
    private var restore: Pair<Long, String?>? = null
    private val layers = mutableStateListOf<Layer>()
    private val scope = CoroutineScope(Dispatchers.Main)

    private val core get() = model.core
    private fun selected(): String? = core.shell().selected

    val canGoBack get() = layers.isNotEmpty()

    // The back gesture's progress over the top layer, 0 to 1, while the
    // finger holds it (predictive back): the layer follows it.
    var backProgress by mutableFloatStateOf(0f)

    // The top layer's kind: `drawer`, `page`, `sheet` or `view`.
    val top: String?
        get() = when (layers.lastOrNull()) {
            Layer.Drawer -> "drawer"
            is Layer.Page -> "page"
            Layer.Sheet -> "sheet"
            is Layer.View -> "view"
            null -> null
        }

    // Closes the top layer, the way the back gesture does.
    fun back() {
        when (val layer = layers.removeLastOrNull() ?: return) {
            Layer.Drawer -> drawer = false
            is Layer.View -> core.select(layer.prev)
            is Layer.Page -> {
                val serial = ++exits
                leavingPage = pages.removeLastOrNull()
                // A thread page keeps its thread while it slides out.
                restore = if (layer.restores) serial to layer.prev else null
                finishExit(serial)
            }
            Layer.Sheet -> {
                val serial = ++exits
                leavingSheet = sheet
                sheet = null
                finishExit(serial)
            }
        }
    }

    // Drops the leaving page or sheet once its exit played, and gives a
    // thread page's replaced selection back.
    private fun finishExit(serial: Long) {
        scope.launch {
            delay(EXIT_MS)
            if (exits == serial) {
                leavingPage = null
                leavingSheet = null
            }
            restore?.takeIf { it.first == serial }?.let {
                restore = null
                core.select(it.second)
            }
        }
    }

    // Replaces the drawer or a sheet on top with `layer`, else pushes it.
    private fun place(layer: Layer) {
        when (layers.lastOrNull()) {
            Layer.Drawer -> {
                drawer = false
                layers[layers.lastIndex] = layer
            }
            Layer.Sheet -> {
                sheet = null
                layers[layers.lastIndex] = layer
            }
            else -> layers.add(layer)
        }
    }

    // The drawer opens over the base views only: a page over them would
    // stay in front of the thread it opens.
    fun openDrawer() {
        if (drawer || pages.isNotEmpty() || sheet != null) return
        drawer = true
        layers.add(Layer.Drawer)
    }

    // A page still leaving gives its place, and its selection, at once.
    private fun settle() {
        leavingPage = null
        restore?.let {
            restore = null
            core.select(it.second)
        }
    }

    // Opens `id` as the base view (from the drawer, the home chips).
    fun openThread(id: String) {
        settle()
        val prev = selected()
        if (prev == id) {
            if (layers.lastOrNull() == Layer.Drawer) back()
            return
        }
        place(Layer.View(prev))
        core.select(id)
    }

    // The home draft of the current space.
    fun goHome() {
        settle()
        val prev = selected() ?: run {
            if (layers.lastOrNull() == Layer.Drawer) back()
            return
        }
        place(Layer.View(prev))
        core.select(null)
    }

    fun openPage(page: Page) {
        settle()
        val layer = if (page is Page.Thread) {
            val prev = selected()
            core.select(page.id)
            Layer.Page(true, prev)
        } else {
            Layer.Page(false, null)
        }
        place(layer)
        pages.add(page)
    }

    fun openSheet(next: Sheet) {
        leavingSheet = null
        if (sheet != null) {
            sheet = next
            return
        }
        sheet = next
        layers.add(Layer.Sheet)
    }

    // Leaves every layer at once (Disconnect, a project's New thread).
    fun reset() {
        layers.clear()
        drawer = false
        pages.clear()
        sheet = null
        leavingPage = null
        leavingSheet = null
        restore = null
    }

    // Leaves the server: the layers, the drafts and the session's data of
    // that server go; the next server starts clean.
    fun disconnect() {
        reset()
        model.texts.clear()
        core.disconnect()
    }
}
