package io.github.artcraftandroid.procraft.ui

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearOutSlowInEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.procraft.canvas.CanvasViewModel
import io.github.artcraftandroid.procraft.core.EdgeCollapse
import io.github.artcraftandroid.procraft.core.LayoutBar
import io.github.artcraftandroid.procraft.core.LayoutPiece
import io.github.artcraftandroid.procraft.core.LayoutRect
import io.github.artcraftandroid.procraft.core.WindowEdge
import kotlin.math.roundToInt

/** How long a slide into a side of the window or back out takes (Collapse to edge), easing out either way. */
private const val SlideMillis = 200

/** How far into its side something is (0 showing, 1 gone), easing out either way as [collapsed] changes. */
@Composable
internal fun collapseSlide(collapsed: Boolean): Float =
    animateFloatAsState(if (collapsed) 1f else 0f, tween(SlideMillis, easing = LinearOutSlowInEasing), label = "collapse").value

/** Moved [slid] of the way into its side, as [place] says (dp of the interface, [px] pixels each). */
internal fun Modifier.collapseSlid(place: EdgeCollapse?, slid: Float, px: Float): Modifier =
    if (place == null || slid == 0f) {
        this
    } else {
        graphicsLayer {
            translationX = place.slideX * slid * px
            translationY = place.slideY * slid * px
        }
    }

/** A chevron in the middle, [size] across, pointing into [edge]. */
private fun DrawScope.chevron(edge: WindowEdge, size: Float, ink: Color) {
    val (along, out) = when (edge) {
        WindowEdge.LEFT -> Offset(0f, 1f) to Offset(-1f, 0f)
        WindowEdge.RIGHT -> Offset(0f, 1f) to Offset(1f, 0f)
        WindowEdge.TOP -> Offset(1f, 0f) to Offset(0f, -1f)
        WindowEdge.BOTTOM -> Offset(1f, 0f) to Offset(0f, 1f)
    }
    val half = size / 2f
    val tip = center + out * (half * 0.5f)
    val width = 1.6.dp.toPx()
    drawLine(ink, tip - out * half + along * half, tip, width)
    drawLine(ink, tip, tip - out * half - along * half, width)
}

/** A window strip's collapse button: a disc with a chevron pointing into [edge], as Fold and Close are drawn. */
@Composable
internal fun FloatingCollapse(modifier: Modifier, edge: WindowEdge, onCollapse: () -> Unit) {
    Box(
        modifier.size(40.dp).semantics { contentDescription = "Collapse" }.clickable(onClick = onCollapse).drawBehind {
            drawCircle(Color.White.copy(alpha = 0.15f), 8.5.dp.toPx())
            chevron(edge, 7.dp.toPx(), Color.Black.copy(alpha = 0.3f))
        },
    )
}

/** [r] (dp of the interface) as a place and a size, [px] pixels each. */
private fun Modifier.at(r: LayoutRect, px: Float): Modifier =
    offset { IntOffset((r.x * px).roundToInt(), (r.y * px).roundToInt()) }.size(r.width.dp, r.height.dp)

/**
 * Collapse to edge's buttons and tabs, over the bars and windows: a button by each bar against a side (none while Edit
 * Layout moves them), sliding it in; and the tab on the side of each bar and window gone in, fading in, a tap or a drag
 * out of the side bringing it back.
 */
@Composable
internal fun CollapseControls(vm: CanvasViewModel) {
    if (!vm.layoutLook.collapse) return
    val px = LocalDensity.current.density
    val belt = vm.beltShown() != null
    for (bar in LayoutBar.entries) {
        if ((bar == LayoutBar.BELT && !belt) || (bar == LayoutBar.SIDE_BAR && !vm.showSideBar)) continue
        key(bar) {
            val collapsed = vm.barCollapsed(bar)
            if (collapsed != null) {
                CollapsedTab(collapsed, px, onTap = { vm.collapseBar(bar, false) }) { dx, dy -> vm.pullBarTab(bar, dx, dy) }
            } else if (!vm.editingLayout) {
                val at = vm.barCollapseButton(bar)
                val place = vm.barCollapsePlace(bar)
                if (at != null && place != null) {
                    Box(
                        Modifier.at(at, px).clip(CircleShape).background(Pc.Bar).keepsItsTouches()
                            .semantics { contentDescription = "Collapse" }
                            .clickable { vm.collapseBar(bar, true) }
                            .drawBehind { chevron(place.edge, size.width * 0.4f, Pc.BarGlyph) },
                    )
                }
            }
        }
    }
    for (piece in listOf(LayoutPiece.LAYERS, LayoutPiece.BRUSHES, LayoutPiece.ACTIONS, LayoutPiece.ADJUSTMENTS)) {
        key(piece) {
            vm.pieceCollapsed(piece)?.let { c ->
                CollapsedTab(c, px, onTap = { vm.collapsePiece(piece, false) }) { dx, dy -> vm.pullPieceTab(piece, dx, dy) }
            }
        }
    }
}

/** The tab of what went into a side ([collapse]): a slim pill with a grip along it, fading in as it is left there. */
@Composable
private fun CollapsedTab(collapse: EdgeCollapse, px: Float, onTap: () -> Unit, onPull: (Float, Float) -> Unit) {
    val shown = remember { Animatable(0f) }
    LaunchedEffect(Unit) { shown.animateTo(1f, tween(SlideMillis, easing = LinearOutSlowInEasing)) }
    val tap by rememberUpdatedState(onTap)
    val pull by rememberUpdatedState(onPull)
    val r = collapse.tab
    Box(
        Modifier.at(r, px).graphicsLayer { alpha = shown.value }.clip(RoundedCornerShape(50))
            .background(Pc.Bar)
            .semantics { contentDescription = "Collapsed" }
            .pointerInput(Unit) { detectTapGestures { tap() } }
            .pointerInput(Unit) {
                var total = Offset.Zero
                detectDragGestures(onDragStart = { total = Offset.Zero }) { change, delta ->
                    change.consume()
                    total += delta
                    pull(total.x / px, total.y / px)
                }
            }
            .drawBehind {
                val along = if (size.height > size.width) Offset(0f, 1f) else Offset(1f, 0f)
                val half = along * (maxOf(size.width, size.height) * 0.25f)
                drawLine(Pc.BarGlyph, center - half, center + half, 2.dp.toPx())
            },
    )
}
