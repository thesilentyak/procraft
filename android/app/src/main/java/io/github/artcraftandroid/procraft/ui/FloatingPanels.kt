package io.github.artcraftandroid.procraft.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.requiredWidth
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemGestures
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.zIndex
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.procraft.canvas.CanvasViewModel
import io.github.artcraftandroid.procraft.core.FloatingPanel
import io.github.artcraftandroid.procraft.core.FloatingRoom
import io.github.artcraftandroid.procraft.core.InterfacePart
import io.github.artcraftandroid.procraft.core.LayoutPiece
import io.github.artcraftandroid.procraft.core.PieceShown
import kotlin.math.roundToInt

// The Layers panel and the brush library torn off into windows of their own, while a plugin lets
// them (the floating_panels hook): each dragged out by a grabber at its top as the colour panel is,
// then a window over the canvas that stays open while one draws, moved by the grabber strip at its
// top, sized from its lower right corner and closed by its Close (its button then hangs it from the
// bar again). Inside, the panel is the same as where it hangs; the window is kept where it was left. Where each goes
// is the core's to work out (PanelLayout): while a plugin lays the panels out, they snap to the window's edges and to
// each other, fold to a tab by a button beside Close, and those docked may make room for the canvas.

/** The strip a floating panel's window adds above the panel: its grabber, and Close at its right. */
private val FloatingTopHeight = 24.dp

/** How far down a floating window's grabber is (dp, at the window's own size): what is held when it is dragged out. */
const val FloatingGrabY = 12f

/** The strip a floating panel's window adds under the panel, where its corner is sized from. */
private val FloatingBottomHeight = 20.dp

/** How far a panel's grabber is dragged before the panel floats, as the colour panel's. */
private val PanelUndockDrag = 30.dp

/**
 * What a panel's window adds to it while it floats: how wide it shows the panel, and what its grabber strip and its
 * corner do (pressed, [onHold], sizing or not; dragged, [onDrag] by all the drag has gone, in window pixels; then
 * [onLetGo]), its Close ([onClose]) and, while the panels fold, its fold button ([onFold]).
 */
class FloatingFrame(
    val width: Dp,
    /** Held, sizing or not, the finger at a point (dp of the interface). */
    val onHold: (sizing: Boolean, at: Offset) -> Unit,
    val onDrag: (Offset) -> Unit,
    val onLetGo: () -> Unit,
    val onClose: () -> Unit,
    val onFold: (() -> Unit)? = null,
    val menu: WindowMenu? = null,
    val tabs: List<WindowTab> = emptyList(),
    /** Collapse to edge: the side its collapse button sends it into, and what that does. */
    val collapseEdge: io.github.artcraftandroid.procraft.core.WindowEdge? = null,
    val onCollapse: () -> Unit = {},
)

/**
 * A tab on a window holding several panels: its name, whether it shows, and what tapping it ([onPick]) and dragging it out
 * do (torn out at a point, [onTear]; then following the finger, [onTorn]; then [onLetGo]), in dp of the interface.
 */
class WindowTab(
    val title: String,
    val active: Boolean,
    val onPick: () -> Unit,
    val onTear: (Offset) -> Unit,
    val onTorn: (Offset) -> Unit,
    val onLetGo: () -> Unit,
)

/** How far a tab is dragged before it is torn out of its window. */
private val TabTearDrag = 26.dp

/** [piece]'s window's tabs, while it holds several panels. */
internal fun windowTabs(vm: CanvasViewModel, piece: LayoutPiece): List<WindowTab> {
    val tabs = vm.tabsOf(piece)
    return tabs.map { p ->
        WindowTab(
            pieceTitle(p),
            active = p == piece,
            onPick = { vm.pickTab(p) },
            onTear = { at -> vm.tearTab(p, at.x, at.y) },
            onTorn = { at -> vm.followTorn(at.x, at.y) },
            onLetGo = { vm.letGoPiece() },
        )
    }
}

/**
 * The tabs of a window holding several panels, in a row across its strip between its menu button and Close (and the
 * [buttons] beside it: Fold, Collapse to edge's): narrower, their titles cut short, as more share the room.
 */
@Composable
private fun BoxScope.WindowTabs(tabs: List<WindowTab>, buttons: Int) {
    val corner = LocalClearance.current.corner
    // the interface's dp, not the window's: the floating windows may show at a size of their own
    val dp = LocalDensity.current.density / LocalPartScale.current
    androidx.compose.foundation.layout.Row(
        Modifier.align(Alignment.TopCenter).padding(top = 1.dp, start = 54.dp, end = 58.dp + 40.dp * buttons),
        horizontalArrangement = androidx.compose.foundation.layout.Arrangement.spacedBy(4.dp, Alignment.CenterHorizontally),
    ) {
        for (tab in tabs) {
            val now by rememberUpdatedState(tab)
            var origin by remember { mutableStateOf(Offset.Zero) }
            Box(
                Modifier.weight(1f, fill = false).height(22.dp).clip(androidx.compose.foundation.shape.RoundedCornerShape(11.dp))
                    .background(if (tab.active) Pc.Accent.copy(alpha = 0.35f) else androidx.compose.ui.graphics.Color.Transparent)
                    .onGloballyPositioned { origin = it.positionInRoot() - corner }
                    .pointerInput(Unit) { detectTapGestures { now.onPick() } }
                    .pointerInput(Unit) {
                        var total = Offset.Zero
                        var torn = false
                        detectDragGestures(
                            onDragStart = {
                                total = Offset.Zero
                                torn = false
                            },
                            onDragEnd = { if (torn) now.onLetGo() },
                            onDragCancel = { if (torn) now.onLetGo() },
                        ) { change, delta ->
                            change.consume()
                            total += delta
                            val at = (origin + change.position) / dp
                            when {
                                torn -> now.onTorn(at)
                                total.getDistance() > TabTearDrag.toPx() -> {
                                    torn = true
                                    now.onTear(at)
                                }
                            }
                        }
                    }
                    .padding(horizontal = 10.dp),
                contentAlignment = Alignment.Center,
            ) {
                androidx.compose.material3.Text(
                    tab.title,
                    color = Pc.PanelTitle,
                    fontSize = androidx.compose.ui.unit.TextUnit(13f, androidx.compose.ui.unit.TextUnitType.Sp),
                    maxLines = 1,
                    overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis,
                )
            }
        }
    }
}

