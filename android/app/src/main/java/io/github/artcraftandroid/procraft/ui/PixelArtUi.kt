package io.github.artcraftandroid.procraft.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.foundation.gestures.detectTapGestures

/**
 * New canvas's Pixel art row: tapped, it opens the sizes a pixel art canvas is made at (square), and
 * Custom, a width and height typed in. [onPick] makes the artwork, its width and height.
 */
@Composable
fun PixelArtNewRow(modifier: Modifier, onPick: (Int, Int) -> Unit) {
    var open by remember { mutableStateOf(false) }
    var custom by remember { mutableStateOf(false) }
    val most = remember { io.github.artcraftandroid.procraft.core.pixelArtMostEdge().toInt() }
    val sizes = remember { io.github.artcraftandroid.procraft.core.pixelArtSizes().map { it.toInt() } }
    Column(modifier.fillMaxWidth().clip(RoundedCornerShape(19.dp)).background(Pc.LightRowStrong)) {
        Row(
            Modifier.fillMaxWidth().height(38.dp).clickable { open = !open }.padding(start = 17.dp, end = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Pixel art", color = Pc.LightText, fontSize = 17.sp, modifier = Modifier.weight(1f))
            SizeChip("${sizes.first()}–${sizes.last()}px")
        }
        if (open) {
            Row(
                Modifier.fillMaxWidth().padding(start = 12.dp, end = 12.dp, bottom = 10.dp),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                sizes.forEach { n -> PickChip("$n", Modifier.weight(1f)) { onPick(n, n) } }
                PickChip("Custom", Modifier.weight(1.6f), chosen = custom) { custom = !custom }
            }
            if (custom) CustomPixelSize(most, onPick)
        }
    }
}

/** Width and height typed in (1 to [most]), and Make. */
@Composable
private fun CustomPixelSize(most: Int, onPick: (Int, Int) -> Unit) {
    var w by remember { mutableStateOf("48") }
    var h by remember { mutableStateOf("48") }
    val width = w.toIntOrNull()?.takeIf { it in 1..most }
    val height = h.toIntOrNull()?.takeIf { it in 1..most }
    Row(
        Modifier.fillMaxWidth().padding(start = 17.dp, end = 12.dp, bottom = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        NumberBox(w, Modifier.width(64.dp)) { w = it }
        Text("×", color = Pc.LightText, fontSize = 17.sp)
        NumberBox(h, Modifier.width(64.dp)) { h = it }
        Text("px", color = Pc.LightText, fontSize = 15.sp, modifier = Modifier.weight(1f))
        PickChip("Make", Modifier.width(72.dp), enabled = width != null && height != null) {
            if (width != null && height != null) onPick(width, height)
        }
    }
}

@Composable
private fun NumberBox(value: String, modifier: Modifier, onChange: (String) -> Unit) {
    Box(modifier.height(32.dp).clip(RoundedCornerShape(8.dp)).background(Pc.LightChip), contentAlignment = Alignment.Center) {
        BasicTextField(
            value,
            { onChange(it.filter(Char::isDigit).take(4)) },
            singleLine = true,
            textStyle = TextStyle(color = Pc.LightText, fontSize = 16.sp, textAlign = TextAlign.Center),
            cursorBrush = SolidColor(Pc.LightText),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

@Composable
private fun SizeChip(text: String) {
    Box(Modifier.height(18.dp).clip(CircleShape).background(Pc.LightChip).padding(horizontal = 8.dp), contentAlignment = Alignment.Center) {
        Text(text, color = Pc.LightText, fontSize = 13.sp, maxLines = 1)
    }
}

@Composable
private fun PickChip(text: String, modifier: Modifier, chosen: Boolean = false, enabled: Boolean = true, onClick: () -> Unit) {
    Box(
        modifier.height(32.dp).clip(CircleShape).background(if (chosen) Pc.LightChip else Pc.LightRow)
            .clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, color = if (enabled) Pc.LightText else Pc.LightText.copy(alpha = 0.4f), fontSize = 15.sp, maxLines = 1)
    }
}

/** The scales past ×1 Share Image offers PNGs at: on a pixel art canvas only. */
fun pixelArtScales(vm: io.github.artcraftandroid.procraft.canvas.CanvasViewModel): List<Int> =
    if (vm.session?.isPixelArt() == true) {
        io.github.artcraftandroid.procraft.core.pixelArtExportScales().map { it.toInt() }.filter { it > 1 }
    } else {
        emptyList()
    }

/**
 * ColorDrop's Contiguous switch on a pixel art canvas: on (blue), a fill takes only what's joined to
 * where it lands; off (outlined), every pixel of that colour on the layer. For the fills from now on.
 */
@Composable
fun PixelContiguous(vm: io.github.artcraftandroid.procraft.canvas.CanvasViewModel, blue: androidx.compose.ui.graphics.Color, ink: androidx.compose.ui.graphics.Color) {
    val on = vm.pixelContiguous
    Box(
        Modifier.height(20.dp).clip(RoundedCornerShape(10.dp))
            .background(if (on) blue else androidx.compose.ui.graphics.Color.Transparent)
            .then(if (on) Modifier else Modifier.border(1.dp, ink, RoundedCornerShape(10.dp)))
            .clickable { vm.choosePixelContiguous(!on) }
            .padding(horizontal = 10.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(if (on) "Contiguous" else "All of this colour", color = if (on) androidx.compose.ui.graphics.Color.White else ink, fontSize = 11.7.sp, maxLines = 1)
    }
}

/**
 * The side bar with, on a pixel art canvas, its palette square under it: tapped, the palette
 * unfolds beside the bar (toward the canvas, [right] for a right-handed bar), a column of swatches
 * to paint with, + adding the current colour and ⋯ offering Paste hex codes, From the picture and
 * Clear. Elsewhere it is the side bar alone.
 */
@Composable
fun SideBarWithPalette(vm: io.github.artcraftandroid.procraft.canvas.CanvasViewModel, right: Boolean, sideBar: @Composable () -> Unit) {
    if (!vm.pixelArtOpen) {
        sideBar()
        return
    }
    androidx.compose.foundation.layout.Row(verticalAlignment = Alignment.Top) {
        if (right && vm.paletteOpen) PaletteStrip(vm, Modifier.padding(end = 8.dp))
        Column(horizontalAlignment = if (right) Alignment.End else Alignment.Start) {
            sideBar()
            PaletteSquare(vm, Modifier.padding(top = 8.dp, start = if (right) 0.dp else 5.dp, end = if (right) 5.dp else 0.dp))
        }
        if (!right && vm.paletteOpen) PaletteStrip(vm, Modifier.padding(start = 8.dp))
    }
}

/** The palette's square under the side bar: its first four colours; a tap unfolds or folds the palette. */
@Composable
private fun PaletteSquare(vm: io.github.artcraftandroid.procraft.canvas.CanvasViewModel, modifier: Modifier) {
    val colours = vm.pixelPalette
    Box(
        modifier.size(40.dp).clip(RoundedCornerShape(10.dp)).background(Pc.SideBar)
            .then(if (vm.paletteOpen) Modifier.border(2.dp, Pc.Accent, RoundedCornerShape(10.dp)) else Modifier)
            .clickable { vm.paletteOpen = !vm.paletteOpen },
        contentAlignment = Alignment.Center,
    ) {
        androidx.compose.foundation.Canvas(Modifier.size(24.dp)) {
            val half = size.width / 2f
            for (i in 0 until 4) {
                val c = colours.getOrNull(i) ?: continue
                drawRect(
                    androidx.compose.ui.graphics.Color(0xFF000000.toInt() or c),
                    topLeft = androidx.compose.ui.geometry.Offset((i % 2) * half, (i / 2) * half),
                    size = androidx.compose.ui.geometry.Size(half, half),
                )
            }
            if (colours.isEmpty()) drawRect(Pc.SideGlyph, style = androidx.compose.ui.graphics.drawscope.Stroke(1.5.dp.toPx()))
        }
    }
}

/** The palette unfolded: two columns of swatches (the current colour ringed), scrolling, then + and ⋯. */
@Composable
private fun PaletteStrip(vm: io.github.artcraftandroid.procraft.canvas.CanvasViewModel, modifier: Modifier) {
    val current = vm.colorRgb()
    var menu by remember { mutableStateOf(false) }
    var held by remember { mutableStateOf<Int?>(null) }
    Column(
        modifier.width(80.dp).heightIn(max = Pc.SideBarHeight + 48.dp).clip(RoundedCornerShape(14.dp)).background(Pc.SideBar).padding(6.dp),
    ) {
        val rows = vm.pixelPalette.chunked(2)
        androidx.compose.foundation.lazy.LazyColumn(Modifier.weight(1f, fill = false), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            items(rows.size) { r ->
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    rows[r].forEach { c ->
                        Box {
                            Swatch(c, chosen = c == current, onTap = { vm.setRgb(c) }, onHold = { held = c })
                            androidx.compose.material3.DropdownMenu(expanded = held == c, onDismissRequest = { held = null }) {
                                androidx.compose.material3.DropdownMenuItem(text = { Text("Remove") }, onClick = {
                                    held = null
                                    vm.choosePixelPalette(vm.pixelPalette - c)
                                })
                            }
                        }
                    }
                }
            }
        }
        Row(Modifier.padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            StripButton("+") { vm.choosePixelPalette(vm.pixelPalette + current) }
            Box {
                StripButton("⋯") { menu = true }
                androidx.compose.material3.DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    androidx.compose.material3.DropdownMenuItem(text = { Text("Paste hex codes") }, onClick = { menu = false; vm.pastePixelPalette() })
                    androidx.compose.material3.DropdownMenuItem(text = { Text("From the picture") }, onClick = { menu = false; vm.pixelPaletteFromPicture() })
                    androidx.compose.material3.DropdownMenuItem(text = { Text("Clear") }, onClick = { menu = false; vm.choosePixelPalette(emptyList()) })
                }
            }
        }
    }
}

@Composable
private fun Swatch(colour: Int, chosen: Boolean, onTap: () -> Unit, onHold: () -> Unit) {
    Box(
        Modifier.size(31.dp).clip(RoundedCornerShape(6.dp))
            .background(androidx.compose.ui.graphics.Color(0xFF000000.toInt() or colour))
            .then(if (chosen) Modifier.border(2.dp, androidx.compose.ui.graphics.Color.White, RoundedCornerShape(6.dp)) else Modifier)
            .pointerInput(colour) { detectTapGestures(onTap = { onTap() }, onLongPress = { onHold() }) },
    )
}

@Composable
private fun StripButton(label: String, onClick: () -> Unit) {
    Box(Modifier.size(31.dp).clip(RoundedCornerShape(6.dp)).background(Pc.SideGlyph.copy(alpha = 0.18f)).clickable(onClick = onClick), contentAlignment = Alignment.Center) {
        Text(label, color = Pc.SideGlyph, fontSize = 18.sp, maxLines = 1)
    }
}
