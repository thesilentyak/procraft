package io.github.artcraftandroid.procraft.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.artcraftandroid.procraft.canvas.CanvasViewModel
import kotlin.math.roundToInt

/** The row held out of the layers panel, as measured off a row: its size and its thumbnail's. */
private val HeldWidth = 360.dp
private val HeldHeight = 69.dp
private val HeldCorner = 8.dp
/** The badge saying it would land (a copy), and the one counting several. */
internal val LandsBadge = Color(0xFF30D158)
private val CountBadge = Color(0xFFFF3B30)

/**
 * The screen's root, under both the gallery and the canvas: it keeps where every finger is, and
 * follows the one holding layers lifted out of the layers panel ([CanvasViewModel.heldLayers]) through
 * whatever the other hand opens, showing the held row under it, until it lets go.
 */
@Composable
internal fun HoldingLayers(vm: CanvasViewModel, content: @Composable BoxScope.() -> Unit) {
    var root by remember { mutableStateOf<LayoutCoordinates?>(null) }
    // gone (the screen made again as the window turns), the finger's lift will never come: let go
    DisposableEffect(vm) {
        onDispose {
            vm.cancelHeld()
            vm.fingersAt.clear()
        }
    }
    Box(
        Modifier.fillMaxSize().onGloballyPositioned { root = it }.pointerInput(vm) {
            awaitPointerEventScope {
                while (true) {
                    // first, before anything under it: the held finger moves the row and nothing else
                    val event = awaitPointerEvent(PointerEventPass.Initial)
                    for (change in event.changes) {
                        val at = root?.takeIf { it.isAttached }?.localToWindow(change.position) ?: change.position
                        if (change.pressed) vm.fingersAt[change.id] = at else vm.fingersAt.remove(change.id)
                        val held = vm.heldLayers ?: continue
                        if (change.id != held.pointer) continue
                        change.consume()
                        if (change.pressed) vm.moveHeld(at) else vm.dropHeld(at)
                    }
                }
            }
        },
    ) {
        content()
        vm.heldLayers?.let { held ->
            val corner = root?.takeIf { it.isAttached }?.windowToLocal(vm.heldLayersAt - held.grab) ?: (vm.heldLayersAt - held.grab)
            HeldRow(held, vm.heldLands(), corner)
        }
    }
}

/** The row held, its corner at [corner]: blue as the current row, its thumbnail and name, a count if it holds several, and a green plus where letting go would land it. */
@Composable
private fun HeldRow(held: CanvasViewModel.HeldLayers, lands: Boolean, corner: Offset) {
    Box(Modifier.offset { IntOffset(corner.x.roundToInt(), corner.y.roundToInt()) }.size(HeldWidth, HeldHeight)) {
        Row(
            Modifier.fillMaxSize().shadow(12.dp, RoundedCornerShape(HeldCorner)).clip(RoundedCornerShape(HeldCorner))
                .background(Pc.RowSelected).graphicsLayer { alpha = 0.95f },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.padding(start = 4.dp).size(93.dp, 61.dp).clip(RoundedCornerShape(4.dp)).background(Pc.ThumbBackground)) {
                held.thumbnail?.let {
                    Image(it, contentDescription = null, contentScale = ContentScale.Fit, filterQuality = FilterQuality.None, modifier = Modifier.fillMaxSize())
                }
            }
            Text(
                held.name,
                color = Color.White,
                fontSize = 17.sp,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.padding(horizontal = 12.dp).width(HeldWidth - 140.dp),
            )
        }
        if (held.items > 1) DragBadge(CountBadge, held.items.toString(), Modifier.align(Alignment.TopStart).offset((-8).dp, (-8).dp))
        if (lands) DragBadge(LandsBadge, "+", Modifier.align(Alignment.TopEnd).offset(8.dp, (-8).dp))
    }
}

@Composable
internal fun DragBadge(fill: Color, text: String, modifier: Modifier) {
    Box(modifier.size(24.dp).clip(CircleShape).background(fill), contentAlignment = Alignment.Center) {
        Text(text, color = Color.White, fontSize = 15.sp, maxLines = 1)
    }
}
