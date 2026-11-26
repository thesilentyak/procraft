package io.github.artcraftandroid.procraft.generation

import android.content.Context
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.artcraftandroid.procraft.canvas.CanvasViewModel
import io.github.artcraftandroid.procraft.core.Generation
import io.github.artcraftandroid.procraft.core.GenerationException
import io.github.artcraftandroid.procraft.core.ParameterInfo
import io.github.artcraftandroid.procraft.core.ParameterKind
import io.github.artcraftandroid.procraft.core.ParameterValue
import io.github.artcraftandroid.procraft.core.WorkflowCard
import io.github.artcraftandroid.procraft.core.WorkflowRun
import io.github.artcraftandroid.procraft.ui.Pc
import io.github.artcraftandroid.procraft.ui.SliderRow
import java.io.File

/**
 * Workflows from ComfyUI in the Generate popover (docs/generation/10-workflows-design.md): their
 * cards in the style row, and with one chosen, its rows under the prompt, folded under More but
 * the prompts. The prompt field itself stays the popover's own, shared with the styles: the
 * workflow's first prompt gets it. The kept workflows are `.json` files in the styles folder,
 * beside the styles; the live one is kept in the app's own files between starts.
 */
internal object Workflows {
    private val WARN = Color(0xFFF2B440)
    private val Field get() = if (Pc.light) Color(0x12000000) else Color(0x12FFFFFF)
    private val RowFill get() = if (Pc.light) Color(0xFFD8D8D8) else Color(0x0DFFFFFF)
    private val Card get() = if (Pc.light) Color(0xFFD7D7D7) else Color(0x2E5B5B5B)
    private val Dim get() = Pc.TextDim
    private val Ink get() = Pc.Text
    private const val LIVE_FILE = "live-workflow.json"

    /** The chosen workflow card's id; none while a style is chosen. */
    var chosen by mutableStateOf<String?>(null)
    /** Counts the times the folder was read, so the cards follow it. */
    var read by mutableStateOf(0)
    private var revision = 0uL
    private var moreOpen by mutableStateOf(false)
    // the chosen workflow's rows' values as typed (by name), and which workflow they are for
    private val values = mutableStateMapOf<String, String>()
    private var valuesFor: String? = null
    // a card's menu (its id), Keep's name, a kept card asked about deleting
    private var menu by mutableStateOf<String?>(null)
    private var keepName by mutableStateOf("")
    private var confirmDelete by mutableStateOf(false)
    var problem by mutableStateOf<String?>(null)

    private fun prefs(context: Context) = context.getSharedPreferences("generation", Context.MODE_PRIVATE)

    /** On the app's first look: the live workflow from the last start, the files, the card chosen. */
    fun start(context: Context, g: Generation) {
        val live = File(context.filesDir, LIVE_FILE)
        g.setLiveWorkflow(runCatching { live.readText() }.getOrNull())
        g.setWorkflowFiles(StylesFolder.readWorkflows(context))
        chosen = prefs(context).getString("workflow", null)
    }

    /** The folder read again (the popover opened). */
    fun reread(context: Context, g: Generation) {
        g.setWorkflowFiles(StylesFolder.readWorkflows(context))
        read++
    }

    /** Each look at the server: a new live workflow is kept for the next start. */
    fun poll(context: Context?, g: Generation) {
        val now = g.liveWorkflowRevision()
        if (now == revision || context == null) return
        revision = now
        // the cards and rows follow it while the popover is open
        read++
        val text = g.liveWorkflowText() ?: return
        runCatching { File(context.filesDir, LIVE_FILE).writeText(text) }
    }

    /** A workflow card or none was chosen. */
    fun choose(context: Context, id: String?) {
        chosen = id
        problem = null
        prefs(context).edit().putString("workflow", id).apply()
    }

    /** Why the chosen workflow can't run; none when it can (or none is chosen). */
    fun reason(g: Generation): String? {
        val id = chosen ?: return null
        return g.workflowCards().firstOrNull { it.id == id }?.reason ?: if (g.workflowCards().none { it.id == id }) "Pick a workflow" else null
    }

