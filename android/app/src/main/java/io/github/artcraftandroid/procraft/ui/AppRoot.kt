package io.github.artcraftandroid.procraft.ui

import androidx.compose.foundation.draganddrop.dragAndDropTarget
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.input.pointer.changedToDownIgnoreConsumed
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.zIndex
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.layout.layout
import androidx.compose.ui.viewinterop.AndroidView
import io.github.artcraftandroid.procraft.core.Adjustment
import io.github.artcraftandroid.procraft.canvas.CanvasSurfaceView
import io.github.artcraftandroid.procraft.canvas.CanvasViewModel
import io.github.artcraftandroid.procraft.canvas.Panel
import io.github.artcraftandroid.procraft.canvas.Screen
import io.github.artcraftandroid.procraft.canvas.SelectionPopup
import io.github.artcraftandroid.procraft.canvas.TransformPopup
import io.github.artcraftandroid.procraft.core.LayerInfo
import io.github.artcraftandroid.procraft.core.ToolKind
import kotlin.math.roundToInt
import kotlinx.coroutines.launch

/** The canvas screen: the canvas, the bars over it, and the panel that is open. */
@Composable
fun AppRoot(vm: CanvasViewModel) {
    // inside the status bar and the camera's cutout, while a plugin asks
    val keptClear = clearance(vm.plugins, vm.pluginList)
    val interfaceSize = fittedInterfaceSize(vm.plugins, vm.pluginList, keptClear)
    androidx.compose.runtime.SideEffect { vm.interfaceShownAt(interfaceSize) }
    ProCraftTheme(interfaceSize, keptClear) {
        // layers lifted out of the panel stay held across the gallery and into another artwork
        HoldingLayers(vm) { Screens(vm, keptClear) }
    }
}

/** The gallery, or the open artwork's canvas with the bars and panels over it. */
@Composable
private fun Screens(vm: CanvasViewModel, keptClear: Clearance) {
    // (while a plugin lays the panels out, windows lie over the bars and each other: the panels nearly solid)
    androidx.compose.runtime.CompositionLocalProvider(LocalSolidPanels provides vm.layoutLook.on) { ScreensShown(vm, keptClear) }
}

