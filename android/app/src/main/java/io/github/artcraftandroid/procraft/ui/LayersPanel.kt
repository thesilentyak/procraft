package io.github.artcraftandroid.procraft.ui

import android.graphics.BlurMaskFilter
import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.MutatePriority
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.layout.requiredWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.lazy.LazyItemScope
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.ClipOp
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import androidx.compose.ui.zIndex
import io.github.artcraftandroid.procraft.canvas.CanvasViewModel
import io.github.artcraftandroid.procraft.core.LayerInfo
import io.github.artcraftandroid.procraft.core.LayerOption
import io.github.artcraftandroid.procraft.core.LayerType
import io.github.artcraftandroid.procraft.core.RowDrop
import io.github.artcraftandroid.procraft.core.blendCoast
import io.github.artcraftandroid.procraft.core.blendCoastAt
import io.github.artcraftandroid.procraft.core.blendTap
import io.github.artcraftandroid.procraft.core.blendTapAt
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

private val LayersWidth = 384.dp
private val RowHeight = 69.dp
/** A layer with a mask, and the mask, are a little shorter. */
private val MaskedRowHeight = 65.dp
private val RowGap = 1.dp
/** A dragged row this near an end of the list scrolls it, at most this far a second. */
private val DragEdge = 56.dp
private val DragScroll = 480.dp
/** A dragged row this far out past the panel's side is lifted out of it, to drop elsewhere. */
private val LiftReach = 24.dp
private val ListCorner = 8.dp
/** A group's row height, and how far in each level of group sits. */
private val GroupHeight = 44.dp
private val GroupIndent = 21.dp
private val GroupLine = Color(0xFF4E4E4E)
/**
 * A closed group's row is taller: its line (20 under the row's top and 20 over its bottom) opens
 * into a window 4 in from them, on its layers' rows drawn small; opening or closing, over this long.
 */
private val GroupClosedHeight = 82.dp
private val GroupLineInset = 20.dp
private val GroupWindowInset = 4.dp
private const val GROUP_TURN_MS = 150
/** The rows' width in the list, which a closed group's window draws its layers' rows to fit. */
private val ListWidth = 360.dp
/** A row's thumbnail (a group's line) held over by a dragged row long enough to take it. */
private val HeldOutline = Color.White
private val MaskOwnerRow = Color(0xFF2C5281)
/** Swiping a row left shows these buttons, each this wide, this far apart. */
private val SwipeButton = 82.dp
private val SwipeGap = 2.dp
private val SwipeFill = Color(0xFF3F3F3F)
private val SwipeInk = Color(0xFFC1C1C1)
private val SwipeDelete = Color(0xFFFF473D)
/** How far right a row slides to be picked. */
private val PickReach = 72.dp
/** A picked row (the current one stays blue). */
private val PickedRow = Color(0xFF2C5281)
private val LockedLetter = Color(0xFF808080)
/** The options never go higher than this from the window's top (the top bar and a gap). */
private val OptionsTop = 57.dp

/**
 * The layers panel: top layer first. Tap a layer to make it current, tap the current one for its
 * options, tap the blend letter for blend mode and opacity, hold and drag to reorder (out past the
 * panel's side, it is lifted out to drop on the canvas or, through the gallery, into another
 * artwork), swipe left for lock, duplicate and delete, swipe right to pick it to act on with the
 * current one (the title's menu then groups, merges, shares, duplicates or deletes them).
 */
