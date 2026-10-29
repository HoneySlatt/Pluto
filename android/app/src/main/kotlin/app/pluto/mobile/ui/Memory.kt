package app.pluto.mobile.ui

// Personalization (`#memory`): the custom instructions every chat gets
// ("About you", "How chats should respond"), and what Chat remembers about
// the user: Use memory, Learn from chats, Add a memory, Delete all
// memories, then the saved memories with Edit and Forget. The core
// (memory.rs) reads and writes the chat spaces' files; the instructions
// being typed stay here until Save.

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pluto.core.MemoryChange
import app.pluto.core.MemoryEntry
import app.pluto.core.PersonalizationView
import app.pluto.mobile.LocalModel
import app.pluto.mobile.Sheet

@Composable
fun MemoryPage() {
    val model = LocalModel.current
    val core = model.core
    // The page reads the files again each time it opens.
    remember { core.personalizationLoad() }
    LaunchedEffect(model.revision) { core.personalizationFollow() }
    val view = remember(model.revision) { core.personalization() }
    PageFrame("Personalization") {
        if (!view.hasChats) {
            EmptyPage(
                "No chats yet",
                "Memory belongs to Chat: open Chat once in Pluto on the computer, then what your chats save about you will appear here.",
            )
            return@PageFrame
        }
        T("Custom instructions", 14f, P.muted, FontWeight.Medium, Modifier.padding(start = 8.dp, end = 8.dp, top = 8.dp, bottom = 8.dp))
        Instructions(view)
        GroupLabel("Memory")
        Memories(view)
        PageNote("Temporary chats never read or save memories.")
    }
}

/// A failed read, with Retry.
@Composable
private fun Unreadable(title: String, error: String) {
    val core = LocalModel.current.core
    Column(Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 64.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        T(title, 17f, weight = FontWeight.Medium, align = TextAlign.Center, modifier = Modifier.padding(bottom = 6.dp))
        T(error, 15f, P.muted, align = TextAlign.Center)
        Btn("Retry", Modifier.padding(top = 16.dp)) { core.personalizationLoad() }
    }
}

@Composable
private fun Instructions(view: PersonalizationView) {
    val core = LocalModel.current.core
    when {
        view.instructionsLoading -> LoadingNote()
        view.instructionsError != null -> Unreadable("Could not read the instructions", view.instructionsError!!)
        else -> {
            // The fields start over with each read of the saved text.
            val drafts = remember(view.generation) { view.instructions.map { mutableStateOf(it.saved) } }
            for ((index, field) in view.instructions.withIndex()) {
                InstructionField(field.title, field.description, drafts[index].value) { drafts[index].value = it }
            }
            val dirty = view.instructions.withIndex().any { (index, field) -> drafts[index].value != field.saved }
            Row(
                Modifier.fillMaxWidth().padding(top = 4.dp).heightIn(min = 48.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp, Alignment.End),
            ) {
                when {
                    view.saving -> T("Saving…", 13f, P.muted)
                    dirty -> {
                        Btn("Cancel", height = 44.dp) {
                            for ((index, field) in view.instructions.withIndex()) drafts[index].value = field.saved
                        }
                        Btn("Save", tone = Tone.Primary, height = 44.dp) {
                            core.saveInstructions(drafts.getOrNull(0)?.value.orEmpty(), drafts.getOrNull(1)?.value.orEmpty())
                        }
                    }
                    else -> T("Saved", 13f, P.muted)
                }
            }
        }
    }
}

/// `.instr`: an instruction's card with its field.
@Composable
private fun InstructionField(title: String, description: String, value: String, onValue: (String) -> Unit) {
    var focused by remember { mutableStateOf(false) }
    val border by animateColorAsState(if (focused) P.accent else P.input, tween(150), label = "instr")
    val card = RoundedCornerShape(24.dp)
    val field = RoundedCornerShape(16.dp)
    Column(
        Modifier.padding(bottom = 10.dp).fillMaxWidth().card(P.surface, card).padding(horizontal = 18.dp, vertical = 16.dp),
    ) {
        T(title, 16f, weight = FontWeight.Medium)
        T(description, 13f, P.muted, modifier = Modifier.padding(top = 2.dp, bottom = 10.dp))
        BasicTextField(
            value,
            onValue,
            Modifier
                .fillMaxWidth()
                .heightIn(min = 76.dp)
                .clip(field)
                .background(P.chip)
                .border(1.dp, border, field)
                .onFocus { focused = it }
                .padding(horizontal = 14.dp, vertical = 12.dp),
            textStyle = style(15f),
            cursorBrush = SolidColor(P.accent),
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
        )
    }
}