/** A floating window's menu, from the button on its strip: Fold, Hide, Pin to Top (or Unpin) and Reset Position. */
class WindowMenu(
    val pinned: Boolean,
    val onFold: (() -> Unit)?,
    val onHide: () -> Unit,
    val onPin: () -> Unit,
    val onReset: () -> Unit,
    /** Collapse to edge: Collapse All, or Open All while any bar or window is collapsed; none with it off. */
    val anyCollapsed: Boolean = false,
    val onCollapseAll: (() -> Unit)? = null,
)

/** [piece]'s window menu, as the layout has it now. */
internal fun windowMenu(vm: CanvasViewModel, piece: LayoutPiece): WindowMenu {
    val pinned = vm.pinnedPiece(piece)
    return WindowMenu(
        pinned = pinned,
        onFold = if (vm.layoutLook.fold) {
            { vm.foldPiece(piece, true) }
        } else {
            null
        },
        onHide = { vm.hidePiece(piece) },
        onPin = { vm.pinPiece(piece, !pinned) },
        onReset = { vm.resetPiece(piece) },
        anyCollapsed = vm.anyCollapsed(),
        onCollapseAll = if (vm.layoutLook.collapse) {
            { vm.collapseAll(!vm.anyCollapsed()) }
        } else {
            null
        },
    )
}

/**
 * A drag, [frame]'s piece held as it starts ([sizing] or not) with the finger where it is (what is dragged lies at
 * [origin], dp of the interface, [px] pixels each), followed all told, and let go.
 */
private suspend fun androidx.compose.ui.input.pointer.PointerInputScope.dragsFrame(
    frame: () -> FloatingFrame,
    sizing: Boolean,
    origin: () -> Offset,
    px: () -> Float,
) {
    var total = Offset.Zero
    detectDragGestures(
        onDragStart = { start ->
            total = Offset.Zero
            frame().onHold(sizing, origin() + start / px())
        },
        onDragEnd = { frame().onLetGo() },
        onDragCancel = { frame().onLetGo() },
    ) { change, delta ->
        change.consume()
        total += delta
        frame().onDrag(total)
    }
}

/**
 * The strip over a floating panel: its grabber in the middle (the whole strip drags the window), Close at its right, and
 * left of it the fold button while the panels fold.
 */
@Composable
internal fun FloatingTop(frame: FloatingFrame) {
    // (the window moving under the drag mustn't start it again)
    val now by rememberUpdatedState(frame)
    // where it is (dp of the interface), for the finger's place as it is held
    val corner = LocalClearance.current.corner
    val px = LocalDensity.current.density / LocalPartScale.current
    var origin by remember { mutableStateOf(Offset.Zero) }
    Box(
        Modifier.fillMaxWidth().height(FloatingTopHeight)
            .onGloballyPositioned { origin = (it.positionInRoot() - corner) / px }
            .pointerInput(Unit) { dragsFrame({ now }, sizing = false, { origin }, { px }) },
    ) {
        // holding several panels: their tabs in place of the grabber
        if (frame.tabs.size > 1) {
            WindowTabs(frame.tabs, buttons = listOfNotNull(frame.onFold, frame.collapseEdge).size)
        } else {
            Box(Modifier.align(Alignment.TopCenter).padding(top = 10.dp).size(40.dp, 4.dp).clip(CircleShape).background(Pc.Grabber))
        }
        FloatingClose(Modifier.align(Alignment.TopEnd).padding(top = 2.dp, end = 14.dp)) { now.onClose() }
        frame.onFold?.let { fold -> FloatingFold(Modifier.align(Alignment.TopEnd).padding(top = 2.dp, end = 54.dp), fold) }
        // Collapse to edge's, beside Fold (or Close)
        frame.collapseEdge?.let { edge ->
            val end = if (frame.onFold != null) 94.dp else 54.dp
            FloatingCollapse(Modifier.align(Alignment.TopEnd).padding(top = 2.dp, end = end), edge) { now.onCollapse() }
        }
        frame.menu?.let { menu -> FloatingMenuButton(Modifier.align(Alignment.TopStart).padding(top = 2.dp, start = 10.dp), menu) }
    }
}

