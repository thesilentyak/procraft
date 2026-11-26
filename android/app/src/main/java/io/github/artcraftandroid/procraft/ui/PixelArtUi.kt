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