@Composable
private fun ScreensShown(vm: CanvasViewModel, keptClear: Clearance) {
    if (vm.screen == Screen.GALLERY) {
        ZoomingGallery(vm, overlay = false)
        // a share from the gallery shows its card there, and a file brought in its own
        ExportProgress(vm.exportShown, vm::cancelExport)
        ImportProgress(vm.importShown, vm::cancelImport)
        PluginOfferAlert(vm)
        WrongBuildAlert(vm)
        return
    }
    // back closes an open panel first, then leaves the artwork for the gallery
    BackHandler {
        when {
            vm.guideEditing -> vm.closeGuideEditor(keep = false)
            vm.cropRect != null -> vm.endCrop(false)
            vm.lightingStudioOpen -> vm.closeLightingStudio(keep = false)
            vm.studioOpen -> vm.closeStudio(keep = false)
            vm.transformPopup != TransformPopup.NONE -> vm.transformPopup = TransformPopup.NONE
            vm.selectionPopup != SelectionPopup.NONE -> vm.selectionPopup = SelectionPopup.NONE
            vm.selecting -> vm.closeSelecting()
            vm.adjustment != null -> vm.endAdjustment(true)
            vm.transforming -> vm.endTransform(true)
            vm.panel != Panel.NONE -> vm.closePanels()
            else -> vm.closeArtwork()
        }
    }
    // panels over the canvas are frosted (where it is on the screen: the corner of the window's
    // content, this view's, and in from it by what the interface keeps clear of; see canvasCorner)
    val content = androidx.compose.ui.platform.LocalView.current
    androidx.compose.runtime.DisposableEffect(content, keptClear) {
        vm.backdrops.canvasOnScreen = {
            val at = IntArray(2).also(content::getLocationOnScreen)
            canvasCorner(androidx.compose.ui.geometry.Offset(at[0].toFloat(), at[1].toFloat()), keptClear)
        }
        onDispose { vm.backdrops.canvasOnScreen = { androidx.compose.ui.geometry.Offset.Zero } }
    }
    CompositionLocalProvider(LocalBackdrops provides vm.backdrops) {
        // files dragged in from another app: pictures as layers, brushes into the library
        val activity = androidx.compose.ui.platform.LocalContext.current.activity()
        val dropped = androidx.compose.runtime.remember(vm) {
            object : androidx.compose.ui.draganddrop.DragAndDropTarget {
                override fun onDrop(event: androidx.compose.ui.draganddrop.DragAndDropEvent): Boolean {
                    val uris = event.droppedFiles(activity)
                    vm.dropOnCanvas(uris)
                    return uris.isNotEmpty()
                }
            }
        }
        // where this box is on the window, for touches on it to be told from the side bar's
        val boxAt = androidx.compose.runtime.remember { floatArrayOf(0f, 0f) }
        BoxWithConstraints(
            Modifier.fillMaxSize().background(Pc.Surround).dragAndDropTarget(
                shouldStartDragAndDrop = { it.fromElsewhere() },
                target = dropped,
            )
                .onGloballyPositioned { it.positionInWindow().let { at -> boxAt[0] = at.x; boxAt[1] = at.y } }
                // a slider's panel stays up after its slider is let go, and goes with the next
                // touch anywhere but on it and the side bar; that touch goes on to do what it
                // does there (a stroke on the canvas is drawn), so it is only seen here
                .pointerInput(vm) {
                    awaitPointerEventScope {
                        while (true) {
                            val event = awaitPointerEvent(androidx.compose.ui.input.pointer.PointerEventPass.Initial)
                            if (vm.sliderPanel == null) continue
                            val off = event.changes.any { c ->
                                c.changedToDownIgnoreConsumed() &&
                                    !vm.keepsSliderPanel(androidx.compose.ui.geometry.Offset(boxAt[0] + c.position.x, boxAt[1] + c.position.y))
                            }
                            if (off) vm.sliderPanel = null
                        }
                    }
                },
        ) {
            AndroidView(factory = { CanvasSurfaceView(it, vm) }, modifier = Modifier.fillMaxSize())
            // the drawing guide is the canvas's: under every bar and panel
            GuideOverlay(vm)
            if (vm.transforming) TransformOverlay(vm)
            if (vm.selecting) SelectionOverlay(vm, Pc.barShown)
            vm.picking?.let { pick ->
                val pickingZ = if (pick.fromReference) 1f else 0f
                Loupe(pick, CanvasSpace.color(vm.colorRgb()), Modifier.zIndex(pickingZ))
                if (!vm.barsHidden) PickingBar(Modifier.offset(y = Pc.barShown).zIndex(pickingZ), pick.fromReference)
            }
            // the bars fade out as the interface hides, and back
            val barsShown = androidx.compose.animation.core.animateFloatAsState(if (vm.barsHidden) 0f else 1f, Motion.bars(), label = "bars").value
            // with the interface hidden, a button in the top left brings it back
            if (barsShown < 1f) {
                Box(
                    Modifier.offset(x = 20.dp).size(44.dp).graphicsLayer { alpha = 1f - barsShown }
                        .pointerInput(Unit) { detectTapGestures { if (vm.barsHidden) vm.toggleBars() } },
                    contentAlignment = Alignment.Center,
                ) {
                    // its glyph is drawn wider than it is tall: 50 units shown 36.7 by 32
                    androidx.compose.foundation.Image(
                        androidx.compose.ui.graphics.vector.rememberVectorPainter(PcIcons.FullScreen),
                        contentDescription = "Show the interface",
                        contentScale = androidx.compose.ui.layout.ContentScale.FillBounds,
                        colorFilter = androidx.compose.ui.graphics.ColorFilter.tint(Pc.SideGlyph),
                        modifier = Modifier.offset(y = 0.3.dp).requiredSize(36.7.dp, 32.dp),
                    )
                }
            }
            if (barsShown > 0f) {
                // the top bar slides up off the screen and the side bar out to its side as they fade
                val fade = Modifier.graphicsLayer { alpha = barsShown }
                val up = Modifier.graphicsLayer { translationY = -size.height * (1f - barsShown) }
                val aside = Modifier.graphicsLayer { translationX = size.width * (1f - barsShown) * if (vm.rightHanded) 1f else -1f }
                // the top bar and the side bar each at the size a plugin may give them
                // the top bar's halves, where they always are, told to Edit layout to move them from
                val barK = Pc.partScale(io.github.artcraftandroid.procraft.core.InterfacePart.TOP_BAR)
                val barWidth = maxWidth / barK
                val halfEnd = (Pc.barFit(barWidth).margin - TopBarHalfEnd) * barK
                val leftLength = topBarHalfLength(true, barWidth) * barK
                val rightLength = topBarHalfLength(false, barWidth) * barK
                val thick = Pc.BarHeight * barK
                androidx.compose.runtime.SideEffect {
                    vm.tellBar(maxWidth.value, maxHeight.value, io.github.artcraftandroid.procraft.core.LayoutBar.TOP_LEFT, io.github.artcraftandroid.procraft.core.BarPlace(halfEnd.value, 0f, false), leftLength.value, thick.value)
                    vm.tellBar(maxWidth.value, maxHeight.value, io.github.artcraftandroid.procraft.core.LayoutBar.TOP_RIGHT, io.github.artcraftandroid.procraft.core.BarPlace((maxWidth - halfEnd - rightLength).value, 0f, false), rightLength.value, thick.value)
                }
                // (each half on glass of its own while moved, or sliding into its side or out: Collapse to edge)
                val halfSlides = listOf(io.github.artcraftandroid.procraft.core.LayoutBar.TOP_LEFT, io.github.artcraftandroid.procraft.core.LayoutBar.TOP_RIGHT).map { collapseSlide(vm.barCollapsed(it) != null) }
                val halvesMoved = vm.barMoved(io.github.artcraftandroid.procraft.core.LayoutBar.TOP_LEFT) != null || vm.barMoved(io.github.artcraftandroid.procraft.core.LayoutBar.TOP_RIGHT) != null ||
                    halfSlides.any { it > 0f }
                val rootDensity = androidx.compose.ui.platform.LocalDensity.current.density
                PartScaled(io.github.artcraftandroid.procraft.core.InterfacePart.TOP_BAR) {
                    if (!halvesMoved) {
                        val height = with(androidx.compose.ui.platform.LocalDensity.current) { Pc.BarHeight.toPx() }
                        val topGlass = BarGlass(barsShown, androidx.compose.ui.geometry.Offset(0f, -height * (1f - barsShown)))
                        TopBar(vm, Modifier.align(Alignment.TopCenter).then(rememberFrost(0.dp, topGlass)).then(up).then(fade))
                    } else {
                        // moved by Edit layout: each half on glass of its own where it was left
                        for ((bar, left) in listOf(io.github.artcraftandroid.procraft.core.LayoutBar.TOP_LEFT to true, io.github.artcraftandroid.procraft.core.LayoutBar.TOP_RIGHT to false)) {
                            val at = vm.barShown(bar) ?: continue
                            // gone into its side: its tab shows there instead
                            val slide = halfSlides[if (left) 0 else 1]
                            if (slide >= 1f && vm.barCollapsed(bar) != null) continue
                            androidx.compose.runtime.key(bar) {
                                // square only on its sides against the window's edges
                                val edges = vm.barTouching(bar)
                                TopBarHalf(
                                    vm,
                                    left,
                                    at.vertical,
                                    barWidth,
                                    Modifier.offset { androidx.compose.ui.unit.IntOffset((at.x * rootDensity).roundToInt(), (at.y * rootDensity).roundToInt()) }
                                        .collapseSlid(vm.barCollapsePlace(bar), slide, rootDensity)
                                        .then(rememberFrost(Pc.BarHeight / 2, pastEdges(edges, Pc.BarHeight / 2, BarGlass()))).then(fade),
                                    edges,
                                )
                            }
                        }
                    }
                }
                val side = Pc.partScale(io.github.artcraftandroid.procraft.core.InterfacePart.SIDE_BAR)
                // the window as the side bar measures it, at its kept place (its modify button
                // dragged, it follows the finger up or down stretching a little toward it, and let
                // go settles back to its width)
                vm.sideBarRoom = floatArrayOf(maxHeight.value / side, Pc.barShown.value / side)
                val sideBar = io.github.artcraftandroid.procraft.core.sideBarFit(maxHeight.value / side, Pc.barShown.value / side, vm.sideBarCentre)
                val sideTop = androidx.compose.runtime.remember { androidx.compose.animation.core.Animatable(sideBar.top) }
                val sideStretch = androidx.compose.runtime.remember { androidx.compose.animation.core.Animatable(0f) }
                val moving = vm.sideBarMoving
                androidx.compose.runtime.LaunchedEffect(moving, vm.sideBarSettling, sideBar.top) {
                    when {
                        moving != null -> {
                            sideTop.snapTo(moving.top)
                            sideStretch.snapTo(moving.stretch)
                        }
                        vm.sideBarSettling -> {
                            kotlinx.coroutines.coroutineScope {
                                launch { sideStretch.animateTo(0f, Motion.sideBarSettle()) }
                                sideTop.animateTo(sideBar.top, Motion.sideBarSettle())
                            }
                            vm.sideBarSettling = false
                        }
                        else -> {
                            sideTop.snapTo(sideBar.top)
                            sideStretch.snapTo(0f)
                        }
                    }
                }
                // a mode taking the screen slides it off its edge, and back when it ends
                val sideMoved = vm.barMoved(io.github.artcraftandroid.procraft.core.LayoutBar.SIDE_BAR)
                // collapsed into its side (Collapse to edge): sliding there, then gone, its tab there instead
                val sideCollapsed = vm.barCollapsed(io.github.artcraftandroid.procraft.core.LayoutBar.SIDE_BAR) != null
                val sideSlide = collapseSlide(sideCollapsed)
                val sideGone = sideCollapsed && sideSlide >= 1f
                // where it always is, told to Edit layout to move it from
                vm.sideBarBounds?.takeIf { sideMoved == null }?.let { b ->
                    val d = androidx.compose.ui.platform.LocalDensity.current.density
                    androidx.compose.runtime.SideEffect {
                        vm.tellBar(maxWidth.value, maxHeight.value, io.github.artcraftandroid.procraft.core.LayoutBar.SIDE_BAR, io.github.artcraftandroid.procraft.core.BarPlace(b.left / d, b.top / d, true), b.height / d, b.width / d)
                    }
                }
                androidx.compose.animation.AnimatedVisibility(
                    vm.showSideBar && !sideGone && !vm.transforming && !vm.selecting && !vm.guideEditing && !vm.lightingStudioOpen && vm.cropRect == null && vm.textInfo == null &&
                        vm.opacityLayer == null &&
                        (vm.adjustment == null || vm.adjustPencil),
                    if (sideMoved != null) {
                        // moved by Edit layout: where it was left
                        Modifier.align(Alignment.TopStart).offset {
                            androidx.compose.ui.unit.IntOffset((sideMoved.x * density).roundToInt(), (sideMoved.y * density).roundToInt())
                        }
                    } else {
                        Modifier.align(if (vm.rightHanded) Alignment.TopEnd else Alignment.TopStart).offset {
                            val k = side * density
                            val outward = if (vm.rightHanded) -1f else 1f
                            // (stretched, what is on it moves out with its far side; its glass reaches back to the edge)
                            androidx.compose.ui.unit.IntOffset((sideStretch.value * k * outward).roundToInt(), (sideTop.value * k).roundToInt())
                        }
                    },
                    enter = Motion.sideBarIn(vm.rightHanded),
                    exit = Motion.sideBarOut(vm.rightHanded),
                ) {
                    // where it is, for a dragged colour over it
                    DisposableEffect(Unit) { onDispose { vm.sideBarBounds = null } }
                    PartScaled(io.github.artcraftandroid.procraft.core.InterfacePart.SIDE_BAR) {
                        // lying down where Edit layout moved it: turned a quarter about its top-left, then down by its width
                        val lying = if (sideMoved?.vertical == false) {
                            Modifier.graphicsLayer {
                                transformOrigin = androidx.compose.ui.graphics.TransformOrigin(0f, 0f)
                                rotationZ = -90f
                                translationY = Pc.SideBarWidth.toPx()
                            }
                        } else {
                            Modifier
                        }
                        SideBar(
                            vm,
                            // (a press on its glass never the canvas's, wherever it is and however it is turned)
                            Modifier.then(aside).then(fade).keepsItsTouches()
                                .onGloballyPositioned {
                                    val b = it.boundsInRoot().translate(-keptClear.corner)
                                    vm.sideBarBounds = b
                                    // back where it always is (Reset): its handle follows as it lands there
                                    if (sideMoved == null && sideSlide == 0f && barsShown >= 1f && sideStretch.value == 0f) {
                                        val d = rootDensity
                                        vm.tellBar(maxWidth.value, maxHeight.value, io.github.artcraftandroid.procraft.core.LayoutBar.SIDE_BAR, io.github.artcraftandroid.procraft.core.BarPlace(b.left / d, b.top / d, true), b.height / d, b.width / d)
                                    }
                                },
                            // (sliding into its side, or lying down: its glass with it, where it is drawn, not standing
                            // where it is laid out with its foot off the screen's bottom)
                            turn = Modifier.collapseSlid(vm.barCollapsePlace(io.github.artcraftandroid.procraft.core.LayoutBar.SIDE_BAR), sideSlide, rootDensity)
                                .then(lying),
                            // standing against a side Edit layout took it to: mirrored to that side
                            right = sideMoved?.takeIf { it.vertical }?.let { vm.barTouching(io.github.artcraftandroid.procraft.core.LayoutBar.SIDE_BAR)?.right } ?: vm.rightHanded,
                            trackHeight = sideBar.track.dp,
                            shown = barsShown,
                            stretch = sideStretch.value.dp,
                            // moved: square only on its sides against the window's edges (its own, lying down)
                            edges = sideMoved?.let { at ->
                                vm.barTouching(io.github.artcraftandroid.procraft.core.LayoutBar.SIDE_BAR)?.let { if (at.vertical) it else turnedSides(it) }
                            },
                            glassEdges = sideMoved?.let { vm.barTouching(io.github.artcraftandroid.procraft.core.LayoutBar.SIDE_BAR) },
                        )
                    }
                }
            }
            NoticeBand(vm)
            DropBandUi(vm)
            // a full-screen page covers the timeline, as it covers the canvas; the panels open over it
            val pageOver = vm.canvasInfoOpen || vm.gestureControlsOpen || vm.cropRect != null || vm.timeLapseReplaying ||
                vm.animatedExport != null || vm.modelAnimatedExport != null
            if (!vm.barsHidden && !pageOver) AnimationBar(vm)
            // the bottom toolbars rise from below the screen, and drop away
            FloatableBottom(
                vm,
                io.github.artcraftandroid.procraft.core.LayoutPiece.TRANSFORM,
                vm.transforming && !vm.barsHidden,
                goesWith = { if (vm.transforming && !vm.barsHidden) TransformPopover(vm, maxWidth, maxHeight) },
            ) { TransformToolbar(vm, maxWidth) }
            BrushCursor(vm)
            // the Reference window floats over the bars and the canvas's own marks; the panels,
            // menus and pages opened later come over it
            ReferenceWindow(vm)
            // the colour panel dragged out by its grabber: a window over the canvas until closed,
            // under the panels, which open over it
            FloatingColors(vm, maxWidth, maxHeight)
            // the Layers panel and the brush library torn off into windows of their own, likewise
            FloatingPanels(vm, maxWidth, maxHeight)
            // Collapse to edge's buttons by the bars and the tabs of what went into a side
            if (!vm.barsHidden) CollapseControls(vm)
            // Edit layout: a handle on each bar, Reset and Done
            if (vm.editingLayout && !vm.barsHidden) EditLayoutHandles(vm)
            // each panel hangs from its button: colour, layers, then the tool that opened the library;
            // it grows out of its arrow as it opens and fades as it closes
            PanelPresence(vm.panel, Panel.NONE, fading = vm.panelFading, onFaded = { vm.panelFading = false }) { shown, alpha, scale, origin ->
                val (width, button) = when (shown) {
                    Panel.LAYERS -> (if (vm.backgroundPicking) 376.dp else 384.dp) to 1
                    Panel.COLOR -> 376.dp to 0
                    Panel.BRUSHES -> 440.dp to when (vm.tool) {
                        ToolKind.ERASE -> 2
                        ToolKind.SMUDGE -> 3
                        else -> 4
                    }
                    Panel.ACTIONS -> 390.dp to 0
                    Panel.ADJUSTMENTS -> 330.dp to 1
                    Panel.GENERATE -> 400.dp to 4
                    Panel.NONE -> 0.dp to 0
                }
                val fromLeft = shown == Panel.ACTIONS || shown == Panel.ADJUSTMENTS || shown == Panel.GENERATE
                // its button where the top bar shows it, at the bar's size
                val bar = Pc.partScale(io.github.artcraftandroid.procraft.core.InterfacePart.TOP_BAR)
                val buttonX = if (fromLeft) Pc.actionCentre(button, maxWidth / bar) * bar else maxWidth - Pc.toolCentreFromRight(button, maxWidth / bar) * bar
                // panels keep clear of the side bar, on whichever side it is
                val clear = if (vm.showSideBar) Pc.sideBarShown + Pc.PopoverGap else Pc.PopoverGap
                // placed at the size it shows (the panels' size), and laid out at its own
                val k = Pc.partScale(io.github.artcraftandroid.procraft.core.InterfacePart.PANELS)
                val place = if (vm.rightHanded) popoverPlace(width * k, buttonX, maxWidth, minRight = clear) else popoverPlace(width * k, buttonX, maxWidth, clear)
                // from where Edit layout moved its half of the top bar, if it did: under the button,
                // over it, or beside it (the core works out where)
                val half = if (fromLeft) io.github.artcraftandroid.procraft.core.LayoutBar.TOP_LEFT else io.github.artcraftandroid.procraft.core.LayoutBar.TOP_RIGHT
                val hang = vm.barMoved(half)?.let {
                    val barWidth = maxWidth / bar
                    val fit = Pc.barFit(barWidth)
                    val along = if (fromLeft) {
                        TopBarHalfEnd + Pc.GalleryWidth + fit.actionGap + fit.button / 2 + (fit.button + fit.actionGap) * button
                    } else {
                        topBarHalfLength(false, barWidth) - TopBarHalfEnd - fit.button / 2 - (fit.button + fit.toolGap) * button
                    }
                    vm.barLayout.hang(half, (along * bar).value, (width * k).value, Pc.ArrowShift.value, Pc.PopoverGap.value, (Pc.ArrowHeight * k).value)
                }
                val hangSide = when (hang?.side) {
                    null, io.github.artcraftandroid.procraft.core.HangSide.BELOW -> ArrowSide.TOP
                    io.github.artcraftandroid.procraft.core.HangSide.ABOVE -> ArrowSide.BOTTOM
                    io.github.artcraftandroid.procraft.core.HangSide.RIGHT -> ArrowSide.LEFT
                    io.github.artcraftandroid.procraft.core.HangSide.LEFT -> ArrowSide.RIGHT
                }
                val panelLeft = hang?.x?.dp ?: place.left
                // its top; hanging over the bar, its bottom
                val panelTop = hang?.y?.dp ?: Pc.barShown
                // the arrow's tip along its edge, as it shows
                val arrowShown = hang?.arrow?.dp ?: place.arrowX
                val sideways = hangSide == ArrowSide.LEFT || hangSide == ArrowSide.RIGHT
                val density = androidx.compose.ui.platform.LocalDensity.current
                val pivot = with(density) {
                    val at = if (sideways) {
                        androidx.compose.ui.geometry.Offset((panelLeft + if (hangSide == ArrowSide.RIGHT) width * k else 0.dp).toPx(), (panelTop + arrowShown).toPx())
                    } else {
                        androidx.compose.ui.geometry.Offset((panelLeft + arrowShown).toPx(), panelTop.toPx())
                    }
                    origin + at
                }
                val left = with(density) { panelLeft.roundToPx() }
                val top = with(density) { panelTop.roundToPx() }
                // dragged out by its grabber, it is not seen (nor its blur) until let go, the window floating
                val undocking = when (shown) {
                    Panel.LAYERS -> vm.panelUndocking == io.github.artcraftandroid.procraft.core.FloatingPanel.LAYERS
                    Panel.BRUSHES -> vm.panelUndocking == io.github.artcraftandroid.procraft.core.FloatingPanel.BRUSHES
                    Panel.ACTIONS -> vm.pieceUndocking == io.github.artcraftandroid.procraft.core.LayoutPiece.ACTIONS
                    Panel.ADJUSTMENTS -> vm.pieceUndocking == io.github.artcraftandroid.procraft.core.LayoutPiece.ADJUSTMENTS
                    else -> false
                }
                val seen = if (undocking) 0f else alpha
                // (where it covers the canvas: the pen hovering over it shows no brush cursor)
                val cover = androidx.compose.runtime.remember { Any() }
                DisposableEffect(cover) { onDispose { vm.covers(cover, null) } }
                // hanging over the bar: placed by its bottom
                val above = if (hangSide == ArrowSide.BOTTOM) {
                    Modifier.layout { measurable, constraints ->
                        val p = measurable.measure(constraints)
                        layout(p.width, p.height) { p.place(0, -p.height) }
                    }
                } else {
                    Modifier
                }
                val growFrom = when (hangSide) {
                    ArrowSide.BOTTOM -> androidx.compose.ui.graphics.TransformOrigin(arrowShown / (width * k), 1f)
                    ArrowSide.LEFT -> androidx.compose.ui.graphics.TransformOrigin(0f, 0.2f)
                    ArrowSide.RIGHT -> androidx.compose.ui.graphics.TransformOrigin(1f, 0.2f)
                    else -> androidx.compose.ui.graphics.TransformOrigin(arrowShown / (width * k), 0f)
                }
                val at = Modifier.offset { androidx.compose.ui.unit.IntOffset(left, top) }.then(above).graphicsLayer {
                    this.alpha = seen
                    scaleX = scale
                    scaleY = scale
                    transformOrigin = growFrom
                }.then(if (vm.panel == Panel.NONE) Modifier.takingNoTouches() else Modifier)
                    .onGloballyPositioned { vm.covers(cover, it.boundsInRoot().translate(-keptClear.corner)) }
                // the screen as a panel at its size measures it: the room under the top bar
                // as it shows, in the panel's own dp (the panels take the bar's height off it)
                // (hanging from a moved button, the room the core leaves it)
                val room = (hang?.room?.dp ?: (maxHeight - Pc.barShown)) / k + Pc.BarHeight
                val arrowX = arrowShown / k
                PartScaled(io.github.artcraftandroid.procraft.core.InterfacePart.PANELS) {
                androidx.compose.runtime.CompositionLocalProvider(LocalHang provides hang?.let { Hang(hangSide, arrowX) }) {
                ProvidePanelLook(PanelLook(seen, scale, pivot)) {
                    when (shown) {
                        Panel.LAYERS -> if (vm.backgroundPicking) {
                            ColorPanel(vm, arrowX, room, at, background = true)
                            // closed any way at all, the colour is kept
                            androidx.compose.runtime.DisposableEffect(Unit) { onDispose { vm.endBackgroundPick() } }
                        } else {
                            LayersPanel(vm, arrowX, at.heightIn(max = room - Pc.BarHeight - Pc.PopoverGap))
                        }
                        Panel.COLOR -> ColorPanel(vm, arrowX, room, at)
                        Panel.BRUSHES -> BrushLibrary(vm, arrowX, at.height(room - Pc.BarHeight - 25.dp))
                        // ending 25 above the screen's edge, their rows scrolling (as measured)
                        Panel.ACTIONS -> ActionsPanel(vm, arrowX, at.heightIn(max = room - Pc.BarHeight - 25.dp))
                        Panel.ADJUSTMENTS -> AdjustmentsPanel(vm, arrowX, at.heightIn(max = room - Pc.BarHeight - 25.dp))
                        Panel.GENERATE -> io.github.artcraftandroid.procraft.generation.GenerationUi.Panel(vm, arrowX, at.heightIn(max = room - Pc.BarHeight - 25.dp))
                        Panel.NONE -> Unit
                    }
                }
                }
                }
                // the Actions panel and the Adjustments menu hanging: a grabber over them tears them off, while a plugin lets them float
                val tearing = when (shown) {
                    Panel.ACTIONS -> io.github.artcraftandroid.procraft.core.LayoutPiece.ACTIONS
                    Panel.ADJUSTMENTS -> io.github.artcraftandroid.procraft.core.LayoutPiece.ADJUSTMENTS
                    else -> null
                }
                if (tearing != null && vm.mayFloat(tearing) && hangSide == ArrowSide.TOP && vm.panel != Panel.NONE) {
                    val grabX = with(density) { (panelLeft + width * k / 2 - 50.dp).roundToPx() }
                    val grabY = with(density) { (panelTop + Pc.ArrowHeight * k).roundToPx() }
                    PieceGrabber(vm, tearing, Modifier.offset { androidx.compose.ui.unit.IntOffset(grabX, grabY) }, density.density)
                }
            }
            // image generation's results tray, out of the way of the modes' own bars
            if (!vm.transforming && !vm.selecting && vm.adjustment == null && !vm.barsHidden) {
                io.github.artcraftandroid.procraft.generation.GenerationUi.ResultsTray(vm, Modifier.align(Alignment.BottomCenter))
            }
            // (its popover goes with the Transform toolbar, floating or not)
            if (vm.transforming && !vm.barsHidden) {
                TransformNumeric(vm)
            }
            TouchCompanion(vm)
            CropOverlay(vm)
            if (vm.textInfo != null) TextEditor(vm)
            RasterizePrompt(vm)
            vm.animatedExport?.let { AnimatedExport(vm, it) }
            vm.modelAnimatedExport?.let { ModelAnimatedExport(vm, it) }
            // full-screen pages slide up over everything, and back down
            androidx.compose.animation.AnimatedVisibility(
                vm.canvasInfoOpen,
                enter = androidx.compose.animation.slideInVertically(Motion.studioIn()) { it },
                exit = androidx.compose.animation.slideOutVertically(Motion.studioOut()) { it },
            ) { Box(Modifier.fillMaxSize()) { CanvasInfo(vm) } }
            androidx.compose.animation.AnimatedVisibility(
                vm.gestureControlsOpen,
                enter = androidx.compose.animation.slideInVertically(Motion.studioIn()) { it },
                exit = androidx.compose.animation.slideOutVertically(Motion.studioOut()) { it },
            ) { Box(Modifier.fillMaxSize()) { GestureControls(vm) } }
            // a plugin's own panel, while its action keeps it open: a ring where the pen was, or a card
            val ring = vm.pluginPanel?.takeIf { it.ring }
            if (ring != null) {
                PluginRingView(vm, ring)
            } else if (vm.pluginPanel != null) {
                Box(Modifier.fillMaxSize()) {
                    // what its rows don't take stays on the panel, never reaching the canvas under it
                    // (being touched is enough for that). Only a press is taken, as the brush
                    // studio's: taking every move too undid the rows' taps and drags whenever a
                    // move came between press and lift, as it always does from a pen
                    val swallows = Modifier.pointerInput(Unit) { awaitEachGesture { awaitFirstDown().consume() } }
                    // floating, at the floating windows' size
                    val box = this
                    val k = Pc.partScale(io.github.artcraftandroid.procraft.core.InterfacePart.FLOATING)
                    PartScaled(io.github.artcraftandroid.procraft.core.InterfacePart.FLOATING) {
                        with(box) { PluginPanelView(vm, Modifier.align(Alignment.TopEnd).padding(top = (Pc.barShown + 12.dp) / k, end = 12.dp / k).then(swallows)) }
                    }
                }
            }
            ModelPicture(vm)
            BlockedDialog(vm)
            ModelWarningsAlert(vm)
            ImportTroubleAlert(vm)
            SharedLayerAlert(vm)
            PluginOfferAlert(vm)
            LeaveUnsavedAlert(vm)
            // a fill a plugin is finishing, or a dropped colour's being made: nothing is touched until
            // it is done (the screen goes on drawing)
            if (vm.fillFinishing || vm.dropFilling) {
                Box(
                    Modifier.fillMaxSize().pointerInput(Unit) {
                        awaitPointerEventScope { while (true) awaitPointerEvent().changes.forEach { it.consume() } }
                    },
                )
            }
            ExportProgress(vm.exportShown, vm::cancelExport)
            ImportProgress(vm.importShown, vm::cancelImport)
            // a passing message goes in the band under the top bar, as what undo and redo did
            vm.message?.let { text ->
                androidx.compose.runtime.LaunchedEffect(text) {
                    vm.showNotice(text)
                    vm.message = null
                }
            }
            androidx.compose.animation.AnimatedVisibility(
                vm.timeLapseReplaying,
                Modifier.matchParentSize(),
                enter = androidx.compose.animation.fadeIn(Motion.replayFade()),
                exit = androidx.compose.animation.fadeOut(Motion.replayFade()),
            ) { TimeLapseReplay(vm) }
            // the gallery zooming into the artwork as it opens, over everything
            ZoomingGallery(vm, overlay = true)
            if (vm.adjustment == Adjustment.CLONE) ClonePuck(vm)
            if (vm.adjustment == Adjustment.PERSPECTIVE_BLUR) PerspectivePuck(vm)
            if (vm.recoloring) RecolorReticle(vm)
            QuickShapeHandles(vm)
            CopyPastePanel(vm)
            QuickMenu(vm)
            LayerChoice(vm)
            if (!vm.barsHidden) QuickShapeBar(vm, Modifier.offset(y = Pc.barShown))
            if (vm.opacityLayer != null && !vm.barsHidden) LayerOpacityBar(vm, Modifier.offset(y = Pc.barShown))
            if (vm.adjustment != null && !vm.barsHidden) {
                AdjustmentBar(vm, Modifier.offset(y = Pc.barShown))
                AdjustModes(vm, Modifier.align(Alignment.TopCenter).offset(y = Pc.barShown + 52.dp))
                AdjustmentMenu(vm, maxWidth, maxHeight)
            }
            // the adjustment's own panel rises from below the screen, and drops away (Liquify's is a piece of its own)
            val bottomPanel = vm.adjustment?.takeIf { !vm.barsHidden && it in BottomPanels }
            val shownPanel = androidx.compose.runtime.remember { arrayOfNulls<Adjustment>(1) }
            if (bottomPanel != null) shownPanel[0] = bottomPanel
            val bottomPiece = if (shownPanel[0] == Adjustment.LIQUIFY) io.github.artcraftandroid.procraft.core.LayoutPiece.LIQUIFY else io.github.artcraftandroid.procraft.core.LayoutPiece.ADJUSTING
            FloatableBottom(vm, bottomPiece, bottomPanel != null) {
                shownPanel[0]?.let {
                    when (it) {
                        Adjustment.HSB -> HsbPanel(vm, Modifier)
                        Adjustment.COLOR_BALANCE -> ColorBalancePanel(vm, Modifier)
                        Adjustment.CURVES -> CurvesPanel(vm, Modifier)
                        Adjustment.GRADIENT_MAP -> GradientMapPanel(vm, Modifier)
                        Adjustment.GLITCH -> GlitchPanel(vm, Modifier)
                        Adjustment.NOISE -> NoisePanel(vm, Modifier)
                        Adjustment.BLOOM -> BloomPanel(vm, Modifier)
                        Adjustment.HALFTONE -> HalftonePanel(vm, Modifier)
                        Adjustment.CHROMATIC_ABERRATION -> ChromaticPanel(vm, Modifier)
                        Adjustment.PERSPECTIVE_BLUR -> PerspectivePanel(vm, Modifier)
                        Adjustment.LIQUIFY -> LiquifyPanel(vm, maxWidth, maxHeight, Modifier)
                        Adjustment.PLUGIN -> PluginFilterPanel(vm, Modifier)
                        else -> Unit
                    }
                }
            }
            // Recolor's panel floats as an adjustment's controls do
            FloatableBottom(vm, io.github.artcraftandroid.procraft.core.LayoutPiece.ADJUSTING, vm.recoloring && !vm.barsHidden) { RecolorPanel(vm, Modifier) }
            FloatableBottom(
                vm,
                io.github.artcraftandroid.procraft.core.LayoutPiece.SELECTION,
                vm.selecting && !vm.barsHidden,
                goesWith = { if (vm.selecting && !vm.barsHidden) SelectionPopover(vm, maxWidth, maxHeight) },
            ) { SelectionToolbar(vm, maxWidth) }
            GuideEditor(vm)
            LightingStudio(vm)
            BrushStudio(vm)
            io.github.artcraftandroid.procraft.generation.GenerationUi.StyleStudio(vm)
            PadMenu(vm, maxWidth)
            // a colour dragged out goes over everything
            DropDot(vm)
        }
    }
}

