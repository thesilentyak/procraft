package io.github.artcraftandroid.procraft.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.artcraftandroid.procraft.canvas.CanvasViewModel
import io.github.artcraftandroid.procraft.canvas.TimeLapseSettings
import io.github.artcraftandroid.procraft.gallery.CanvasPreset
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

private val ScreenFill get() = if (Pc.light) Color(0xFFF2F2F2) else Color(0xFF1F1F1F)
private val PanelFill get() = if (Pc.light) Color(0xFFDFDFDF) else Color(0xFF282828)
private val Ink get() = if (Pc.light) Color(0xFF3E3E3E) else Color(0xFFAAAAAA)
private val IdleInk get() = if (Pc.light) Color(0xFF3E3E3E) else Color(0xFF797979)
private val Editing = Color(0xFF436BF0)
private val KeypadFill get() = if (Pc.light) Color(0xFFD1D3D8) else Color(0xFF1A1A1A)
private val DigitFill get() = if (Pc.light) Color(0xFFFFFFFF) else Color(0xFF525252)
private val KeyFill get() = if (Pc.light) Color(0xFFABB2BB) else Color(0xFF2B2B2B)
private val UnitChosen get() = if (Pc.light) Color(0xFF818C96) else Color(0xFFA1A1A1)
private val SwitchTrack get() = if (Pc.light) Color(0xFFD5D5D5) else Color(0xFF353535)
private val SwitchIdle get() = if (Pc.light) Color(0xFF3B3B3B) else Color(0xFFC1C1C1)
private val CapsuleFill get() = if (Pc.light) Color(0xFFFBFBFB) else Color(0xFF1A1A1A)
private val CapsuleEdge get() = if (Pc.light) Color(0xFFE0E0E0) else Color(0xFF373737)
private val DoneBlue get() = Pc.tinted(Color(0xFF0190FF))

/** Where the last row the keypad types into (DPI) ends, down from the top: the page's 120, two
 * rows and their line, 24 between the groups, one row. */
private val TypedBottom = 286.dp

/**
 * How Custom Canvas lays out in a window [width] across. Wider than 700 the sections are a
 * rounded panel 319 across, 10 in from the window's left, top and bottom, under its title, and the
 * page starts 372 in. In a narrower window (split screen, a phone) they are a rail of icons 81
 * across, flush with the window's left, top and bottom, untitled; the page starts 123 in and the
 * name 111 in, from its left.
 */
internal class CanvasSheetFit(width: Dp) {
    val narrow = width <= 700.dp
    val railWidth = if (narrow) 81.dp else 319.dp
    val railInset = if (narrow) 0.dp else 10.dp
    val railCorner = if (narrow) 0.dp else 20.dp
    val pageStart = if (narrow) 123.dp else 372.dp

    /** The name's left and its width, in a narrow window (20 clear of Cancel and Done). */
    val nameStart = 111.dp
    val nameWidth = (width - nameStart - 180.dp).coerceAtLeast(0.dp)

    /** A section's cell, 8 in from the panel's left and 52 tall: in the rail as wide as it leaves
     * (8 in on each side). */
    val cellWidth = if (narrow) railWidth - 16.dp else 304.dp
}

/**
 * The keypad's sizes in a window [width] across, upright or [wide], with [room] left under the
 * rows it types into.
 *
 * - Up to 420 across it is compact: five columns 5 apart (units, three of digits, Backspace /
 *   Next / Done) in a cluster up to 320 across, 8 clear of the window's sides; keys 60 tall, 5
 *   apart, 12 under the panel's top and 9 over its bottom. Backspace is a row tall, Next 9 under
 *   it fills to the second row's foot, and Done is two rows tall. Short unit names, ⌫ for
 *   Backspace.
 * - Wider, it is the tablet's: upright keys 120 x 71 (8 apart down) in a 342 panel; wide 150 x
 *   87.5 (9 apart) in a 457 one, the rows from 47 below its top; 9 apart across, 30 between the
 *   three groups. Where the window is narrower than that and 16, the keys, gaps and words are that
 *   much narrower and smaller.
 *
 * Where the window leaves it less height than that ([room]), it and its keys are that much shorter
 * (to half), so it stays under the rows it types into.
 */