@Composable
fun LayersPanel(vm: CanvasViewModel, arrowX: Dp, modifier: Modifier = Modifier, floating: FloatingFrame? = null) {
    var options by remember { mutableStateOf<ULong?>(null) }
    var blendFor by remember { mutableStateOf<ULong?>(null) }
    // blend modes opening under a layer: the rows under it go at once and the panel grows over where
    // they were; closing, it shrinks back to them and they come back a moment later. [blendAt] is
    // the row they open under (-1 when nothing is moving), [belowShown] the rows under it.
    var blendAt by remember { mutableIntStateOf(-1) }
    var belowShown by remember { mutableFloatStateOf(1f) }
    val blendScope = rememberCoroutineScope()
    var blendSettled by remember { mutableStateOf<kotlinx.coroutines.Job?>(null) }
    fun changeBlend(id: ULong?) {
        if (id == blendFor) return
        val shown = vm.layers.filter { !vm.hidden(it) }
        val at = listOfNotNull(blendFor, id).map { b -> shown.indexOfFirst { it.id == b } }.filter { it >= 0 }.minOrNull() ?: -1
        val opening = id != null
        blendFor = id
        blendAt = at
        belowShown = 0f
        blendSettled?.cancel()
        blendSettled = blendScope.launch {
            kotlinx.coroutines.delay(if (opening) Motion.BLEND_OPEN_MS else Motion.BLEND_CLOSE_MS + Motion.BLEND_ROWS_WAIT_MS)
            androidx.compose.animation.core.animate(0f, 1f, animationSpec = Motion.blendRowsBack()) { v, _ -> belowShown = v }
            blendAt = -1
        }
    }
    var renaming by remember { mutableStateOf<ULong?>(null) }
    var swiped by remember { mutableStateOf<ULong?>(null) }
    var titleMenu by remember { mutableStateOf(false) }
    // the + offering plugins' kinds of layer beside a plain one
    var addMenu by remember { mutableStateOf(false) }
    // the filter layers' list the +'s Filter Layer opens
    var filterList by remember { mutableStateOf(false) }
    var deletingChosen by remember { mutableStateOf(false) }
    val context = androidx.compose.ui.platform.LocalContext.current
    val scope = rememberCoroutineScope()
    var dragId by remember { mutableStateOf<ULong?>(null) }
    var dragOffset by remember { mutableFloatStateOf(0f) }
    // the rows that go with the dragged one (its mask, a group's layers)
    var dragUnit by remember { mutableStateOf(emptySet<ULong>()) }
    // where the dragged row lands if let go now, and the row it has been held over long enough to
    // go onto it (its thumbnail outlined)
    var dropAt by remember { mutableStateOf<RowDrop?>(null) }
    var heldOver by remember { mutableStateOf<ULong?>(null) }
    // where each row is in the window: for dropping a dragged row and placing the options
    val rows = remember { mutableMapOf<ULong, Rect>() }
    // where layers held out of a panel drop: on its rows, within its box
    DisposableEffect(rows) {
        vm.layerRows = rows
        onDispose {
            // (a panel torn off shows floating as it goes from where it hung: that one's rows stay)
            if (vm.layerRows === rows) {
                vm.layerRows = emptyMap()
                vm.layersBox = null
            }
        }
    }

    // a model's meshes are pictured as the model is seen when the panel opens
    if (vm.modelOpen) LaunchedEffect(Unit) { vm.refreshSubmeshThumbnails() }
    // floating in a window of its own: as wide as the window, no arrow, a strip over it and under it
    val side = if (floating != null) ArrowSide.NONE else ArrowSide.TOP
    Popover(floating?.width ?: LayersWidth, arrowX, modifier.onGloballyPositioned { vm.layersBox = it.boundsInWindow() }, side = side) {
        floating?.let { FloatingTop(it) }
        // kept while they fade out
        val open = options?.let { id -> vm.layers.firstOrNull { it.id == id }?.let { l -> rows[id]?.let { l to it } } }
        val last = remember { arrayOfNulls<Pair<LayerInfo, Rect>>(1) }
        if (open != null) last[0] = open
        last[0]?.let { (l, row) -> LayerOptions(open != null, vm, l, row.center.y, onRename = { renaming = l.id }, onDone = { options = null }) }
        Box {
            PanelHeader(
                "Layers",
                chevronOn = vm.anyPicked,
                onTitle = { titleMenu = !titleMenu },
                menu = {
                    LayersTitleMenu(titleMenu, vm, onDismiss = { titleMenu = false }, onShare = {
                        scope.launch {
                            val files = vm.chosenLayerFiles()
                            if (files.isNotEmpty()) shareFiles(context, files, "image/png")
                        }
                    }, onDelete = { deletingChosen = true })
                },
            ) {
                val kinds = vm.layerKinds()
                val filters = vm.layerFilters()
                val asks = kinds.isNotEmpty() || filters.isNotEmpty()
                Box(
                    Modifier.padding(top = 12.dp).size(40.dp, 41.dp).clickable { if (!asks) vm.addLayer() else addMenu = true },
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(PcIcons.PlusMedium, contentDescription = "New layer", tint = Pc.PanelGlyph, modifier = Modifier.requiredSize(41.dp))
                    NewLayerMenu(addMenu && asks, kinds, filters.isNotEmpty(), vm, onDismiss = { addMenu = false }, onFilters = { filterList = true })
                }
                if (filterList) {
                    FilterLayerList(filters, onPick = { filterList = false; vm.addFilterLayer(it) }, onClose = { filterList = false })
                }
            }
            // hanging from its button, its grabber tears it off into a window of its own, while a plugin lets it
            if (floating == null && io.github.artcraftandroid.procraft.core.FloatingPanel.LAYERS in Pc.floatingPanels) {
                PanelGrabber(vm, io.github.artcraftandroid.procraft.core.FloatingPanel.LAYERS)
            }
        }
        // a layer added at the top of what shows comes in there, the rows under it sliding down,
        // rather than the list holding its place and leaving the new row out of sight above
        val list = rememberLazyListState()
        val ids = vm.layers.filter { !vm.hidden(it) }.map { it.id }
        val lastIds = remember { arrayOfNulls<List<ULong>>(1) }
        val before = lastIds[0]
        if (before != null && ids != before) {
            val added = ids.indices.firstOrNull { ids[it] !in before }
            if (added != null && added <= list.firstVisibleItemIndex) list.requestScrollToItem(added)
        }
        lastIds[0] = ids
        var listAt by remember { mutableStateOf(Offset.Zero) }
        val listBox = remember { arrayOf(Rect.Zero) }
        // held near an end of the list it scrolls, faster the nearer, the row staying under the finger
        val rowGap = with(LocalDensity.current) { RowGap.toPx() }
        val dragEdge = with(LocalDensity.current) { DragEdge.toPx() }
        val dragScroll = with(LocalDensity.current) { DragScroll.toPx() }
        val liftReach = with(LocalDensity.current) { LiftReach.toPx() }
        LaunchedEffect(dragId) {
            val id = dragId ?: return@LaunchedEffect
            var last = androidx.compose.runtime.withFrameNanos { it }
            // the row under the dragged one's middle, and since when
            var over: ULong? = null
            var overSince = last
            // placed again only when it moves or a hold comes due
            var placedFor: Pair<Float, ULong?>? = null
            while (true) {
                val now = androidx.compose.runtime.withFrameNanos { it }
                val seconds = (now - last) / 1e9f
                last = now
                val y = (rows[id] ?: continue).center.y + dragOffset
                // held over one row a moment, it goes onto it when let go
                val under = rows.entries.firstOrNull { (k, r) -> k !in dragUnit && y >= r.top && y < r.bottom }?.key
                if (under != over) {
                    over = under
                    overSince = now
                }
                heldOver = over.takeIf { (now - overSince) / 1e9f >= vm.rowHoldSeconds }
                if (placedFor != y to heldOver) {
                    placedFor = y to heldOver
                    dropAt = vm.rowDrop(id, rows, y, heldOver)
                }
                val box = listBox[0]
                val towards = when {
                    y < box.top + dragEdge -> (y - box.top - dragEdge) / dragEdge
                    y > box.bottom - dragEdge -> (y - box.bottom + dragEdge) / dragEdge
                    else -> 0f
                }.coerceIn(-1f, 1f)
                if (towards != 0f) dragOffset += list.scrollBy(towards * dragScroll * seconds)
            }
        }
        // two rows being pinched, each following its finger part of the way
        var squeeze by remember { mutableStateOf<Squeeze?>(null) }
        LazyColumn(
            // (floating, it fills its window)
            Modifier.weight(1f, fill = floating != null).padding(horizontal = 12.dp)
                .onGloballyPositioned {
                    listAt = it.positionInWindow()
                    listBox[0] = it.boundsInWindow()
                }
                // two rows pinched together merge with every layer between
                .pinchRows({ listAt }, { y -> rows.entries.firstOrNull { y >= it.value.top && y <= it.value.bottom }?.key }, { squeeze = it }) { a, b ->
                    vm.mergeBetween(a, b)
                },
            state = list,
            verticalArrangement = Arrangement.spacedBy(RowGap),
            contentPadding = PaddingValues(bottom = 12.dp),
        ) {
            // a closed group's layers are left out
            val shown = vm.layers.filter { !vm.hidden(it) }
            // a layer's row: in the list, or on a model under its submesh, indented
            val layerItem: @Composable LazyItemScope.(Int, LayerInfo, Shape, Dp, String?) -> Unit = { index, l, shape, indent, subtitle ->
                val dragging = l.id in dragUnit
                DisposableEffect(l.id) { onDispose { rows.remove(l.id) } }
                // scrolled out of the list as it is dragged, the row stays, its gesture going on
                val pinnable = androidx.compose.ui.layout.LocalPinnableContainer.current
                val pinned = remember { arrayOfNulls<androidx.compose.ui.layout.PinnableContainer.PinnedHandle>(1) }
                val aside by androidx.compose.animation.core.animateFloatAsState(
                    remember(l.id) { androidx.compose.runtime.derivedStateOf { if (dragId == null) 0f else makingWay(l.id, dragUnit, dropAt, rows, rowGap) } }.value,
                    Motion.rowFade(),
                )
                Column(
                    Modifier.animateItem(Motion.rowFade(), if (dragging || blendAt >= 0) null else Motion.rowMove(), Motion.rowFade())
                        .onGloballyPositioned { rows[l.id] = it.boundsInWindow() }
                        .zIndex(if (dragging) 1f else squeeze?.above(l.id) ?: 0f).graphicsLayer {
                            translationY = if (dragging) dragOffset else (squeeze?.offset(l.id) ?: 0f) + aside
                            alpha = if (blendAt in 0 until index) belowShown else 1f
                        }
                        .twoFingers(
                            l.kind == LayerType.PAINT,
                            onTap = {
                                // two fingers tapped on a layer (not a group or mask): its opacity, dragged across the canvas
                                vm.selectLayer(l.id)
                                vm.startLayerOpacity()
                            },
                            // swiped right, its Alpha Lock on or off; held, its contents selected
                            onSwipeRight = { vm.toggleAlphaLock(l) },
                            onHold = { vm.selectLayerContents(l) },
                        )
                        .pointerInput(l.id) {
                            if (l.kind == LayerType.MASK) return@pointerInput
                            detectDragGesturesAfterLongPress(
                                onDragStart = {
                                    dragId = l.id
                                    dragOffset = 0f
                                    dragUnit = vm.movingWith(l) + l.id
                                    dropAt = null
                                    heldOver = null
                                    swiped = null
                                    pinned[0] = pinnable?.pin()
                                },
                                onDrag = { change, amount ->
                                    change.consume()
                                    dragOffset += amount.y
                                    // dragged out past the panel's side, the row (with what goes with it) is
                                    // lifted out, held by the finger from then on wherever it goes
                                    val at = vm.fingersAt[change.id]
                                    val box = vm.layersBox
                                    if (at != null && box != null && (at.x < box.left - liftReach || at.x > box.right + liftReach)) {
                                        val corner = (rows[l.id]?.topLeft ?: at) + Offset(0f, dragOffset)
                                        vm.liftLayers(l, change.id, at, corner)
                                    }
                                },
                                onDragEnd = {
                                    // where its middle came to rest, placed as the core places it
                                    rows[l.id]?.let { vm.dropRow(l, vm.rowDrop(l.id, rows, it.center.y + dragOffset, heldOver)) }
                                    dragId = null
                                    dragOffset = 0f
                                    dragUnit = emptySet()
                                    dropAt = null
                                    heldOver = null
                                    pinned[0]?.release()
                                    pinned[0] = null
                                },
                                onDragCancel = {
                                    dragId = null
                                    dragOffset = 0f
                                    dragUnit = emptySet()
                                    dropAt = null
                                    heldOver = null
                                    pinned[0]?.release()
                                    pinned[0] = null
                                },
                            )
                        },
                ) {
                    val onTap = {
                        swiped = null
                        // on a model the row means the layer itself: its colour, no channel's tile
                        if (vm.modelOpen && vm.channelTileChosen && l.kind != LayerType.GROUP) {
                            vm.leaveChannelTiles()
                            if (!l.current) vm.selectLayer(l.id)
                        } else if (vm.anyPicked) {
                            // picks give way to the layer tapped, the current one too
                            vm.selectLayer(l.id)
                            options = null
                        } else if (l.current) {
                            options = if (options == l.id) null else l.id
                            changeBlend(null)
                        } else {
                            vm.selectLayer(l.id)
                            options = null
                        }
                    }
                    // renamed in place
                    val name: @Composable (Color, Modifier) -> Unit = { ink, m ->
                        if (renaming == l.id) {
                            InlineName(l.name, ink, 17.sp, m) {
                                vm.rename(l, it)
                                renaming = null
                            }
                        } else {
                            Text(l.name, color = ink, fontSize = 17.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = m)
                        }
                    }
                    SwipeRow(vm, l, open = swiped == l.id, enabled = dragId == null, onOpen = { swiped = if (it) l.id else null }, onPick = { vm.pickLayer(l) }) {
                        // held over long enough to go onto it: its thumbnail (a group's line) outlined
                        val held = (dropAt as? RowDrop.Onto)?.target == l.id
                        if (l.kind == LayerType.GROUP) {
                            GroupRow(vm, l, shape, name, onTap, indent, held) { vm.toggleCollapsed(l) }
                        } else LayerRow(
                            vm,
                            l,
                            shape,
                            name,
                            // the current mask's layer, or the current layer's mask
                            owner = vm.layers.any {
                                it.current && (it.kind == LayerType.MASK && it.parent == l.id || l.kind == LayerType.MASK && l.parent == it.id)
                            },
                            onTap = onTap,
                            onBlend = {
                                changeBlend(if (blendFor == l.id) null else l.id)
                                options = null
                            },
                            indent = indent,
                            subtitle = subtitle,
                            // on a model: its channels, and the row giving its blue to the channel painted
                            onCube = if (vm.modelOpen && l.kind != LayerType.MASK) ({ vm.toggleChannelTray(l.id) }) else null,
                            cubeLit = vm.modelOpen && l.current && vm.channelTileChosen,
                            selected = l.current && !(vm.modelOpen && vm.channelTileChosen),
                            held = held,
                        )
                    }
                    if (vm.modelOpen && vm.channelTrayFor == l.id) ChannelTray(vm, l)
                    // the blend modes open under the row, the panel growing as they fade in, and close back
                    androidx.compose.animation.AnimatedVisibility(
                        blendFor == l.id,
                        // held at the top, uncovered as the panel grows and covered as it shrinks
                        enter = androidx.compose.animation.expandVertically(Motion.blendOpenSize(), expandFrom = Alignment.Top) +
                            androidx.compose.animation.fadeIn(Motion.blendOpen()),
                        exit = androidx.compose.animation.shrinkVertically(Motion.blendCloseSize(), shrinkTowards = Alignment.Top) +
                            androidx.compose.animation.fadeOut(Motion.blendClose()),
                    ) { BlendEditor(vm, l, open = blendFor == l.id) }
                }
            }
            if (!vm.modelOpen) {
                itemsIndexed(shown, key = { _, l -> l.id.toLong() }) { index, l ->
                    // the list's ends round; the rows between are square
                    val shape = RoundedCornerShape(
                        topStart = if (index == 0) ListCorner else 0.dp,
                        topEnd = if (index == 0) ListCorner else 0.dp,
                        bottomStart = if (index == shown.size - 1) ListCorner else 0.dp,
                        bottomEnd = if (index == shown.size - 1) ListCorner else 0.dp,
                    )
                    layerItem(index, l, shape, 0.dp, null)
                }
            } else {
                // on a model: each texture set headed by its name, its meshes under it; the open
                // mesh's texture set's layers under that, shared by every mesh in the set
                vm.textureSets.forEachIndexed { set, setName ->
                    val meshes = vm.submeshes.withIndex().filter { it.value.textureSet.toInt() == set }
                    if (meshes.isEmpty()) return@forEachIndexed
                    item(key = "set $set") { SetHeader(setName, first = set == 0) }
                    meshes.forEachIndexed { k, (m, info) ->
                        val open = vm.openSubmesh == m
                        val lastMesh = k == meshes.size - 1
                        val layersUnder = open && set == vm.textureSet && shown.isNotEmpty()
                        item(key = "mesh $m") {
                            SubmeshRow(
                                vm,
                                m,
                                info.name,
                                open = layersUnder,
                                shape = RoundedCornerShape(
                                    topStart = if (k == 0) ListCorner else 0.dp,
                                    topEnd = if (k == 0) ListCorner else 0.dp,
                                    bottomStart = if (lastMesh && !layersUnder) ListCorner else 0.dp,
                                    bottomEnd = if (lastMesh && !layersUnder) ListCorner else 0.dp,
                                ),
                            )
                        }
                        if (layersUnder) {
                            itemsIndexed(shown, key = { _, l -> "layer ${l.id}" }) { index, l ->
                                val end = lastMesh && index == shown.size - 1
                                val shape = RoundedCornerShape(bottomStart = if (end) ListCorner else 0.dp, bottomEnd = if (end) ListCorner else 0.dp)
                                layerItem(index, l, shape, GroupIndent, if (meshes.size > 1) "Shared" else null)
                            }
                        }
                    }
                }
            }
            // it moves down and up with the rows above it
            if (!vm.modelOpen) item(key = "background") {
                Box(
                    Modifier.animateItem(null, if (dragId != null || blendAt >= 0) null else Motion.rowMove(), null)
                        .padding(top = 10.dp).graphicsLayer { alpha = if (blendAt >= 0) belowShown else 1f },
                ) {
                    Column {
                        BackgroundRow(vm)
                        PluginLayersRows(vm)
                    }
                }
            }
        }
        floating?.let { FloatingBottom(it) }
    }
    // Delete on a locked layer's row: why not, OK alone
    Alert(
        if (vm.lockedDeleteTold) Unit else null,
        title = { "Locked layer selection" },
        message = { "A locked layer can’t be deleted. Unlock it first." },
        action = "OK",
        icon = PcIcons.PageLocked,
        destructive = false,
        alone = true,
        onCancel = { vm.lockedDeleteTold = false },
        onAction = { vm.lockedDeleteTold = false },
    )
    // the title menu's Delete asks first
    Alert(
        if (deletingChosen) Unit else null,
        title = { "Delete these layers?" },
        message = { "Undo brings them back." },
        action = "Delete",
        icon = PcIcons.Trash,
        onCancel = { deletingChosen = false },
        onAction = {
            // as measured, a locked one among them stops it, said why
            if (vm.chosenLocked()) vm.lockedDeleteTold = true else vm.deletePicked()
            deletingChosen = false
        },
    )
}