/**
 * The current layer turned a change away (locked, hidden or left out of the picture, or an empty
 * group): Cancel, unlock or show it, or open Layers. What was tried is not done afterwards.
 */
@Composable
private fun BlockedDialog(vm: CanvasViewModel) {
    val last = androidx.compose.runtime.remember { arrayOfNulls<Pair<CanvasViewModel.Blocked, LayerInfo>>(1) }
    vm.blocked?.let { last[0] = it }
    val why = last[0]?.first
    Alert(
        vm.blocked,
        title = { (w, _) ->
            when (w) {
                CanvasViewModel.Blocked.LOCKED -> "Layer is locked"
                CanvasViewModel.Blocked.HIDDEN -> "Layer is hidden"
                CanvasViewModel.Blocked.EMPTY_GROUP -> "Group is empty"
                CanvasViewModel.Blocked.MESH_HIDDEN -> "Mesh is hidden"
                CanvasViewModel.Blocked.MESH_LOCKED -> "Mesh is locked"
                CanvasViewModel.Blocked.GROUP_HIDDEN -> "Group is hidden"
                CanvasViewModel.Blocked.GROUP -> "Group is selected"
                CanvasViewModel.Blocked.SEVERAL -> "Several layers are chosen"
                CanvasViewModel.Blocked.ALPHA_LOCKED -> "Alpha Lock is on"
            }
        },
        message = { (w, l) ->
            when (w) {
                CanvasViewModel.Blocked.LOCKED -> "“${l.name}” can't be changed while it is locked."
                CanvasViewModel.Blocked.HIDDEN, CanvasViewModel.Blocked.GROUP_HIDDEN -> hiddenMessage(l.name, vm.blockedBy)
                CanvasViewModel.Blocked.EMPTY_GROUP -> "“${l.name}” holds no layers to change."
                CanvasViewModel.Blocked.GROUP -> "“${l.name}” is a group: choose one of the layers in it."
                CanvasViewModel.Blocked.SEVERAL -> "This works on one layer at a time: choose just one."
                CanvasViewModel.Blocked.MESH_HIDDEN -> "“${vm.meantMeshName()}” can't be painted while it is hidden."
                CanvasViewModel.Blocked.MESH_LOCKED -> "“${vm.meantMeshName()}” can't be painted while it is locked."
                CanvasViewModel.Blocked.ALPHA_LOCKED -> "“${l.name}” can't be liquified while its alpha is locked."
            }
        },
        action = "Open Layers",
        onCancel = { vm.answerBlocked(fix = false, openLayers = false) },
        onAction = { vm.answerBlocked(fix = false, openLayers = true) },
        icon = when (why) {
            CanvasViewModel.Blocked.LOCKED, CanvasViewModel.Blocked.MESH_LOCKED, CanvasViewModel.Blocked.ALPHA_LOCKED -> PcIcons.PageLocked
            CanvasViewModel.Blocked.HIDDEN, CanvasViewModel.Blocked.MESH_HIDDEN, CanvasViewModel.Blocked.GROUP_HIDDEN -> PcIcons.EyeStruck
            else -> PcIcons.Folder
        },
        destructive = false,
        extra = when (why) {
            CanvasViewModel.Blocked.LOCKED, CanvasViewModel.Blocked.MESH_LOCKED -> "Unlock"
            CanvasViewModel.Blocked.ALPHA_LOCKED -> "Turn Off"
            CanvasViewModel.Blocked.HIDDEN, CanvasViewModel.Blocked.MESH_HIDDEN, CanvasViewModel.Blocked.GROUP_HIDDEN -> "Show"
            else -> null
        },
        onExtra = { vm.answerBlocked(fix = true, openLayers = false) },
    )
}