/** The window's menu button, three dots on a disc, and the menu it opens: Fold, Hide, Pin to Top and Reset Position. */
@Composable
internal fun FloatingMenuButton(modifier: Modifier, menu: WindowMenu) {
    var open by remember { mutableStateOf(false) }
    Box(modifier) {
        Box(
            Modifier.size(40.dp).semantics { contentDescription = "Window" }.clickable { open = true }.drawBehind {
                drawCircle(androidx.compose.ui.graphics.Color.White.copy(alpha = 0.15f), 8.5.dp.toPx())
                for (dx in listOf(-3.6f, 0f, 3.6f)) {
                    drawCircle(androidx.compose.ui.graphics.Color.Black.copy(alpha = 0.35f), 1.1.dp.toPx(), center + Offset(dx.dp.toPx(), 0f))
                }
            },
        )
        val place = remember { MenuPlace { anchor, _ -> IntOffset(anchor.left, anchor.bottom) } }
        LibraryMenu(open, androidx.compose.ui.graphics.TransformOrigin(0f, 0f), place, onDismiss = { open = false }, overPanel = true) {
            val row: @Composable (String, () -> Unit) -> Unit = { label, act ->
                MenuItem(label, Pc.MenuInk, null, onClick = {
                    open = false
                    act()
                }) {}
            }
            menu.onFold?.let { row("Fold", it) }
            row("Hide", menu.onHide)
            row(if (menu.pinned) "Unpin" else "Pin to Top", menu.onPin)
            row("Reset Position", menu.onReset)
            menu.onCollapseAll?.let { row(if (menu.anyCollapsed) "Open All" else "Collapse All", it) }
        }
    }
}

/**
 * The grabber over a piece that isn't the Layers panel or the library (the Actions panel and the Adjustments menu where
 * they hang, a toolbar or an adjustment's controls at the bottom), while a plugin lets it float: dragged out past
 * [PanelUndockDrag], its window follows the finger, its strip's middle under it; let go, it floats there. [dp] is the
 * interface's dp in the pixels it is drawn in.
 */
@Composable
internal fun PieceGrabber(vm: CanvasViewModel, piece: LayoutPiece, modifier: Modifier, dp: Float) {
    var origin by remember { mutableStateOf(Offset.Zero) }
    val corner = LocalClearance.current.corner
    val k = Pc.partScale(InterfacePart.FLOATING)
    Box(
        modifier.size(100.dp, 30.dp).onGloballyPositioned { origin = it.positionInRoot() - corner }.pointerInput(vm, piece) {
            var total = Offset.Zero
            detectDragGestures(
                onDragStart = { total = Offset.Zero },
                onDragEnd = { vm.undockedPiece(piece) },
                onDragCancel = { vm.undockedPiece(piece) },
            ) { change, delta ->
                change.consume()
                total += delta
                if (vm.pieceUndocking == piece || total.getDistance() > PanelUndockDrag.toPx()) {
                    val at = origin + change.position
                    vm.undockPiece(piece, at.x / dp, at.y / dp, FloatingTopHeight.value / 2 / k)
                }
            }
        },
    ) {
        Box(Modifier.align(Alignment.TopCenter).padding(top = 4.dp).size(40.dp, 4.dp).clip(CircleShape).background(Pc.Grabber))
    }
}

/** Fold: a disc with a bar across it, the panel going to its tab. */
@Composable
internal fun FloatingFold(modifier: Modifier, onFold: () -> Unit) {
    Box(
        modifier.size(40.dp).semantics { contentDescription = "Fold" }.clickable(onClick = onFold).drawBehind {
            drawCircle(androidx.compose.ui.graphics.Color.White.copy(alpha = 0.15f), 8.5.dp.toPx())
            val d = 3.6.dp.toPx()
            drawLine(androidx.compose.ui.graphics.Color.Black.copy(alpha = 0.3f), center + Offset(-d, 0f), center + Offset(d, 0f), 1.3.dp.toPx())
        },
    )
}

/**
 * A floating panel folded to its tab: its name and an unfold mark, at [shown] (dp of the interface) at the floating
 * windows' size. A tap unfolds it; dragged, it moves as the panel would, snapping as it does.
 */