internal class KeypadFit(width: Dp, wide: Boolean, room: Dp) {
    val compact = width <= 420.dp
    private val full = if (compact) 276.dp else if (wide) 457.dp else 342.dp
    private val fit = (room / full).coerceIn(0.5f, 1f)

    /** How much of the tablet's width the keys keep (1 in a compact keypad, whose keys are its own). */
    val scale = if (compact) 1f else ((width - 16.dp) / (if (wide) 828.dp else 678.dp)).coerceIn(0f, 1f)

    /** A column of keys across, and a key's height. */
    val keyWidth = if (compact) ((minOf(320.dp, width - 16.dp) - 20.dp) / 5).coerceAtLeast(0.dp) else (if (wide) 150.dp else 120.dp) * scale
    val keyHeight = (if (compact) 60.dp else if (wide) 87.5.dp else 71.dp) * fit

    /** Between keys across, between rows, and between the three groups. */
    val across = if (compact) 5.dp else 9.dp * scale
    val down = (if (compact) 5.dp else if (wide) 9.dp else 8.dp) * fit
    val groups = if (compact) 5.dp else 30.dp * scale

    /** Backspace and Next: their heights and the gap between them. */
    val backspaceHeight = keyHeight
    val commandGap = if (compact) 9.dp * fit else down
    val nextHeight = keyHeight + down - commandGap

    /** The panel's height, and where the rows start in it (upright, centred in it instead). */
    val height = full * fit
    val top = (if (compact) 12.dp else if (wide) 47.dp else 0.dp) * fit
    val centred = !compact && !wide

    /** The rows' height: four keys and three gaps. */
    val rows = keyHeight * 4 + down * 3

    /** Backspace's word, and the commands' words and ⌫ as a share of the tablet's (its 15, for
     * 18 point type): compact, Next and Done at 16 and ⌫ at 28; the tablet's as narrowed. */
    val backspace = if (compact) "⌫" else "Backspace"
    val commandsText = if (compact) 16f / 18f else scale
    val backspaceText = if (compact) 28f / 18f else scale
}

/** The units sizes are typed in: the key's name (in full, and short on a compact keypad), the unit's (as presets keep it) and its mark. */
internal enum class SizeUnit(val label: String, val short: String, val key: String, val mark: String) {
    MILLIMETERS("Millimeters", "MM", "mm", "mm"), CENTIMETERS("Centimeters", "CM", "cm", "cm"), INCHES("Inches", "IN", "in", "″"), PIXELS("Pixels", "PX", "px", "px")
}

/** The fields the keypad types into. */
private enum class Field { WIDTH, HEIGHT, DPI }

/** A section of the rail: its name, and its icon shown [size] across. */
private class Section(val label: String, val icon: ImageVector, val size: Dp)

private val Sections = listOf(
    Section("Dimensions", PcIcons.InfoDimensions, 50.dp),
    Section("Color profile", PcIcons.InfoProfile, 35.dp),
    Section("Time-lapse settings", PcIcons.InfoVideo, 60.dp),
    Section("Canvas properties", PcIcons.TabCanvas, 40.dp),
)

/**
 * Custom Canvas, full screen: a panel of sections (dimensions, colour profile, time-lapse
 * settings, canvas properties) on the left, the chosen one on the right with the canvas's name,
 * Cancel and Done; sizes are typed on a keypad of its own, in pixels, inches, centimetres or
 * millimetres. It edits [CanvasViewModel.editingPreset], or makes a new canvas ([onMade] then).
 */