/** Why layer or group [name], which would hide a change, takes none, by what hides it ([by]). */
private fun hiddenMessage(name: String, by: io.github.artcraftandroid.procraft.core.HiddenBy?): String =
    "“$name” can't be changed while " + when (by) {
        io.github.artcraftandroid.procraft.core.HiddenBy.MASK_LAYER -> "its layer is hidden."
        io.github.artcraftandroid.procraft.core.HiddenBy.GROUP -> "its group is hidden."
        io.github.artcraftandroid.procraft.core.HiddenBy.INSIDE -> "a layer in it is hidden."
        io.github.artcraftandroid.procraft.core.HiddenBy.SOLO -> "another layer is shown alone."
        io.github.artcraftandroid.procraft.core.HiddenBy.ITSELF, null -> "it is hidden."
    }

/**
 * The artwork open couldn't be saved to leave it (the storage full, say): Stay, the default (Cancel's
 * place, as a tap beside it or back), or Leave Without Saving, which says what it loses.
 */
@Composable
internal fun LeaveUnsavedAlert(vm: CanvasViewModel) {
    Alert(
        vm.leaveUnsaved,
        title = { it.title },
        message = { it.message },
        action = "Leave Without Saving",
        cancel = "Stay",
        onCancel = { vm.stayUnsaved() },
        onAction = { vm.leaveWithoutSaving() },
    )
}

