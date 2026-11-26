package io.github.artcraftandroid.procraft.ui

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.PopupProperties
import io.github.artcraftandroid.procraft.canvas.CanvasViewModel
import io.github.artcraftandroid.procraft.core.PluginControl
import io.github.artcraftandroid.procraft.core.PluginTexture

private val TexturesVeil = Color(0x80000000)
private val TexturesSheet get() = if (Pc.light) Color(0xFFF2F2F7) else Color(0xFF1C1C1E)
private val TexturesTitle get() = if (Pc.light) Color(0xFF3A3A3A) else Color(0xFFC1C1C1)
private val TexturesDim = Color(0xFF868686)
private val TexturesEdge get() = if (Pc.light) Color(0xFFC8C8C8) else Color(0xFF5A5A5A)

/** The texture [control] has chosen, in words: a grain's name, the name of a picture of the plugin's, or None. */
internal fun textureName(vm: CanvasViewModel, control: PluginTexture): String {
    val v = control.value
    val named = when {
        v > 0 -> vm.grainLibrary.getOrNull(v)?.first
        v < 0 -> control.own.getOrNull(-1 - v)?.let { key -> pluginFileName(vm, key) }
        else -> null
    }
    return named ?: "None"
}

/** What the picture picked for the plugin's file control [key] is called, if one is. */
private fun pluginFileName(vm: CanvasViewModel, key: String): String? =
    vm.pluginList.flatMap { it.controls }.filterIsInstance<PluginControl.File>().firstOrNull { it.control.key == key }?.control?.chosen

/** A plugin's texture control (interface 15), as its card and its panels show it: its label, the texture chosen in blue; a tap opens the Textures sheet. */
@Composable
internal fun PluginTextureRow(control: PluginTexture, vm: CanvasViewModel) {
    var open by remember(control.key) { mutableStateOf(false) }
    Row(
        Modifier.fillMaxWidth().height(44.dp).clickable { open = true }.padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(control.label, color = Pc.ActionsRow, fontSize = 17.sp, maxLines = 1)
        Box(Modifier.weight(1f))
        Text(textureName(vm, control), color = Pc.Accent, fontSize = 17.sp, maxLines = 1, modifier = Modifier.padding(start = 12.dp))
    }
    if (open) TextureLibrary(vm, control.key) { open = false }
}

/**
 * The Textures sheet a texture control opens, over the whole window as the Source Library is: the app's grains, then the
 * pictures picked for the plugin's own file controls it offers, then a tile that picks another into the first of them with
 * none (and chooses it). A tap chooses one and closes it; holding one of the plugin's pictures offers to delete it.
 */
