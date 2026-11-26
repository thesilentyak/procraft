package io.github.artcraftandroid.procraft.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.input.pointer.AwaitPointerEventScope
import androidx.compose.foundation.layout.wrapContentHeight
import kotlinx.coroutines.withTimeoutOrNull
import kotlin.math.roundToInt
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onPlaced
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.artcraftandroid.procraft.canvas.CanvasViewModel
import io.github.artcraftandroid.procraft.canvas.Panel
import io.github.artcraftandroid.procraft.core.ShadowStep
import io.github.artcraftandroid.procraft.core.SideSliderTap
import io.github.artcraftandroid.procraft.core.ToolKind
import io.github.artcraftandroid.procraft.core.sideSliderDragShare
import io.github.artcraftandroid.procraft.core.sideSliderShownPercent
import io.github.artcraftandroid.procraft.core.sideSliderTap
import io.github.artcraftandroid.procraft.core.sideThumbShadow

private val SliderLeft = 5.dp
private val SizeTrackTop = 6.dp
private val OpacityTrackTop = 248.dp

/**
 * Whether window point [p] is over the top bar (down to [barBottom], window pixels) or the side bar
 * ([sideBar], where it is shown), with the bars shown ([hidden] not).
 */
internal fun overBars(p: Offset, barBottom: Float, sideBar: androidx.compose.ui.geometry.Rect?, hidden: Boolean): Boolean =
    !hidden && (p.y <= barBottom || sideBar?.contains(p) == true)

/**
 * Whether window point [p] is over the bars (see [overBars]) or one of [covers], the panels open over
 * the canvas (window pixels), however the bars show: the canvas under them isn't where the pen hovers.
 */
internal fun overInterface(
    p: Offset,
    barBottom: Float,
    sideBar: androidx.compose.ui.geometry.Rect?,
    hidden: Boolean,
    covers: Collection<androidx.compose.ui.geometry.Rect>,
): Boolean = overBars(p, barBottom, sideBar, hidden) || covers.any { it.contains(p) }

/** A tap target without a ripple: the bars show their state through colour alone. */
private fun Modifier.tap(enabled: Boolean = true, onClick: () -> Unit): Modifier = composed {
    clickable(remember { MutableInteractionSource() }, indication = null, enabled = enabled, onClick = onClick)
}

/** The bar across the top of the canvas: artwork actions on the left, painting tools on the right. */
@Composable
fun TopBar(vm: CanvasViewModel, modifier: Modifier = Modifier) {
    // (a press on the bar between its buttons is the bar's: never a dot on the canvas, nor a colour picked)
    BoxWithConstraints(modifier.fillMaxWidth().height(Pc.BarHeight).background(Pc.Bar).keepsItsTouches()) {
        val fit = Pc.barFit(maxWidth)
        Row(
            Modifier.align(Alignment.CenterStart).padding(start = fit.margin),
            horizontalArrangement = Arrangement.spacedBy(fit.actionGap),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            ActionButtons(vm, fit, upright = false)
        }
        Row(
            Modifier.align(Alignment.CenterEnd).padding(end = fit.margin),
            horizontalArrangement = Arrangement.spacedBy(fit.toolGap),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            ToolButtons(vm, fit, upright = false)
        }
    }
}

/** Gallery and the actions: the top bar's left half, each button turned to stand in a column while [upright]. */
@Composable
private fun ActionButtons(vm: CanvasViewModel, fit: Pc.BarFit, upright: Boolean) {
    val gallery = if (upright) Modifier.size(Pc.BarButtonHeight, Pc.GalleryWidth) else Modifier.size(Pc.GalleryWidth, Pc.BarButtonHeight)
    Box(gallery.tap { vm.closeArtwork() }, contentAlignment = if (upright) Alignment.Center else Alignment.CenterStart) {
        // upright, the word reads up the column
        val turned = if (upright) Modifier.requiredSize(Pc.GalleryWidth, Pc.BarButtonHeight).graphicsLayer { rotationZ = -90f } else Modifier.offset(y = (-1).dp)
        Box(turned, contentAlignment = Alignment.CenterStart) {
            Text("Gallery", color = Pc.BarText, fontSize = 18.sp, fontWeight = FontWeight.Medium, maxLines = 1, softWrap = false)
        }
    }
    ActionButton(PcIcons.Actions, "Actions", fit.button, selected = vm.panel == Panel.ACTIONS, upright = upright) { vm.toggle(Panel.ACTIONS) }
    ActionButton(PcIcons.Adjustments, "Adjustments", fit.button, selected = vm.panel == Panel.ADJUSTMENTS || vm.adjustment != null || vm.opacityLayer != null || vm.recoloring, upright = upright) { vm.adjustmentsPressed() }
    // lit while the selection tool is open, or while something is selected
    ActionButton(PcIcons.Selection, "Selection", fit.button, selected = vm.selecting || vm.hasSelection, glyph = 33.dp, upright = upright) { vm.toggleSelecting() }
    ActionButton(PcIcons.Transform, "Transform", fit.button, selected = vm.transforming, glyph = 44.dp, upright = upright) { vm.toggleTransform() }
    io.github.artcraftandroid.procraft.generation.GenerationUi.BarButton(vm, fit.button, upright)
}

