package io.github.artcraftandroid.procraft.ui

import android.graphics.Bitmap
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.viewModelScope
import io.github.artcraftandroid.procraft.canvas.CanvasViewModel
import io.github.artcraftandroid.procraft.core.SpriteSheetImport
import io.github.artcraftandroid.procraft.core.SpriteSheetLook
import io.github.artcraftandroid.procraft.core.ThumbnailImage
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.nio.ByteBuffer

/**
 * A sprite sheet being imported (Actions › Add › Insert a sprite sheet): what reading it found,
 * and the name, grid and background the user may change (docs/pixel-art/05-sprite-import-design.md).
 */
class SpriteSheetImporting(val sheet: SpriteSheetImport) {
    var look by mutableStateOf<SpriteSheetLook?>(null)
    var name by mutableStateOf(sheet.suggestedName())
    /** Frames across and down, 0 for found by the gaps. */
    var across by mutableIntStateOf(0)
    var down by mutableIntStateOf(0)
    /** The background colour chosen (0xRRGGBB), none for the one found. */
    var background by mutableStateOf<UInt?>(null)
    var reading by mutableStateOf(true)
    var adding by mutableStateOf(false)
    /** The frames (by number, from 0) left out of the import: tapped off in the sheet. */
    val skipped = mutableStateListOf<Int>()
    /** Reads started: only the last one's look is shown (an earlier, slower one may end later). */
    var reads = 0
}

/** Opens the picked sheet at [uri] and reads it for the import sheet. */
fun CanvasViewModel.insertSpriteSheet(uri: android.net.Uri) {
    viewModelScope.launch {
        val picked = sheetPicture(uri)
        if (picked == null) {
            message = "This picture can't be opened."
            return@launch
        }
        val importing = SpriteSheetImporting(SpriteSheetImport(picked.first, picked.second))
        spriteSheet = importing
        readSpriteSheet(importing)
    }
}

/** Reads the sheet again, as its grid and background now are. */
fun CanvasViewModel.readSpriteSheet(importing: SpriteSheetImporting) {
    val s = session ?: return
    importing.reading = true
    val read = ++importing.reads
    val (across, down, background) = Triple(importing.across.toUInt(), importing.down.toUInt(), importing.background)
    viewModelScope.launch {
        val look = withContext(Dispatchers.Default) {
            runCatching { importing.sheet.look(s, across, down, background) }.getOrNull()
        }
        if (read != importing.reads) return@launch
        importing.look = look
        importing.skipped.clear()
        importing.reading = false
    }
}

/** Adds the sheet's frames as an animation, and closes the import sheet. */
private fun CanvasViewModel.addSpriteSheet(importing: SpriteSheetImporting) {
    val s = session ?: return
    importing.adding = true
    viewModelScope.launch {
        val done = withContext(Dispatchers.Default) { runCatching { importing.sheet.insert(s, importing.name, importing.skipped.map { it.toUInt() }) }.getOrNull() }
        spriteSheet = null
        refresh()
        when {
            done == null -> showNotice("No room for these frames", warning = true)
            done.grown -> showNotice("Added ${done.frames} frames; the canvas grew to fit")
            else -> showNotice("Added ${done.frames} frames")
        }
    }
}

private fun ThumbnailImage.sheetBitmap(): ImageBitmap {
    val b = Bitmap.createBitmap(width.toInt(), height.toInt(), Bitmap.Config.ARGB_8888)
    b.copyPixelsFromBuffer(ByteBuffer.wrap(rgba))
    return b.asImageBitmap()
}