    /** The cards, after the styles in the style row; a hold opens Keep (the live one) or the kept one's menu. */
    @Composable
    fun Cards(g: Generation, onChoose: () -> Unit) {
        val context = LocalContext.current
        val cards = remember(read, g.liveWorkflowRevision(), g.server().connection) { g.workflowCards() }
        // a chosen card that's gone (its file deleted): back to the styles
        if (chosen != null && cards.none { it.id == chosen }) chosen = null
        for (card in cards) {
            val shape = RoundedCornerShape(12.dp)
            Box {
                Column(
                    Modifier.width(116.dp).height(76.dp).clip(shape).background(Card)
                        .then(if (card.id == chosen) Modifier.border(2.dp, Pc.Accent, shape) else Modifier)
                        .combinedClickable(remember { MutableInteractionSource() }, indication = null,
                            onLongClick = { menu = card.id; keepName = ""; confirmDelete = false },
                            onClick = { choose(context, card.id); onChoose() })
                        .padding(10.dp),
                ) {
                    Row(Modifier.weight(1f)) {
                        Text(card.name, color = Ink, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                        Badge()
                    }
                    Text(if (card.reason != null) (if (card.live) "Waiting for ComfyUI" else "Can't be read") else if (card.live) "Follows ComfyUI" else "Kept", color = if (card.reason != null) WARN else Dim, fontSize = 11.sp, maxLines = 1)
                }
                if (menu == card.id) Menu(g, card)
            }
        }
    }

    @Composable
    private fun Badge() {
        val (ink, fill) = if (Pc.light) Color(0xFF1F6F5C) to Color(0x2630B58F) else Color(0xFF8FE0C8) to Color(0x3330B58F)
        Box(Modifier.padding(start = 4.dp).height(16.dp).clip(RoundedCornerShape(8.dp)).background(fill).padding(horizontal = 6.dp), contentAlignment = Alignment.Center) {
            Text("Workflow", color = ink, fontSize = 10.sp, fontWeight = FontWeight.SemiBold, maxLines = 1)
        }
    }

    /** Under a card: Keep (a name, then Keep) for the live one; Export and Delete for a kept one. */
    @Composable
    private fun Menu(g: Generation, card: WorkflowCard) {
        val context = LocalContext.current
        val down = with(LocalDensity.current) { (76.dp + 6.dp).roundToPx() }
        androidx.compose.ui.window.Popup(
            offset = androidx.compose.ui.unit.IntOffset(0, down),
            onDismissRequest = { menu = null },
            properties = androidx.compose.ui.window.PopupProperties(focusable = true),
        ) {
            Column(Modifier.width(240.dp).clip(RoundedCornerShape(14.dp)).background(if (Pc.light) Color(0xF7FAFAFA) else Color(0xF7313135)).padding(vertical = 4.dp)) {
                if (card.live) {
                    Text("Keep a copy of this workflow as a card of its own", color = Dim, fontSize = 12.sp, modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 10.dp))
                    Box(Modifier.padding(12.dp).fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(Field).padding(horizontal = 10.dp, vertical = 9.dp)) {
                        if (keepName.isEmpty()) Text("Name", color = Pc.Disabled, fontSize = 15.sp)
                        BasicTextField(keepName, { keepName = it.take(60) }, singleLine = true, textStyle = TextStyle(color = Ink, fontSize = 15.sp), cursorBrush = SolidColor(Pc.Accent), modifier = Modifier.fillMaxWidth())
                    }
                    Item("Keep", enabled = keepName.isNotBlank()) {
                        menu = null
                        problem = try {
                            val file = g.keepWorkflow(keepName)
                            StylesFolder.write(context, file.name, file.text, null)?.let { "Couldn't keep it: $it" }
                        } catch (e: GenerationException) {
                            e.message
                        }
                        reread(context, g)
                        if (problem == null) choose(context, "file:${keepName.trim().let { if (it.endsWith(".json", true)) it else "$it.json" }}")
                    }
                } else {
                    val file = card.id.removePrefix("file:")
                    Item("Export…") {
                        menu = null
                        StylesFolder.export(context, file)?.let { problem = "Couldn't export it: $it" }
                    }
                    Item(if (confirmDelete) "Tap again to delete" else "Delete…", danger = true) {
                        if (!confirmDelete) {
                            confirmDelete = true
                        } else {
                            menu = null
                            StylesFolder.delete(context, file)?.let { problem = "Couldn't delete it: $it" }
                            reread(context, g)
                        }
                    }
                }
            }
        }
    }