/** The tools, Layers and the colour: the top bar's right half, standing in a column while [upright]. */
@Composable
private fun ToolButtons(vm: CanvasViewModel, fit: Pc.BarFit, upright: Boolean) {
    // the layers and colour panels take the highlight from the current tool while open
    val panelOpen = vm.panel == Panel.LAYERS || vm.panel == Panel.COLOR || vm.transforming || vm.selecting || vm.adjustment != null
    // painting an adjustment on (or cloning), the brush shows it
    ToolButton(
        if (vm.adjustPencil) PcIcons.BrushPaintedOn else PcIcons.Brush,
        "Paint",
        fit.button,
        selected = vm.adjustPencil || (!panelOpen && vm.tool == ToolKind.PAINT),
        onHold = { vm.toolHeld(ToolKind.PAINT) },
        upright = upright,
    ) {
        vm.toolPressed(ToolKind.PAINT)
    }
    ToolButton(PcIcons.Smudge, "Smudge", fit.button, selected = !panelOpen && vm.tool == ToolKind.SMUDGE, onHold = { vm.toolHeld(ToolKind.SMUDGE) }, upright = upright) {
        vm.toolPressed(ToolKind.SMUDGE)
    }
    ToolButton(PcIcons.Erase, "Erase", fit.button, selected = !panelOpen && vm.tool == ToolKind.ERASE, onHold = { vm.toolHeld(ToolKind.ERASE) }, upright = upright) {
        vm.toolPressed(ToolKind.ERASE)
    }
    ToolButton(PcIcons.Layers, "Layers", fit.button, selected = vm.panel == Panel.LAYERS, upright = upright) { vm.toggle(Panel.LAYERS) }
    ColorButton(vm, fit.button, upright)
}

/** The room at either end of half of the top bar moved by Edit layout, past its first and last buttons. */
val TopBarHalfEnd = 10.dp

/** How long half of the top bar is ([left], else the right), laid in a row on a bar [barWidth] wide: its buttons, gaps and ends. */
fun topBarHalfLength(left: Boolean, barWidth: Dp): Dp {
    val fit = Pc.barFit(barWidth)
    return if (left) {
        // (and image generation's button after Transform, when it's built in)
        TopBarHalfEnd * 2 + Pc.GalleryWidth + (fit.button + fit.actionGap) * (if (io.github.artcraftandroid.procraft.generation.GenerationUi.enabled) 5 else 4)
    } else {
        TopBarHalfEnd * 2 + fit.button * 5 + fit.toolGap * 4
    }
}

/** How long a moved bar's corners take to turn square or round as it comes to an edge or leaves it. */
private const val BarCornerMillis = 150

/**
 * A bar's corners as Edit layout leaves it: [radius] round all round, but square on each side of it against the
 * window's edges ([sides], the bar's own way up), each easing over as the bar is dragged so none snaps. Top-left,
 * top-right, bottom-right, bottom-left.
 */
@Composable
internal fun barCorners(sides: io.github.artcraftandroid.procraft.core.BarSides, radius: Dp): List<Dp> {
    val square = listOf(sides.left || sides.top, sides.top || sides.right, sides.right || sides.bottom, sides.bottom || sides.left)
    return square.map {
        androidx.compose.animation.core.animateDpAsState(
            if (it) 0.dp else radius,
            androidx.compose.animation.core.tween(BarCornerMillis),
            label = "bar corner",
        ).value
    }
}

/**
 * [sides] of a bar as the window has them, as the bar's own: for one drawn lying down by turning it a quarter back
 * about its top-left (its own left lies at the window's bottom, its top at the left, and so on round).
 */
internal fun turnedSides(sides: io.github.artcraftandroid.procraft.core.BarSides): io.github.artcraftandroid.procraft.core.BarSides =
    io.github.artcraftandroid.procraft.core.BarSides(left = sides.bottom, top = sides.left, right = sides.top, bottom = sides.right)

/** [sides] of a bar against the window's edges as its glass reaches past them, [past] out (none: as a free bar's). */
internal fun pastEdges(sides: io.github.artcraftandroid.procraft.core.BarSides?, past: Dp, glass: BarGlass): BarGlass =
    if (sides == null) {
        glass
    } else {
        glass.copy(
            pastLeft = if (sides.left) past else 0.dp,
            pastRight = if (sides.right) past else 0.dp,
            pastBottom = if (sides.bottom) past else 0.dp,
            pastTop = if (sides.top) past else 0.dp,
        )
    }

/**
 * Half of the top bar ([left], else the right) where Edit layout moved it: on glass of its own, its buttons in a row,
 * or in a column while [upright] against a side; as long either way ([topBarHalfLength]). Round at its ends, square on
 * the sides of it against the window's edges ([edges]).
 */