@Composable
internal fun FloatingTab(vm: CanvasViewModel, piece: LayoutPiece, shown: PieceShown, title: String) {
    val dp = LocalDensity.current.density
    val frame = remember(vm, piece, dp) {
        FloatingFrame(
            width = shown.width.dp,
            onHold = { _, at -> vm.holdPiece(piece, false, at) },
            onDrag = { px -> vm.dragPiece(px.x / dp, px.y / dp) },
            onLetGo = { vm.letGoPiece() },
            onClose = {},
        )
    }
    val now by rememberUpdatedState(frame)
    // (where the tab is, dp of the interface: the finger's place as it is held)
    val tab = Offset(shown.x, shown.y)
    val tabNow by rememberUpdatedState(tab)
    PartScaled(InterfacePart.FLOATING) {
        val at = Modifier.offset { IntOffset((shown.x * dp).roundToInt(), (shown.y * dp).roundToInt()) }
        Popover(
            shown.width.dp,
            0.dp,
            at.height(shown.height.dp)
                .semantics { contentDescription = "$title, folded" }
                .pointerInput(Unit) { detectTapGestures { vm.foldPiece(piece, false) } }
                .pointerInput(Unit) { dragsFrame({ now }, sizing = false, { tabNow }, { dp }) },
            side = ArrowSide.NONE,
        ) {
            androidx.compose.foundation.layout.Row(
                Modifier.fillMaxSize().padding(horizontal = 14.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                androidx.compose.material3.Text(
                    title,
                    color = Pc.PanelTitle,
                    fontSize = androidx.compose.ui.unit.TextUnit(15f, androidx.compose.ui.unit.TextUnitType.Sp),
                    fontWeight = androidx.compose.ui.text.font.FontWeight.SemiBold,
                    maxLines = 1,
                    modifier = Modifier.weight(1f),
                )
                // the unfold mark: a chevron opening downward
                Box(
                    Modifier.size(14.dp).drawBehind {
                        val w = 1.6.dp.toPx()
                        val (l, m, r) = Triple(Offset(0f, size.height * 0.3f), Offset(size.width / 2, size.height * 0.72f), Offset(size.width, size.height * 0.3f))
                        drawLine(Pc.PanelTitle, l, m, w, androidx.compose.ui.graphics.StrokeCap.Round)
                        drawLine(Pc.PanelTitle, m, r, w, androidx.compose.ui.graphics.StrokeCap.Round)
                    },
                )
            }
        }
    }
}

/** The room a floating panel's window leaves under the panel, for its corner, which sizes it ([FloatingCorner]). */
@Composable
internal fun FloatingBottom(@Suppress("UNUSED_PARAMETER") frame: FloatingFrame) {
    androidx.compose.foundation.layout.Spacer(Modifier.fillMaxWidth().height(FloatingBottomHeight))
}

/** A floating panel's lower right corner, two short lines across it: dragged, it sizes the window. */
@Composable
private fun FloatingCorner(frame: FloatingFrame, modifier: Modifier) {
    // (the window growing under the drag mustn't start it again)
    val now by rememberUpdatedState(frame)
    val corner = LocalClearance.current.corner
    val px = LocalDensity.current.density / LocalPartScale.current
    var origin by remember { mutableStateOf(Offset.Zero) }
    Box(
        modifier.padding(end = 6.dp).size(36.dp, FloatingBottomHeight)
            .onGloballyPositioned { origin = (it.positionInRoot() - corner) / px }
            .pointerInput(Unit) { dragsFrame({ now }, sizing = true, { origin }, { px }) }
            .drawBehind {
                // two lines across the corner, the nearer shorter
                val ink = Pc.Grabber
                val w = 2.dp.toPx()
                val end = Offset(size.width - 12.dp.toPx(), size.height - 6.dp.toPx())
                for (reach in listOf(10.dp.toPx(), 5.dp.toPx())) {
                    drawLine(ink, end + Offset(-reach, 0f), end + Offset(0f, -reach), w, androidx.compose.ui.graphics.StrokeCap.Round)
                }
            },
    )
}

/**
 * The grabber at the top of a panel hanging from its button, while a plugin lets it float: dragged out past
 * [PanelUndockDrag], the panel goes unseen and its window follows the finger; let go, it floats there.
 */
@Composable
internal fun BoxScope.PanelGrabber(vm: CanvasViewModel, panel: FloatingPanel) {
    Box(Modifier.align(Alignment.TopCenter).padding(top = 4.dp).size(40.dp, 4.dp).clip(CircleShape).background(Pc.Grabber))
    var origin by remember { mutableStateOf(Offset.Zero) }
    // the interface's dp, not this panel's: a plugin may show the panels at a size of their own
    val dp = LocalDensity.current.density / LocalPartScale.current
    // where it is in the interface: in the window, less what the interface keeps clear of
    val corner = LocalClearance.current.corner
    Box(
        // (narrower than the colour panel's: clear of a title beside it)
        Modifier.align(Alignment.TopCenter).size(100.dp, 30.dp).onGloballyPositioned { origin = it.positionInRoot() - corner }.pointerInput(vm, panel) {
            var total = Offset.Zero
            detectDragGestures(
                onDragStart = { total = Offset.Zero },
                onDragEnd = { vm.undockedPanel(panel) },
                onDragCancel = { vm.undockedPanel(panel) },
            ) { change, delta ->
                change.consume()
                total += delta
                if (vm.panelUndocking == panel || total.getDistance() > PanelUndockDrag.toPx()) {
                    val at = origin + change.position
                    vm.undockPanel(panel, at.x / dp, at.y / dp)
                }
            }
        },
    )
}

/**
 * The panels floating in windows of their own, each where it was left, at the floating windows' size: over the canvas
 * (which takes no touch under them, and every touch around them), under the panels hanging from the bar; folded, their
 * tabs. [maxWidth] by [maxHeight] is the window; they keep clear of the side bar as the panels do.
 */
@Composable
fun FloatingPanels(vm: CanvasViewModel, maxWidth: Dp, maxHeight: Dp) {
    // (the side bar moved off its edge by Edit layout leaves the room whole)
    val atEdge = vm.barMoved(io.github.artcraftandroid.procraft.core.LayoutBar.SIDE_BAR) == null && vm.barCollapsed(io.github.artcraftandroid.procraft.core.LayoutBar.SIDE_BAR) == null
    val clear = if (vm.showSideBar && atEdge) Pc.sideBarShown + Pc.PopoverGap else 0.dp
    // (a drag from the top edge's gesture zone is the system's: no strip goes up into it)
    val gestures = with(LocalDensity.current) { androidx.compose.foundation.layout.WindowInsets.systemGestures.getTop(this).toDp() }
    val kept = with(LocalDensity.current) { LocalClearance.current.top.toDp() }
    val top = (gestures - kept).coerceAtLeast(0.dp).value
    val room = if (vm.rightHanded) {
        FloatingRoom(0f, top, (maxWidth - clear).value, maxHeight.value)
    } else {
        FloatingRoom(clear.value, top, maxWidth.value, maxHeight.value)
    }
    val k = Pc.partScale(InterfacePart.FLOATING)
    val bar = Pc.barShown.value
    SideEffect { vm.layoutRoom(room, maxWidth.value, maxHeight.value, bar, k) }
    // the Tool Belt, under the windows
    // (not while a panel hangs open from its button: it would lie over the panel)
    if (vm.panel == io.github.artcraftandroid.procraft.canvas.Panel.NONE) vm.beltShown()?.let { ToolBelt(vm, it) }
    // over one another as they were last pressed, those pinned over the rest: each always in the same
    // place among them, stacked by its order (raised, none moves, so a drag on one goes on)
    // (in a box of their own, stacked among themselves: under the panels hanging from the bar)
    val order = vm.pieceOrder()
    Box(Modifier.size(maxWidth, maxHeight)) {
        // (each in its slot whether it shows or not: one coming to show before it, as a tab torn out of
        // its window does, never moves it or composes it anew, which would end the drag on its strip)
        for ((piece, showing) in floatingWindowSlots(order)) key(piece) {
            val panel = when (piece) {
                LayoutPiece.LAYERS -> FloatingPanel.LAYERS
                LayoutPiece.BRUSHES -> FloatingPanel.BRUSHES
                else -> null
            }
            val window = vm.shownPiece(piece)?.takeIf { showing }
            if (window != null) {
                // collapsed into its side (Collapse to edge): sliding there, then gone, its tab there instead
                val collapsed = vm.pieceCollapsed(piece) != null
                val slide = collapseSlide(collapsed)
                if (!(collapsed && slide >= 1f)) {
                    val px = LocalDensity.current.density
                    Box(
                        Modifier.zIndex(order.indexOf(piece).toFloat()).size(maxWidth, maxHeight)
                            .collapseSlid(vm.pieceCollapsePlace(piece), slide, px),
                    ) {
                        when {
                            window.folded -> FloatingTab(vm, piece, window, pieceTitle(piece))
                            panel != null -> FloatingPanelWindow(vm, panel, window)
                            else -> FloatingPopoverWindow(vm, piece, window)
                        }
                    }
                }
            }
        }
    }
}

/** The pieces drawn in windows by [FloatingPanels]: the Layers panel, the library, the Actions panel and the Adjustments menu. */
private val FloatingWindowPieces = listOf(LayoutPiece.LAYERS, LayoutPiece.BRUSHES, LayoutPiece.ACTIONS, LayoutPiece.ADJUSTMENTS)

/**
 * Each piece drawn in a window, always in the same slot (the same place among the others, whichever of them show), and
 * whether it shows now (it is in [order], the windows showing).
 */
internal fun floatingWindowSlots(order: List<LayoutPiece>): List<Pair<LayoutPiece, Boolean>> =
    FloatingWindowPieces.map { it to (it in order) }

/** A floating piece's name, on its tab. */
internal fun pieceTitle(piece: LayoutPiece): String = when (piece) {
    LayoutPiece.LAYERS -> "Layers"
    LayoutPiece.BRUSHES -> "Brush library"
    LayoutPiece.COLOURS -> "Colors"
    LayoutPiece.ACTIONS -> "Actions"
    LayoutPiece.ADJUSTMENTS -> "Adjustments"
    LayoutPiece.ADJUSTING -> "Adjustment"
    LayoutPiece.SELECTION -> "Selection"
    LayoutPiece.TRANSFORM -> "Transform"
    LayoutPiece.LIQUIFY -> "Liquify"
    LayoutPiece.REFERENCE -> "Reference"
}

/** The Actions panel or the Adjustments menu floating where the layout has it, at the floating windows' size. */
@Composable
private fun FloatingPopoverWindow(vm: CanvasViewModel, piece: LayoutPiece, window: PieceShown) {
    val dp = LocalDensity.current.density
    val sizable = io.github.artcraftandroid.procraft.core.pieceSizes(piece).sizable
    val menu = windowMenu(vm, piece)
    val tabs = vm.tabsOf(piece)
    val edge = vm.pieceCollapseEdge(piece)
    val frame = remember(vm, piece, dp, window.width, menu.pinned, menu.onFold != null, menu.anyCollapsed, tabs, edge) {
        FloatingFrame(
            width = window.width.dp,
            onHold = { sizing, at -> vm.holdPiece(piece, sizing, at) },
            onDrag = { px -> vm.dragPiece(px.x / dp, px.y / dp) },
            onLetGo = { vm.letGoPiece() },
            onClose = { vm.closePiece(piece) },
            menu = menu,
            tabs = windowTabs(vm, piece),
            collapseEdge = edge,
            onCollapse = { vm.collapsePiece(piece, true) },
        )
    }
    PartScaled(InterfacePart.FLOATING) {
        val at = Modifier.offset { IntOffset((window.x * dp).roundToInt(), (window.y * dp).roundToInt()) }
        Box(
            at.size(window.width.dp, window.height.dp).pointerInput(Unit) {
                // the canvas under it takes no touch
                awaitEachGesture { awaitFirstDown().consume() }
            },
        ) {
            val fills = Modifier.fillMaxSize()
            androidx.compose.runtime.CompositionLocalProvider(LocalFloat provides FloatWindow(window.width.dp, window.height.dp, frame, sizable)) {
                when (piece) {
                    LayoutPiece.ACTIONS -> ActionsPanel(vm, 0.dp, fills)
                    else -> AdjustmentsPanel(vm, 0.dp, fills)
                }
            }
            if (sizable) FloatingCorner(frame, Modifier.align(Alignment.BottomEnd))
        }
    }
}

/** Where a piece at the bottom is laid out while it doesn't float (dp of the interface), as last measured. */
private class Docked(var rect: androidx.compose.ui.geometry.Rect? = null)

/** The tallest a toolbar or an adjustment's controls are: what measures taller is a menu of theirs over the window. */
private val MostBottomHeight = 400.dp

/**
 * [content] (a toolbar, or an adjustment's controls), [piece] in the layout: where it always is at the bottom of the window,
 * with a grabber over it while a plugin lets it float; or, torn off, where the layout has it, a strip over it to move it
 * (its menu, its fold and its Close on it). It shows while [visible]; it draws itself [lift] above where it is laid out.
 * [goesWith] (the menus it opens) moves with it, as the whole layer they are drawn in moves. The layout is told how big
 * it is, at the size it shows.
 */
@Composable
fun BoxScope.FloatableBottom(
    vm: CanvasViewModel,
    piece: LayoutPiece,
    visible: Boolean,
    lift: Dp = 25.dp,
    goesWith: @Composable () -> Unit = {},
    content: @Composable () -> Unit,
) {
    val density = LocalDensity.current.density
    val corner = LocalClearance.current.corner
    val k = Pc.partScale(InterfacePart.FLOATING)
    val docked = remember { Docked() }
    var measured by remember { mutableStateOf<androidx.compose.ui.geometry.Rect?>(null) }
    val shown = if (visible) vm.shownPiece(piece) else null
    val floating = shown?.takeIf { !it.folded }
    // shown again as its tool opens, if its window was hidden
    androidx.compose.runtime.LaunchedEffect(visible) { if (visible) vm.hidePiece(piece, false) }
    // where it shows, where it always is
    val home = measured?.translate(0f, -lift.value)
    // from where it always is to where the layout has it (its strip over it)
    val shift = if (floating != null && home != null) {
        Offset(floating.x - home.left, floating.y + FloatingTopHeight.value - home.top)
    } else {
        Offset.Zero
    }
    val layer = Modifier.matchParentSize().offset { IntOffset((shift.x * density).roundToInt(), (shift.y * density).roundToInt()) }
    Box(layer) {
        androidx.compose.animation.AnimatedVisibility(
            visible && shown?.folded != true,
            Modifier.align(Alignment.BottomCenter).graphicsLayer { alpha = if (floating != null && home == null) 0f else 1f },
            enter = if (floating != null) androidx.compose.animation.fadeIn() else Motion.toolbarIn(),
            exit = if (floating != null) androidx.compose.animation.fadeOut() else Motion.toolbarOut(),
        ) {
            Box(
                Modifier.onGloballyPositioned { c ->
                    // where it is drawn, less the shift: where it always is
                    val b = c.boundsInRoot().translate(-corner)
                    val r = androidx.compose.ui.geometry.Rect(b.left / density - shift.x, b.top / density - shift.y, b.right / density - shift.x, b.bottom / density - shift.y)
                    // (a menu of its own over the whole window is passed over)
                    if (r.height > MostBottomHeight.value) return@onGloballyPositioned
                    if (docked.rect != r) {
                        docked.rect = r
                        measured = r
                    }
                    // the layout told its size at the floating windows' size (it shows at its own)
                    vm.tellPieceSize(piece, r.width / k, (r.height + FloatingTopHeight.value) / k)
                },
            ) { content() }
        }
        goesWith()
        val at = home
        if (visible && at != null) {
            if (floating != null) {
                // its strip over it
                val menu = windowMenu(vm, piece)
                val dp = density
                val frame = remember(vm, piece, dp, menu.pinned, menu.onFold != null) {
                    FloatingFrame(
                        width = at.width.dp,
                        onHold = { sizing, at -> vm.holdPiece(piece, sizing, at) },
                        onDrag = { px -> vm.dragPiece(px.x / dp, px.y / dp) },
                        onLetGo = { vm.letGoPiece() },
                        onClose = { vm.closePiece(piece) },
                        menu = menu,
                    )
                }
                Box(
                    Modifier.offset { IntOffset((at.left * density).roundToInt(), ((at.top - FloatingTopHeight.value) * density).roundToInt()) }
                        .size(at.width.dp, FloatingTopHeight).clip(androidx.compose.foundation.shape.RoundedCornerShape(topStart = 12.dp, topEnd = 12.dp))
                        .background(Pc.PopoverFill),
                ) { FloatingTop(frame) }
            } else if (shown == null && vm.mayFloat(piece)) {
                // where it always is: its grabber over it
                PieceGrabber(
                    vm,
                    piece,
                    Modifier.offset { IntOffset(((at.center.x - 50f) * density).roundToInt(), ((at.top - 30f) * density).roundToInt()) },
                    density,
                )
            }
        }
    }
    // folded: its tab
    if (shown?.folded == true) FloatingTab(vm, piece, shown, pieceTitle(piece))
}

@Composable
private fun FloatingPanelWindow(vm: CanvasViewModel, panel: FloatingPanel, window: PieceShown) {
    val piece = if (panel == FloatingPanel.LAYERS) LayoutPiece.LAYERS else LayoutPiece.BRUSHES
    val dp = LocalDensity.current.density
    // what shows when it opens: the layers' thumbnails, the latest brushes used
    when (panel) {
        FloatingPanel.LAYERS -> LaunchedEffect(Unit) { vm.refreshThumbnailsShown() }
        FloatingPanel.BRUSHES -> LaunchedEffect(vm.currentBrush) { vm.readRecentBrushes() }
    }
    // the same while the window only moves, so the panel in it isn't composed again for that
    val folds = vm.layoutLook.fold
    val menu = windowMenu(vm, piece)
    val tabs = vm.tabsOf(piece)
    val edge = vm.pieceCollapseEdge(piece)
    val frame = remember(vm, panel, dp, window.width, folds, menu.pinned, menu.anyCollapsed, tabs, edge) {
        FloatingFrame(
            width = window.width.dp,
            onHold = { sizing, at -> vm.holdPiece(piece, sizing, at) },
            onDrag = { px -> vm.dragPiece(px.x / dp, px.y / dp) },
            onLetGo = { vm.letGoPiece() },
            onClose = { vm.closeFloating(panel) },
            onFold = if (folds) {
                { vm.foldPiece(piece, true) }
            } else {
                null
            },
            menu = menu,
            tabs = windowTabs(vm, piece),
            collapseEdge = edge,
            onCollapse = { vm.collapsePiece(piece, true) },
        )
    }
    // laid out at the floating windows' size, placed in the window's pixels
    PartScaled(InterfacePart.FLOATING) {
        val at = Modifier.offset { IntOffset((window.x * dp).roundToInt(), (window.y * dp).roundToInt()) }
        Box(
            at.size(window.width.dp, window.height.dp).pointerInput(Unit) {
                // the canvas under it takes no touch
                awaitEachGesture { awaitFirstDown().consume() }
            },
        ) {
            val fills = Modifier.fillMaxSize()
            when (panel) {
                FloatingPanel.LAYERS -> LayersPanel(vm, 0.dp, fills, floating = frame)
                FloatingPanel.BRUSHES -> BrushLibrary(vm, 0.dp, fills, floating = frame)
            }
            FloatingCorner(frame, Modifier.align(Alignment.BottomEnd))
        }
    }
}

/**
 * Edit layout's handles: an outline round each bar (the top bar's halves and the side bar) that drags it, snapping to the
 * window's edges and the other bars, a bar against a side standing upright; Reset and Done at the bottom.
 */
@Composable
fun BoxScope.EditLayoutHandles(vm: CanvasViewModel) {
    val d = LocalDensity.current.density
    // the name asked for in the app's own alert, over the whole screen: a new layout's (no from), or one renamed
    var asking by remember { mutableStateOf<Pair<String?, String>?>(null) }
    var text by remember(asking) { mutableStateOf(asking?.second ?: "") }
    Alert(
        asking,
        title = { if (it.first == null) "Save Layout" else "Rename Layout" },
        message = { "" },
        action = if (asking?.first == null) "Save" else "Rename",
        onCancel = { asking = null },
        onAction = { (from, _) ->
            if (from == null) vm.saveLayout(text) else vm.renameLayout(from, text)
            asking = null
        },
        destructive = false,
        field = text,
        onField = { text = it },
    )
    // (the Tool Belt's while it shows)
    val belt = vm.beltShown() != null
    for (bar in io.github.artcraftandroid.procraft.core.LayoutBar.entries) {
        if (bar == io.github.artcraftandroid.procraft.core.LayoutBar.BELT && !belt) continue
        val r = vm.barChanges.let { vm.barLayout.rect(bar) } ?: continue
        key(bar) {
            val rNow by rememberUpdatedState(r)
            Box(
                Modifier.offset { IntOffset((r.x * d).roundToInt(), (r.y * d).roundToInt()) }.size(r.width.dp, r.height.dp)
                    .border(2.dp, Pc.Accent, androidx.compose.foundation.shape.RoundedCornerShape(12.dp))
                    .semantics { contentDescription = "Move" }
                    .pointerInput(bar) {
                        var total = Offset.Zero
                        detectDragGestures(
                            onDragStart = { start ->
                                total = Offset.Zero
                                // the finger's place in the window (dp): where the bar turns and goes decides by it
                                vm.holdBar(bar, rNow.x + start.x / d, rNow.y + start.y / d)
                            },
                            onDragEnd = { vm.letGoBar() },
                            onDragCancel = { vm.letGoBar() },
                        ) { change, delta ->
                            change.consume()
                            total += delta
                            vm.dragBar(total.x / d, total.y / d)
                        }
                    }
                    // a tap on a bar does nothing while it moves
                    .pointerInput(Unit) { detectTapGestures {} },
            )
        }
    }
    androidx.compose.foundation.layout.Row(
        Modifier.align(Alignment.BottomCenter).padding(bottom = 28.dp).clip(CircleShape).background(Pc.PopoverFill)
            .keepsItsTouches().padding(horizontal = 8.dp).height(44.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // the named layouts, the windows hidden from their menus (Show Hidden), Reset and Done
        var layouts by remember { mutableStateOf(false) }
        if (layouts) NamedLayoutsList(vm, onAsk = { asking = it }) { layouts = false }
        val rows = mutableListOf<Pair<String, () -> Unit>>("Layouts" to { layouts = !layouts })
        if (vm.hiddenPieces().isNotEmpty()) rows += "Show Hidden" to { vm.showHiddenPieces() }
        // Collapse to edge: every bar and window against a side into it, or back out
        if (vm.layoutLook.collapse) {
            val any = vm.anyCollapsed()
            rows += (if (any) "Open All" else "Collapse All") to { vm.collapseAll(!any) }
        }
        rows += "Reset" to { vm.resetLayout() }
        rows += "Done" to { vm.doneEditLayout() }
        for ((label, act) in rows) {
            androidx.compose.material3.Text(
                label,
                color = Pc.Accent,
                fontSize = androidx.compose.ui.unit.TextUnit(17f, androidx.compose.ui.unit.TextUnitType.Sp),
                modifier = Modifier.clickable { act() }.padding(horizontal = 16.dp, vertical = 10.dp),
            )
        }
    }
}

/**
 * Edit layout's named layouts: each by its name (a tap brings it back, the one last chosen ticked), renamed or deleted
 * from its row, and Save Layout, which saves the bars and panels where they are under a name of the painter's.
 */
@Composable
private fun NamedLayoutsList(vm: CanvasViewModel, onAsk: (Pair<String?, String>) -> Unit, onClose: () -> Unit) {
    val names = vm.layoutNames()
    val chosen = vm.chosenLayout()
    androidx.compose.ui.window.Popup(alignment = Alignment.BottomCenter, offset = IntOffset(0, -260), onDismissRequest = onClose) {
        androidx.compose.foundation.layout.Column(
            Modifier.size(320.dp, (12 + 44 * names.size + 44).dp).clip(androidx.compose.foundation.shape.RoundedCornerShape(16.dp))
                .background(Pc.PopoverFill).padding(vertical = 6.dp),
        ) {
            for (name in names) {
                androidx.compose.foundation.layout.Row(
                    Modifier.fillMaxWidth().height(44.dp).clickable { vm.chooseLayout(name) }.padding(start = 16.dp, end = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    androidx.compose.material3.Text(
                        (if (name == chosen) "\u2713 " else "") + name,
                        color = Pc.MenuInk,
                        fontSize = androidx.compose.ui.unit.TextUnit(17f, androidx.compose.ui.unit.TextUnitType.Sp),
                        maxLines = 1,
                        modifier = Modifier.weight(1f),
                    )
                    androidx.compose.material3.Text(
                        "Rename",
                        color = Pc.Accent,
                        fontSize = androidx.compose.ui.unit.TextUnit(15f, androidx.compose.ui.unit.TextUnitType.Sp),
                        modifier = Modifier.clickable { onAsk(name to name) }.padding(horizontal = 8.dp, vertical = 10.dp),
                    )
                    androidx.compose.material3.Text(
                        "Delete",
                        color = Pc.Accent,
                        fontSize = androidx.compose.ui.unit.TextUnit(15f, androidx.compose.ui.unit.TextUnitType.Sp),
                        modifier = Modifier.clickable { vm.deleteLayout(name) }.padding(horizontal = 8.dp, vertical = 10.dp),
                    )
                }
            }
            androidx.compose.material3.Text(
                "Save Layout",
                color = Pc.Accent,
                fontSize = androidx.compose.ui.unit.TextUnit(17f, androidx.compose.ui.unit.TextUnitType.Sp),
                modifier = Modifier.fillMaxWidth().height(44.dp).clickable { onAsk(null to vm.nextLayoutName()) }.padding(horizontal = 16.dp, vertical = 12.dp),
            )
        }
    }
}

/**
 * The Tool Belt: a chip for each panel parked on it (a window let go on it), at the floating windows' size. A tap puts one
 * back where it was; dragged off, it floats under the finger until let go. Empty, it says what it is.
 */
@Composable
private fun ToolBelt(vm: CanvasViewModel, belt: io.github.artcraftandroid.procraft.core.BeltShown) {
    val density = LocalDensity.current.density
    val corner = LocalClearance.current.corner
    // where it shows: moved in Edit Layout, upright against a side
    val place = vm.beltPlace(belt)
    val upright = place.vertical
    PartScaled(InterfacePart.FLOATING) {
        val dp = LocalDensity.current.density / LocalPartScale.current
        val at = Modifier.offset { IntOffset((place.x * density).roundToInt(), (place.y * density).roundToInt()) }
        val (w, h) = if (upright) belt.height to belt.width else belt.width to belt.height
        // round, but square on its sides against the window's edges
        val c = barCorners(
            vm.barTouching(io.github.artcraftandroid.procraft.core.LayoutBar.BELT) ?: io.github.artcraftandroid.procraft.core.BarSides(left = false, top = false, right = false, bottom = false),
            18.dp,
        )
        val chips: @Composable (@Composable () -> Unit) -> Unit = { content ->
            val shape = Modifier.size(w.dp, h.dp).clip(androidx.compose.foundation.shape.RoundedCornerShape(c[0], c[1], c[2], c[3])).background(Pc.PopoverFill).keepsItsTouches()
            if (upright) {
                androidx.compose.foundation.layout.Column(at.then(shape), horizontalAlignment = Alignment.CenterHorizontally) { content() }
            } else {
                androidx.compose.foundation.layout.Row(at.then(shape), verticalAlignment = Alignment.CenterVertically) { content() }
            }
        }
        chips {
            if (belt.pieces.isEmpty()) {
                androidx.compose.material3.Text(
                    "Tool Belt",
                    color = Pc.PanelTitle.copy(alpha = 0.5f),
                    fontSize = androidx.compose.ui.unit.TextUnit(13f, androidx.compose.ui.unit.TextUnitType.Sp),
                    maxLines = 1,
                    softWrap = false,
                    modifier = Modifier.padding(horizontal = 16.dp).graphicsLayer { rotationZ = if (upright) -90f else 0f },
                )
            }
            for (piece in belt.pieces) {
                var origin by remember { mutableStateOf(Offset.Zero) }
                Box(
                    (if (upright) Modifier.size(belt.height.dp, 120.dp) else Modifier.width(120.dp).fillMaxHeight())
                        .onGloballyPositioned { origin = it.positionInRoot() - corner }
                        .pointerInput(piece) { detectTapGestures { vm.unparkPiece(piece) } }
                        .pointerInput(piece) {
                            var total = Offset.Zero
                            var off = false
                            detectDragGestures(
                                onDragStart = {
                                    total = Offset.Zero
                                    off = false
                                },
                                onDragEnd = { if (off) vm.letGoPiece() },
                                onDragCancel = { if (off) vm.letGoPiece() },
                            ) { change, delta ->
                                change.consume()
                                total += delta
                                val p = (origin + change.position) / dp
                                when {
                                    off -> vm.followTorn(p.x, p.y)
                                    total.getDistance() > 26.dp.toPx() -> {
                                        off = true
                                        vm.unparkAt(piece, p.x, p.y)
                                    }
                                }
                            }
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    androidx.compose.material3.Text(
                        pieceTitle(piece),
                        color = Pc.PanelTitle,
                        fontSize = androidx.compose.ui.unit.TextUnit(13f, androidx.compose.ui.unit.TextUnitType.Sp),
                        maxLines = 1,
                        softWrap = false,
                        modifier = Modifier.requiredWidth(116.dp).graphicsLayer { rotationZ = if (upright) -90f else 0f },
                        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                    )
                }
            }
        }
    }
}