    @Composable
    private fun Item(words: String, enabled: Boolean = true, danger: Boolean = false, onClick: () -> Unit) {
        Text(
            words, color = when { !enabled -> Dim; danger -> Color(0xFFE84F51); else -> Ink }, fontSize = 15.sp,
            modifier = Modifier.fillMaxWidth().tap(enabled, onClick).padding(horizontal = 16.dp, vertical = 13.dp),
        )
    }

    /** The chosen workflow's rows but its first prompt (the popover's prompt field is that): other
     *  prompts in view, the rest under More. */
    @Composable
    fun Parameters(vm: CanvasViewModel, g: Generation) {
        val context = LocalContext.current
        val id = chosen ?: return
        val params = remember(id, read, g.liveWorkflowRevision(), g.server().connection) { g.workflowParameters(id) }
        if (valuesFor != id) {
            valuesFor = id
            values.clear()
            val p = prefs(context)
            for (param in params) p.getString("wf:$id:${param.name}", null)?.let { values[param.name] = it }
        }
        fun set(name: String, value: String) {
            values[name] = value
            prefs(context).edit().putString("wf:$id:$name", value).apply()
        }
        val prompt = params.firstOrNull { it.kind == ParameterKind.PROMPT }
        val inView = params.filter { it.inView && it != prompt }
        val folded = params.filter { !it.inView }
        for (p in inView) ParamRow(vm, p, values[p.name] ?: p.default) { set(p.name, it) }
        if (folded.isNotEmpty()) {
            Row(Modifier.padding(start = 18.dp, end = 18.dp, top = 12.dp, bottom = 6.dp).tap { moreOpen = !moreOpen }, verticalAlignment = Alignment.CenterVertically) {
                Text("MORE", color = Dim, fontSize = 12.sp, fontWeight = FontWeight.Medium)
                Text(if (moreOpen) "  ⌃" else "  ⌄", color = Dim, fontSize = 13.sp)
            }
            if (moreOpen) {
                for (p in folded) ParamRow(vm, p, values[p.name] ?: p.default) { set(p.name, it) }
                Text("Back to defaults", color = Pc.Accent, fontSize = 14.sp, modifier = Modifier.padding(start = 18.dp, top = 8.dp, bottom = 4.dp).tap {
                    val edit = prefs(context).edit()
                    for (p in params) edit.remove("wf:$id:${p.name}")
                    edit.apply()
                    values.clear()
                })
            }
        }
        if (prompt == null && params.isEmpty()) Text("This workflow has nothing to set: Generate runs it.", color = Dim, fontSize = 13.sp, modifier = Modifier.padding(start = 18.dp, end = 18.dp, top = 8.dp))
    }