@Composable
fun TopBarHalf(vm: CanvasViewModel, left: Boolean, upright: Boolean, barWidth: Dp, modifier: Modifier, edges: io.github.artcraftandroid.procraft.core.BarSides? = null) {
    val fit = Pc.barFit(barWidth)
    val gap = if (left) fit.actionGap else fit.toolGap
    val c = barCorners(edges ?: io.github.artcraftandroid.procraft.core.BarSides(left = false, top = false, right = false, bottom = false), Pc.BarHeight / 2)
    val glass = modifier.clip(RoundedCornerShape(c[0], c[1], c[2], c[3])).background(Pc.Bar).keepsItsTouches()
    if (upright) {
        androidx.compose.foundation.layout.Column(
            glass.size(Pc.BarHeight, topBarHalfLength(left, barWidth)).padding(vertical = TopBarHalfEnd),
            verticalArrangement = Arrangement.spacedBy(gap),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            if (left) ActionButtons(vm, fit, upright = true) else ToolButtons(vm, fit, upright = true)
        }
    } else {
        Row(
            glass.size(topBarHalfLength(left, barWidth), Pc.BarHeight).padding(horizontal = TopBarHalfEnd),
            horizontalArrangement = Arrangement.spacedBy(gap),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (left) ActionButtons(vm, fit, upright = false) else ToolButtons(vm, fit, upright = false)
        }
    }
}

/** An action button: its glyph on a dark disc, which turns blue while its panel is open. */
@Composable
internal fun ActionButton(icon: ImageVector, label: String, width: Dp, selected: Boolean = false, glyph: Dp = 36.dp, upright: Boolean = false, onClick: () -> Unit = {}) {
    val lit = animateHighlight(selected)
    val k = Pc.barGlyphScale(width)
    Box(Modifier.buttonSize(width, upright).tap(onClick = onClick), contentAlignment = Alignment.Center) {
        Box(Modifier.size(Pc.ActionDiscSize * k).clip(CircleShape).background(lerp(Pc.ActionDisc, Pc.BarGlyphSelected, lit)))
        // the glyphs are drawn in a 44 frame, shown [glyph] across (most 36)
        Icon(icon, contentDescription = label, tint = lerp(Pc.ActionGlyph, Color.White, lit), modifier = Modifier.requiredSize(glyph * k))
    }
}

/** A tap target that also does [onHold] when held instead. */
private fun Modifier.tapOrHold(onClick: () -> Unit, onHold: () -> Unit): Modifier = composed {
    val tap by rememberUpdatedState(onClick)
    val hold by rememberUpdatedState(onHold)
    pointerInput(Unit) { detectTapGestures(onLongPress = { hold() }, onTap = { tap() }) }
}

/** A tool button: tapped, the tool (or its brushes); held ([onHold]), it takes the current brush. */
@Composable
private fun ToolButton(icon: ImageVector, label: String, width: Dp, selected: Boolean, onHold: (() -> Unit)? = null, upright: Boolean = false, onClick: () -> Unit) {
    val touch = if (onHold != null) Modifier.tapOrHold(onClick, onHold) else Modifier.tap(onClick = onClick)
    Box(Modifier.buttonSize(width, upright).then(touch), contentAlignment = Alignment.Center) {
        // a 44 frame shown as it is, over the 38 tall button
        Icon(icon, contentDescription = label, tint = lerp(Pc.BarGlyph, Pc.BarGlyphSelected, animateHighlight(selected)), modifier = Modifier.requiredSize(44.dp * Pc.barGlyphScale(width)))
    }
}

/** A button of the top bar's: [width] along the bar, its height across; turned, in a column while [upright]. */
private fun Modifier.buttonSize(width: Dp, upright: Boolean): Modifier =
    if (upright) size(Pc.BarButtonHeight, width) else size(width, Pc.BarButtonHeight)

@Composable
private fun ColorButton(vm: CanvasViewModel, width: Dp, upright: Boolean = false) {
    val rgb = vm.colorRgb()
    // ringed only where the colour would fade into the bar: dark ones on the dark bar, light ones
    // on the light bar
    val ringed = if (Pc.light) vm.brightness > LIGHT_RING_ABOVE else vm.brightness < DARK_RING_BELOW
    // held a moment, the previous colour (and held again, back)
    // dragged out onto the canvas, it fills there (ColorDrop)
    Box(Modifier.buttonSize(width, upright).dropSource(vm, onHold = { vm.previousColor() }) { vm.toggle(Panel.COLOR) }, contentAlignment = Alignment.Center) {
        val dot = Modifier.size(Pc.ColorDot * Pc.barGlyphScale(width)).clip(CircleShape).background(CanvasSpace.color(rgb))
        Box(if (ringed) dot.border(1.dp, Pc.ColorRing, CircleShape) else dot)
    }
}

/** The colour button's brightness (HSB) under which it is ringed on the dark bar. */
private const val DARK_RING_BELOW = 0.35f

/** The colour button's brightness over which it is ringed on the light bar. */
private const val LIGHT_RING_ABOVE = 0.85f

/**
 * The side bar's body across, in its own pixels ([width] wide from 0), stretched [stretch] toward the
 * finger: the bar is placed that far out from its edge, so its body reaches back as far on its edge
 * side (left, or right if [right]-handed) to stay against the window's edge. (left, right).
 */
internal fun sideBarBody(width: Float, stretch: Float, right: Boolean): Pair<Float, Float> {
    val s = stretch.coerceAtLeast(0f)
    return if (right) 0f to width + s else (0f - s) to width
}