@Composable
private fun Memories(view: PersonalizationView) {
    val model = LocalModel.current
    val core = model.core
    val nav = model.nav
    when {
        view.memoryLoading -> LoadingNote("Loading memories")
        view.memoryError != null -> Unreadable("Could not read the memories", view.memoryError!!)
        else -> {
            Group {
                SetToggle(
                    "brain",
                    "Use memory",
                    view.enabled,
                    "Chats remember what you share about yourself and use it in later conversations. You can also ask a chat to remember, correct or forget something.",
                    enabled = !view.changing,
                ) { core.changeMemory(MemoryChange.Enabled(it)) }
                SetToggle(
                    "sparkles",
                    "Learn from chats",
                    view.learn,
                    "After a chat goes quiet, Pluto updates your memories with what it taught about you, using your text generation model.",
                    enabled = view.enabled && !view.changing,
                ) { core.changeMemory(MemoryChange.Learn(it)) }
                SetRow(
                    "plus",
                    "Add a memory",
                    detail = "Write something your chats should know about you.",
                    enabled = view.enabled,
                ) { nav.openSheet(Sheet.MemoryEdit(null)) }
                if (view.entries.isNotEmpty()) {
                    SetRow(
                        "trash-2",
                        "Delete all memories",
                        detail = "Forget everything your chats saved.",
                        danger = true,
                        chevron = false,
                    ) { nav.openSheet(Sheet.ClearMemories) }
                }
            }
            GroupLabel(if (view.changing) "Saved memories · saving…" else "Saved memories")
            if (view.entries.isEmpty()) {
                Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 32.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    T("No saved memories", 17f, weight = FontWeight.Medium, align = TextAlign.Center, modifier = Modifier.padding(bottom = 6.dp))
                    T("Tell a chat to remember something, or share what it should know about you.", 15f, P.muted, align = TextAlign.Center)
                }
            } else {
                Box(Modifier.alpha(if (view.enabled) 1f else .45f)) {
                    Group {
                        for (entry in view.entries) androidx.compose.runtime.key(entry.id) { MemoryRow(entry) }
                    }
                }
            }
        }
    }
}

/// `.mem`: a saved memory, when it was saved, and its actions.
@Composable
private fun MemoryRow(entry: MemoryEntry) {
    val nav = LocalModel.current.nav
    Row(
        Modifier.fillMaxWidth().background(P.overlay).padding(start = 18.dp, end = 4.dp, top = 14.dp, bottom = 14.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Column(Modifier.weight(1f)) {
            T(entry.text, 15f, line = 1.45f)
            T(entry.`when`, 12f, P.muted, modifier = Modifier.padding(top = 4.dp))
        }
        Box(
            Modifier.offset(y = (-8).dp).size(44.dp).clip(CircleShape).tap(label = "Actions for this memory") {
                nav.openSheet(Sheet.MemoryMenu(entry.id))
            },
            contentAlignment = Alignment.Center,
        ) { Icon("ellipsis-vertical", P.muted) }
    }
}

/// "Add a memory" (no id) or "Edit memory".
@Composable
fun MemoryEditSheet(id: String?) {
    val model = LocalModel.current
    val core = model.core
    val view = remember(model.revision) { core.personalization() }
    val initial = remember(id) { id?.let { wanted -> core.personalization().entries.find { it.id == wanted }?.text }.orEmpty() }
    var text by remember(id) { mutableStateOf(initial) }
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    Box(Modifier.padding(start = 20.dp, end = 20.dp, bottom = 4.dp)) {
        BigField(text, { text = it }, "What to remember", Modifier.focusRequester(focus))
    }
    SheetActions {
        Btn("Cancel", Modifier.weight(1f)) { model.nav.back() }
        Btn(
            if (id != null) "Save" else "Add memory",
            Modifier.weight(1f),
            tone = Tone.Primary,
            enabled = !view.changing && text.isNotBlank(),
        ) {
            val value = text.trim()
            core.changeMemory(if (id != null) MemoryChange.Edit(id, value) else MemoryChange.Add(value))
            model.nav.back()
        }
    }
}

/// `textarea.big`: the sheet's field.
@Composable
private fun BigField(value: String, onValue: (String) -> Unit, placeholder: String, modifier: Modifier = Modifier, minHeight: Dp = 110.dp) {
    var focused by remember { mutableStateOf(false) }
    val border by animateColorAsState(if (focused) P.accent else P.input, tween(150), label = "big")
    val shape = RoundedCornerShape(20.dp)
    BasicTextField(
        value,
        onValue,
        modifier
            .fillMaxWidth()
            .heightIn(min = minHeight)
            .clip(shape)
            .background(P.chip)
            .border(1.dp, border, shape)
            .onFocus { focused = it }
            .padding(horizontal = 16.dp, vertical = 14.dp),
        textStyle = style(16f),
        cursorBrush = SolidColor(P.accent),
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
        decorationBox = { field ->
            if (value.isEmpty()) T(placeholder, 16f, P.muted)
            field()
        },
    )
}

@Composable
fun MemoryMenuSheet(id: String) {
    val model = LocalModel.current
    SRow("pencil", "Edit") { model.nav.openSheet(Sheet.MemoryEdit(id)) }
    SRow("trash-2", "Forget", danger = true) {
        model.core.changeMemory(MemoryChange.Forget(id))
        model.nav.back()
    }
}

@Composable
fun ClearMemoriesSheet() {
    val model = LocalModel.current
    SheetConfirm("Your chats will forget everything they saved about you. This cannot be undone.")
    SheetActions {
        Btn("Cancel", Modifier.weight(1f)) { model.nav.back() }
        Btn("Delete all", Modifier.weight(1f), tone = Tone.Danger) {
            model.core.changeMemory(MemoryChange.Clear)
            model.nav.back()
        }
    }
}