    /** One row: its control for its kind. */
    @Composable
    private fun ParamRow(vm: CanvasViewModel, p: ParameterInfo, value: String, set: (String) -> Unit) {
        val focus = LocalFocusManager.current
        when (p.kind) {
            ParameterKind.TOGGLE -> Row(Modifier.padding(horizontal = 16.dp, vertical = 4.dp).fillMaxWidth().height(44.dp).clip(RoundedCornerShape(12.dp)).background(RowFill).tap { set((!isOn(value)).toString()) }.padding(horizontal = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(p.name, color = Ink, fontSize = 14.sp, modifier = Modifier.weight(1f))
                Switch(isOn(value))
            }
            ParameterKind.NUMBER, ParameterKind.INTEGER -> {
                val min = p.min
                val max = p.max
                val number = value.toDoubleOrNull() ?: 0.0
                val shown = if (p.kind == ParameterKind.INTEGER) Math.round(number).toString() else "%.2f".format(number)
                if (min != null && max != null && max > min) {
                    Box(Modifier.padding(horizontal = 6.dp)) {
                        SliderRow(p.name, ((number - min) / (max - min)).toFloat(), shown) { t ->
                            val v = min + (max - min) * t
                            set(if (p.kind == ParameterKind.INTEGER) Math.round(v).toString() else "%.2f".format(java.util.Locale.ROOT, v))
                        }
                    }
                } else {
                    Field(p.name, value, KeyboardType.Decimal, set)
                }
            }
            ParameterKind.CHOICE -> Choice(p, value, set)
            ParameterKind.IMAGE_LAYER, ParameterKind.MASK_LAYER -> LayerPicker(vm, p, value, set)
            ParameterKind.PROMPT, ParameterKind.NEGATIVE_PROMPT -> {
                Text(p.name.uppercase(), color = Dim, fontSize = 12.sp, fontWeight = FontWeight.Medium, modifier = Modifier.padding(start = 18.dp, top = 12.dp, bottom = 6.dp))
                Box(Modifier.padding(horizontal = 16.dp).fillMaxWidth().heightIn(min = 56.dp).clip(RoundedCornerShape(12.dp)).background(Field).padding(horizontal = 12.dp, vertical = 10.dp)) {
                    BasicTextField(value, set, textStyle = TextStyle(color = Ink, fontSize = 15.sp), cursorBrush = SolidColor(Pc.Accent), maxLines = 4,
                        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done), keyboardActions = KeyboardActions(onDone = { focus.clearFocus() }), modifier = Modifier.fillMaxWidth())
                }
            }
            ParameterKind.TEXT -> Field(p.name, value, KeyboardType.Text, set)
        }
    }