/**
 * The side bar against the left edge (the right one, [right]-handed): brush size, the modify
 * button, opacity, undo and redo, its tracks [trackHeight] long. On the right everything is
 * mirrored but the glyphs. Stretched [stretch] toward the finger (its modify button dragged out),
 * it is placed that far out by its caller and its glass reaches back as far: its edge side stays
 * against the window's edge (see [sideBarBody]). Moved by Edit layout, it is square only on the sides of it against the
 * window's edges ([edges], its own way up), and round all round away from them. [turn] slides it into its side or lays
 * it down: its glass goes with it (what [modifier] does, as it slides out of sight, the glass follows by [shown]).
 */
@Composable
fun SideBar(
    vm: CanvasViewModel,
    modifier: Modifier = Modifier,
    turn: Modifier = Modifier,
    right: Boolean = false,
    trackHeight: Dp = Pc.SliderHeight,
    shown: Float = 1f,
    stretch: Dp = 0.dp,
    edges: io.github.artcraftandroid.procraft.core.BarSides? = null,
    glassEdges: io.github.artcraftandroid.procraft.core.BarSides? = edges,
) {
    // a control's left edge, mirrored on the right
    fun x(left: Dp, width: Dp) = if (right) Pc.SideBarWidth - left - width else left
    // shorter tracks bring what is under each up with them
    val shorter = Pc.SliderHeight - trackHeight
    val opacityTop = OpacityTrackTop - shorter
    // its glass slides out with it as the interface hides, its screen side's corners off the screen
    val aside = with(androidx.compose.ui.platform.LocalDensity.current) { Pc.SideBarWidth.toPx() } * (1f - shown) * if (right) 1f else -1f
    val moved = Offset(aside, 0f)
    // its glass goes past the screen's edge (to take that side's corners off the screen), the
    // further for its stretch
    val past = Pc.SideBarCorner + stretch.coerceAtLeast(0.dp)
    // (its frost by the window's sides it touches: lying down, its own are turned)
    val body = pastEdges(glassEdges, past, BarGlass(shown, moved, pastLeft = if (right) 0.dp else past, pastRight = if (right) past else 0.dp))
    val corners = edges?.let { barCorners(it, Pc.SideBarCorner) }
    // the tracks' own thin glass, unless a plugin has them frosted as the bar is (which the bar's
    // glass under them is already)
    val track = BarGlass(shown, moved, window = true).takeIf { Pc.sliderLook.frosted != true }
    Box(
        // (its glass where it is drawn: laid down, across, not standing where it was laid out)
        turn.then(rememberFrost(Pc.SideBarCorner, body)).then(modifier).size(Pc.SideBarWidth, Pc.SideBarHeight - shorter * 2)
            // a touch on it keeps a slider's panel up
            .onGloballyPositioned { vm.sideBarOnWindow = it.boundsInWindow() }
            .drawBehind {
                // translucent, with windows where the slider tracks are: they show what is behind
                val r = Pc.SideBarCorner.toPx()
                val track = Size(Pc.SliderWidth.toPx(), trackHeight.toPx())
                val trackR = CornerRadius(Pc.SliderCorner.toPx())
                val windows = listOf(SizeTrackTop, opacityTop).map { top ->
                    val o = Offset(x(SliderLeft, Pc.SliderWidth).toPx(), top.toPx())
                    RoundRect(o.x, o.y, o.x + track.width, o.y + track.height, trackR)
                }
                val body = Path().apply {
                    fillType = PathFillType.EvenOdd
                    // square against the screen's edge, reaching back to it however it is stretched
                    val (left, rightSide) = sideBarBody(size.width, stretch.toPx(), right)
                    val (edge, outer) = CornerRadius.Zero to CornerRadius(r)
                    val each = corners?.map { CornerRadius(it.toPx()) }
                    addRoundRect(
                        when {
                            each != null -> RoundRect(left, 0f, rightSide, size.height, each[0], each[1], each[2], each[3])
                            right -> RoundRect(left, 0f, rightSide, size.height, outer, edge, edge, outer)
                            else -> RoundRect(left, 0f, rightSide, size.height, edge, outer, outer, edge)
                        },
                    )
                    windows.forEach { addRoundRect(it) }
                }
                drawPath(body, Pc.SideBar)
                // a darker hairline round each window
                val edge = Pc.SliderEdgeWidth.toPx()
                val ring = Path().apply {
                    fillType = PathFillType.EvenOdd
                    for (w in windows) {
                        addRoundRect(RoundRect(w.left - edge, w.top - edge, w.right + edge, w.bottom + edge, CornerRadius(trackR.x + edge)))
                        addRoundRect(w)
                    }
                }
                drawPath(ring, Pc.SliderEdge)
            },
    ) {
        BrushSlider(vm.size, vm::changeSize, Modifier.offset(x(SliderLeft, Pc.SliderWidth), SizeTrackTop).then(track?.let { rememberFrost(Pc.SliderCorner, it) } ?: Modifier), panel(vm, CanvasViewModel.SliderPanel.SIZE), vm.sizeNotches, trackHeight)
        // the modify button: the next touch on the canvas picks a colour
        // (where it is on the window as a touch goes on: the bar may move under the finger)
        val placed = remember { arrayOfNulls<androidx.compose.ui.layout.LayoutCoordinates>(1) }
        Box(
            Modifier.offset(x(5.dp, 40.dp), 191.dp - shorter).size(40.dp, 41.dp).onPlaced { placed[0] = it }.pointerInput(Unit) {
                // held, it changes what a canvas touch does; tapped, it does what Tap □ is set to;
                // dragged out from the bar's edge and then up or down, it moves the bar
                awaitEachGesture {
                    val down = awaitFirstDown()
                    down.consume()
                    // the window's pixels as the bar's points
                    fun onWindow(p: Offset): Offset = (placed[0]?.takeIf { it.isAttached }?.localToWindow(p) ?: p) / density
                    val first = onWindow(down.position)
                    val touch = vm.modifyTouch(down.id.value, first.x, first.y, down.uptimeMillis)
                    var lifted = false
                    try {
                        while (!lifted) {
                            val c = awaitPointerEvent().changes.firstOrNull { it.id == down.id } ?: continue
                            if (!c.pressed) {
                                // a cancelled touch ends already consumed; a lift doesn't
                                touch.lifted(c.id.value, cancelled = c.isConsumed)
                                c.consume()
                                lifted = true
                            } else {
                                val on = c.position.x in 0f..size.width.toFloat() && c.position.y in 0f..size.height.toFloat()
                                val at = onWindow(c.position)
                                touch.moved(c.id.value, at.x, at.y, c.uptimeMillis, onButton = on)
                                c.consume()
                            }
                        }
                    } finally {
                        // the bar put away mid-touch: let go of it all the same
                        if (!lifted) touch.lifted(down.id.value, cancelled = true)
                    }
                }
            },
            contentAlignment = Alignment.Center,
        ) {
            Box(
                Modifier.size(Pc.ModifySize).drawBehind {
                    val w = Pc.ModifyStroke.toPx()
                    drawRoundRect(
                        if (vm.pickNext) Pc.Accent else Pc.SideGlyph,
                        topLeft = Offset(w / 2f, w / 2f),
                        size = Size(size.width - w, size.height - w),
                        cornerRadius = CornerRadius(Pc.ModifyCorner.toPx() - w / 2f),
                        style = Stroke(w),
                    )
                },
            )
        }
        BrushSlider(vm.opacity, vm::changeOpacity, Modifier.offset(x(SliderLeft, Pc.SliderWidth), opacityTop).then(track?.let { rememberFrost(Pc.SliderCorner, it) } ?: Modifier), panel(vm, CanvasViewModel.SliderPanel.OPACITY), vm.opacityNotches, trackHeight)
        // beside the slider it belongs to, a little above it, outside the bar
        vm.sliderPanel?.let { which ->
            val top = if (which == CanvasViewModel.SliderPanel.SIZE) SizeTrackTop else opacityTop
            SliderPanel(vm, which, Modifier.offset(x(SliderLeft + Pc.SliderWidth + SliderPanelGap, SliderPanelSide), top - SliderPanelRise))
        }
        SideButton(PcIcons.Undo, "Undo", vm.canUndo, Modifier.offset(x(5.dp, 40.dp), 430.5.dp - shorter * 2)) { vm.undo() }
        SideButton(PcIcons.Redo, "Redo", vm.canRedo, Modifier.offset(x(5.dp, 40.dp), 472.5.dp - shorter * 2)) { vm.redo() }
    }
}