/**
 * The title's menu, under it: Select All; Group and Merge (with layers picked); Share and
 * Duplicate; Delete.
 */
@Composable
private fun LayersTitleMenu(open: Boolean, vm: CanvasViewModel, onDismiss: () -> Unit, onShare: () -> Unit, onDelete: () -> Unit) {
    val picked = vm.anyPicked
    // mostly over the panel, whose rows the canvas's frosting doesn't blur: nearly solid
    LibraryMenu(
        open,
        androidx.compose.ui.graphics.TransformOrigin(0.5f, 0f),
        MenuPlace { a, size -> IntOffset(a.center.x - size.width / 2, a.bottom) },
        onDismiss,
        overPanel = true,
    ) {
        MenuItem(PcIcons.MenuSelectAll, "Select All") {
            onDismiss()
            vm.pickAllLayers()
        }
        MenuDivider()
        if (picked) {
            MenuItem(PcIcons.MenuGroup, "Group") {
                onDismiss()
                vm.groupPicked()
            }
            MenuItem(PcIcons.MenuMerge, "Merge") {
                onDismiss()
                vm.mergePicked()
            }
            MenuDivider()
        }
        MenuItem(PcIcons.MenuShare, "Share") {
            onDismiss()
            onShare()
        }
        MenuItem(PcIcons.MenuDuplicate, "Duplicate") {
            onDismiss()
            vm.duplicatePicked()
        }
        MenuDivider()
        MenuItem(PcIcons.MenuTrash, "Delete", Pc.MenuDanger) {
            onDismiss()
            onDelete()
        }
    }
}

/**
 * The + with plugins' kinds of layer on: a plain layer, or one of each kind, named as the plugin names it; with
 * filter layers offered ([filters]), a Filter Layer that lists them ([onFilters]).
 */
@Composable
private fun NewLayerMenu(
    open: Boolean,
    kinds: List<io.github.artcraftandroid.procraft.core.PluginKind>,
    filters: Boolean,
    vm: CanvasViewModel,
    onDismiss: () -> Unit,
    onFilters: () -> Unit,
) {
    LibraryMenu(
        open,
        androidx.compose.ui.graphics.TransformOrigin(1f, 0f),
        MenuPlace { a, size -> IntOffset(a.right - size.width, a.bottom) },
        onDismiss,
        overPanel = true,
    ) {
        MenuItem(PcIcons.MenuNewLayer, "Layer") {
            onDismiss()
            vm.addLayer()
        }
        for (kind in kinds) {
            // beside its badge, as its layers' thumbnails show it (a plug when it draws none)
            val badge = remember(kind) { io.github.artcraftandroid.procraft.core.pluginKindBadge(kind) }
            val ink = Pc.MenuInk
            MenuItem(kind.label, ink, null, onClick = {
                onDismiss()
                vm.addPluginLayer(kind.key)
            }) {
                androidx.compose.foundation.Canvas(Modifier.size(MenuBadgeWidth, MenuBadgeHeight)) { badgeDrawing(badge, ink, MenuBadgeLine.toPx()) }
            }
        }
        if (filters) {
            val badge = remember { io.github.artcraftandroid.procraft.core.filterLayerBadge() }
            val ink = Pc.MenuInk
            MenuItem("Filter Layer", ink, null, onClick = {
                onDismiss()
                onFilters()
            }) {
                androidx.compose.foundation.Canvas(Modifier.size(MenuBadgeWidth, MenuBadgeHeight)) { badgeDrawing(badge, ink, MenuBadgeLine.toPx()) }
            }
        }
    }
}

/** A kind's badge as a menu row's glyph: its box 1.6 times a thumbnail's (18 by 14), its lines as thick as the menu glyphs'. */
private val MenuBadgeWidth = 28.8.dp
private val MenuBadgeHeight = 22.4.dp
private val MenuBadgeLine = 2.4.dp

/**
 * How far row [id] stands aside while the rows of [unit] are dragged: the rows between where they
 * were and where they land ([place]) take the room they left, so the gap shows where they will
 * land; held onto a row ([RowDrop.Onto]), or over their own place, none moves.
 */
internal fun makingWay(id: ULong, unit: Set<ULong>, place: RowDrop?, rows: Map<ULong, Rect>, gap: Float): Float {
    if (id in unit) return 0f
    val row = rows[id] ?: return 0f
    val moved = unit.mapNotNull { rows[it] }
    if (moved.isEmpty()) return 0f
    val (top, bottom) = moved.minOf { it.top } to moved.maxOf { it.bottom }
    val room = bottom - top + gap
    // the line between the rows they land between
    val slot = when (place) {
        is RowDrop.Above -> rows[place.target]?.top ?: return 0f
        is RowDrop.Below -> rows[place.target]?.bottom ?: return 0f
        is RowDrop.Bottom -> Float.POSITIVE_INFINITY
        else -> return 0f
    }
    return when {
        row.bottom <= top && row.top >= slot -> room
        row.top >= bottom && row.bottom <= slot -> -room
        else -> 0f
    }
}

/**
 * A row that slides left to show Lock, Duplicate (not for a mask) and Delete, or a little right to
 * pick it ([onPick]), springing back.
 */
@Composable
private fun SwipeRow(
    vm: CanvasViewModel,
    l: LayerInfo,
    open: Boolean,
    enabled: Boolean,
    onOpen: (Boolean) -> Unit,
    onPick: () -> Unit,
    content: @Composable () -> Unit,
) {
    val buttons = if (l.kind == LayerType.MASK) 2 else 3
    // the row slides 6 past the buttons
    val reach = with(LocalDensity.current) { (SwipeButton * buttons + SwipeGap * (buttons - 1) + 6.dp).toPx() }
    val pickReach = with(LocalDensity.current) { PickReach.toPx() }
    val offset = remember { Animatable(0f) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(open, reach) { offset.animateTo(if (open) -reach else 0f, Motion.swipeSettle()) }
    // the list only clips top and bottom; a slid row stops at its edge
    Box(Modifier.clipToBounds()) {
        if (offset.value < 0f) {
            Row(Modifier.matchParentSize(), horizontalArrangement = Arrangement.spacedBy(SwipeGap, Alignment.End)) {
                SwipeAction(if (l.locked) "Unlock" else "Lock", SwipeFill, SwipeInk) {
                    vm.toggleLocked(l)
                    onOpen(false)
                }
                if (l.kind != LayerType.MASK) SwipeAction("Duplicate", SwipeFill, SwipeInk) {
                    vm.duplicate(l)
                    onOpen(false)
                }
                // the only layer can't go: it is cleared instead
                val alone = vm.layers.count { it.kind != LayerType.MASK } == 1
                SwipeAction(if (alone) "Clear" else "Delete", SwipeDelete, Color.White) {
                    if (alone) vm.clear(l) else vm.deleteAsking(l)
                    onOpen(false)
                }
            }
        }
        Box(
            Modifier.offset { IntOffset(offset.value.roundToInt(), 0) }.pointerInput(l.id, open, enabled, reach) {
                if (!enabled) return@pointerInput
                detectHorizontalDragGestures(
                    onHorizontalDrag = { change, amount ->
                        change.consume()
                        // closed, it goes right as far as picking takes
                        scope.launch { offset.snapTo((offset.value + amount).coerceIn(-reach, if (open) 0f else pickReach)) }
                    },
                    onDragEnd = {
                        if (offset.value > pickReach / 2f) {
                            onPick()
                            scope.launch { offset.animateTo(0f, Motion.swipeSettle()) }
                            return@detectHorizontalDragGestures
                        }
                        val opened = offset.value < -reach / 2f
                        if (opened == open) scope.launch { offset.animateTo(if (open) -reach else 0f, Motion.swipeSettle()) } else onOpen(opened)
                    },
                    onDragCancel = { scope.launch { offset.animateTo(if (open) -reach else 0f, Motion.swipeSettle()) } },
                )
            },
        ) { content() }
    }
}

@Composable
private fun SwipeAction(label: String, fill: Color, ink: Color, onClick: () -> Unit) {
    Box(
        Modifier.width(SwipeButton).fillMaxHeight().clip(RoundedCornerShape(8.dp)).background(fill).clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = ink, fontSize = 15.sp, fontWeight = FontWeight.Medium, maxLines = 1)
    }
}