/** A layer a texture set's meshes share is about to go from all of them. */
@Composable
internal fun SharedLayerAlert(vm: CanvasViewModel) {
    Alert(
        vm.deletingShared,
        title = { "Delete shared layer?" },
        message = { "“${it.name}” is on every mesh of this texture set." },
        action = "Delete",
        onCancel = { vm.deletingShared = null },
        onAction = {
            vm.deletingShared = null
            vm.delete(it)
        },
    )
}

/** What a model newly opened may not paint well with, said once. */
@Composable
internal fun ModelWarningsAlert(vm: CanvasViewModel) {
    Alert(
        vm.modelWarnings.takeIf { it.isNotEmpty() },
        title = { "Unsupported model features" },
        message = { w -> "Painting on this model may not go as expected because of:\n" + w.joinToString("\n") },
        action = "OK",
        onCancel = { vm.modelWarnings = emptyList() },
        onAction = { vm.modelWarnings = emptyList() },
        destructive = false,
        alone = true,
    )
}

/**
 * What a plugin's brush engine couldn't do with a brush file (too big for it on this device, too
 * long, unreadable): its brushes came in with the built-in reader, and the plugin is still on.
 */
@Composable
internal fun ImportTroubleAlert(vm: CanvasViewModel) {
    Alert(
        vm.importTrouble,
        title = { it.first },
        message = { it.second },
        action = "OK",
        onCancel = { vm.importTrouble = null },
        onAction = { vm.importTrouble = null },
        destructive = false,
        alone = true,
    )
}