@Composable
private fun SideButton(icon: ImageVector, label: String, enabled: Boolean, modifier: Modifier, onClick: () -> Unit) {
    // turning on or off (a stroke kept, the last step undone) fades over 0.15 s either way, linear,
    // as measured: the lit glyph over the unlit one
    val lit = androidx.compose.animation.core.animateFloatAsState(if (enabled) 1f else 0f, Motion.highlightOff(), label = "enabled").value
    Box(modifier.size(40.dp, 41.dp).tap(enabled, onClick), contentAlignment = Alignment.Center) {
        // a 50 frame shown 36 across
        if (lit < 1f) Icon(icon, contentDescription = label.takeIf { lit <= 0f }, tint = Pc.SideGlyphDisabled, modifier = Modifier.requiredSize(36.dp))
        if (lit > 0f) Icon(icon, contentDescription = label, tint = Pc.SideGlyph.copy(alpha = Pc.SideGlyph.alpha * lit), modifier = Modifier.requiredSize(36.dp))
    }
}

/**
 * What a slider does with its panel: [show] it (dragged or held, it comes up and stays up when the
 * slider is let go, for its + to be tapped), [hide] it, and whether a slider's panel is [up].
 */
class SliderPanelHooks(val show: () -> Unit, val hide: () -> Unit, val up: () -> Boolean)

private fun panel(vm: CanvasViewModel, which: CanvasViewModel.SliderPanel) = SliderPanelHooks(
    show = { vm.sliderPanel = which },
    hide = { vm.sliderPanel = null },
    up = { vm.sliderPanel != null },
)

private val SliderPanelSide = 190.dp
private val SliderPanelGap = 8.dp
private val SliderPanelRise = 10.dp
// dark glass over the canvas (what is behind it blurred), or light
private val SliderPanelFill get() = if (Pc.light) Color(0xFFE7E7E7) else Color(0xC93E3E3E)
private val SliderPanelInk get() = if (Pc.light) Color(0xFF393939) else Color(0xFFF0F0F0)
// the dot is drawn in the interface's ink, not the brush's colour
private val SliderPanelDot get() = if (Pc.light) Color.Black else Color.White