/** A panel's title row, [height] tall: the title [start] in, its menu chevron, and [action] [end] from the right. */
@Composable
internal fun PanelHeader(
    title: String,
    start: Dp = 24.dp,
    end: Dp = 18.dp,
    height: Dp = 64.dp,
    chevron: Boolean = true,
    chevronOn: Boolean = false,
    onTitle: (() -> Unit)? = null,
    menu: @Composable () -> Unit = {},
    action: @Composable () -> Unit,
) {
    Box(Modifier.fillMaxWidth().height(height)) {
        Row(
            Modifier.align(Alignment.TopStart).padding(start = start, top = 19.dp)
                .then(if (onTitle != null) Modifier.clickable(interactionSource = null, indication = null, onClick = onTitle) else Modifier),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(title, color = Pc.PanelTitle, fontSize = 24.sp, fontWeight = FontWeight.Bold, maxLines = 1)
            menu()
            if (chevron) {
                Box(
                    Modifier.padding(start = 10.dp).offset(y = (-1.5).dp).size(17.dp).clip(CircleShape)
                        .background(if (chevronOn) Pc.RowSelected else Pc.ChevronDisc),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(PcIcons.ChevronDown, contentDescription = null, tint = Pc.ChevronGlyph, modifier = Modifier.size(13.dp))
                }
            }
        }
        Box(Modifier.align(Alignment.TopEnd).padding(end = end)) { action() }
    }
}

/** A row's name, where it always is; locked, a small padlock in the gap before it. */
@Composable
private fun LockedName(l: LayerInfo, ink: Color, name: @Composable (Color, Modifier) -> Unit, modifier: Modifier) {
    Box(modifier, contentAlignment = Alignment.CenterStart) {
        // the padlock's glyph ends 12 short of the name
        if (l.locked) Icon(PcIcons.LockRow, contentDescription = "Locked", tint = ink, modifier = Modifier.offset(x = 3.dp).size(17.dp))
        name(ink, Modifier.padding(start = 29.dp, end = 6.dp))
    }
}

/**
 * A group's row: where a thumbnail would be, a line while it is open or, closed (the row taller),
 * a window on its layers' rows drawn small; its name, its open or closed arrow and its box. Held
 * over by a dragged row long enough to take it ([held]), the line or window is outlined.
 */
@Composable
private fun GroupRow(
    vm: CanvasViewModel,
    l: LayerInfo,
    shape: Shape,
    name: @Composable (Color, Modifier) -> Unit,
    onTap: () -> Unit,
    indent: Dp,
    held: Boolean,
    onToggle: () -> Unit,
) {
    val height by androidx.compose.animation.core.animateDpAsState(
        if (l.collapsed) GroupClosedHeight else GroupHeight,
        androidx.compose.animation.core.tween(GROUP_TURN_MS, easing = androidx.compose.animation.core.LinearEasing),
        label = "group row",
    )
    // 0 open, 1 closed: the line's 20 above and below become 4, the line the window between
    val closed = ((height - GroupHeight) / (GroupClosedHeight - GroupHeight)).coerceIn(0f, 1f)
    val inset = androidx.compose.ui.unit.lerp(GroupLineInset, GroupWindowInset, closed)
    val window = RoundedCornerShape(androidx.compose.ui.unit.lerp(2.dp, 4.dp, closed))
    Row(
        Modifier.fillMaxWidth().height(height).clip(shape)
            .background(if (l.current) Pc.RowSelected else if (l.picked) PickedRow else Pc.LayerRow)
            .labelled(l)
            .clickable(onClick = onTap)
            .padding(start = indent + GroupIndent * l.depth.toInt()),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val ink = if (l.current || l.picked) Color.White else Pc.PanelTitle
        Box(
            Modifier.padding(start = 4.dp).size(93.dp, height - inset * 2).clip(window).background(GroupLine)
                .then(if (held) Modifier.border(2.dp, HeldOutline, window) else Modifier),
        ) {
            if (closed > 0f) GroupContents(vm, l)
        }
        LockedName(l, ink, name, Modifier.weight(1f))
        Box(Modifier.offset(x = 8.dp).size(44.dp, GroupHeight).clickable(onClick = onToggle), contentAlignment = Alignment.Center) {
            Icon(if (l.collapsed) PcIcons.ChevronRight else PcIcons.ChevronDown, contentDescription = if (l.collapsed) "Open" else "Close", tint = ink, modifier = Modifier.size(16.dp))
        }
        Box(Modifier.size(44.dp, GroupHeight).visibilityTouch(vm, l), contentAlignment = Alignment.Center) {
            CheckBox(l.visible && !l.soloedOut, if (l.current || l.picked) Color.White else Pc.CheckFill)
        }
        Spacer(Modifier.width(8.dp))
    }
}

/**
 * A closed group's layers' rows as the panel would list them open, drawn small to fit its window's
 * width less 1 on each side, from its top: their fills, thumbnails, names, blend letters and boxes.
 */
@Composable
private fun GroupContents(vm: CanvasViewModel, group: LayerInfo) {
    // a group closed inside it keeps its own layers out
    val inside = vm.layers.filter { l ->
        vm.isInside(l, group.id) && vm.layers.none { it.kind == LayerType.GROUP && it.collapsed && it.id != group.id && vm.isInside(l, it.id) && vm.isInside(it, group.id) }
    }
    androidx.compose.foundation.layout.BoxWithConstraints(Modifier.fillMaxSize()) {
        val scale = (maxWidth - 2.dp) / ListWidth
        Column(
            Modifier.wrapContentSize(Alignment.TopStart, unbounded = true).requiredWidth(ListWidth)
                .graphicsLayer {
                    scaleX = scale
                    scaleY = scale
                    transformOrigin = androidx.compose.ui.graphics.TransformOrigin(0f, 0f)
                    translationX = 1.dp.toPx()
                    translationY = 1.dp.toPx()
                },
            verticalArrangement = Arrangement.spacedBy(RowGap),
        ) {
            inside.forEach { MiniRow(vm, it, (it.depth.toInt() - group.depth.toInt() - 1).coerceAtLeast(0)) }
        }
    }
}

/** A row of a closed group's window, as its row in the list looks, without its controls. */
@Composable
private fun MiniRow(vm: CanvasViewModel, l: LayerInfo, depth: Int) {
    val mask = l.kind == LayerType.MASK
    val masked = vm.hasMask(l)
    val group = l.kind == LayerType.GROUP
    val ink = if (l.current || l.picked) Color.White else Pc.PanelTitle
    Row(
        Modifier.fillMaxWidth().height(if (group) GroupHeight else if (mask || masked) MaskedRowHeight else RowHeight)
            .background(if (l.current) Pc.RowSelected else if (l.picked) PickedRow else Pc.LayerRow)
            .labelled(l)
            .padding(start = GroupIndent * depth),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (group) {
            Box(Modifier.padding(start = 4.dp).size(93.dp, 4.dp).clip(RoundedCornerShape(2.dp)).background(GroupLine))
        } else {
            val corner = 4.dp
            val thumbShape = when {
                mask -> RoundedCornerShape(topStart = corner, topEnd = corner)
                masked -> RoundedCornerShape(bottomStart = corner, bottomEnd = corner)
                else -> RoundedCornerShape(corner)
            }
            Box(Modifier.padding(start = 4.dp).offset(y = if (mask) 2.dp else if (masked) (-2).dp else 0.dp)) { Thumb(vm, l, thumbShape) }
        }
        Text(l.name, color = ink, fontSize = 17.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f).padding(start = 29.dp, end = 6.dp))
        if (group) {
            Box(Modifier.offset(x = 8.dp).size(44.dp, GroupHeight), contentAlignment = Alignment.Center) {
                Icon(if (l.collapsed) PcIcons.ChevronRight else PcIcons.ChevronDown, contentDescription = null, tint = ink, modifier = Modifier.size(16.dp))
            }
        } else if (mask) {
            Spacer(Modifier.width(36.dp))
        } else {
            Box(Modifier.offset(x = 8.dp).size(44.dp, 47.dp), contentAlignment = Alignment.Center) {
                Text(blendCode(vm, l.blend), color = if (l.locked && !l.current && !l.picked) LockedLetter else ink, fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
            }
        }
        Box(Modifier.size(44.dp, 47.dp), contentAlignment = Alignment.Center) {
            CheckBox(l.visible && !l.soloedOut, if (l.current || l.picked) Color.White else Pc.CheckFill)
        }
        Spacer(Modifier.width(8.dp))
    }
}

