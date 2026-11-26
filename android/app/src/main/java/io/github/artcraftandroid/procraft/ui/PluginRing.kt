package io.github.artcraftandroid.procraft.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.procraft.canvas.CanvasViewModel
import io.github.artcraftandroid.procraft.core.BrushInfo
import io.github.artcraftandroid.procraft.core.PanelRow
import io.github.artcraftandroid.procraft.core.PluginPanel
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.sin

/** How far from the middle a ring's first row runs, and each next one further. */
private val RingFirst = 62.dp
private val RingStep = 58.dp
/** A colour's disc on a ring, and a brush's. */
private val RingColour = 34.dp
private val RingBrush = 52.dp
/** How far the ring's backdrop reaches past its outer row, and stays in from the window's edges. */
private val RingMargin = 12.dp

/** Row [k]'s distance from the middle of a ring. */
internal fun ringRadius(k: Int): Dp = RingFirst + RingStep * k

/** The size of each of [n] items on a ring [radius] out, [base] across at most: they never touch, 6 dp apart at least. */
internal fun ringItem(base: Dp, radius: Dp, n: Int): Dp =
    if (n <= 1) base else min(base.value, (2 * radius.value * sin(PI / n)).toFloat() - 6f).dp

/** Where item [i] of [n] sits on a ring [radius] out, from its middle: the first at the top, the rest clockwise. */
internal fun ringPlace(i: Int, n: Int, radius: Dp): DpOffset {
    val angle = -PI / 2 + 2 * PI * i / n.coerceAtLeast(1)
    return DpOffset((radius.value * cos(angle)).toFloat().dp, (radius.value * sin(angle)).toFloat().dp)
}

/** How far a ring of [rows] reaches from its middle, its backdrop with it. */
internal fun ringReach(rows: List<PanelRow>): Dp {
    val outer = rows.lastIndex.coerceAtLeast(0)
    val item = if (rows.lastOrNull() is PanelRow.Brushes) RingBrush else RingColour
    return ringRadius(outer) + item / 2 + RingMargin
}

/** The middle of a ring reaching [reach] at [want], moved in so it stays in a window [width] × [height] (all dp) where it fits. */
internal fun ringMiddle(want: DpOffset, reach: Dp, width: Dp, height: Dp): DpOffset {
    fun inside(v: Dp, most: Dp): Dp = if (most < reach * 2) most / 2 else v.coerceIn(reach, most - reach)
    return DpOffset(inside(want.x, width), inside(want.y, height))
}

/**
 * A plugin's panel shown as a ring (interface 11): round where its action came from (the pen whose
 * button ran it, or the window's middle), its rows of colours and brushes each a ring about it, the
 * first innermost, on a round backdrop. A colour or a brush tapped is told to the plugin as its
 * place; a tap anywhere else closes it, as Done would.
 */
@Composable
fun PluginRingView(vm: CanvasViewModel, panel: PluginPanel) {
    val density = LocalDensity.current
    BoxWithConstraints(
        Modifier.fillMaxSize().pointerInput(panel) {
            awaitEachGesture {
                awaitFirstDown().consume()
                vm.closePluginPanel()
            }
        },
    ) {
        val reach = ringReach(panel.rows)
        val want = panel.at?.let { with(density) { DpOffset(it.x.toDp(), it.y.toDp()) } } ?: DpOffset(maxWidth / 2, maxHeight / 2)
        val middle = ringMiddle(want, reach, maxWidth, maxHeight)
        // the backdrop, which takes its own taps (between the items) without closing
        Box(
            Modifier.offset(middle.x - reach, middle.y - reach).size(reach * 2).clip(CircleShape).background(Pc.PopoverFrost)
                .semantics { contentDescription = panel.title }
                .pointerInput(Unit) { awaitEachGesture { awaitFirstDown().consume() } },
        )
        // where the pen was
        Box(Modifier.offset(middle.x - 4.dp, middle.y - 4.dp).size(8.dp).border(1.5.dp, Pc.PanelGlyph, CircleShape))
        panel.rows.forEachIndexed { k, row ->
            val radius = ringRadius(k)
            when (row) {
                is PanelRow.Colours -> row.colours.forEachIndexed { i, colour ->
                    val d = ringItem(RingColour, radius, row.colours.size)
                    val at = ringPlace(i, row.colours.size, radius)
                    val chosen = row.chosen?.toInt() == i
                    Box(
                        Modifier.offset(middle.x + at.x - d / 2, middle.y + at.y - d / 2).size(d).clip(CircleShape)
                            .clickable { vm.touchPluginPanel(k, i.toFloat()) }
                            .then(if (chosen) Modifier.border(2.dp, Pc.Text, CircleShape).padding(4.dp) else Modifier)
                            .clip(CircleShape).background(Color(0xFF000000L or colour.toLong())),
                    )
                }
                is PanelRow.Brushes -> row.brushes.forEachIndexed { i, b ->
                    val d = ringItem(RingBrush, radius, row.brushes.size)
                    val at = ringPlace(i, row.brushes.size, radius)
                    RingBrush(vm, b, row.chosen?.toInt() == i, d, Modifier.offset(middle.x + at.x - d / 2, middle.y + at.y - d / 2)) {
                        vm.touchPluginPanel(k, i.toFloat())
                    }
                }
                // a ring holds no other kind of row
                else -> {}
            }
        }
    }
}

/** A brush on a ring or a panel's row: its sample stroke in a disc [d] across, ringed in the accent while [chosen]. */
@Composable
internal fun RingBrush(vm: CanvasViewModel, b: io.github.artcraftandroid.procraft.core.PanelBrush, chosen: Boolean, d: Dp, modifier: Modifier, onTap: () -> Unit) {
    val info = BrushInfo(b.set, b.index, b.name)
    val preview = vm.previews["${b.set}-${b.index}"]
    LaunchedEffect(b.set, b.index, preview == null) { if (preview == null) vm.loadPreview(info) }
    Box(
        modifier.size(d).clip(CircleShape).background(if (chosen) Pc.RowSelected else Pc.ActionDisc)
            .then(if (chosen) Modifier.border(2.dp, Pc.Accent, CircleShape) else Modifier.border(1.dp, Pc.SwatchEdge, CircleShape))
            .clickable(onClick = onTap).semantics { contentDescription = b.name },
        contentAlignment = Alignment.Center,
    ) {
        preview?.let {
            // white strokes: dark on the light interface's discs
            val tint = if (Pc.light && !chosen) androidx.compose.ui.graphics.ColorFilter.tint(Color.Black, androidx.compose.ui.graphics.BlendMode.SrcIn) else null
            Image(it, contentDescription = null, contentScale = ContentScale.Crop, colorFilter = tint, modifier = Modifier.fillMaxSize())
        }
    }
}