@Composable
fun CustomCanvasScreen(vm: CanvasViewModel, onMade: () -> Unit) {
    val start = remember { vm.editingPreset?.let { vm.canvasPresets.getOrNull(it) } ?: CanvasPreset("Untitled Canvas", "px", 1080f, 1080f, 300f) }
    var section by remember { mutableIntStateOf(0) }
    var name by remember { mutableStateOf(start.name) }
    // the size in the unit chosen
    var width by remember { mutableStateOf(start.sizeWidth) }
    var height by remember { mutableStateOf(start.sizeHeight) }
    var dpi by remember { mutableStateOf(start.dpi) }
    var unit by remember { mutableStateOf(SizeUnit.entries.firstOrNull { it.key == start.unit } ?: SizeUnit.PIXELS) }
    var field by remember { mutableStateOf<Field?>(Field.WIDTH) }
    // what has been typed into the field, until it is left
    var typed by remember { mutableStateOf("") }
    var background by remember { mutableIntStateOf(start.background) }
    var hidden by remember { mutableStateOf(start.backgroundHidden) }
    var resolution by remember { mutableIntStateOf(start.timeLapse.resolution) }
    var quality by remember { mutableIntStateOf(start.timeLapse.quality) }
    var hevc by remember { mutableStateOf(start.timeLapse.hevc) }
    var profile by remember { mutableStateOf(start.profile) }
    // which profiles the list shows
    var cmyk by remember { mutableStateOf(io.github.artcraftandroid.procraft.core.isCmykProfile(start.profile)) }
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    val importer = androidx.activity.compose.rememberLauncherForActivityResult(androidx.activity.result.contract.ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let {
            scope.launch {
                vm.importProfile(it)?.let { name ->
                    profile = name
                    cmyk = io.github.artcraftandroid.procraft.core.isCmykProfile(name)
                }
            }
        }
    }
    // the background's colour picker is open
    var picking by remember { mutableStateOf(false) }

    fun preset() = CanvasPreset(name.trim().ifEmpty { "Untitled Canvas" }, unit.key, width, height, dpi, background, hidden, TimeLapseSettings(resolution, quality, hevc), profile)
    // what typing has made of the field so far goes into it
    fun commit() {
        val v = typed.toFloatOrNull()
        if (v != null && v > 0f) when (field) {
            Field.WIDTH -> width = v
            Field.HEIGHT -> height = v
            Field.DPI -> dpi = v.coerceIn(1f, 9999f)
            null -> Unit
        }
        typed = ""
    }
    // the size kept, in another unit (straight from one length to another; through the pixels
    // otherwise)
    fun convert(to: SizeUnit) {
        val p = preset()
        val from = CanvasPreset.perInch(unit.key)
        fun change(v: Float, px: Int) = CanvasPreset.perInch(to.key)?.let { if (from != null) v / from * it else px / dpi * it } ?: px.toFloat()
        width = change(width, p.width)
        height = change(height, p.height)
        unit = to
    }
    fun create() {
        commit()
        if (vm.finishCustomCanvas(preset())) onMade()
    }
    BackHandler { vm.closeCustomCanvas() }
    BoxWithConstraints(Modifier.fillMaxSize().background(ScreenFill).pointerInput(Unit) { detectTapGestures { } }) {
        val fit = CanvasSheetFit(maxWidth)
        // the sections (in a narrow window their icons alone, in a rail)
        Column(
            Modifier.padding(start = fit.railInset, top = fit.railInset, bottom = fit.railInset).width(fit.railWidth).fillMaxHeight()
                .clip(RoundedCornerShape(fit.railCorner)).background(PanelFill),
        ) {
            if (fit.narrow) {
                Spacer(Modifier.height(8.dp))
            } else {
                Text("Custom Canvas", color = Ink, fontSize = 22.sp, fontWeight = FontWeight.Bold, modifier = Modifier.padding(start = 24.dp, top = 16.dp, bottom = 17.dp))
            }
            Sections.forEachIndexed { i, s ->
                val chosen = i == section
                Row(
                    Modifier.padding(start = 8.dp, bottom = 10.dp).size(fit.cellWidth, 52.dp).clip(CircleShape).background(if (chosen) Pc.Accent else Color.Transparent).clickable {
                        commit()
                        section = i
                    }.padding(start = if (fit.narrow) 0.dp else 16.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    // icons centred 43 in, whatever their size; the names 69 in (in the rail the
                    // icons centred in their cells, unnamed)
                    Box(if (fit.narrow) Modifier.fillMaxWidth() else Modifier.width(38.dp), contentAlignment = Alignment.Center) {
                        Icon(s.icon, contentDescription = if (fit.narrow) s.label else null, tint = if (chosen) Color.White else IdleInk, modifier = Modifier.requiredSize(s.size))
                    }
                    if (!fit.narrow) Text(s.label, color = if (chosen) Color.White else IdleInk, fontSize = 15.sp, modifier = Modifier.padding(start = 7.dp))
                }
            }
        }
        // the name, Cancel and Done
        // on a wide screen centred over the page, upright from its left; in a narrow window from
        // the rail's side
        val wide = maxWidth > maxHeight
        BasicTextField(
            name, { name = it.take(64) }, singleLine = true,
            textStyle = TextStyle(
                color = Ink, fontSize = 22.sp, fontWeight = FontWeight.Bold,
                textAlign = if (wide && !fit.narrow) androidx.compose.ui.text.style.TextAlign.Center else androidx.compose.ui.text.style.TextAlign.Start,
            ),
            cursorBrush = SolidColor(Pc.Accent),
            modifier = if (fit.narrow) {
                Modifier.offset(x = fit.nameStart, y = 28.dp).width(fit.nameWidth)
            } else if (wide) {
                Modifier.padding(start = 372.dp, end = 32.dp).offset(y = 28.dp).fillMaxWidth()
            } else {
                Modifier.offset(x = 394.dp, y = 28.dp).width(maxWidth - 394.dp - 160.dp)
            },
        )
        Row(Modifier.align(Alignment.TopEnd).padding(top = 20.dp, end = 20.dp), verticalAlignment = Alignment.CenterVertically) {
            // Cancel, and Import beside it on the colour profile page
            Row(Modifier.height(43.dp).clip(CircleShape).background(CapsuleFill).border(1.dp, CapsuleEdge, CircleShape), verticalAlignment = Alignment.CenterVertically) {
                CapsuleText("Cancel") { vm.closeCustomCanvas() }
                if (section == 1) CapsuleText("Import") { importer.launch(arrayOf("*/*")) }
            }
            Spacer(Modifier.width(13.dp))
            Box(Modifier.size(44.dp, 45.dp).clip(CircleShape).background(DoneBlue).clickable { create() }, contentAlignment = Alignment.Center) {
                Icon(PcIcons.DoneTick, contentDescription = "Create", tint = Color.White, modifier = Modifier.requiredSize(40.dp))
            }
        }
        Column(Modifier.padding(start = fit.pageStart, end = 32.dp, top = 120.dp)) {
            when (section) {
                0 -> {
                    Group {
                        SizeRow("Width", if (field == Field.WIDTH && typed.isNotEmpty()) typed else CanvasPreset.number(width), unit.mark, field == Field.WIDTH) { commit(); field = Field.WIDTH }
                        Line()
                        SizeRow("Height", if (field == Field.HEIGHT && typed.isNotEmpty()) typed else CanvasPreset.number(height), unit.mark, field == Field.HEIGHT) { commit(); field = Field.HEIGHT }
                    }
                    Spacer(Modifier.height(24.dp))
                    Group {
                        SizeRow("DPI", if (field == Field.DPI && typed.isNotEmpty()) typed else "${dpi.roundToInt()}", "", field == Field.DPI) { commit(); field = Field.DPI }
                        Line()
                        val p = preset()
                        SizeRow("Maximum Layers", "${vm.layersFor(p.width, p.height)}", "", false) {}
                    }
                }
                1 -> {
                    KindSwitch(cmyk) { cmyk = it }
                    Spacer(Modifier.height(24.dp))
                    Group {
                        val names = if (cmyk) io.github.artcraftandroid.procraft.core.cmykProfiles() else io.github.artcraftandroid.procraft.core.colorProfiles()
                        for ((i, name) in names.withIndex()) {
                            if (i > 0) Line()
                            ChoiceRow(name, name == profile) { profile = name }
                        }
                    }
                }
                2 -> {
                    Group {
                        for ((i, pair) in listOf("1080p" to "1920 × 1080px", "2K" to "2732 × 2048px", "4K" to "3840 × 2160px").withIndex()) {
                            if (i > 0) Line()
                            ChoiceRow(pair.first, i == resolution, pair.second) { resolution = i }
                        }
                    }
                    Spacer(Modifier.height(24.dp))
                    Group {
                        for ((i, label) in listOf("Low quality", "Good quality", "Studio quality", "Lossless").withIndex()) {
                            if (i > 0) Line()
                            ChoiceRow(label, i == quality, tick = true) { quality = i }
                        }
                    }
                    Spacer(Modifier.height(24.dp))
                    Group { SwitchLine("HEVC", hevc) { hevc = it } }
                }
                else -> {
                    Group {
                        Row(
                            Modifier.fillMaxWidth().height(47.dp).clickable { picking = true }.padding(horizontal = 16.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text("Background color", color = Ink, fontSize = 15.sp, modifier = Modifier.weight(1f))
                            Box(Modifier.size(24.dp).clip(CircleShape).background(Color(0xFF000000 or background.toLong())).border(1.dp, Color(0xFF555555), CircleShape))
                        }
                    }
                    Spacer(Modifier.height(24.dp))
                    Group { SwitchLine("Background hidden", hidden) { hidden = it } }
                }
            }
        }
        // the keypad, while a size is typed
        if (section == 0 && field != null) {
            Keypad(
                Modifier.align(Alignment.BottomCenter),
                KeypadFit(maxWidth, wide, maxHeight - TypedBottom),
                unit,
                onUnit = {
                    commit()
                    convert(it)
                },
                onKey = { k ->
                    when (k) {
                        "⌫" -> typed = typed.dropLast(1)
                        "." -> if ('.' !in typed && field != Field.DPI && unit != SizeUnit.PIXELS) typed += "."
                        else -> if (typed.length < 7) typed += k
                    }
                },
                onNext = {
                    commit()
                    field = when (field) { Field.WIDTH -> Field.HEIGHT; Field.HEIGHT -> Field.DPI; else -> Field.WIDTH }
                },
                onDone = {
                    commit()
                    field = null
                },
            )
        }
        // the background's colour, in a compact picker hanging from its swatch (60 in from the
        // right, under the row) at the screen's edge
        if (picking && section == 3) {
            Box(Modifier.fillMaxSize().pointerInput(Unit) { detectTapGestures { picking = false } })
            val pick = remember { HeldColor(background) { background = it } }
            CompactColors(vm, pick, arrowX = 200.dp, modifier = Modifier.offset(x = maxWidth - 260.dp, y = 168.dp))
        }
    }
}

/** One of the header capsule's words, a button. */
@Composable
private fun CapsuleText(label: String, onClick: () -> Unit) {
    Box(Modifier.fillMaxHeight().clickable(onClick = onClick).padding(horizontal = 13.dp), contentAlignment = Alignment.Center) {
        Text(label, color = Color(0xFFA5A5A5), fontSize = 17.sp)
    }
}

/** RGB or CMYK: which profiles the colour profile page lists, as wide as the list, the chosen half blue. */
@Composable
private fun KindSwitch(cmyk: Boolean, onChange: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth().height(34.dp).clip(CircleShape).background(SwitchTrack).padding(2.dp)) {
        for ((i, label) in listOf("RGB", "CMYK").withIndex()) {
            val on = cmyk == (i == 1)
            val ink = if (on) Color.White else SwitchIdle
            Row(
                Modifier.weight(1f).fillMaxHeight().clip(CircleShape).background(if (on) Pc.Accent else Color.Transparent).clickable { onChange(i == 1) },
                horizontalArrangement = Arrangement.Center,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(if (i == 0) PcIcons.ProfileRgb else PcIcons.ProfileCmyk, contentDescription = null, tint = ink, modifier = Modifier.size(20.dp))
                Text(label, color = ink, fontSize = 15.sp, modifier = Modifier.padding(start = 8.dp))
            }
        }
    }
}

@Composable
private fun Group(content: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(PanelFill), content = content)
}