/**
 * A slider's panel: its name and value ("Size 8%"), a button saving the value as a notch on the
 * slider ("+"; "-" takes away the notch it is on), and in the middle the brush's stamp as it would
 * paint now (soft where it is soft), its size on the screen (and at its opacity).
 */
@Composable
private fun SliderPanel(vm: CanvasViewModel, which: CanvasViewModel.SliderPanel, modifier: Modifier) {
    val size = which == CanvasViewModel.SliderPanel.SIZE
    val value = if (size) vm.size else vm.opacity
    val diameter = androidx.compose.runtime.remember(vm.size) { vm.brushScreenDiameter() }
    // the brush's stamp as it paints (soft where it is soft), as many pixels across as it shows
    val density = androidx.compose.ui.platform.LocalDensity.current
    val radius = with(density) { (diameter / 2f).coerceIn(1.5f, SliderPanelSide.toPx() / 2f - 8.dp.toPx()) }
    val side = (radius * 2f).roundToInt().coerceAtLeast(2)
    val stamp = androidx.compose.runtime.remember(vm.size, side) { vm.brushStamp(side) }
    // its own size, past the bar's edge (the bar's width would otherwise squeeze it)
    Box(
        modifier.wrapContentSize(Alignment.TopStart, unbounded = true).size(SliderPanelSide)
            // a touch on it keeps it up
            .onGloballyPositioned { vm.sliderPanelOnWindow = it.boundsInWindow() }
            .then(if (Pc.light) Modifier else rememberFrost(22.dp))
            .clip(RoundedCornerShape(22.dp)).background(SliderPanelFill)
            // a hairline rim, as measured (about 25 over the glass)
            .border(1.dp, Pc.GlassMenuEdge, RoundedCornerShape(22.dp)),
    ) {
        Text(
            androidx.compose.ui.text.buildAnnotatedString {
                pushStyle(androidx.compose.ui.text.SpanStyle(fontWeight = FontWeight.Bold))
                append(if (size) "Size" else "Opacity")
                pop()
                append(" ${sideSliderShownPercent(size, value)}%")
            },
            color = SliderPanelInk,
            fontSize = 19.sp,
            maxLines = 1,
            modifier = Modifier.offset(16.dp, 6.dp).height(32.dp).wrapContentHeight(Alignment.CenterVertically),
        )
        // read again as the notches change, so a value saved or taken away shows at once
        val notches = if (size) vm.sizeNotches else vm.opacityNotches
        val onNotch = androidx.compose.runtime.remember(notches, value) { vm.notchAt(size, value) != null }
        val canSave = androidx.compose.runtime.remember(notches, value) { vm.canSaveNotch(size) }
        if (onNotch || canSave) {
            Box(Modifier.offset(150.dp, 6.dp).size(32.dp).tap { vm.toggleNotch(size) }, contentAlignment = Alignment.Center) {
                Icon(
                    if (onNotch) PcIcons.NotchRemove else PcIcons.NotchAdd,
                    contentDescription = if (onNotch) "Remove" else "Save",
                    tint = SliderPanelInk,
                    modifier = Modifier.size(24.dp),
                )
            }
        }
        androidx.compose.foundation.Canvas(Modifier.fillMaxSize()) {
            val ink = SliderPanelDot.copy(alpha = if (size) 1f else value.coerceIn(0.02f, 1f))
            if (stamp == null) {
                drawCircle(ink, radius, center)
            } else {
                drawImage(
                    stamp,
                    topLeft = androidx.compose.ui.geometry.Offset(center.x - side / 2f, center.y - side / 2f),
                    colorFilter = androidx.compose.ui.graphics.ColorFilter.tint(ink),
                )
            }
        }
    }
}

/**
 * An upright slider moved by dragging anywhere on it: the thumb follows the finger (relative,
 * not jump-to-touch), finer the farther out to the side the finger goes, 1 at the top. Moved or
 * held still, its [panel] comes up, and stays up when the finger lifts; its thumb tapped brings
 * the panel up, or takes it away; a tap elsewhere on the track takes the thumb there (and the
 * panel away). Its [notches] (saved values) show as marks on the track; a value dragged near one
 * is pulled onto it, and a tap on one goes there. On a notch, the thumb shows a blue bar.
 */