@Composable
private fun LayerRow(
    vm: CanvasViewModel,
    l: LayerInfo,
    shape: Shape,
    name: @Composable (Color, Modifier) -> Unit,
    owner: Boolean,
    onTap: () -> Unit,
    onBlend: () -> Unit,
    indent: Dp = 0.dp,
    subtitle: String? = null,
    onCube: (() -> Unit)? = null,
    cubeLit: Boolean = false,
    selected: Boolean = l.current,
    held: Boolean = false,
) {
    val mask = l.kind == LayerType.MASK
    val masked = vm.hasMask(l)
    Row(
        Modifier.fillMaxWidth().height(if (mask || masked) MaskedRowHeight else RowHeight).clip(shape)
            // the layer a selected mask belongs to shows a darker blue
            .background(if (selected) Pc.RowSelected else if (l.picked) PickedRow else if (owner) MaskOwnerRow else Pc.LayerRow)
            .labelled(l)
            .clickable(onClick = onTap)
            .padding(start = indent + (if (l.clipped) 18.dp else 0.dp) + GroupIndent * l.depth.toInt()),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val ink = if (selected || l.picked) Color.White else Pc.PanelTitle
        if (l.clipped) Icon(PcIcons.Clip, contentDescription = "Clipped", tint = ink, modifier = Modifier.size(16.dp))
        // a mask's thumbnail and its layer's join into one, rounded only at the outer corners
        val corner = 4.dp
        val thumbShape = when {
            mask -> RoundedCornerShape(topStart = corner, topEnd = corner)
            masked -> RoundedCornerShape(bottomStart = corner, bottomEnd = corner)
            else -> RoundedCornerShape(corner)
        }
        val shift = when {
            mask -> 2.dp
            masked -> (-2).dp
            else -> 0.dp
        }
        Box(Modifier.padding(start = 4.dp).offset(y = shift)) { Thumb(vm, l, thumbShape, held) }
        // what the layer is, under its name (which stays in the middle): the reference layer, one
        // drawing assist is on for, a private one, a model's layer shared by its texture set's meshes
        val tag = when {
            l.reference && l.assisted -> "Assisted Reference"
            l.reference -> subtitle?.let { "$it Reference" } ?: "Reference"
            l.control == io.github.artcraftandroid.procraft.core.LayerControl.LINE_ART -> "Line art control"
            l.control == io.github.artcraftandroid.procraft.core.LayerControl.SCRIBBLE -> "Scribble control"
            l.control == io.github.artcraftandroid.procraft.core.LayerControl.DEPTH -> "Depth control"
            subtitle != null -> subtitle
            l.assisted -> "Assisted"
            l.private -> "Private"
            else -> null
        }
        Box(Modifier.weight(1f).fillMaxHeight(), contentAlignment = Alignment.CenterStart) {
            LockedName(l, ink, name, Modifier)
            if (tag != null) {
                Text(tag, color = ink, fontSize = 13.sp, maxLines = 1, modifier = Modifier.offset(y = 17.5.dp).padding(start = 29.dp))
            }
        }
        // a model's layer: the button that shows its channels, blue while one of them is painted;
        // laid out narrower than it is touched (36 from the blend letter, as before), so the name
        // keeps its room up to the icon
        if (onCube != null) {
            Spacer(Modifier.width(10.dp))
            Box(Modifier.width(12.dp).height(47.dp), contentAlignment = Alignment.Center) {
                Box(Modifier.requiredSize(44.dp, 47.dp).clickable(onClick = onCube), contentAlignment = Alignment.Center) {
                    Icon(PcIcons.Tab3D, contentDescription = "Channels", tint = if (cubeLit) Pc.Accent else ink, modifier = Modifier.requiredSize(48.dp))
                }
            }
        }
        // the blend and visibility buttons overlap by 8, as measured; a mask has no blend
        if (mask) {
            Spacer(Modifier.width(36.dp))
        } else {
            Box(Modifier.offset(x = 8.dp).size(44.dp, 47.dp).clickable(onClick = onBlend), contentAlignment = Alignment.Center) {
                // a locked layer's mode is greyed
                val letter = if (l.locked && !l.current && !l.picked) LockedLetter else ink
                Text(blendCode(vm, l.blend), color = letter, fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
            }
        }
        Box(Modifier.size(44.dp, 47.dp).visibilityTouch(vm, l), contentAlignment = Alignment.Center) {
            CheckBox(l.visible && !l.soloedOut, if (selected || l.picked) Color.White else Pc.CheckFill)
        }
        Spacer(Modifier.width(8.dp))
    }
}

private val TrayFill = Color(0xFF303032)
private val TileFill = Color(0xFF3B3B3D)

/**
 * A model's layer's channels under its row: Color, Roughness and Metallic, each its picture of
 * the layer; the one painted on (of the current layer) blue. A tap paints on it.
 */
@Composable
@OptIn(ExperimentalFoundationApi::class)
private fun ChannelTray(vm: CanvasViewModel, l: LayerInfo) {
    Row(Modifier.fillMaxWidth().height(159.dp).background(TrayFill).padding(4.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        listOf("Color", "Roughness", "Metallic").forEachIndexed { c, label ->
            val chosen = l.current && vm.channelTileChosen && vm.modelChannel == c
            // held, a picture replaces what the channel holds
            val replace = androidx.activity.compose.rememberLauncherForActivityResult(
                androidx.activity.result.contract.ActivityResultContracts.PickVisualMedia(),
            ) { uri -> uri?.let { vm.replaceMaterialMap(l, c, it) } }
            Column(
                Modifier.weight(1f).fillMaxHeight().clip(RoundedCornerShape(6.dp)).background(if (chosen) Pc.Accent else TileFill)
                    .combinedClickable(
                        onLongClick = {
                            val images = androidx.activity.result.contract.ActivityResultContracts.PickVisualMedia.ImageOnly
                            replace.launch(androidx.activity.result.PickVisualMediaRequest(images))
                        },
                    ) {
                        if (!l.current) vm.selectLayer(l.id)
                        vm.chooseChannelTile(c)
                    }
                    .padding(6.dp),
            ) {
                Box(Modifier.fillMaxWidth().height(102.dp).clip(RoundedCornerShape(3.dp)).background(Pc.ThumbBackground)) {
                    vm.channelThumbnails[c to l.id]?.let { Image(it, contentDescription = null, contentScale = ContentScale.Fit, modifier = Modifier.fillMaxSize()) }
                }
                Text(label, color = if (chosen) Color.White else Pc.PanelTitle, fontSize = 15.sp, maxLines = 1, modifier = Modifier.padding(start = 2.dp, top = 9.dp))
            }
        }
    }
}

/** A layer's visibility box: filled with a check when shown, an outline when hidden. */
@Composable
private fun CheckBox(checked: Boolean, fill: Color) {
    val shape = RoundedCornerShape(5.dp)
    val tick = androidx.compose.ui.graphics.vector.rememberVectorPainter(PcIcons.Tick)
    Box(
        Modifier.size(18.dp)
            .graphicsLayer { compositingStrategy = androidx.compose.ui.graphics.CompositingStrategy.Offscreen }
            .clip(shape)
            .then(if (checked) Modifier.background(fill) else Modifier.border(1.5.dp, fill, shape))
            .drawWithContent {
                drawContent()
                if (!checked) return@drawWithContent
                // the tick is cut out of the box: the row shows through it
                val side = 14.dp.toPx()
                val inset = (size.width - side) / 2f
                val cut = androidx.compose.ui.graphics.Paint().apply { blendMode = androidx.compose.ui.graphics.BlendMode.DstOut }
                drawContext.canvas.saveLayer(Rect(Offset.Zero, size), cut)
                translate(inset, inset) { with(tick) { draw(Size(side, side)) } }
                drawContext.canvas.restore()
            },
    )
}

@Composable
private fun Thumb(vm: CanvasViewModel, l: LayerInfo, shape: Shape, held: Boolean = false) {
    Box(
        Modifier.size(width = 93.dp, height = 61.dp).clip(shape)
            // held over by a dragged row long enough to take it
            .then(if (held) Modifier.border(2.dp, HeldOutline, shape) else Modifier)
            // a mask's thumbnail sits on white
            .then(
                when {
                    l.alphaLock -> Modifier.drawBehind { checker() }
                    l.kind == LayerType.MASK -> Modifier.background(Color.White)
                    else -> Modifier.background(Pc.ThumbBackground)
                },
            ),
        contentAlignment = Alignment.Center,
    ) {
        // the thumbnail is what is drawn, fitted; small drawings show their pixels; a text layer's
        // is its glyph
        if (l.text) {
            Icon(PcIcons.TextLayer, contentDescription = null, tint = Pc.TextLayerGlyph, modifier = Modifier.fillMaxSize())
        } else {
            vm.thumbnails[l.id]?.let {
                Image(it, contentDescription = null, contentScale = ContentScale.Fit, filterQuality = FilterQuality.None, modifier = Modifier.fillMaxSize())
            }
        }
        // a plugin's layer: its kind's badge in its corner, as its manifest draws it
        l.pluginBadge?.let { badge ->
            androidx.compose.foundation.Canvas(Modifier.align(Alignment.BottomEnd).padding(3.dp).size(18.dp, 14.dp)) {
                drawRoundRect(PluginBadgeBehind, cornerRadius = androidx.compose.ui.geometry.CornerRadius(4.dp.toPx()))
                badgeDrawing(badge, Color.White, 1.5.dp.toPx())
            }
        }
    }
}

/** How wide the colour a plugin labels a layer with shows, down the left edge of its row. */
private val LabelWidth = 5.dp

/** The colour a plugin labels [l] with, if one does, down the left edge of its row. */
private fun Modifier.labelled(l: LayerInfo): Modifier {
    val label = l.label ?: return this
    val colour = Color(0xFF000000L or label.toLong())
    return drawBehind { drawRect(colour, size = Size(LabelWidth.toPx(), size.height)) }
}

/** The badge behind a plugin's layer's glyph on its thumbnail. */
private val PluginBadgeBehind = Color(0x99000000)

/**
 * A plugin's [badge]'s lines and dots in [ink] over the whole of this scope (its box, 0 to 1 across and down): its lines
 * [width] thick, its dots squares twice that across.
 */
private fun DrawScope.badgeDrawing(badge: io.github.artcraftandroid.procraft.core.PluginBadge, ink: Color, width: Float) {
    val (w, h) = size.width to size.height
    for (line in badge.lines) {
        val p = line.points
        if (p.size < 4) continue
        val path = Path().apply {
            moveTo(w * p[0], h * p[1])
            var i = 2
            while (i + 1 < p.size) {
                if (line.curved && i + 5 < p.size) {
                    cubicTo(w * p[i], h * p[i + 1], w * p[i + 2], h * p[i + 3], w * p[i + 4], h * p[i + 5])
                    i += 6
                } else {
                    lineTo(w * p[i], h * p[i + 1])
                    i += 2
                }
            }
        }
        drawPath(path, ink, style = androidx.compose.ui.graphics.drawscope.Stroke(width))
    }
    val node = width * 2
    for (i in 0 until badge.dots.size / 2) {
        val (x, y) = w * badge.dots[2 * i] to h * badge.dots[2 * i + 1]
        drawRect(ink, Offset(x - node / 2, y - node / 2), Size(node, node))
    }
}

/** The transparency checkerboard behind thumbnails. */
private fun androidx.compose.ui.graphics.drawscope.DrawScope.checker() {
    val cell = 6.dp.toPx()
    drawRect(Color(0xFFFFFFFF))
    var y = 0
    while (y * cell < size.height) {
        var x = 0
        while (x * cell < size.width) {
            if ((x + y) % 2 == 1) drawRect(Color(0xFFD6D6D6), Offset(x * cell, y * cell), Size(cell, cell))
            x++
        }
        y++
    }
}

/**
 * The letters on a layer's row for its mode: a two-word name's initials (the second small), a
 * one-word name's first letter, or its first two where one letter is already taken.
 */
private fun blendCode(vm: CanvasViewModel, blend: UInt): String {
    val name = vm.blendModeList.firstOrNull { it.id == blend }?.name ?: return "N"
    if (name in TWO_LETTER_BLENDS) return name.take(2)
    val words = name.split(' ')
    return if (words.size == 1) name.take(1) else words[0].take(1) + words[1].take(1).lowercase()
}

private val TWO_LETTER_BLENDS = setOf("Darken", "Shade", "Lighten", "Subtract", "Divide", "Saturation")

@Composable
private fun BlendEditor(vm: CanvasViewModel, l: LayerInfo, open: Boolean) {
    var opacity by remember(l.id) { mutableFloatStateOf(l.opacity) }
    Column(Modifier.fillMaxWidth().padding(top = 22.dp, bottom = 15.dp)) {
        Row(Modifier.padding(horizontal = 13.dp).fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text("Opacity", color = Pc.PanelTitle, fontSize = 17.sp, modifier = Modifier.weight(1f))
            Text(if (opacity >= 0.995f) "Max" else "${(opacity * 100).roundToInt()}%", color = Pc.PanelTitle, fontSize = 17.sp)
        }
        OpacitySlider(
            opacity,
            onChange = {
                opacity = it
                vm.setLayerOpacity(l, it, done = false)
            },
            onDone = { vm.setLayerOpacity(l, opacity, done = true) },
        )
        // a picker: the rows slide under a band fixed in the middle, the row there the layer's mode
        // as it moves (one undo step a movement); let go, the list slows straight onto a row; a
        // tap on another row makes it the mode at once and scrolls it to the band, untouchable
        val modes = vm.blendModeList
        val owner = vm.session
        var shownMode by remember(owner, l.id) { mutableStateOf(l.blend) }
        var tapping by remember(owner, l.id) { mutableStateOf(false) }
        val list = rememberLazyListState(modes.indexOfFirst { it.id == l.blend }.coerceAtLeast(0))
        val scope = rememberCoroutineScope()
        val density = LocalDensity.current.density
        val (rowPx, before, after) = with(LocalDensity.current) {
            val height = BlendRow.roundToPx()
            val padding = BlendListHeight.roundToPx() - height
            // Divide the padding in pixels so the last row can center at every density.
            Triple(height, (padding / 2).toDp(), (padding - padding / 2).toDp())
        }
        val bandTop = with(LocalDensity.current) { before.roundToPx() }
        val position by remember(list, rowPx) {
            derivedStateOf { list.firstVisibleItemIndex * rowPx + list.firstVisibleItemScrollOffset }
        }
        val centered by remember(rowPx) { derivedStateOf { (position + rowPx / 2) / rowPx } }
        val fling = remember(rowPx, density, modes.size) { BlendFling(rowPx, density, modes.size) { position } }

        fun preview(index: Int) {
            val mode = modes.getOrNull(index) ?: return
            if (tapping || mode.id == shownMode) return
            shownMode = mode.id
            vm.previewBlend(owner, l.id, mode.id)
        }

        LaunchedEffect(open, owner, list, modes, rowPx) {
            if (!open) return@LaunchedEffect
            // the mode under the band as the list moves, by finger or by itself
            launch { snapshotFlow { centered }.collect { preview(it) } }
            snapshotFlow { if (list.isScrollInProgress) null else position }.collect { idlePosition ->
                if (idlePosition == null) return@collect
                // Give a fling starting as the drag ends a frame to take over the list.
                withFrameNanos { }
                if (list.isScrollInProgress || position != idlePosition || tapping) return@collect
                if (position % rowPx != 0) {
                    // stopped between rows (a touch caught it moving): onto the nearest
                    val coast = blendCoast(position / density, 0f, rowPx / density, modes.size.toUInt())
                    coroutineScope { launch { list.scroll { followBlend(rowPx, density, { position }) { blendCoastAt(coast, it) } } } }
                } else {
                    preview(centered)
                    vm.finishBlend(owner)
                }
            }
        }
        DisposableEffect(open, owner, l.id) {
            onDispose { if (open) vm.finishBlend(owner) }
        }
        LaunchedEffect(open, l.blend, rowPx) {
            if (!open || l.blend == shownMode) return@LaunchedEffect
            // Undo and redo can change the mode while the picker is open.
            shownMode = l.blend
            val index = modes.indexOfFirst { it.id == l.blend }
            if (index >= 0) list.scrollToItem(index)
        }

        val highlight = Pc.RowSelected
        Box(
            Modifier.padding(top = 11.dp).fillMaxWidth().height(BlendListHeight)
                .clip(RoundedCornerShape(12.dp)).background(Pc.LayerRow)
                .drawBehind { drawRect(highlight, Offset(0f, bandTop.toFloat()), Size(size.width, rowPx.toFloat())) },
        ) {
            LazyColumn(
                Modifier.fillMaxSize(),
                state = list,
                contentPadding = PaddingValues(top = before, bottom = after),
                flingBehavior = fling,
                userScrollEnabled = !tapping,
            ) {
                itemsIndexed(modes) { i, m ->
                    Box(
                        Modifier.fillMaxWidth().height(BlendRow)
                            .clickable(enabled = !tapping) {
                                val from = position / density
                                val to = blendTap(i.toUInt(), from, rowPx / density, modes.size.toUInt()) ?: return@clickable
                                shownMode = m.id
                                vm.setBlend(l, m.id)
                                tapping = true
                                scope.launch {
                                    try {
                                        list.scroll(MutatePriority.PreventUserInput) {
                                            followBlend(rowPx, density, { position }) { blendTapAt(from, to, it) }
                                        }
                                    } finally {
                                        tapping = false
                                    }
                                }
                            },
                    ) {
                        // grey outside the band and white inside it: a name half under it is half and half
                        BlendLabel(
                            m.name, Pc.PanelTitle, BlendGlyphInk,
                            Modifier.drawWithContent {
                                val (top, bottom) = blendBandInRow(position, i, rowPx)
                                clipRect(top = top, bottom = bottom, clipOp = ClipOp.Difference) { this@drawWithContent.drawContent() }
                            },
                        )
                        BlendLabel(
                            m.name, Color.White, Color.White,
                            // (read out once, from the grey copy)
                            Modifier.clearAndSetSemantics { }.drawWithContent {
                                val (top, bottom) = blendBandInRow(position, i, rowPx)
                                clipRect(top = top, bottom = bottom) { this@drawWithContent.drawContent() }
                            },
                        )
                    }
                }
            }
        }
    }
}

/** A blend mode's name and glyph, in [ink] and [glyphInk]. */
@Composable
private fun BlendLabel(name: String, ink: Color, glyphInk: Color, modifier: Modifier = Modifier) {
    Row(modifier.fillMaxSize().padding(start = 12.dp, end = 18.5.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(name, color = ink, fontSize = 17.sp, maxLines = 1, modifier = Modifier.weight(1f))
        PcIcons.BlendGlyphs[name]?.let { Icon(it, contentDescription = null, tint = glyphInk, modifier = Modifier.size(26.dp)) }
    }
}

/** The layer's blend list: nine rows of [BlendRow] showing; its glyphs a darker grey than its names. */
private val BlendRow = 38.8.dp
private val BlendListHeight = 349.dp
private val BlendGlyphInk = Color(0xFF808080)

/** A thin blue slider with a white knob, set by dragging anywhere along it. */
@Composable
internal fun OpacitySlider(value: Float, onChange: (Float) -> Unit, onDone: () -> Unit) {
    val change by rememberUpdatedState(onChange)
    val done by rememberUpdatedState(onDone)
    Box(
        Modifier.padding(horizontal = 13.dp, vertical = 2.dp).fillMaxWidth().height(16.dp)
            .pointerInput(Unit) {
                awaitEachGesture {
                    val down = awaitFirstDown()
                    down.consume()
                    change((down.position.x / size.width).coerceIn(0f, 1f))
                    while (true) {
                        val c = awaitPointerEvent().changes.firstOrNull { it.id == down.id } ?: break
                        if (!c.pressed) break
                        change((c.position.x / size.width).coerceIn(0f, 1f))
                        c.consume()
                    }
                    done()
                }
            }
            .drawBehind {
                val y = size.height / 2f
                val h = 3.dp.toPx()
                drawRoundRect(Pc.SliderRest, Offset(0f, y - h / 2f), Size(size.width, h), CornerRadius(h / 2f))
                drawRoundRect(Pc.Accent, Offset(0f, y - h / 2f), Size(size.width * value, h), CornerRadius(h / 2f))
                val knob = Size(26.dp.toPx(), 15.dp.toPx())
                val x = (size.width * value).coerceIn(knob.width / 2f, size.width - knob.width / 2f)
                drawRoundRect(Color.White, Offset(x - knob.width / 2f, y - knob.height / 2f), knob, CornerRadius(knob.height / 2f))
            },
    )
}
/**
 * The current layer's options, left of the panel and centred on its row. What doesn't apply to
 * it is left out; lock, duplicate and delete are on the row's swipe.
 */
@Composable
private fun LayerOptions(open: Boolean, vm: CanvasViewModel, l: LayerInfo, rowY: Float, onRename: () -> Unit, onDone: () -> Unit) {
    val density = LocalDensity.current
    val place = remember(rowY, density) {
        with(density) { OptionsPlace((-183).dp.roundToPx(), rowY, OptionsTop.roundToPx(), OptionsShadowRoom.roundToPx()) }
    }
    MenuPresence(open, androidx.compose.ui.graphics.TransformOrigin(1f, 0.5f)) { look ->
        PcPopup(textScale = MATCHED_POPUP_TEXT, popupPositionProvider = place, onDismissRequest = onDone, properties = PopupProperties(focusable = true)) {
            // frosted over the canvas, as the library's menus are; empty (a locked group's or
            // mask's), as measured, a pill 64 tall with the frost alone, untinted
            val rows = vm.layerOptions(l)
            var choosingControl by remember(l.id) { mutableStateOf(false) }
            // a plugin's actions for the layer, after the app's own
            val actions = remember { vm.plugins.actionsIn(io.github.artcraftandroid.procraft.core.PluginPlace.LAYER) }
            val tint = if (rows.isEmpty() && actions.isEmpty()) Color.Transparent else Pc.GlassMenu
            val room = with(density) { OptionsShadowRoom.toPx() }
            // the room round it for its shadow: a tap there closes it, as one further out does
            Box(
                look.pointerInput(Unit) {
                    detectTapGestures { p ->
                        if (p.x < room || p.y < room || p.x > size.width - room || p.y > size.height - room) onDone()
                    }
                }.padding(OptionsShadowRoom).drawBehind { optionsShadow() },
            ) {
            Column(Modifier.width(194.dp).heightIn(min = 64.dp).clip(RoundedCornerShape(24.dp)).then(rememberFrost(24.dp)).background(tint).padding(vertical = 7.dp)) {
                // Control layer's own choices, in the menu's place once it's tapped
                if (choosingControl) {
                    val kinds = listOf(null to "None", io.github.artcraftandroid.procraft.core.LayerControl.LINE_ART to "Line art", io.github.artcraftandroid.procraft.core.LayerControl.SCRIBBLE to "Scribble", io.github.artcraftandroid.procraft.core.LayerControl.DEPTH to "Depth")
                    for ((kind, words) in kinds) Option(words, checked = l.control == kind, enabled = true, onClick = { vm.setLayerControl(l, kind); onDone() })
                }
                // the core's rows: each closes the menu, a switch's too (it shows ticked next time)
                if (!choosingControl) for (r in rows) {
                    val act: () -> Unit = when (r.option) {
                        LayerOption.RENAME -> onRename
                        LayerOption.UNGROUP -> { { vm.ungroup(l) } }
                        LayerOption.FLATTEN -> { { vm.flattenGroup(l) } }
                        LayerOption.SELECT -> { { vm.selectLayerContents(l) } }
                        LayerOption.COPY -> { { vm.copyLayerContents(l) } }
                        LayerOption.FILL -> { { vm.fillLayer(l) } }
                        LayerOption.CLEAR -> { { vm.clear(l) } }
                        LayerOption.ALPHA_LOCK -> { { vm.toggleAlphaLock(l) } }
                        LayerOption.MASK -> { { vm.addMask(l) } }
                        LayerOption.CLIPPING_MASK -> { { vm.toggleClipped(l) } }
                        LayerOption.DRAWING_ASSIST -> { { vm.setGuideAssisted(!l.assisted) } }
                        LayerOption.INVERT -> { { vm.invertLayer(l) } }
                        LayerOption.REFERENCE -> { { vm.toggleReference(l) } }
                        LayerOption.EDIT_TEXT -> { { vm.editText(l) } }
                        LayerOption.FILTER_SETTINGS -> { { vm.startFilterLayerSettings(l) } }
                        LayerOption.RASTERIZE -> { { vm.rasterize(l) } }
                        LayerOption.RECOLOUR -> { { vm.restyleStrokes(l, colour = true) } }
                        LayerOption.APPLY_BRUSH -> { { vm.restyleStrokes(l, colour = false) } }
                        LayerOption.MERGE_DOWN -> { { vm.mergeDown(l) } }
                        LayerOption.COMBINE_DOWN -> { { vm.combineDown(l) } }
                        LayerOption.MERGE_MASK -> { { vm.mergeMask(l) } }
                        LayerOption.CONTROL_LAYER -> { { choosingControl = true } }
                    }
                    Option(r.label, checked = r.switchOn, enabled = r.enabled, onClick = { act(); if (r.option != LayerOption.CONTROL_LAYER) onDone() })
                }
                for (action in actions) {
                    Option(action.label, onClick = { onDone(); vm.runPluginAction(action.key) })
                }
            }
            }
        }
    }
}

/** Room round the options for their shadow, which has faded out by then. */
private val OptionsShadowRoom = 52.dp

/**
 * The options' shadow round this (their size), as measured: black at 14%, blurred by 15 (its
 * sigma) and 6 lower. None of it under them: they show the frost alone when empty.
 */
private fun DrawScope.optionsShadow() {
    val outline = Path().apply {
        addRoundRect(RoundRect(0f, 0f, size.width, size.height, CornerRadius(24.dp.toPx())))
    }
    val shadow = Path().apply {
        addRoundRect(RoundRect(0f, 6.dp.toPx(), size.width, size.height + 6.dp.toPx(), CornerRadius(24.dp.toPx())))
    }
    // the blur's radius for that sigma, as the platform's blur takes it
    val radius = (15.dp.toPx() - 0.5f) / 0.57735f
    drawIntoCanvas { canvas ->
        val paint = androidx.compose.ui.graphics.Paint().apply {
            color = Color.Black.copy(alpha = 0.14f)
            asFrameworkPaint().maskFilter = BlurMaskFilter(radius, BlurMaskFilter.Blur.NORMAL)
        }
        canvas.save()
        canvas.clipPath(outline, ClipOp.Difference)
        canvas.drawPath(shadow, paint)
        canvas.restore()
    }
}

/**
 * Puts the options left of the panel, centred on the row tapped and kept below the top bar; the
 * popup is [room] bigger all round for their shadow.
 */
private class OptionsPlace(private val left: Int, private val rowY: Float, private val top: Int, private val room: Int) : PopupPositionProvider {
    override fun calculatePosition(anchorBounds: IntRect, windowSize: IntSize, layoutDirection: LayoutDirection, popupContentSize: IntSize): IntOffset {
        val bottom = (windowSize.height - popupContentSize.height + room).coerceAtLeast(top - room)
        val y = (rowY - popupContentSize.height / 2f).roundToInt().coerceIn(top - room, bottom)
        return IntOffset(anchorBounds.left + left - room, y)
    }
}

@Composable
private fun Option(label: String, checked: Boolean? = null, enabled: Boolean = true, destructive: Boolean = false, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().height(38.dp).clickable(enabled = enabled, onClick = onClick).padding(start = 19.dp, end = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val color = when {
            !enabled -> Pc.MenuDisabled
            destructive -> Color(0xFFFF453A)
            else -> Pc.MenuInk
        }
        Text(label, color = color, fontSize = 17.sp, modifier = Modifier.weight(1f))
        if (checked == true) Icon(PcIcons.Tick, contentDescription = null, tint = color, modifier = Modifier.size(18.dp))
    }
}
/** A model's texture set's name, heading its meshes: 41 tall, 16 under the set before it. */
@Composable
private fun SetHeader(name: String, first: Boolean) {
    Column {
        if (!first) Spacer(Modifier.height(16.dp))
        Box(Modifier.fillMaxWidth().height(41.dp), contentAlignment = Alignment.TopStart) {
            Text(name, color = Pc.PanelTitle, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, modifier = Modifier.padding(start = 16.dp, top = 12.dp))
        }
    }
}

/** A model's mesh opened in the layers panel: its row darkened blue. */
private val SubmeshOpen = Color(0xFF25344A)
private val SubmeshTile = Color(0xFF434246)

/**
 * One of a model's meshes: its picture, its name, the arrow that opens it (its texture set's
 * layers under it) and its box (shown or hidden). A tap anywhere but the box opens or closes it.
 */
@Composable
private fun SubmeshRow(vm: CanvasViewModel, index: Int, name: String, open: Boolean, shape: Shape) {
    val hidden = vm.submeshesHidden.getOrNull(index) == true
    val locked = vm.submeshesLocked.getOrNull(index) == true
    val picked = index in vm.activeSubmeshes
    // slid left it shows Lock (or Unlock); a little right, it joins the meshes picked
    val reach = with(LocalDensity.current) { (SwipeButton + 6.dp).toPx() }
    val pickReach = with(LocalDensity.current) { PickReach.toPx() }
    val offset = remember { Animatable(0f) }
    val scope = rememberCoroutineScope()
    var opened by remember { mutableStateOf(false) }
    Box(Modifier.clipToBounds()) {
    if (offset.value < 0f) {
        Row(Modifier.matchParentSize(), horizontalArrangement = Arrangement.End) {
            SwipeAction(if (locked) "Unlock" else "Lock", SwipeFill, SwipeInk) {
                vm.toggleSubmeshLocked(index)
                opened = false
                scope.launch { offset.animateTo(0f, Motion.swipeSettle()) }
            }
        }
    }
    Row(
        Modifier.offset { IntOffset(offset.value.roundToInt(), 0) }
            .pointerInput(index, opened, reach) {
                detectHorizontalDragGestures(
                    onHorizontalDrag = { change, amount ->
                        change.consume()
                        scope.launch { offset.snapTo((offset.value + amount).coerceIn(-reach, if (opened) 0f else pickReach)) }
                    },
                    onDragEnd = {
                        if (offset.value > pickReach / 2f) {
                            vm.pickMoreSubmeshes(index)
                            opened = false
                        } else {
                            opened = offset.value < -reach / 2f
                        }
                        scope.launch { offset.animateTo(if (opened) -reach else 0f, Motion.swipeSettle()) }
                    },
                    onDragCancel = { scope.launch { offset.animateTo(if (opened) -reach else 0f, Motion.swipeSettle()) } },
                )
            }
            .fillMaxWidth().height(RowHeight).clip(shape).background(if (open || picked) SubmeshOpen else Pc.LayerRow)
            .clickable { vm.openSubmesh(index) },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.padding(start = 4.dp).size(93.dp, 61.dp).clip(RoundedCornerShape(4.dp)).background(SubmeshTile), contentAlignment = Alignment.Center) {
            vm.submeshThumbnails[index]?.let { Image(it, contentDescription = null, contentScale = ContentScale.Fit, modifier = Modifier.fillMaxSize().padding(4.dp)) }
        }
        Text(name, color = Pc.PanelTitle, fontSize = 17.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f).padding(start = 29.dp, end = 6.dp))
        if (locked) Icon(PcIcons.PageLocked, contentDescription = "Locked", tint = Pc.PanelTitle, modifier = Modifier.size(16.dp))
        Box(Modifier.offset(x = 8.dp).size(44.dp, 51.dp).clickable { vm.openSubmesh(index) }, contentAlignment = Alignment.Center) {
            Icon(if (open) PcIcons.ChevronDown else PcIcons.ChevronRight, contentDescription = if (open) "Close" else "Open", tint = Pc.PanelTitle, modifier = Modifier.size(16.dp))
        }
        Box(Modifier.size(44.dp, 47.dp).clickable { vm.toggleSubmeshHidden(index) }, contentAlignment = Alignment.Center) {
            CheckBox(!hidden, Pc.CheckFill)
        }
        Spacer(Modifier.width(8.dp))
    }
    }
}

@Composable
private fun BackgroundRow(vm: CanvasViewModel) {
    val bg = vm.background
    Row(
        Modifier.fillMaxWidth().height(50.dp).clip(RoundedCornerShape(17.dp)).background(Pc.BackgroundRow),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(Modifier.weight(1f).fillMaxHeight().clickable { vm.pickBackground() }, verticalAlignment = Alignment.CenterVertically) {
            // hidden, the swatch shows no colour: a faint white over the row
            val swatch = if (bg.visible) Color(bg.r, bg.g, bg.b) else Color.White.copy(alpha = 0.06f)
            Box(Modifier.padding(start = 4.dp).size(92.dp, 42.dp).clip(RoundedCornerShape(14.dp)).background(swatch))
            Text("Background color", color = Pc.PanelTitle, fontSize = 17.sp, maxLines = 1, modifier = Modifier.weight(1f).padding(start = 14.dp))
        }
        Box(Modifier.size(44.dp, 41.dp).clickable { vm.toggleBackground() }, contentAlignment = Alignment.Center) {
            CheckBox(bg.visible, Pc.CheckFill)
        }
        Spacer(Modifier.width(8.dp))
    }
}

/**
 * The actions the plugins that are on put in the Layers panel, under the background's row: a row
 * each, by its label, as the background's is drawn. A tap runs it, as one in the Canvas tab does.
 */
@Composable
private fun PluginLayersRows(vm: CanvasViewModel) {
    val actions = remember(vm.pluginList) { vm.plugins.actionsIn(io.github.artcraftandroid.procraft.core.PluginPlace.LAYERS) }
    for (action in actions) {
        Row(
            Modifier.padding(top = 10.dp).fillMaxWidth().height(50.dp).clip(RoundedCornerShape(17.dp)).background(Pc.BackgroundRow)
                .clickable { vm.runPluginAction(action.key) },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(action.label, color = Pc.PanelTitle, fontSize = 17.sp, maxLines = 1, modifier = Modifier.weight(1f).padding(start = 18.dp, end = 18.dp))
        }
    }
}

/** A layer's visibility box: tapped, shown or hidden (or, with a layer shown alone, every layer again); held, shown alone (or every layer again). */
private fun Modifier.visibilityTouch(vm: CanvasViewModel, l: LayerInfo): Modifier =
    pointerInput(l) { detectTapGestures(onTap = { vm.toggleVisible(l) }, onLongPress = { vm.toggleSolo(l) }) }

/**
 * Two fingers on a row: tapping (down and up together, still) do [onTap], swiped right together do
 * [onSwipeRight], held still do [onHold]. One finger's touches go on to the rest as they are; once
 * a second is down they are the two fingers' alone.
 */
private fun Modifier.twoFingers(enabled: Boolean, onTap: () -> Unit, onSwipeRight: () -> Unit, onHold: () -> Unit): Modifier =
    if (!enabled) this else pointerInput(Unit) {
        awaitEachGesture {
            val first = awaitFirstDown(requireUnconsumed = false, pass = androidx.compose.ui.input.pointer.PointerEventPass.Initial)
            var two: Long? = null
            var middle: Offset? = null
            var moved = false
            var done = false
            while (true) {
                val e = withTimeoutOrNull(TWO_FINGER_HOLD_MS / 4) {
                    awaitPointerEvent(androidx.compose.ui.input.pointer.PointerEventPass.Initial)
                }
                // the touches' clock (uptime), also while no event comes
                val now = e?.changes?.maxOfOrNull { it.uptimeMillis } ?: android.os.SystemClock.uptimeMillis()
                if (e != null) {
                    val fingers = e.changes.filter { it.pressed }
                    if (fingers.size >= 2) {
                        val between = (fingers[0].position + fingers[1].position) / 2f
                        val start = middle ?: between.also { middle = it }
                        if (two == null) two = now
                        e.changes.forEach { it.consume() }
                        val d = between - start
                        if (d.getDistance() > viewConfiguration.touchSlop) moved = true
                        if (!done && d.x > TWO_FINGER_SWIPE.toPx() && d.x > 2f * kotlin.math.abs(d.y)) {
                            done = true
                            onSwipeRight()
                        }
                    } else if (two == null) {
                        e.changes.firstOrNull { it.id == first.id }?.let {
                            if ((it.position - first.position).getDistance() > viewConfiguration.touchSlop) moved = true
                        }
                    }
                    if (e.changes.none { it.pressed }) {
                        val since = two
                        if (!done && since != null && !moved && now - first.uptimeMillis <= TWO_FINGER_TAP_MS) onTap()
                        break
                    }
                }
                // two fingers kept still long enough: held
                val since = two
                if (!done && !moved && since != null && now - since >= TWO_FINGER_HOLD_MS) {
                    done = true
                    onHold()
                }
            }
        }
    }

/** How long two fingers may stay down and still tap, milliseconds. */
private const val TWO_FINGER_TAP_MS = 400L

/** How long two fingers kept still on a row hold it, milliseconds. */
private const val TWO_FINGER_HOLD_MS = 500L

/** How far two fingers go right together to swipe a row. */
private val TWO_FINGER_SWIPE = 40.dp

/** Two rows being pinched: each moved a share of its finger's way ([SQUEEZE_FOLLOW]), the upper on top. */
private class Squeeze(val upper: ULong, val lower: ULong, val upperBy: Float, val lowerBy: Float) {
    fun offset(id: ULong): Float? = when (id) {
        upper -> upperBy * SQUEEZE_FOLLOW
        lower -> lowerBy * SQUEEZE_FOLLOW
        else -> null
    }

    fun above(id: ULong): Float? = if (id == upper) 1f else null
}

/**
 * Two fingers put on two rows of the list (their layers by [rowAt], from a window y; [origin] the
 * list's place in the window) and pinched together to [PINCH_MERGE] of how far apart they were:
 * the list stays put from then, and as a finger lets go [onPinched] gets the two layers. Meanwhile
 * [onSqueeze] follows the two rows' fingers (none once they let go).
 */
private fun Modifier.pinchRows(
    origin: () -> Offset,
    rowAt: (Float) -> ULong?,
    onSqueeze: (Squeeze?) -> Unit,
    onPinched: (ULong, ULong) -> Unit,
): Modifier = this.pointerInput(Unit) {
    val initial = androidx.compose.ui.input.pointer.PointerEventPass.Initial
    awaitEachGesture {
        awaitFirstDown(requireUnconsumed = false, pass = initial)
        var ends: Pair<ULong, ULong>? = null
        // the fingers on the upper and lower rows, and where they went down
        var fingers: Pair<androidx.compose.ui.input.pointer.PointerId, androidx.compose.ui.input.pointer.PointerId>? = null
        var starts = 0f to 0f
        var apart = 0f
        var pinched = false
        while (true) {
            val e = awaitPointerEvent(initial)
            val down = e.changes.filter { it.pressed }
            if (ends == null && down.size == 2) {
                val (a, b) = down.sortedBy { it.position.y }
                val ra = rowAt(origin().y + a.position.y)
                val rb = rowAt(origin().y + b.position.y)
                if (ra != null && rb != null && ra != rb) {
                    ends = ra to rb
                    fingers = a.id to b.id
                    starts = a.position.y to b.position.y
                    apart = b.position.y - a.position.y
                }
            }
            val pair = ends
            val ids = fingers
            if (pair != null && ids != null && down.size >= 2) {
                val upper = down.firstOrNull { it.id == ids.first }?.position?.y ?: starts.first
                val lower = down.firstOrNull { it.id == ids.second }?.position?.y ?: starts.second
                if (kotlin.math.abs(lower - upper) <= apart * PINCH_MERGE) pinched = true
                if (pinched) e.changes.forEach { it.consume() }
                onSqueeze(Squeeze(pair.first, pair.second, upper - starts.first, lower - starts.second))
            }
            if (pair != null && down.size < 2) {
                onSqueeze(null)
                if (pinched) onPinched(pair.first, pair.second)
                break
            }
            if (down.isEmpty()) {
                onSqueeze(null)
                break
            }
        }
    }
}

/** How much of its finger's way a pinched row moves (as measured, about a quarter to a third). */
private const val SQUEEZE_FOLLOW = 0.3f

/** Two rows pinched to this share of how far apart they were merge (as measured: a pinch a third closed merges, opened again or not). */
private const val PINCH_MERGE = 0.65f