/** The adjustments whose controls are a panel at the bottom of the screen. */
private val BottomPanels = setOf(
    Adjustment.HSB, Adjustment.COLOR_BALANCE, Adjustment.CURVES, Adjustment.GRADIENT_MAP, Adjustment.GLITCH, Adjustment.NOISE,
    Adjustment.BLOOM, Adjustment.HALFTONE, Adjustment.CHROMATIC_ABERRATION, Adjustment.PERSPECTIVE_BLUR, Adjustment.LIQUIFY,
    Adjustment.PLUGIN,
)

/**
 * What undo or redo did (and other passing messages), in a band across the screen just under the
 * top bar: it slides down from behind the bar, stays a moment and slides back up. A new message
 * takes the place of the one there: down, its text changes in place; moving, the band goes on
 * down from where it is. Either way it stays its full time from the new message.
 */
@Composable
private fun NoticeBand(vm: CanvasViewModel) {
    val shown = androidx.compose.runtime.remember { androidx.compose.animation.core.Animatable(0f) }
    var text by androidx.compose.runtime.remember { androidx.compose.runtime.mutableStateOf("") }
    val n = vm.notice
    androidx.compose.runtime.LaunchedEffect(n) {
        if (n == null) return@LaunchedEffect
        text = n.text
        val start = System.nanoTime()
        if (shown.value < 1f) shown.animateTo(1f, Motion.noticeIn())
        val shownFor = if (n.warning) Motion.NOTICE_WARNING_SHOWN_MS else Motion.NOTICE_SHOWN_MS
        kotlinx.coroutines.delay((shownFor - (System.nanoTime() - start) / 1_000_000).coerceAtLeast(0L))
        shown.animateTo(0f, Motion.noticeOut())
        vm.noticeGone(n)
    }
    if (shown.value <= 0f && n == null) return
    Box(Modifier.offset(y = Pc.barShown).fillMaxWidth().height(24.dp).clipToBounds()) {
        // frosted over the canvas, as measured (a thin line under it doesn't show through); moved
        // as it is placed, so the frost follows it
        Box(
            Modifier.fillMaxSize()
                .offset { androidx.compose.ui.unit.IntOffset(0, kotlin.math.round(-(1f - shown.value) * 24.dp.toPx()).toInt()) }
                .then(rememberFrost(0.dp)).background(NoticeFill),
            contentAlignment = Alignment.Center,
        ) {
            Text(text, color = NoticeInk, fontSize = 13.sp, maxLines = 1)
        }
    }
}

private val NoticeFill = Color(0xC2424242)
private val NoticeInk = Color(0xFFD7D7D7)