@Composable
fun BrushSlider(value: Float, onChange: (Float) -> Unit, modifier: Modifier = Modifier, panel: SliderPanelHooks? = null, notches: List<Float> = emptyList(), height: Dp = Pc.SliderHeight) {
    val current by rememberUpdatedState(value)
    val change by rememberUpdatedState(onChange)
    val hooks by rememberUpdatedState(panel)
    val marks by rememberUpdatedState(notches)
    fun near(v: Float, reach: Float = CanvasViewModel.NOTCH_NEAR) =
        marks.filter { kotlin.math.abs(it - v) < reach }.minByOrNull { kotlin.math.abs(it - v) }
    Box(
        modifier.size(Pc.SliderWidth, height)
            .pointerInput(height) {
                val travel = (height - Pc.ThumbHeight - Pc.ThumbInset * 2).toPx()
                awaitEachGesture {
                    val down = awaitFirstDown()
                    down.consume()
                    var last = down.position.y
                    var moved = false
                    var lifted = false
                    // the system took the touch back (the pen coming near takes the screen from a
                    // finger): it wasn't lifted, so the panel stays up for the pen to tap its +
                    var taken = false
                    // where the finger has taken it, before a notch pulls it in
                    var raw = current
                    // follows the finger; false once it is lifted. Another pointer's event (the
                    // pen hovering in) isn't this touch's.
                    suspend fun AwaitPointerEventScope.follow(): Boolean {
                        val c = awaitPointerEvent().changes.firstOrNull { it.id == down.id } ?: return true
                        if (!c.pressed) {
                            // a cancelled touch ends already consumed; a lift doesn't
                            taken = c.isConsumed
                            return false
                        }
                        val dy = c.position.y - last
                        if (dy != 0f) {
                            if (!moved) hooks?.show()
                            moved = true
                            last = c.position.y
                            // the farther out to the side the finger is, the finer the thumb follows
                            val across = (c.position.x - size.width / 2f).toDp().value
                            raw = (raw - dy / travel * sideSliderDragShare(across)).coerceIn(0f, 1f)
                            change(near(raw) ?: raw)
                        }
                        c.consume()
                        return true
                    }
                    withTimeoutOrNull(viewConfiguration.longPressTimeoutMillis) {
                        while (!lifted && !moved) lifted = !follow()
                    }
                    if (!lifted && !moved) hooks?.show()
                    // a tap on the thumb brings up the panel (or takes it away); elsewhere the
                    // thumb goes there (or onto a notch's mark there) and the panel goes
                    if (lifted && !moved && !taken) {
                        val inset = Pc.ThumbInset.toPx()
                        val at = 1f - (down.position.y - inset - Pc.ThumbHeight.toPx() / 2f) / travel
                        val up = hooks?.up?.invoke() == true
                        when (val tap = sideSliderTap(at, current, Pc.ThumbHeight.toPx() / 2f / travel, marks, up)) {
                            is SideSliderTap.Panel -> hooks?.show()
                            is SideSliderTap.ClosePanel -> hooks?.hide()
                            is SideSliderTap.Jump -> {
                                change(tap.value)
                                hooks?.hide()
                            }
                        }
                    }
                    while (!lifted) lifted = !follow()
                    // let go, the panel stays up for its + (and so does it for a touch the pen took)
                    if (taken) hooks?.show()
                }
            }
            .drawBehind {
                val inset = Pc.ThumbInset.toPx()
                val thumb = Size(Pc.ThumbWidth.toPx(), Pc.ThumbHeight.toPx())
                val at = Offset((size.width - thumb.width) / 2f, inset + (size.height - thumb.height - inset * 2) * (1f - value))
                val knob = RoundRect(at.x, at.y, at.x + thumb.width, at.y + thumb.height, CornerRadius(thumb.height / 2f))
                // the track has a window where the thumb is, so the thumb shows over the bar alone;
                // a lighter rim runs round it
                val corner = Pc.SliderCorner.toPx()
                val rim = Pc.SliderRimWidth.toPx()
                val inside = RoundRect(rim, rim, size.width - rim, size.height - rim, CornerRadius(corner - rim))
                val track = Path().apply {
                    fillType = PathFillType.EvenOdd
                    addRoundRect(inside)
                    addRoundRect(knob)
                }
                drawPath(track, Pc.SliderTrack)
                val ring = Path().apply {
                    fillType = PathFillType.EvenOdd
                    addRoundRect(RoundRect(0f, 0f, size.width, size.height, CornerRadius(corner)))
                    addRoundRect(inside)
                }
                drawPath(ring, Pc.SliderRim)
                // the notches: short bars across the track; on one, the thumb shows a blue bar
                val bar = Size(NotchWidth.toPx(), NotchHeight.toPx())
                val travel = size.height - thumb.height - inset * 2
                for (n in notches) {
                    val y = inset + thumb.height / 2f + travel * (1f - n)
                    drawRoundRect(Pc.NotchMark, Offset((size.width - bar.width) / 2f, y - bar.height / 2f), bar, CornerRadius(bar.height / 2f))
                }
                // the thumb's soft shadow, a point below it: under the thumb, only its rim shows
                for (step in ThumbShadow) {
                    val grow = step.grow.dp.toPx()
                    drawRoundRect(
                        Color.Black.copy(alpha = step.alpha),
                        Offset(at.x - grow, at.y - grow + step.down.dp.toPx()),
                        Size(thumb.width + grow * 2f, thumb.height + grow * 2f),
                        CornerRadius(thumb.height / 2f + grow),
                    )
                }
                drawPath(Path().apply { addRoundRect(knob) }, Pc.SliderThumb)
                if (notches.any { kotlin.math.abs(it - value) < 1e-4f }) {
                    drawRoundRect(Pc.Accent, Offset((size.width - bar.width) / 2f, at.y + (thumb.height - bar.height) / 2f), bar, CornerRadius(bar.height / 2f))
                }
            },
    )
}

/** Liquify's ring: its reach as a thin dark circle with a light edge, a fine cross in its middle. */
@Composable
private fun LiquifyRing(vm: CanvasViewModel) {
    val at = vm.liquifyRingAt ?: return
    val r = vm.liquifyRingRadius
    androidx.compose.foundation.Canvas(Modifier.fillMaxSize()) {
        for ((width, color) in listOf(3.dp.toPx() to Color.White, 1.dp.toPx() to Color.Black)) {
            drawCircle(color, r, at, style = Stroke(width))
            val arm = 5.dp.toPx()
            drawLine(color, at - Offset(arm, 0f), at + Offset(arm, 0f), width)
            drawLine(color, at - Offset(0f, arm), at + Offset(0f, arm), width)
        }
    }
}