@Composable
private fun Line() {
    Box(Modifier.fillMaxWidth().height(1.dp).background(ScreenFill))
}

@Composable
private fun SizeRow(label: String, value: String, unit: String, editing: Boolean, onClick: () -> Unit) {
    Row(Modifier.fillMaxWidth().height(47.dp).clickable(onClick = onClick).padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = Ink, fontSize = 15.sp, modifier = Modifier.weight(1f))
        Text(value, color = if (editing) Editing else Ink, fontSize = 15.sp)
        if (unit.isNotEmpty()) Text(" $unit", color = Ink, fontSize = 15.sp)
    }
}

@Composable
private fun ChoiceRow(label: String, chosen: Boolean, detail: String? = null, tick: Boolean = false, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().height(47.dp).background(if (chosen) Pc.Accent else Color.Transparent).clickable(onClick = onClick).padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, color = if (chosen) Color.White else Ink, fontSize = 15.sp, modifier = Modifier.weight(1f))
        if (detail != null) Text(detail, color = if (chosen) Color.White else Ink, fontSize = 15.sp)
        if (tick && chosen) Icon(PcIcons.MenuTick, contentDescription = null, tint = Color.White, modifier = Modifier.size(20.dp))
    }
}

@Composable
private fun SwitchLine(label: String, on: Boolean, onChange: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth().height(47.dp).clickable { onChange(!on) }.padding(start = 16.dp, end = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = Ink, fontSize = 15.sp, modifier = Modifier.weight(1f))
        Switch(on, enabled = true)
    }
}