    @Composable
    private fun Field(name: String, value: String, keyboard: KeyboardType, set: (String) -> Unit) {
        val focus = LocalFocusManager.current
        Row(Modifier.padding(horizontal = 16.dp, vertical = 4.dp).fillMaxWidth().height(44.dp).clip(RoundedCornerShape(12.dp)).background(RowFill).padding(horizontal = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(name, color = Ink, fontSize = 14.sp, maxLines = 1, modifier = Modifier.weight(1f))
            BasicTextField(value, set, singleLine = true, modifier = Modifier.width(150.dp),
                textStyle = TextStyle(color = Ink, fontSize = 14.sp, textAlign = androidx.compose.ui.text.style.TextAlign.End), cursorBrush = SolidColor(Pc.Accent),
                keyboardOptions = KeyboardOptions(keyboardType = keyboard, imeAction = ImeAction.Done), keyboardActions = KeyboardActions(onDone = { focus.clearFocus() }))
        }
    }

    /** A list: the chosen one shown, a tap opens the others. */
    @Composable
    private fun Choice(p: ParameterInfo, value: String, set: (String) -> Unit) {
        var open by remember { mutableStateOf(false) }
        Box {
            Row(Modifier.padding(horizontal = 16.dp, vertical = 4.dp).fillMaxWidth().height(44.dp).clip(RoundedCornerShape(12.dp)).background(RowFill).tap { open = true }.padding(horizontal = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(p.name, color = Ink, fontSize = 14.sp, maxLines = 1, modifier = Modifier.weight(1f))
                Text(value.ifEmpty { p.choices.firstOrNull() ?: "" }, color = Dim, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.width(170.dp), textAlign = androidx.compose.ui.text.style.TextAlign.End)
                Text("  ›", color = Dim, fontSize = 16.sp)
            }
            if (open) Options(p.choices, onDismiss = { open = false }) { set(it); open = false }
        }
    }

    /** A layer picker: the layer named like the row, else the current one, until another is picked. */
    @Composable
    private fun LayerPicker(vm: CanvasViewModel, p: ParameterInfo, value: String, set: (String) -> Unit) {
        var open by remember { mutableStateOf(false) }
        val layers = vm.session?.layers() ?: emptyList()
        val picked = value.toULongOrNull()?.let { id -> layers.firstOrNull { it.id == id } }
        val shown = picked?.name ?: (layers.firstOrNull { it.name.trim().equals(p.name.trim(), true) } ?: layers.firstOrNull { it.current })?.name ?: "No layer"
        Box {
            Row(Modifier.padding(horizontal = 16.dp, vertical = 4.dp).fillMaxWidth().height(44.dp).clip(RoundedCornerShape(12.dp)).background(RowFill).tap { open = true }.padding(horizontal = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(p.name, color = Ink, fontSize = 14.sp, maxLines = 1, modifier = Modifier.weight(1f))
                Text(shown, color = Dim, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.width(170.dp), textAlign = androidx.compose.ui.text.style.TextAlign.End)
                Text("  ›", color = Dim, fontSize = 16.sp)
            }
            if (open) Options(layers.map { it.name }, onDismiss = { open = false }) { name -> layers.firstOrNull { it.name == name }?.let { set(it.id.toString()) }; open = false }
        }
    }

    @Composable
    private fun Options(options: List<String>, onDismiss: () -> Unit, pick: (String) -> Unit) {
        val down = with(LocalDensity.current) { 50.dp.roundToPx() }
        androidx.compose.ui.window.Popup(offset = androidx.compose.ui.unit.IntOffset(0, down), onDismissRequest = onDismiss, properties = androidx.compose.ui.window.PopupProperties(focusable = true)) {
            Column(Modifier.width(260.dp).heightIn(max = 320.dp).clip(RoundedCornerShape(14.dp)).background(if (Pc.light) Color(0xF7FAFAFA) else Color(0xF7313135))
                .verticalScroll(rememberScrollState())) {
                for (o in options) Text(o, color = Ink, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.fillMaxWidth().tap { pick(o) }.padding(horizontal = 16.dp, vertical = 12.dp))
            }
        }
    }

    /** Starts the chosen workflow with `prompt` as its first prompt; why it couldn't, or null. */
    fun run(vm: CanvasViewModel, g: Generation, prompt: String, seed: ULong?, count: Int): String? {
        val session = vm.session ?: return "No artwork is open"
        val id = chosen ?: return "Pick a workflow"
        val params = g.workflowParameters(id)
        val out = mutableListOf<ParameterValue>()
        val first = params.firstOrNull { it.kind == ParameterKind.PROMPT }
        for (p in params) {
            val v = if (p == first) prompt else values[p.name] ?: continue
            out += when (p.kind) {
                ParameterKind.TOGGLE -> ParameterValue.Toggle(p.name, isOn(v))
                ParameterKind.NUMBER, ParameterKind.INTEGER -> ParameterValue.Number(p.name, v.toDoubleOrNull() ?: continue)
                ParameterKind.IMAGE_LAYER, ParameterKind.MASK_LAYER -> ParameterValue.Layer(p.name, v.toULongOrNull() ?: continue)
                else -> ParameterValue.Text(p.name, v)
            }
        }
        problem = null
        return try {
            g.generateWorkflow(session, WorkflowRun(id, out, seed, count.toUInt()))
            null
        } catch (e: GenerationException) {
            e.message
        }
    }

    private fun isOn(value: String) = value.trim().lowercase() in setOf("true", "1", "yes", "on")

    @Composable
    private fun Switch(on: Boolean) {
        Box(Modifier.width(50.dp).height(30.dp).clip(RoundedCornerShape(15.dp)).background(if (on) Pc.Accent else Pc.SwitchOff), contentAlignment = if (on) Alignment.CenterEnd else Alignment.CenterStart) {
            Box(Modifier.padding(3.dp).size(24.dp).clip(CircleShape).background(Color.White))
        }
    }

    private fun Modifier.tap(enabled: Boolean = true, onClick: () -> Unit): Modifier = composed {
        clickable(remember { MutableInteractionSource() }, indication = null, enabled = enabled, onClick = onClick)
    }
}