private val NotchWidth = 20.dp
private val NotchHeight = 4.dp

/** How a side slider's thumb shadow is drawn: worked out once, the same on either look. */
private val ThumbShadow: List<ShadowStep> by lazy { sideThumbShadow() }

/**
 * The brush's outline where the pen paints or hovers, in the brush's own shape (the round tip's a
 * circle): a thin dark line with a light edge, or in the colour painted with. Hovering, the brush's
 * stamp fills it in the colour, as strong as the brush paints, if its Hover fill says so.
 */
@Composable
fun BrushCursor(vm: CanvasViewModel) {
    LiquifyRing(vm)
    val at = vm.cursorAt ?: return
    val r = vm.cursorRadius
    if (r < 2f) return
    val style = vm.cursorStyle
    val paint = CanvasSpace.color(vm.colorRgb())
    val shape = vm.cursorShape
    val turn = vm.cursorTurn
    val fill = vm.cursorFill.takeIf { vm.cursorHovering }
    // as the brush would paint at its estimated pressure: as strong, and as big
    val look = vm.cursorFillLook
    val strength = hoverFillStrength(vm.opacity, look.opacity)
    val fillR = r * look.size.coerceIn(0f, 1f)
    androidx.compose.foundation.Canvas(Modifier.fillMaxSize()) {
        // the stamp under the outline, turned as the stamps are
        if (fill != null && fillR >= 1f) {
            rotate(Math.toDegrees(turn.toDouble()).toFloat(), pivot = at) {
                drawImage(
                    fill,
                    dstOffset = androidx.compose.ui.unit.IntOffset((at.x - fillR).roundToInt(), (at.y - fillR).roundToInt()),
                    dstSize = androidx.compose.ui.unit.IntSize((2f * fillR).roundToInt(), (2f * fillR).roundToInt()),
                    alpha = strength,
                    colorFilter = androidx.compose.ui.graphics.ColorFilter.tint(paint),
                )
            }
        }
        // its loops across the brush's diameter, about where it is, turned as the stamps are
        val (sin, cos) = kotlin.math.sin(turn) to kotlin.math.cos(turn)
        val paths = shape.map { ring ->
            Path().apply {
                for (i in 0 until ring.size / 2) {
                    val (dx, dy) = ring[i * 2] to ring[i * 2 + 1]
                    val x = at.x + (dx * cos - dy * sin) * r * 2f
                    val y = at.y + (dx * sin + dy * cos) * r * 2f
                    if (i == 0) moveTo(x, y) else lineTo(x, y)
                }
                close()
            }
        }
        fun outline(color: Color, width: Float) {
            if (paths.isEmpty()) drawCircle(color, r, at, style = Stroke(width))
            for (p in paths) drawPath(p, color, style = Stroke(width))
        }
        // (a fill at the faintest opacities is as good as none: the lines show with it)
        when (cursorLines(style, vm.cursorOutline, filled = fill != null && strength >= CURSOR_FILL_SEEN)) {
            CursorLines.NONE -> Unit
            CursorLines.CONTRAST -> { outline(Color.White, 3.dp.toPx()); outline(Color.Black, 1.dp.toPx()) }
            CursorLines.COLOUR -> { outline(Color(0x99FFFFFF), 3.dp.toPx()); outline(paint, 1.5.dp.toPx()) }
        }
    }
}

/** A hover fill drawn at least this strong (the brush's opacity) shows by itself. */
private const val CURSOR_FILL_SEEN = 0.1f

/** How strongly the hover fill shows: the opacity painted with ([opacity]), times what the brush's estimated hover pressure leaves of it ([atPressure]). */
internal fun hoverFillStrength(opacity: Float, atPressure: Float): Float = opacity.coerceIn(0f, 1f) * atPressure.coerceIn(0f, 1f)

/** How the brush cursor's outline is drawn: not at all, a thin dark line with a light edge, or in the colour. */
internal enum class CursorLines { NONE, CONTRAST, COLOUR }

/**
 * The brush cursor's outline in [style], for a brush whose own outline is [brushOutline] (0 none,
 * 1 contrasting lines, 2 the colour), its hover fill shown under it if [filled]. Per brush, one
 * whose own is none shows its fill alone where it has one (hovering); painting, or with no fill,
 * it still shows contrasting lines, so a brush cursor turned on never goes missing.
 */
internal fun cursorLines(style: CanvasViewModel.CursorStyle, brushOutline: Int, filled: Boolean): CursorLines = when (style) {
    CanvasViewModel.CursorStyle.CONTRAST -> CursorLines.CONTRAST
    CanvasViewModel.CursorStyle.COLOR -> CursorLines.COLOUR
    CanvasViewModel.CursorStyle.BRUSH -> when (brushOutline) {
        0 -> if (filled) CursorLines.NONE else CursorLines.CONTRAST
        2 -> CursorLines.COLOUR
        else -> CursorLines.CONTRAST
    }
}