/**
 * The keypad: units on the left, the digits, and Backspace, Next and Done on the right, sized by
 * [fit] (see [KeypadFit]). A compact one names the units short and shows ⌫ for Backspace, its
 * Next and Done a little smaller (16 to the tablet's 18) and ⌫ larger (28).
 */
@Composable
private fun Keypad(modifier: Modifier, fit: KeypadFit, unit: SizeUnit, onUnit: (SizeUnit) -> Unit, onKey: (String) -> Unit, onNext: () -> Unit, onDone: () -> Unit) {
    val w = fit.keyWidth
    val h = fit.keyHeight
    val down = fit.down
    val across = fit.across
    val words = 15.sp * fit.scale
    val digits = 26.sp * fit.scale
    val commands = 15.sp * fit.commandsText
    Box(
        modifier.fillMaxWidth().height(fit.height).background(KeypadFill).padding(top = fit.top),
        contentAlignment = if (fit.centred) Alignment.Center else Alignment.TopCenter,
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(fit.groups)) {
            Column(verticalArrangement = Arrangement.spacedBy(down)) {
                for (u in listOf(SizeUnit.MILLIMETERS, SizeUnit.CENTIMETERS, SizeUnit.INCHES, SizeUnit.PIXELS)) {
                    val ink = if (u != unit) Ink else if (Pc.light) Color.White else Color(0xFF1F1F1F)
                    Key(if (fit.compact) u.short else u.label, if (u == unit) UnitChosen else KeyFill, ink, words, w, h) { onUnit(u) }
                }
            }
            Column(verticalArrangement = Arrangement.spacedBy(down)) {
                for (row in listOf(listOf("7", "8", "9"), listOf("4", "5", "6"), listOf("1", "2", "3"))) {
                    Row(horizontalArrangement = Arrangement.spacedBy(across)) { for (k in row) Key(k, DigitFill, Ink, digits, w, h) { onKey(k) } }
                }
                Row(horizontalArrangement = Arrangement.spacedBy(across)) {
                    Key("0", DigitFill, Ink, digits, w * 2 + across, h) { onKey("0") }
                    Key(".", DigitFill, Ink, digits, w, h) { onKey(".") }
                }
            }
            Column {
                Key(fit.backspace, KeyFill, Ink, 15.sp * fit.backspaceText, w, fit.backspaceHeight, description = "Backspace".takeIf { fit.compact }) { onKey("⌫") }
                Spacer(Modifier.height(fit.commandGap))
                Key("Next", KeyFill, Ink, commands, w, fit.nextHeight) { onNext() }
                Spacer(Modifier.height(down))
                Key("Done", KeyFill, Ink, commands, w, h * 2 + down) { onDone() }
            }
        }
    }
}

@Composable
private fun Key(label: String, fill: Color, ink: Color, size: androidx.compose.ui.unit.TextUnit, width: Dp, height: Dp, description: String? = null, onClick: () -> Unit) {
    val named = if (description != null) Modifier.semantics { contentDescription = description } else Modifier
    Box(Modifier.size(width, height).clip(RoundedCornerShape(8.dp)).background(fill).clickable(onClick = onClick).then(named), contentAlignment = Alignment.Center) {
        Text(label, color = ink, fontSize = size)
    }
}