/** The import sheet: the sheet with each frame found boxed, the frames playing, the name. */
@Composable
fun SpriteSheetImportSheet(vm: CanvasViewModel, importing: SpriteSheetImporting) {
    val look = importing.look
    val shown = remember(look) { look?.shown?.sheetBitmap() }
    val allFrames = remember(look) { look?.frames?.map { it.sheetBitmap() }.orEmpty() }
    val frames = allFrames.filterIndexed { k, _ -> k !in importing.skipped }
    var more by remember { mutableStateOf(false) }
    var picking by remember { mutableStateOf(false) }
    Dialog(onDismissRequest = { vm.spriteSheet = null }, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Column(
            Modifier.widthIn(max = 640.dp).imePadding().padding(24.dp).clip(RoundedCornerShape(16.dp)).background(Pc.PopoverFill)
                .verticalScroll(rememberScrollState()).padding(20.dp),
        ) {
            Text("Insert a sprite sheet", color = Pc.Text, fontSize = 20.sp)
            Spacer(Modifier.height(14.dp))
            BoxWithConstraints { SheetPictures(wide = maxWidth >= 574.dp) {
                // the sheet, each frame found boxed and numbered; while picking, a tap takes its colour,
                // else a tap on a frame leaves it out (or puts it back)
                Box(Modifier.width(380.dp).height(240.dp).clip(RoundedCornerShape(8.dp)).background(Pc.ThumbBackground), contentAlignment = Alignment.Center) {
                    if (shown != null && look != null) {
                        val aspect = look.pictureWidth.toFloat() / look.pictureHeight.toFloat()
                        val w = if (aspect >= 380f / 240f) 380.dp else (240f * aspect).dp
                        val h = if (aspect >= 380f / 240f) (380f / aspect).dp else 240.dp
                        Box(
                            Modifier.size(w, h).pointerInput(look, picking) {
                                detectTapGestures { at ->
                                    if (!picking) {
                                        val px = at.x / size.width * look.pictureWidth.toFloat()
                                        val py = at.y / size.height * look.pictureHeight.toFloat()
                                        val k = look.places.indexOfFirst {
                                            px >= it.x.toFloat() && px < (it.x + it.width).toFloat() &&
                                                py >= it.y.toFloat() && py < (it.y + it.height).toFloat()
                                        }
                                        if (k >= 0) {
                                            if (k in importing.skipped) importing.skipped.remove(k) else importing.skipped.add(k)
                                        }
                                        return@detectTapGestures
                                    }
                                    val x = (at.x / size.width * look.shown.width.toFloat()).toInt().coerceIn(0, look.shown.width.toInt() - 1)
                                    val y = (at.y / size.height * look.shown.height.toFloat()).toInt().coerceIn(0, look.shown.height.toInt() - 1)
                                    val i = (y * look.shown.width.toInt() + x) * 4
                                    val px = look.shown.rgba
                                    importing.background = ((px[i].toUInt() and 0xFFu) shl 16) or ((px[i + 1].toUInt() and 0xFFu) shl 8) or (px[i + 2].toUInt() and 0xFFu)
                                    picking = false
                                    vm.readSpriteSheet(importing)
                                }
                            },
                        ) {
                            Image(shown, null, Modifier.size(w, h), contentScale = ContentScale.FillBounds, filterQuality = FilterQuality.None)
                            Canvas(Modifier.size(w, h)) {
                                val sx = size.width / look.pictureWidth.toFloat()
                                val sy = size.height / look.pictureHeight.toFloat()
                                look.places.forEachIndexed { k, p ->
                                    val topLeft = Offset(p.x.toFloat() * sx, p.y.toFloat() * sy)
                                    val box = Size(p.width.toFloat() * sx, p.height.toFloat() * sy)
                                    val left = k in importing.skipped
                                    if (left) drawRect(Color(0xB0000000), topLeft, box)
                                    drawRect(if (left) Color(0xFF808080) else Color(0xFF0076FD), topLeft, box, style = Stroke(2.dp.toPx()))
                                    drawIntoCanvas {
                                        val paint = android.graphics.Paint().apply {
                                            color = android.graphics.Color.WHITE
                                            textSize = 12.sp.toPx()
                                            isAntiAlias = true
                                            setShadowLayer(3f, 0f, 0f, android.graphics.Color.BLACK)
                                        }
                                        it.nativeCanvas.drawText("${k + 1}", topLeft.x + 4.dp.toPx(), topLeft.y + 14.dp.toPx(), paint)
                                    }
                                }
                            }
                        }
                    } else if (importing.reading) {
                        Text("Reading the sheet…", color = Pc.TextDim, fontSize = 14.sp)
                    }
                }
                // the frames, cleaned, playing at the artwork's speed
                Box(Modifier.size(180.dp, 240.dp).clip(RoundedCornerShape(8.dp)).background(Pc.ThumbBackground), contentAlignment = Alignment.Center) {
                    if (frames.isNotEmpty() && look != null) {
                        var tick by remember(look) { mutableIntStateOf(0) }
                        LaunchedEffect(look) {
                            while (true) {
                                delay((1000L / look.fps.toLong().coerceAtLeast(1)))
                                tick++
                            }
                        }
                        Image(
                            frames[tick % frames.size], null, Modifier.padding(12.dp).size(156.dp, 216.dp),
                            contentScale = ContentScale.Fit, filterQuality = FilterQuality.None,
                        )
                    } else if (!importing.reading) {
                        Text("No sprites found", color = Pc.TextDim, fontSize = 14.sp)
                    }
                }
            } }
            Spacer(Modifier.height(10.dp))
            val said = when {
                importing.reading -> ""
                look == null -> "Nothing was found. Try Not right? below: set the grid, or pick the background."
                else -> buildString {
                    val n = look.places.size - importing.skipped.size
                    append("$n ${if (n == 1) "frame" else "frames"}, ${look.frames.firstOrNull()?.width} × ${look.frames.firstOrNull()?.height}")
                    if (importing.skipped.isNotEmpty()) append(" (${importing.skipped.size} left out)")
                    if (look.pixelSize > 1.5f) append(", drawn ×${"%.1f".format(look.pixelSize)}")
                    if (look.tooBig) append(". Too big for a pixel art canvas (1024 a side at most).")
                    else if (look.grows) append(". The canvas will grow to ${look.canvasWidth} × ${look.canvasHeight} to fit.")
                    if (!look.tooBig && importing.skipped.isEmpty() && look.places.size > 1) append(" Tap a frame to leave it out.")
                }
            }
            if (said.isNotEmpty()) Text(said, color = Pc.TextDim, fontSize = 14.sp)
            Spacer(Modifier.height(10.dp))
            OutlinedTextField(
                importing.name, { importing.name = it.take(40) }, singleLine = true,
                label = { Text("Name") }, placeholder = { Text("idle, walk, attack…") }, modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.height(6.dp))
            Text(
                if (more) "Not right? ▴" else "Not right? ▾", color = Pc.Accent, fontSize = 15.sp,
                modifier = Modifier.clickable { more = !more }.padding(vertical = 8.dp),
            )
            if (more) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("Grid", color = Pc.Text, fontSize = 15.sp, modifier = Modifier.width(110.dp))
                    SheetStepper("Across", importing.across) { importing.across = it; if (importing.down == 0 && it > 0) importing.down = 1; vm.readSpriteSheet(importing) }
                    Spacer(Modifier.width(16.dp))
                    SheetStepper("Down", importing.down) { importing.down = it; if (importing.across == 0 && it > 0) importing.across = 1; vm.readSpriteSheet(importing) }
                }
                Text("Auto finds frames by the gaps between them; set a grid when sprites touch.", color = Pc.TextDim, fontSize = 13.sp)
                Spacer(Modifier.height(8.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("Background", color = Pc.Text, fontSize = 15.sp, modifier = Modifier.width(110.dp))
                    val shownColour = importing.background ?: look?.background
                    Box(
                        Modifier.size(28.dp).clip(RoundedCornerShape(6.dp)).border(1.dp, Pc.Divider, RoundedCornerShape(6.dp))
                            .background(shownColour?.let { Color(0xFF000000 or it.toLong()) } ?: Color.Transparent),
                    )
                    Spacer(Modifier.width(10.dp))
                    Text(
                        if (shownColour == null) "Transparent" else if (importing.background == null) "Found" else "Picked",
                        color = Pc.TextDim, fontSize = 14.sp,
                    )
                    Spacer(Modifier.width(16.dp))
                    TextButton(onClick = { picking = !picking }) { Text(if (picking) "Tap the sheet…" else "Pick from the sheet") }
                    if (importing.background != null) {
                        TextButton(onClick = { importing.background = null; vm.readSpriteSheet(importing) }) { Text("Auto") }
                    }
                }
            }
            Spacer(Modifier.height(8.dp))
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                TextButton(onClick = { vm.spriteSheet = null }) { Text("Cancel") }
                val n = (look?.places?.size ?: 0) - importing.skipped.size
                TextButton(
                    enabled = look != null && !look.tooBig && n > 0 && !importing.reading && !importing.adding,
                    onClick = { vm.addSpriteSheet(importing) },
                ) { Text(if (n > 0) "Import $n ${if (n == 1) "frame" else "frames"}" else "Import") }
            }
        }
    }
}

/** A small number to step: 0 shows as Auto. */
@Composable
private fun SheetStepper(label: String, value: Int, onChange: (Int) -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = Pc.TextDim, fontSize = 14.sp)
        Spacer(Modifier.width(6.dp))
        Text("−", color = Pc.Accent, fontSize = 20.sp, modifier = Modifier.clickable { if (value > 0) onChange(value - 1) }.padding(horizontal = 10.dp))
        Text(if (value == 0) "Auto" else "$value", color = Pc.Text, fontSize = 15.sp, modifier = Modifier.widthIn(min = 36.dp))
        Text("+", color = Pc.Accent, fontSize = 20.sp, modifier = Modifier.clickable { if (value < 16) onChange(value + 1) }.padding(horizontal = 10.dp))
    }
}

/** The sheet and the playing frames: side by side when there's room, else one above the other. */
@Composable
private fun SheetPictures(wide: Boolean, content: @Composable () -> Unit) {
    if (wide) {
        Row(horizontalArrangement = Arrangement.spacedBy(14.dp)) { content() }
    } else {
        Column(verticalArrangement = Arrangement.spacedBy(14.dp)) { content() }
    }
}