@Composable
private fun TextureLibrary(vm: CanvasViewModel, key: String, onClose: () -> Unit) {
    // the control as it is now: a picture added or deleted changes the plugin's list
    val control = vm.pluginList.flatMap { it.controls }.filterIsInstance<PluginControl.Texture>().firstOrNull { it.control.key == key }?.control
    if (control == null) {
        onClose()
        return
    }
    val own: List<Triple<String, String, ImageBitmap?>> = control.own.mapNotNull { file ->
        pluginFileName(vm, file)?.let { name -> Triple(file, name, remember(file, name) { vm.pluginPictureThumbnail(file) }) }
    }
    val free = control.own.indexOfFirst { pluginFileName(vm, it) == null }
    var deleting by remember { mutableStateOf<String?>(null) }
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null && free >= 0) {
            vm.pickPluginTexture(control.key, control.own[free], -1 - free, uri)
            onClose()
        }
    }
    val atOrigin = remember {
        object : androidx.compose.ui.window.PopupPositionProvider {
            override fun calculatePosition(
                anchorBounds: androidx.compose.ui.unit.IntRect,
                windowSize: androidx.compose.ui.unit.IntSize,
                layoutDirection: androidx.compose.ui.unit.LayoutDirection,
                popupContentSize: androidx.compose.ui.unit.IntSize,
            ) = androidx.compose.ui.unit.IntOffset.Zero
        }
    }
    // each tile: its name, its picture, and the value it sets (none: the tile that adds)
    data class Tile(val name: String, val image: ImageBitmap?, val value: Int?, val file: String?)
    val tiles = buildList {
        vm.grainLibrary.forEachIndexed { n, (name, image) -> add(Tile(name, image, n, null)) }
        own.forEach { (file, name, image) -> add(Tile(name, image, -1 - control.own.indexOf(file), file)) }
        if (free >= 0) add(Tile("Add a picture", null, null, null))
    }
    PcPopup(atOrigin, onDismissRequest = onClose, properties = PopupProperties(focusable = true, clippingEnabled = false)) {
        Box(Modifier.fillMaxSize().background(TexturesVeil).pointerInput(Unit) { awaitEachGesture { awaitFirstDown().consume() } }) {
            Column(
                Modifier.align(Alignment.TopCenter).offset(y = 74.dp).size(580.dp, 575.dp).clip(RoundedCornerShape(26.dp)).background(TexturesSheet)
                    .pointerInput(Unit) { awaitEachGesture { awaitFirstDown(requireUnconsumed = false) } },
            ) {
                Row(Modifier.fillMaxWidth().padding(start = 19.dp, end = 19.dp, top = 40.dp).height(39.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text("Textures", color = TexturesTitle, fontSize = 32.sp, fontWeight = FontWeight.Bold, maxLines = 1, modifier = Modifier.weight(1f))
                    Text("Cancel", color = TexturesDim, fontSize = 17.sp, maxLines = 1, modifier = Modifier.clickable(onClick = onClose))
                }
                LazyVerticalGrid(
                    columns = GridCells.Fixed(3),
                    contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 31.dp, bottom = 24.dp),
                    horizontalArrangement = Arrangement.spacedBy(53.dp),
                    verticalArrangement = Arrangement.spacedBy(18.dp),
                    modifier = Modifier.fillMaxSize(),
                ) {
                    items(tiles.size) { k ->
                        val tile = tiles[k]
                        val chosen = tile.value != null && tile.value == control.value
                        Column(
                            Modifier.width(147.dp).pointerInput(tile) {
                                detectTapGestures(
                                    onTap = {
                                        when (val value = tile.value) {
                                            null -> picker.launch(arrayOf("image/*"))
                                            else -> {
                                                vm.setPluginTexture(control.key, value)
                                                onClose()
                                            }
                                        }
                                    },
                                    onLongPress = { tile.file?.let { deleting = it } },
                                )
                            },
                        ) {
                            Box(
                                Modifier.size(147.dp, 146.dp).clip(RoundedCornerShape(8.dp)).background(Color.Black)
                                    .border(if (chosen) 3.dp else 1.dp, if (chosen) Pc.Accent else TexturesEdge, RoundedCornerShape(8.dp)),
                                contentAlignment = Alignment.Center,
                            ) {
                                when {
                                    tile.image != null -> Image(tile.image, contentDescription = tile.name, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
                                    tile.value == null -> Text("+", color = Pc.Accent, fontSize = 48.sp)
                                }
                            }
                            Text(tile.name, color = TexturesTitle, fontSize = 13.sp, fontWeight = FontWeight.Bold, maxLines = 1, modifier = Modifier.padding(start = 3.dp, top = 5.dp).height(21.dp))
                        }
                    }
                }
            }
        }
    }
    deleting?.let { file ->
        AlertDialog(
            onDismissRequest = { deleting = null },
            title = { Text("Delete this picture?") },
            text = { Text("It goes from the plugin's textures.") },
            confirmButton = {
                TextButton(onClick = {
                    vm.clearPluginFile(file)
                    deleting = null
                }) { Text("Delete", color = Color(0xFFFF453A)) }
            },
            dismissButton = { TextButton(onClick = { deleting = null }) { Text("Cancel") } },
        )
    }
}
