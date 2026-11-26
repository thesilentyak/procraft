package io.github.artcraftandroid.procraft.ui

import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.isImeVisible
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.input.pointer.AwaitPointerEventScope
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerInputChange
import androidx.compose.ui.input.pointer.PointerInputScope
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.onLongClick
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.geometry.isSpecified
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculateCentroid
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionOnScreen
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
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
import io.github.artcraftandroid.procraft.canvas.CanvasViewModel
import io.github.artcraftandroid.procraft.canvas.RECENT_SET
import io.github.artcraftandroid.procraft.core.BrushHoldMove
import io.github.artcraftandroid.procraft.core.BrushInfo
import io.github.artcraftandroid.procraft.core.brushHoldMove
import io.github.artcraftandroid.procraft.core.brushMenuWait
import io.github.artcraftandroid.procraft.core.brushRowSlides
import io.github.artcraftandroid.procraft.core.ToolKind
import kotlinx.coroutines.launch
import kotlin.math.roundToInt
import kotlin.math.roundToLong

private val LibraryWidth = 440.dp
private val SetsWidth = 168.dp
private val BrushHeight = 80.dp
private val BrushGap = 8.dp
/** The library's menus: dark glass with a light edge, rows 40 high. */
private val LibraryMenuWidth = 253.dp
private val MenuFill = Color(0xFA303335)
private val MenuEdge = Color(0xFF5A5D5F)
private val MenuRule = Color(0xFF3F4347)
/** Menus stay below the top bar. */
private val MenuTop = 57.dp
/** A brush swiped left shows three buttons filling its place, this far apart. */
private val SwipeGap = 5.5.dp
private val SwipeFill = Color(0xFF3F3F3F)
private val SwipeInk = Color(0xFFC5C5C5)
private val SwipeDelete = Color(0xFFFF3A46)
/** A brush swiped right is picked, this fill over its row; it follows the finger this far. */
private val PickedBrush = Color(0xFF2C5281)
private val PickReach = 72.dp

/** A sample stroke tinted with [tint], the colour a plugin gives it; as drawn (white) without one. */
private fun pluginFilter(tint: Color?): androidx.compose.ui.graphics.ColorFilter? =
    tint?.let { androidx.compose.ui.graphics.ColorFilter.tint(it, androidx.compose.ui.graphics.BlendMode.Modulate) }

/**
 * A sample stroke on a cell, the brush in use's if [selected]: in the colour a plugin gives in [tints] (each brush's, then
 * the brush in use's), else dark on the light interface's cells.
 */
private fun previewFilter(tints: Pair<Color?, Color?>, selected: Boolean): androidx.compose.ui.graphics.ColorFilter? =
    pluginFilter((if (selected) tints.second else tints.first) ?: Color.Black.takeIf { Pc.light && !selected })

/** A brush's name on its cell: on a dark pill where its sample stroke runs under it. */
@Composable
private fun BrushName(name: String, selected: Boolean, shaded: Boolean, modifier: Modifier) {
    Text(
        name,
        color = if (selected) Color.White else Pc.PanelTitle,
        fontSize = 14.sp,
        fontWeight = FontWeight.SemiBold,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = modifier.offset(x = (-6).dp, y = (-2).dp)
            .then(if (shaded) Modifier.clip(RoundedCornerShape(50)).background(NamePill) else Modifier)
            .padding(horizontal = 6.dp, vertical = 2.dp),
    )
}

private val NamePill = Color(0x80000000)
/** The search field with the space above and below it, which scrolls away with the lists. */
private val SearchBlock = 60.dp
private val SearchFill = Color(0x1F767680)
private val SearchHint = Color(0x4DEBEBF5)
private val ResultFill = Color(0x14FFFFFF)

/**
 * The search field under the library's header: a magnifier, the text and, once typed, a clear
 * button. Putting the keyboard away leaves the field.
 */
@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
private fun LibrarySearch(query: String, onQuery: (String) -> Unit, onFocus: (Boolean) -> Unit) {
    val focus = androidx.compose.ui.platform.LocalFocusManager.current
    val keyboard = WindowInsets.isImeVisible
    var hadKeyboard by remember { mutableStateOf(false) }
    LaunchedEffect(keyboard) {
        if (keyboard) hadKeyboard = true else if (hadKeyboard) {
            hadKeyboard = false
            focus.clearFocus()
        }
    }
    Row(
        Modifier.padding(start = 12.dp, end = 12.dp, top = 10.dp, bottom = 10.dp).fillMaxWidth().height(40.dp)
            .clip(RoundedCornerShape(10.dp)).background(SearchFill),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Canvas(Modifier.size(41.dp, 40.dp)) {
            // a magnifier: a ring and its handle
            val c = Offset(size.width * 0.47f, size.height * 0.46f)
            val r = 7.dp.toPx()
            drawCircle(SearchHint, r, c, style = androidx.compose.ui.graphics.drawscope.Stroke(2.dp.toPx()))
            val d = r * 0.7071f
            drawLine(SearchHint, c + Offset(d, d), c + Offset(d + 5.dp.toPx(), d + 5.dp.toPx()), 2.4.dp.toPx(), cap = androidx.compose.ui.graphics.StrokeCap.Round)
        }
        Box(Modifier.weight(1f), contentAlignment = Alignment.CenterStart) {
            if (query.isEmpty()) Text("Search", color = SearchHint, fontSize = 17.sp, maxLines = 1)
            androidx.compose.foundation.text.BasicTextField(
                query,
                onValueChange = onQuery,
                singleLine = true,
                textStyle = androidx.compose.ui.text.TextStyle(color = Color.White, fontSize = 17.sp),
                cursorBrush = androidx.compose.ui.graphics.SolidColor(Pc.Accent),
                keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(imeAction = androidx.compose.ui.text.input.ImeAction.Search),
                keyboardActions = androidx.compose.foundation.text.KeyboardActions(onSearch = { focus.clearFocus() }),
                modifier = Modifier.fillMaxWidth().onFocusChanged { onFocus(it.isFocused) },
            )
        }
        if (query.isNotEmpty()) {
            // a filled circle with a cross cut out of it
            Canvas(Modifier.padding(end = 5.dp).size(19.dp, 18.dp).clip(CircleShape).clickable { onQuery("") }) {
                val c = center
                drawCircle(SearchHint, size.minDimension / 2f, c)
                val k = 3.5.dp.toPx()
                for (s in listOf(1f, -1f)) {
                    drawLine(Color.Black, c + Offset(-k, -k * s), c + Offset(k, k * s), 1.8.dp.toPx(), cap = androidx.compose.ui.graphics.StrokeCap.Round, blendMode = androidx.compose.ui.graphics.BlendMode.Clear)
                }
            }
        }
    }
}

/**
 * The brushes of every set whose name holds the search, two to a row; tapping one ([onPick]) ends
 * the search and shows it in its own set, chosen there.
 */
@Composable
private fun SearchResults(vm: CanvasViewModel, query: String, onPick: (BrushInfo) -> Unit) {
    val found = if (query.isBlank()) emptyList() else vm.brushSets.flatMap { it.brushes }.filter { it.name.contains(query.trim(), ignoreCase = true) }
    val tints = remember(vm.colorRgb(), vm.pluginLooks) { vm.previewTints() }
    if (found.isEmpty()) {
        Box(Modifier.fillMaxSize().padding(top = 24.dp), contentAlignment = Alignment.TopCenter) {
            Text("No results", color = Pc.PanelTitle, fontSize = 15.sp, fontWeight = FontWeight.Medium)
        }
        return
    }
    LazyVerticalGrid(
        columns = GridCells.Fixed(2),
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 12.dp, top = 11.dp, end = 12.dp, bottom = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        items(found, key = { "${it.set}-${it.index}" }) { b ->
            val key = "${b.set}-${b.index}"
            val preview = vm.previews[key]
            LaunchedEffect(key, preview == null, vm.previewRound) { if (preview == null) vm.loadPreview(b) }
            val selected = vm.highlighted(b)
            Box(
                Modifier.fillMaxWidth().height(81.dp).clip(RoundedCornerShape(12.dp))
                    .background(if (selected) Pc.RowSelected else ResultFill)
                    .clickable { onPick(b) },
            ) {
                preview?.let {
                    val tint = pluginFilter(if (selected) tints.second else tints.first)
                    Image(it, contentDescription = null, contentScale = ContentScale.Crop, colorFilter = tint, modifier = Modifier.fillMaxSize())
                }
                BrushName(b.name, selected, vm.shadedNames[key] == true, Modifier.padding(start = 14.dp, top = 11.dp, end = 10.dp))
            }
        }
    }
}

/**
 * The brush library: sets on the left, the set's brushes with a sample stroke on the right. Tap
 * the chosen set for its menu, swipe a brush left to share, duplicate or delete it, hold it for
 * its menu (and drag it on from there onto a set, which opens after a moment over it, or to
 * another place in its own; brushes picked go with it), pinch the lists in for every library's
 * card, and add brushes and sets with the plus.
 */
@Composable
fun BrushLibrary(vm: CanvasViewModel, arrowX: Dp, modifier: Modifier = Modifier, floating: FloatingFrame? = null) {
    // closed, hanging from its button or floating: the core lets go of what its strokes are drawn on
    androidx.compose.runtime.DisposableEffect(vm) { onDispose { vm.libraryClosed() } }
    val sets = vm.brushSets
    val tints = remember(vm.colorRgb(), vm.pluginLooks) { vm.previewTints() }
    // the Recent set, or one of the library's
    val recent = vm.shownSet == RECENT_SET
    val shown = sets.getOrNull(vm.shownSet)
    // every library's card, or a library with no sets yet: a panel of their own
    if (vm.showingLibraries || sets.isEmpty() || (shown == null && !recent)) {
        LibrariesPanel(vm, arrowX, modifier, floating)
        return
    }
    // the brushes shown, and whether each is pinned in the Recent set
    val listed = if (recent) {
        vm.recentBrushes.mapNotNull { r -> sets.getOrNull(r.set.toInt())?.brushes?.getOrNull(r.index.toInt())?.let { it to r.pinned } }
    } else {
        shown?.brushes.orEmpty().map { it to false }
    }
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val density = LocalDensity.current
    val focus = androidx.compose.ui.platform.LocalFocusManager.current
    var adding by remember { mutableStateOf(false) }
    // the title's menu: the library renamed, or every library's card
    var titleMenu by remember { mutableStateOf(false) }
    var setMenu by remember { mutableStateOf<Int?>(null) }
    var recentMenu by remember { mutableStateOf(false) }
    // the set menu's icon grid is open; where its Customize icon row and the set are on screen
    var iconPicker by remember { mutableStateOf(false) }
    var customizeRow by remember { mutableStateOf<Pair<Float, Float>?>(null) }
    var setScreenTop by remember { mutableStateOf(0f) }
    // the search: what is typed, and whether the field is in use
    var query by remember { mutableStateOf("") }
    var searching by remember { mutableStateOf(false) }
    var deleting by remember { mutableStateOf<Pair<String, () -> Unit>?>(null) }
    // a brush held: its menu (by place, "set-index")
    var brushMenu by remember { mutableStateOf<String?>(null) }
    // two fingers pinching on the lists: how far (their spread, a part of what it was); the lists
    // follow, as far as the pinch can take them, and spring back when it isn't taken
    var pinched by remember { mutableStateOf<Float?>(null) }
    var listsOrigin by remember { mutableStateOf(Offset.Zero) }
    val pinchShown = androidx.compose.animation.core.animateFloatAsState(
        pinched?.coerceIn(PINCH_SHOWN_LEAST, PINCH_SHOWN_MOST) ?: 1f,
        if (pinched != null) androidx.compose.animation.core.snap() else androidx.compose.animation.core.spring(),
        label = "library pinch",
    )
    androidx.compose.runtime.DisposableEffect(Unit) {
        onDispose {
            vm.pickedBrushes.clear()
            vm.renamingBrush = null
        }
    }
    // a brush held and dragged: which, where the finger is and where on the brush it took hold
    // (window pixels); and where the sets, the brushes and the lists are on the screen
    var dragged by remember { mutableStateOf<BrushInfo?>(null) }
    // the brushes going with it (it among them, those picked in its set), in their order; the
    // finger dragging them, and the dragged cell's size (its row may go as another set opens)
    var draggedWith by remember { mutableStateOf<List<BrushInfo>>(emptyList()) }
    var dragPointer by remember { mutableStateOf<androidx.compose.ui.input.pointer.PointerId?>(null) }
    var dragSize by remember { mutableStateOf(androidx.compose.ui.geometry.Size.Zero) }
    // or a set held and dragged (its place)
    var draggedSet by remember { mutableStateOf<Int?>(null) }
    var dragAt by remember { mutableStateOf(Offset.Zero) }
    var grab by remember { mutableStateOf(Offset.Zero) }
    val setBounds = remember { mutableStateMapOf<Int, Rect>() }
    val brushBounds = remember { mutableStateMapOf<String, Rect>() }
    var listBounds by remember { mutableStateOf(Rect.Zero) }
    // where the shown set's brushes list is
    var brushListBounds by remember { mutableStateOf(Rect.Zero) }
    // the set the dragged brush or set is over, if any
    val overSet = if (dragged == null && draggedSet == null) null else setBounds.entries.firstOrNull { it.value.contains(dragAt) }?.key?.takeIf { it != draggedSet }
    // What the dragged brushes are over now: a set's row, or a brush of the set shown (which half),
    // or its list. Read afresh from the library and the rows as they are, never from values this
    // composition worked out: the lists' pointer handler below keeps the drop() of the composition
    // it started in (a newer reference to a local function is equal to the first, so no state
    // takes it), and a set opens under a drag.
    fun dropOver(): io.github.artcraftandroid.procraft.core.BrushDropOver? {
        val showing = vm.shownSet.takeIf { it != RECENT_SET && it in vm.brushSets.indices }
        val brushes = showing?.let { vm.brushSets.getOrNull(it) }?.brushes.orEmpty()
        val rows = brushes.mapNotNull { t -> brushBounds["${t.set}-${t.index}"]?.let { t.index.toInt() to it } }
        return brushDropOver(dragAt, setBounds, showing, brushes.size, brushListBounds, rows)
    }
    // where the dragged brushes land let go now (the core's rule), if anywhere
    fun dropPlace(): io.github.artcraftandroid.procraft.core.BrushDrop? {
        val b = dragged ?: return null
        val over = dropOver() ?: return null
        val moving = draggedWith.ifEmpty { listOf(b) }.filter { it.set == b.set }.map { it.index }
        return io.github.artcraftandroid.procraft.core.brushDropPlace(b.set, moving, over)
    }
    // into another set they go as copies, as a green plus on them says while they would
    val dropCopies = dragged != null && dropPlace()?.copies == true
    fun drop() {
        val b = dragged ?: return
        val moving = draggedWith.ifEmpty { listOf(b) }
        val over = dropOver()
        dragged = null
        draggedWith = emptyList()
        dragPointer = null
        vm.dropBrushesOver(moving, over ?: return)
    }
    // held over another set a moment, the dragged brush opens it, to be let go among its brushes
    LaunchedEffect(overSet, dragged != null) {
        val over = overSet ?: return@LaunchedEffect
        if (dragged == null || over == vm.shownSet) return@LaunchedEffect
        kotlinx.coroutines.delay(SPRING_LOAD_MS)
        if (dragged != null) vm.showSet(over)
    }
    // a set dropped onto another takes its place
    fun dropSet() {
        val from = draggedSet ?: return
        draggedSet = null
        setBounds.entries.firstOrNull { it.value.contains(dragAt) }?.key?.let { vm.moveBrushSet(from, it) }
    }
    fun share(b: BrushInfo?) {
        scope.launch { vm.brushFileToShare(b)?.let { shareFile(context, it, "application/octet-stream") } }
    }
    // how far the search field (and the lists under it) has scrolled up out of sight: all of it
    // when the library opens
    val searchBlock = with(density) { SearchBlock.toPx() }
    var searchAway by remember { mutableFloatStateOf(searchBlock) }
    val searchScroll = remember(searchBlock) {
        object : NestedScrollConnection {
            override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
                // scrolling up, the field goes first
                if (available.y >= 0f || searchAway >= searchBlock) return Offset.Zero
                val taken = maxOf(available.y, searchAway - searchBlock)
                searchAway -= taken
                return Offset(0f, taken)
            }

            override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
                // pulled down past a list's top (not flung), the field comes back
                if (available.y <= 0f || searchAway <= 0f || source != NestedScrollSource.UserInput) return Offset.Zero
                val taken = minOf(available.y, searchAway)
                searchAway -= taken
                return Offset(0f, taken)
            }
        }
    }

    Popover(floating?.width ?: LibraryWidth, arrowX, modifier, side = if (floating != null) ArrowSide.NONE else ArrowSide.TOP) {
        floating?.let { FloatingTop(it) }
        val renaming = vm.renamingLibrary == vm.libraryPlace
        Box {
            PanelHeader(
                if (renaming) "" else vm.libraryName,
                start = 20.dp,
                end = 12.dp,
                height = 52.dp,
                chevronOn = titleMenu,
                onTitle = { titleMenu = !titleMenu },
                menu = {
                    if (renaming) InlineName(vm.libraryName, Pc.PanelTitle, 24.sp) { vm.renameLibrary(vm.libraryPlace, it) }
                    LibraryTitleMenu(vm, titleMenu) { titleMenu = false }
                },
            ) {
                // brushes picked: Delete in the plus's place, and Combine for one with the current brush
                if (vm.pickedBrushes.isNotEmpty()) Row(Modifier.padding(top = 16.dp)) {
                    if (vm.canCombinePicked) {
                        Box(Modifier.size(80.dp, 32.dp).clickable { vm.combinePicked() }, contentAlignment = Alignment.Center) {
                            Text("Combine", color = SwipeInk, fontSize = 15.sp, maxLines = 1)
                        }
                        Spacer(Modifier.width(4.dp))
                    }
                    Box(
                        Modifier.size(63.dp, 32.dp).clickable { deleting = "Delete these brushes?" to { vm.deletePickedBrushes() } },
                        contentAlignment = Alignment.Center,
                    ) {
                        Text("Delete", color = SwipeInk, fontSize = 15.sp, maxLines = 1)
                    }
                } else Box(Modifier.padding(top = 12.dp).size(41.dp, 40.dp).clickable { adding = true }, contentAlignment = Alignment.Center) {
                    Icon(PcIcons.PlusMedium, contentDescription = "Add", tint = Pc.PanelGlyph, modifier = Modifier.requiredSize(41.dp))
                    // hangs below the plus, their right edges in line
                    LibraryMenu(adding, androidx.compose.ui.graphics.TransformOrigin(1f, 0f), MenuPlace { a, size -> IntOffset(a.right - size.width, a.bottom) }, onDismiss = { adding = false }, overPanel = true) {
                        // the Recent set holds no brushes of its own to add one to
                        if (!recent) {
                            MenuItem(PcIcons.MenuNewBrush, "Create new brush") {
                                adding = false
                                vm.newBrush()
                            }
                        }
                        MenuItem(PcIcons.MenuNewSet, "Create new set") {
                            adding = false
                            vm.newBrushSet()
                        }
                        MenuDivider(11.dp)
                        MenuItem(PcIcons.MenuFolder, "Import from Files") {
                            adding = false
                            vm.pickBrushFile?.invoke()
                        }
                    }
                }
            }
            LibraryGrabber(vm, floating)
        }
        // the search field scrolls away with the lists: out of sight at first, it comes down as a
        // list is pulled down past its top, and goes again as the lists scroll up. Two fingers
        // pinched in on them shrink them, and let go small enough show every library's card
        Box(
            Modifier.weight(1f).clipToBounds().nestedScroll(searchScroll)
                .onGloballyPositioned { listsOrigin = it.positionInWindow() }
                .pointerInput(Unit) {
                    awaitLibraryPinch({ listsOrigin }, { zoom, _ -> pinched = zoom }) { zoom, _ ->
                        pinched = null
                        if (zoom <= PINCH_TO_LIBRARIES) {
                            brushMenu = null
                            vm.renamingBrush = null
                            vm.showingLibraries = true
                        }
                    }
                }
                .graphicsLayer {
                    val k = pinchShown.value
                    scaleX = k
                    scaleY = k
                },
        ) {
        Column(
            Modifier.layout { measurable, constraints ->
                val away = if (searching || query.isNotEmpty()) 0 else searchAway.roundToInt()
                val tall = constraints.maxHeight + away
                val placeable = measurable.measure(constraints.copy(minHeight = tall, maxHeight = tall))
                layout(constraints.maxWidth, constraints.maxHeight) { placeable.place(0, -away) }
            },
        ) {
        LibrarySearch(query, onQuery = { query = it }, onFocus = { searching = it })
        Box(
            Modifier.weight(1f).onGloballyPositioned { listBounds = it.boundsInWindow() }
                // a dragged brush is followed here, over both lists, once its row has begun the drag:
                // on as its set's list gives way to another's
                .pointerInput(Unit) {
                    awaitEachGesture {
                        awaitFirstDown(requireUnconsumed = false)
                        while (true) {
                            val event = awaitPointerEvent(PointerEventPass.Initial)
                            val held = dragPointer
                            if (dragged != null && held != null) {
                                val change = event.changes.firstOrNull { it.id == held }
                                if (change == null) {
                                    // the finger gone without lifting (the gesture taken away)
                                    dragged = null
                                    draggedWith = emptyList()
                                    dragPointer = null
                                } else {
                                    change.consume()
                                    dragAt = listBounds.topLeft + change.position
                                    // (an earlier composition's drop(): it reads what it needs as it is now)
                                    if (!change.pressed) drop()
                                }
                            }
                            if (event.changes.none { it.pressed }) break
                        }
                    }
                },
        ) {
        if (searching || query.isNotEmpty()) {
            SearchResults(vm, query) { b ->
                // as the original: the search ends, the field goes, and the brush shows in its own
                // set, chosen there and scrolled to, the set too
                query = ""
                focus.clearFocus()
                searching = false
                searchAway = searchBlock
                vm.findBrush(b)
            }
        } else {
        Row(Modifier.fillMaxSize()) {
            // a brush found (from the search, or the Recent set) shows its set (the Recent row first)
            val setsState = rememberLazyListState()
            val found = vm.foundBrush
            LaunchedEffect(found) {
                if (found == null) return@LaunchedEffect
                val item = vm.shownSet + 1
                if (setsState.layoutInfo.visibleItemsInfo.none { it.index == item && it.offset >= 0 && it.offset + it.size <= setsState.layoutInfo.viewportEndOffset }) {
                    setsState.scrollToItem(item)
                }
            }
            LazyColumn(
                Modifier.width(SetsWidth).fillMaxHeight().background(Pc.SetColumn),
                state = setsState,
                contentPadding = PaddingValues(12.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                // the brushes last painted with, and those pinned there, above the sets
                item {
                    Box {
                        Row(
                            Modifier.fillMaxWidth().height(50.dp).clip(RoundedCornerShape(12.dp))
                                .background(if (recent) Pc.RowSelected else Pc.SetCell)
                                .clickable { if (recent) recentMenu = true else vm.showSet(RECENT_SET) },
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            val ink = if (recent) Color.White else Pc.PanelTitle
                            Icon(PcIcons.RecentSet, contentDescription = null, tint = ink, modifier = Modifier.padding(start = 4.dp).requiredSize(40.dp))
                            Text("Recent", color = ink, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, modifier = Modifier.padding(start = 1.dp, end = 8.dp))
                        }
                        // as a set's menu, of one item and 65 tall, as measured
                        val left = with(density) { 6.dp.roundToPx() }
                        val below = with(density) { MenuTop.roundToPx() }
                        LibraryMenu(recentMenu, androidx.compose.ui.graphics.TransformOrigin(1f, 0.5f), MenuPlace(below) { a, size -> IntOffset(a.left - size.width - left, a.center.y - size.height / 2) }, onDismiss = { recentMenu = false }) {
                            Box(Modifier.padding(vertical = 5.5.dp)) {
                                MenuItem(PcIcons.MenuClear, "Clear") {
                                    recentMenu = false
                                    vm.clearRecentBrushes()
                                }
                            }
                        }
                    }
                }
                itemsIndexed(sets) { i, set ->
                    val selected = i == vm.shownSet
                    // gone from the list, it is nowhere: where it last was may be under another row
                    androidx.compose.runtime.DisposableEffect(i) { onDispose { setBounds.remove(i) } }
                    Box {
                        Row(
                            Modifier.fillMaxWidth().height(50.dp).onGloballyPositioned {
                                setBounds[i] = it.boundsInWindow()
                                if (selected) setScreenTop = it.positionOnScreen().y
                            }
                                // held, it lifts and goes where it is dragged; faint where it was
                                .alpha(if (draggedSet == i) 0.3f else 1f)
                                .pointerInput(i) {
                                    detectDragGesturesAfterLongPress(
                                        onDragStart = { at ->
                                            setMenu = null
                                            focus.clearFocus()
                                            grab = at
                                            dragAt = (setBounds[i]?.topLeft ?: Offset.Zero) + at
                                            draggedSet = i
                                        },
                                        onDrag = { change, by ->
                                            change.consume()
                                            dragAt += by
                                        },
                                        onDragEnd = { dropSet() },
                                        onDragCancel = { draggedSet = null },
                                    )
                                }
                                .clip(RoundedCornerShape(12.dp))
                                .background(if (overSet == i) Pc.Accent else if (selected) Pc.RowSelected else Pc.SetCell)
                                .clickable { if (selected) setMenu = i else vm.showSet(i) },
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            val ink = if (selected) Color.White else Pc.PanelTitle
                            // a 40 frame centred 24 in
                            Icon(PcIcons.SetIcons.getOrElse(set.icon.toInt()) { PcIcons.BrushSet }, contentDescription = null, tint = ink, modifier = Modifier.padding(start = 4.dp).requiredSize(40.dp))
                            val nameModifier = Modifier.padding(start = 1.dp, end = 8.dp)
                            if (vm.renamingSet == i) {
                                InlineName(set.name, ink, 14.sp, nameModifier) { vm.renameBrushSet(i, it) }
                            } else {
                                Text(
                                    set.name,
                                    color = ink,
                                    fontSize = 14.sp,
                                    fontWeight = FontWeight.SemiBold,
                                    lineHeight = 16.sp,
                                    maxLines = 2,
                                    overflow = TextOverflow.Ellipsis,
                                    modifier = nameModifier,
                                )
                            }
                        }
                        // centred on the set, a little left of it
                        val left = with(density) { 6.dp.roundToPx() }
                        val below = with(density) { MenuTop.roundToPx() }
                        // the icons to choose from, right of the menu over the library: under the Customize icon
                        // row, or over it when there is no room below; scrolled to the set's own icon
                        MenuPresence(setMenu == i && iconPicker, androidx.compose.ui.graphics.TransformOrigin(0f, 0f)) { look ->
                            PcPopup(
                                popupPositionProvider = remember(customizeRow, setScreenTop, density) {
                                    MenuPlace(below) { a, size ->
                                        val x = a.left - with(density) { PickerInset.roundToPx() }
                                        val row = customizeRow ?: return@MenuPlace IntOffset(x, a.top)
                                        val top = a.top + (row.first - setScreenTop).roundToInt()
                                        val bottom = a.top + (row.second - setScreenTop).roundToInt()
                                        val room = (vm.windowSize.y - with(density) { PickerMargin.toPx() }).roundToInt()
                                        IntOffset(x, if (bottom + size.height <= room) bottom else top - size.height)
                                    }
                                },
                                onDismissRequest = { iconPicker = false },
                            ) {
                                val shape = RoundedCornerShape(26.dp)
                                // an icon that isn't one of the picker's starts it at the top
                                val own = set.icon.toInt().takeIf { it in 0 until PcIcons.PICKED_SET_ICONS }
                                val first = if (own == null) 0 else (own / PICKER_COLUMNS - 3).coerceAtLeast(0) * PICKER_COLUMNS
                                LazyVerticalGrid(
                                    columns = GridCells.Fixed(PICKER_COLUMNS),
                                    state = androidx.compose.foundation.lazy.grid.rememberLazyGridState(initialFirstVisibleItemIndex = first),
                                    modifier = look.size(250.dp, 325.dp).clip(shape).background(MenuFill).border(1.dp, MenuEdge, shape).padding(3.dp),
                                ) {
                                    items(PcIcons.PICKED_SET_ICONS) { k ->
                                        val chosen = k == set.icon.toInt()
                                        Box(
                                            Modifier.size(48.dp).clip(RoundedCornerShape(21.dp)).background(if (chosen) Pc.Accent else Color.Transparent).clickable {
                                                vm.changeBrushSetIcon(i, k)
                                                iconPicker = false
                                                setMenu = null
                                            },
                                            contentAlignment = Alignment.Center,
                                        ) {
                                            Icon(PcIcons.SetIcons[k], contentDescription = PcIcons.SetIconNames[k], tint = Color.White, modifier = Modifier.requiredSize(40.dp))
                                        }
                                    }
                                }
                            }
                        }
                        // left of the set, 6 clear of it, centred on it
                        LibraryMenu(setMenu == i, androidx.compose.ui.graphics.TransformOrigin(1f, 0.5f), MenuPlace(below) { a, size -> IntOffset(a.left - size.width - left, a.center.y - size.height / 2) }, onDismiss = { setMenu = null }) {
                            MenuItem(PcIcons.MenuRename, "Rename") {
                                setMenu = null
                                vm.renamingSet = i
                            }
                            MenuItem(PcIcons.MenuDuplicate, "Duplicate") {
                                setMenu = null
                                vm.duplicateBrushSet(i)
                            }
                            Box(Modifier.onGloballyPositioned { c -> c.positionOnScreen().y.let { customizeRow = it to it + c.size.height } }) {
                                MenuItem(PcIcons.SetIcons.getOrElse(set.icon.toInt()) { PcIcons.BrushSet }, "Customize icon", trailing = PcIcons.MenuChevron) {
                                    iconPicker = !iconPicker
                                }
                            }
                            MenuDivider()
                            MenuItem(PcIcons.MenuShare, "Share") {
                                setMenu = null
                                share(null)
                            }
                            // the only set too: the library is then left with none
                            MenuDivider()
                            MenuItem(PcIcons.MenuTrash, "Delete", Pc.MenuDanger) {
                                setMenu = null
                                deleting = "Delete this set and its brushes?" to { vm.deleteBrushSet(i) }
                            }
                        }
                    }
                }
            }
            // another set's brushes replace these at once (rows fade only as brushes come and go)
            androidx.compose.runtime.key(vm.shownSet) {
                // a brush found from the Recent set is scrolled to in its own
                val listState = rememberLazyListState()
                LaunchedEffect(vm.foundBrush) {
                    val found = vm.foundBrush ?: return@LaunchedEffect
                    vm.foundBrush = null
                    listed.indexOfFirst { (b, _) -> "${b.set}-${b.index}" == found }.takeIf { it >= 0 }?.let { listState.scrollToItem(it) }
                }
                LazyColumn(
                    Modifier.weight(1f).fillMaxHeight().background(Pc.BrushColumn).onGloballyPositioned { brushListBounds = it.boundsInWindow() },
                    state = listState,
                    contentPadding = PaddingValues(start = 12.dp, top = 9.75.dp, end = 12.dp, bottom = 12.dp),
                    verticalArrangement = Arrangement.spacedBy(BrushGap),
                ) {
                    items(listed, key = { (b, _) -> "${b.set}-${b.index}" }) { (b, pinned) ->
                        val key = "${b.set}-${b.index}"
                        // the brush in use alone (a set opened makes its last choice the brush)
                        val selected = vm.highlighted(b)
                        val picked = key in vm.pickedBrushes
                        // previews go by place, and are dropped when the library changes
                        val preview = vm.previews[key]
                        LaunchedEffect(key, preview == null, vm.previewRound) { if (preview == null) vm.loadPreview(b) }
                        // gone from the list, it is nowhere: where it last was may be under another row
                        androidx.compose.runtime.DisposableEffect(key) { onDispose { brushBounds.remove(key) } }
                        // the brush in this place now, for a hold that began before it moved here
                        val brushNow by androidx.compose.runtime.rememberUpdatedState(b)
                        Box(Modifier.animateItem(Motion.rowFade(), Motion.rowMove(), Motion.rowFade())) {
                            SwipeBrush(
                                open = vm.swipedBrush == key,
                                onOpen = { vm.swipedBrush = if (it) key else null },
                                // held to its menu, the press is the hold's: a late move is the list's
                                slides = brushRowSlides(brushMenu == key),
                                // the current brush counts as picked already
                                onPick = if (recent || selected) null else ({ vm.togglePickedBrush(b) }),
                                actions = if (recent) {
                                    listOf(
                                        SwipeAction(PcIcons.SwipeFind, "Find") { vm.findBrush(b) },
                                        if (pinned) {
                                            SwipeAction(PcIcons.SwipeUnpin, "Unpin") { vm.pinRecentBrush(b, false) }
                                        } else {
                                            SwipeAction(PcIcons.SwipePin, "Pin") { vm.pinRecentBrush(b, true) }
                                        },
                                        SwipeAction(PcIcons.SwipeClear, "Clear") { vm.clearRecentBrush(b) },
                                    )
                                } else {
                                    listOf(
                                        SwipeAction(PcIcons.SwipeShare, "Share") { share(b) },
                                        SwipeAction(PcIcons.SwipeDuplicate, "Duplicate") { vm.duplicateBrush(b) },
                                        SwipeAction(PcIcons.SwipeTrash, "Delete", SwipeDelete, Color.White) { deleting = "Delete this brush?" to { vm.deleteBrush(b) } },
                                    )
                                },
                            ) {
                                Box(
                                    Modifier.fillMaxWidth().height(BrushHeight).onGloballyPositioned { brushBounds[key] = it.boundsInWindow() }
                                        .alpha(if (draggedWith.any { d -> d.set == b.set && d.index == b.index }) 0.3f else 1f)
                                        .pointerInput(key, recent) {
                                            // held still the core's menu time (as measured), its menu shows with the
                                            // finger still down, and stays when it lifts (let go before, it was a
                                            // tap); moved on within the core's drag window after that, the menu goes
                                            // and the brush follows the finger (the Recent set's brushes stay where
                                            // they are); moved on later, the list has the press and scrolls, the menu
                                            // staying. One hold for both: a tap's own hold would keep the rest of the
                                            // press from the drag
                                            awaitEachGesture {
                                                val down = awaitFirstDown(requireUnconsumed = false)
                                                // the menu shows the hold after the finger landed (the
                                                // touch's own time), not after the press reached here
                                                val wait = brushMenuWaitMillis(android.os.SystemClock.uptimeMillis() - down.uptimeMillis)
                                                val menuAt = android.os.SystemClock.uptimeMillis() + wait
                                                val long = awaitBrushHold(down, wait) ?: return@awaitEachGesture
                                                vm.swipedBrush = null
                                                focus.clearFocus()
                                                brushMenu = key
                                                // the rest of the press is the hold's until it is decided: let go it is
                                                // no tap (taken before the tap under it sees it); dragged, the lists' box
                                                // follows it from there (this row may go, as another set opens); too
                                                // late to drag, the list scrolls with it (not taken while it waits, or
                                                // the list would give it up)
                                                while (true) {
                                                    val change = awaitPointerEvent(PointerEventPass.Initial).changes.firstOrNull { it.id == long.id }
                                                    if (change == null || change.isConsumed) break
                                                    if (!change.pressed) {
                                                        change.consume()
                                                        break
                                                    }
                                                    val moved = (change.position - long.position).getDistance()
                                                    val since = (change.uptimeMillis - menuAt) / 1000.0
                                                    val what = brushHoldMove(moved, viewConfiguration.touchSlop, since, !recent)
                                                    if (what == BrushHoldMove.LEAVES) break
                                                    if (what == BrushHoldMove.DRAGS) {
                                                        change.consume()
                                                        brushMenu = null
                                                        vm.swipedBrush = null
                                                        val now = brushNow
                                                        val bounds = brushBounds[key]
                                                        grab = long.position
                                                        dragAt = (bounds?.topLeft ?: Offset.Zero) + change.position
                                                        dragSize = bounds?.size ?: androidx.compose.ui.geometry.Size.Zero
                                                        draggedWith = draggedTogether(now, vm.pickedBrushes, vm.highlighted(now), vm.brushSets.getOrNull(now.set.toInt())?.brushes.orEmpty())
                                                        dragPointer = long.id
                                                        dragged = now
                                                        break
                                                    }
                                                }
                                            }
                                        }
                                        .semantics {
                                            onLongClick {
                                                brushMenu = key
                                                true
                                            }
                                        }
                                        .clip(RoundedCornerShape(12.dp))
                                        .background(if (selected) Pc.RowSelected else if (picked) PickedBrush else Color.Transparent)
                                        .clickable {
                                            // with a brush swiped open, a tap only closes it
                                            if (vm.swipedBrush != null) {
                                                vm.swipedBrush = null
                                                return@clickable
                                            }
                                            // and with brushes picked, only lets them go
                                            if (vm.pickedBrushes.isNotEmpty()) {
                                                vm.pickedBrushes.clear()
                                                return@clickable
                                            }
                                            // the brush in use opens in the studio; another becomes the brush
                                            val inUse = vm.highlighted(b)
                                            vm.chooseBrush(b)
                                            if (inUse) vm.openStudio()
                                        },
                                ) {
                                    // the sample stroke filling the cell's height (its ends past the cell's sides),
                                    // under the name
                                    preview?.let {
                                        // the samples are drawn white: dark on the light interface's cells, or in
                                        // the colour a plugin gives
                                        Image(
                                            it, contentDescription = null, contentScale = ContentScale.Crop,
                                            colorFilter = previewFilter(tints, selected),
                                            modifier = Modifier.fillMaxSize(),
                                        )
                                    }
                                    if (vm.renamingBrush == key) {
                                        InlineName(b.name, if (selected || picked) Color.White else Pc.PanelTitle, 14.sp, Modifier.padding(start = 18.dp, top = 11.75.dp, end = 12.dp), FontWeight.SemiBold) {
                                            vm.renameBrush(b, it)
                                        }
                                    } else {
                                        BrushName(b.name, selected || picked, vm.shadedNames[key] == true, Modifier.padding(start = 18.dp, top = 11.75.dp))
                                    }
                                    // a pinned brush's mark, at the cell's top right
                                    if (pinned) {
                                        Icon(
                                            PcIcons.PinMark, contentDescription = "Pinned", tint = if (selected) Color.White else Pc.PanelTitle,
                                            modifier = Modifier.align(Alignment.TopEnd).padding(top = 12.5.dp, end = 11.dp).requiredSize(16.dp),
                                        )
                                    }
                                }
                            }
                            // held, its menu under it, their left edges in line: in a set Rename, Duplicate,
                            // Share and Delete; in the Recent set Find, Pin or Unpin, Share and Clear
                            LibraryMenu(
                                brushMenu == key, androidx.compose.ui.graphics.TransformOrigin(0f, 0f),
                                MenuPlace(with(density) { MenuTop.roundToPx() }) { a, _ -> IntOffset(a.left, a.bottom) },
                                onDismiss = { brushMenu = null }, overPanel = true,
                            ) {
                                fun pick(action: () -> Unit) {
                                    brushMenu = null
                                    action()
                                }
                                if (recent) {
                                    MenuItem(PcIcons.SwipeFind, "Find", iconSize = 26.dp) { pick { vm.findBrush(b) } }
                                    if (pinned) {
                                        MenuItem(PcIcons.SwipeUnpin, "Unpin", iconSize = 26.dp) { pick { vm.pinRecentBrush(b, false) } }
                                    } else {
                                        MenuItem(PcIcons.SwipePin, "Pin", iconSize = 26.dp) { pick { vm.pinRecentBrush(b, true) } }
                                    }
                                } else {
                                    MenuItem(PcIcons.MenuRename, "Rename") { pick { vm.renamingBrush = key } }
                                    MenuItem(PcIcons.MenuDuplicate, "Duplicate") { pick { vm.duplicateBrush(b) } }
                                }
                                MenuDivider()
                                MenuItem(PcIcons.MenuShare, "Share") { pick { share(b) } }
                                MenuDivider()
                                if (recent) {
                                    MenuItem(PcIcons.MenuClear, "Clear") { pick { vm.clearRecentBrush(b) } }
                                } else {
                                    MenuItem(PcIcons.MenuTrash, "Delete", Pc.MenuDanger) { pick { deleting = "Delete this brush?" to { vm.deleteBrush(b) } } }
                                }
                            }
                        }
                    }
                }
            }
        }
        }
            // the dragged brush follows the finger
            dragged?.let { d ->
                val at = dragAt - grab - listBounds.topLeft
                Box(
                    Modifier.offset { IntOffset(at.x.roundToInt(), at.y.roundToInt()) }
                        .size(with(density) { dragSize.width.toDp() }, BrushHeight),
                ) {
                    Box(Modifier.matchParentSize().clip(RoundedCornerShape(12.dp)).background(Pc.RowSelected.copy(alpha = 0.9f))) {
                        vm.previews["${d.set}-${d.index}"]?.let {
                            Image(it, contentDescription = null, contentScale = ContentScale.Crop, colorFilter = pluginFilter(tints.second), modifier = Modifier.fillMaxSize())
                        }
                        Text(d.name, color = Color.White, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, modifier = Modifier.padding(start = 18.dp, top = 11.dp))
                        // several going together: how many
                        if (draggedWith.size > 1) {
                            Box(
                                Modifier.align(Alignment.TopEnd).padding(top = 8.dp, end = 8.dp).size(24.dp).clip(CircleShape).background(Color.White),
                                contentAlignment = Alignment.Center,
                            ) {
                                Text("${draggedWith.size}", color = Pc.RowSelected, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, maxLines = 1)
                            }
                        }
                    }
                    // over another set they would be copied into: a green plus on its corner
                    if (dropCopies) DragBadge(LandsBadge, "+", Modifier.align(Alignment.TopEnd).offset(12.dp, (-12).dp))
                }
            }
            // and the dragged set
            draggedSet?.let { i ->
                val set = sets.getOrNull(i) ?: return@let
                val at = dragAt - grab - listBounds.topLeft
                Row(
                    Modifier.offset { IntOffset(at.x.roundToInt(), at.y.roundToInt()) }
                        .size(with(density) { (setBounds[i]?.width ?: 0f).toDp() }, 50.dp)
                        .clip(RoundedCornerShape(12.dp)).background(Pc.RowSelected.copy(alpha = 0.9f)),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(PcIcons.SetIcons.getOrElse(set.icon.toInt()) { PcIcons.BrushSet }, contentDescription = null, tint = Color.White, modifier = Modifier.padding(start = 4.dp).requiredSize(40.dp))
                    Text(
                        set.name,
                        color = Color.White,
                        fontSize = 14.sp,
                        fontWeight = FontWeight.SemiBold,
                        lineHeight = 16.sp,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.padding(start = 1.dp, end = 8.dp),
                    )
                }
            }
        }
        }
        }
        floating?.let { FloatingBottom(it) }
    }

    // while the question is up, brushes read from a file wait to join the library
    LaunchedEffect(deleting != null) { vm.brushQuestionOpen = deleting != null }
    Alert(
        deleting,
        title = { (title, _) -> title },
        message = { "It can't be brought back." },
        action = "Delete",
        icon = PcIcons.Trash,
        onCancel = { deleting = null },
        onAction = { (_, action) ->
            deleting = null
            action()
        },
    )
}

/** A button a swiped brush shows: its icon and label, its colours, and what it does. */
private class SwipeAction(val icon: ImageVector, val label: String, val fill: Color = SwipeFill, val ink: Color = SwipeInk, val onClick: () -> Unit)

/**
 * A brush that slides away to the left, showing its [actions] in its place: Share, Duplicate and
 * Delete; in the Recent set Find, Pin (or Unpin) and Clear. Slid right past half of [PickReach] it
 * springs back and is picked or let go ([onPick]).
 */
@Composable
private fun SwipeBrush(
    open: Boolean,
    onOpen: (Boolean) -> Unit,
    actions: List<SwipeAction>,
    onPick: (() -> Unit)? = null,
    slides: Boolean = true,
    content: @Composable () -> Unit,
) {
    var width by remember { mutableIntStateOf(0) }
    val reach = if (onPick == null) 0f else with(LocalDensity.current) { PickReach.toPx() }
    val offset = remember { Animatable(0f) }
    val scope = rememberCoroutineScope()
    // read as the drag goes (the press may have shown the brush's menu since it began)
    val slidesNow by androidx.compose.runtime.rememberUpdatedState(slides)
    LaunchedEffect(open, width) { offset.animateTo(if (open) -width.toFloat() else 0f, Motion.swipeSettle()) }
    Box(Modifier.fillMaxWidth().onSizeChanged { width = it.width }.clipToBounds()) {
        if (offset.value < 0f) {
            Row(Modifier.matchParentSize(), horizontalArrangement = Arrangement.spacedBy(SwipeGap)) {
                for (a in actions) {
                    SwipeButton(a.icon, a.label, a.fill, a.ink, Modifier.weight(1f)) {
                        onOpen(false)
                        a.onClick()
                    }
                }
            }
        }
        Box(
            Modifier.offset { IntOffset(offset.value.roundToInt(), 0) }.pointerInput(open, width, reach, slides) {
                // a press held to the brush's menu isn't followed from then on (nor taken from the
                // list, which scrolls with it)
                if (!slides) return@pointerInput
                detectHorizontalDragGestures(
                    onHorizontalDrag = { change, amount ->
                        if (!slidesNow) return@detectHorizontalDragGestures
                        change.consume()
                        scope.launch { offset.snapTo(brushSlide(offset.value, amount, width.toFloat(), reach, open)) }
                    },
                    onDragEnd = {
                        val (pick, opened) = brushSlideEnd(offset.value, width.toFloat(), reach, open, slidesNow)
                        if (pick) onPick?.invoke()
                        if (opened == open) scope.launch { offset.animateTo(if (open) -width.toFloat() else 0f, Motion.swipeSettle()) } else onOpen(opened)
                    },
                    onDragCancel = { scope.launch { offset.animateTo(if (open) -width.toFloat() else 0f, Motion.swipeSettle()) } },
                )
            },
        ) { content() }
    }
}

@Composable
private fun SwipeButton(icon: ImageVector, label: String, fill: Color, ink: Color, modifier: Modifier, onClick: () -> Unit) {
    Column(
        modifier.fillMaxHeight().clip(RoundedCornerShape(16.dp)).background(fill).clickable(onClick = onClick).padding(top = 5.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        // 19 across, as measured, the label where it was
        Icon(icon, contentDescription = null, tint = ink, modifier = Modifier.size(19.dp))
        Spacer(Modifier.height(2.dp))
        Text(label, color = ink, fontSize = 14.sp, fontWeight = FontWeight.Medium, maxLines = 1)
    }
}

/** A menu of the library (and the layers panel's title), placed by [place]. */
@Composable
internal fun LibraryMenu(
    open: Boolean,
    origin: androidx.compose.ui.graphics.TransformOrigin,
    place: PopupPositionProvider,
    onDismiss: () -> Unit,
    overPanel: Boolean = false,
    content: @Composable ColumnScope.() -> Unit,
) {
    MenuPresence(open, origin) { look ->
        PcPopup(textScale = MATCHED_POPUP_TEXT, popupPositionProvider = place, onDismissRequest = onDismiss, properties = PopupProperties(focusable = true)) {
            val shape = RoundedCornerShape(16.dp)
            // frosted over the canvas; over the library itself, nearly solid
            val glass = if (overPanel) Modifier.background(Pc.GlassMenuOverPanel) else rememberFrost(16.dp).background(Pc.GlassMenu)
            Column(look.width(LibraryMenuWidth).clip(shape).then(glass).border(1.dp, MenuEdge, shape).padding(vertical = 7.dp), content = content)
        }
    }
}

/** A menu row: its icon (a 40 frame) centred 29.5 in, the label from 52, a chevron 27.5 from the right. */
@Composable
internal fun MenuItem(icon: ImageVector, label: String, ink: Color = Pc.MenuInk, trailing: ImageVector? = null, iconSize: Dp = 40.dp, onClick: () -> Unit) {
    MenuItem(label, ink, trailing, onClick) { Icon(icon, contentDescription = null, tint = ink, modifier = Modifier.requiredSize(iconSize)) }
}

/** A menu row whose glyph [content] draws, centred where an icon's 40 frame sits. */
@Composable
internal fun MenuItem(label: String, ink: Color, trailing: ImageVector?, onClick: () -> Unit, content: @Composable () -> Unit) {
    Row(Modifier.fillMaxWidth().height(40.dp).clickable(onClick = onClick), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.padding(start = 9.5.dp).size(40.dp), contentAlignment = Alignment.Center) { content() }
        // the menus' 17 pt system text is as wide as 18 here
        Text(label, color = ink, fontSize = 18.sp, maxLines = 1, modifier = Modifier.padding(start = 2.5.dp).weight(1f))
        if (trailing != null) Icon(trailing, contentDescription = null, tint = ink, modifier = Modifier.padding(end = 7.5.dp).requiredSize(40.dp))
    }
}

@Composable
internal fun MenuDivider(gap: Dp = 10.dp) {
    Box(Modifier.padding(horizontal = 15.5.dp, vertical = gap).fillMaxWidth().height(1.dp).background(MenuRule))
}

/** Places a menu from its anchor's bounds and its own size, kept in the window below [top]. */
/** The set icon picker: its columns, how far left of the set it starts, and the room it keeps below. */
private const val PICKER_COLUMNS = 5
private val PickerInset = 11.dp
private val PickerMargin = 25.dp

internal class MenuPlace(private val top: Int = 0, private val place: (IntRect, IntSize) -> IntOffset) : PopupPositionProvider {
    override fun calculatePosition(anchorBounds: IntRect, windowSize: IntSize, layoutDirection: LayoutDirection, popupContentSize: IntSize): IntOffset {
        val p = place(anchorBounds, popupContentSize)
        return IntOffset(
            p.x.coerceIn(0, (windowSize.width - popupContentSize.width).coerceAtLeast(0)),
            p.y.coerceIn(top, (windowSize.height - popupContentSize.height).coerceAtLeast(top)),
        )
    }
}

/** A library's title menu: rename it, share it, or go back to every library's card. */
@Composable
private fun LibraryTitleMenu(vm: CanvasViewModel, open: Boolean, onDismiss: () -> Unit) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    LibraryMenu(open, androidx.compose.ui.graphics.TransformOrigin(0f, 0f), MenuPlace { a, _ -> IntOffset(a.left, a.bottom) }, onDismiss = onDismiss, overPanel = true) {
        MenuItem(PcIcons.MenuRename, "Rename") {
            onDismiss()
            vm.renamingLibrary = vm.libraryPlace
        }
        MenuItem(PcIcons.MenuShare, "Share") {
            onDismiss()
            scope.launch { vm.libraryFileToShare()?.let { shareFile(context, it, "application/octet-stream") } }
        }
        MenuDivider(11.dp)
        MenuItem(PcIcons.MenuLibraries, "Back to libraries") {
            onDismiss()
            vm.showingLibraries = true
        }
    }
}

/**
 * Two fingers pinched on a library and let go at this much of their spread at the start (or less)
 * go to every library's card; pinched out on a card past this much, that library opens. While they
 * pinch, a library's lists shrink with them no further than the least here, and grow no further
 * than the most.
 */
private const val PINCH_TO_LIBRARIES = 0.66f
private const val PINCH_TO_LIBRARY = 2f
private const val PINCH_SHOWN_LEAST = 0.6f
private const val PINCH_SHOWN_MOST = 1.05f

/**
 * Two fingers pinching here (its corner in the window at [origin]): [onPinch] hears how far (their
 * spread now, a part of what it was) and where they are (window pixels) as they go, and [onLet]
 * both when the first of them lifts. While they pinch, and after until the last lifts, their
 * touches go no further (the lists under them stay, and nothing is tapped or held); a lone
 * finger's go on as ever.
 */
private suspend fun PointerInputScope.awaitLibraryPinch(
    origin: () -> Offset,
    onPinch: (Float, Offset) -> Unit,
    onLet: (Float, Offset) -> Unit,
) {
    awaitEachGesture {
        awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
        var zoom = 1f
        var at: Offset? = null
        while (true) {
            val event = awaitPointerEvent(PointerEventPass.Initial)
            val down = event.changes.count { it.pressed }
            val pinching = at
            if (down >= 2) {
                zoom *= event.calculateZoom()
                val centre = event.calculateCentroid(useCurrent = true)
                val now = if (centre.isSpecified) origin() + centre else pinching ?: origin()
                at = now
                onPinch(zoom, now)
                event.changes.forEach { it.consume() }
            } else if (pinching != null) {
                event.changes.forEach { it.consume() }
                onLet(zoom, pinching)
                var rest = event
                while (rest.changes.any { it.pressed }) {
                    rest = awaitPointerEvent(PointerEventPass.Initial)
                    rest.changes.forEach { it.consume() }
                }
                break
            } else if (down == 0) {
                break
            }
        }
    }
}

private val CardHeight = 490.dp
private val CardGap = 8.dp
private val CardCorner = 16.dp
private val CardSetRow = 19.dp
private val CardBrushHeight = 30.dp
/** As many brushes as a card has room for under its name, the last cut off by its bottom: only these have their strokes drawn. */
private val CardBrushRows = ((CardHeight - 47.dp) / (CardBrushHeight + 2.dp)).toInt() + 1

/**
 * The library popover showing every library's card (tapped, that library is brought into use;
 * held, its menu), with "+" for a new one or one from a file and the title's menu to put the
 * shipped one back; or a library with no sets yet, with "+" to make one.
 */
@Composable
private fun LibrariesPanel(vm: CanvasViewModel, arrowX: Dp, modifier: Modifier, floating: FloatingFrame?) {
    val showing = vm.showingLibraries
    var titleMenu by remember { mutableStateOf(false) }
    var adding by remember { mutableStateOf(false) }
    var cardMenu by remember { mutableStateOf<Int?>(null) }
    // the cards where they show and the grid's corner (window pixels); two fingers pinching out:
    // the card they began on, and how far they have gone
    val cardBounds = remember { mutableStateMapOf<Int, Rect>() }
    var gridOrigin by remember { mutableStateOf(Offset.Zero) }
    var pinchedCard by remember { mutableStateOf<Int?>(null) }
    var cardZoom by remember { mutableFloatStateOf(1f) }
    Popover(floating?.width ?: LibraryWidth, arrowX, modifier, side = if (floating != null) ArrowSide.NONE else ArrowSide.TOP) {
        floating?.let { FloatingTop(it) }
        val renaming = !showing && vm.renamingLibrary == vm.libraryPlace
        Box {
            PanelHeader(
                when {
                    showing -> "Brush libraries"
                    renaming -> ""
                    else -> vm.libraryName
                },
                start = 20.dp,
                end = 12.dp,
                height = 52.dp,
                chevronOn = titleMenu,
                onTitle = { titleMenu = true },
                menu = {
                    if (renaming) InlineName(vm.libraryName, Pc.PanelTitle, 24.sp) { vm.renameLibrary(vm.libraryPlace, it) }
                    if (showing) {
                        LibraryMenu(titleMenu, androidx.compose.ui.graphics.TransformOrigin(0f, 0f), MenuPlace { a, _ -> IntOffset(a.left, a.bottom) }, onDismiss = { titleMenu = false }, overPanel = true) {
                            MenuItem(PcIcons.MenuRestore, "Restore default library") {
                                titleMenu = false
                                vm.restoreLibraries()
                            }
                        }
                    } else {
                        LibraryTitleMenu(vm, titleMenu) { titleMenu = false }
                    }
                },
            ) {
                Box(Modifier.padding(top = 12.dp).size(41.dp, 40.dp).clickable { adding = true }, contentAlignment = Alignment.Center) {
                    Icon(PcIcons.PlusMedium, contentDescription = "Add", tint = Pc.PanelGlyph, modifier = Modifier.requiredSize(41.dp))
                    LibraryMenu(adding, androidx.compose.ui.graphics.TransformOrigin(1f, 0f), MenuPlace { a, size -> IntOffset(a.right - size.width, a.bottom) }, onDismiss = { adding = false }, overPanel = true) {
                        if (showing) {
                            MenuItem(PcIcons.MenuNewLibrary, "Create new library") {
                                adding = false
                                vm.newLibrary()
                            }
                            MenuItem(PcIcons.MenuFolder, "Import from Files") {
                                adding = false
                                vm.pickBrushFile?.invoke()
                            }
                        } else {
                            MenuItem(PcIcons.MenuNewSet, "Create new set") {
                                adding = false
                                vm.newBrushSet()
                            }
                            MenuDivider(11.dp)
                            MenuItem(PcIcons.MenuFolder, "Import from Files") {
                                adding = false
                                vm.pickBrushFile?.invoke()
                            }
                        }
                    }
                }
            }
            LibraryGrabber(vm, floating)
        }
        if (showing) {
            LazyVerticalGrid(
                GridCells.Fixed(2),
                Modifier.weight(1f, fill = floating != null).onGloballyPositioned { gridOrigin = it.positionInWindow() }.pointerInput(Unit) {
                    // two fingers pinched out on a card: it grows with them, and let go far enough apart it opens
                    awaitLibraryPinch({ gridOrigin }, { zoom, at ->
                        if (pinchedCard == null) pinchedCard = cardBounds.entries.firstOrNull { it.value.contains(at) }?.key
                        cardZoom = zoom
                    }) { zoom, _ ->
                        val card = pinchedCard
                        pinchedCard = null
                        cardZoom = 1f
                        if (zoom > PINCH_TO_LIBRARY && card != null) vm.openLibrary(card)
                    }
                },
                contentPadding = PaddingValues(start = 12.dp, end = 12.dp, bottom = 12.dp),
                horizontalArrangement = Arrangement.spacedBy(CardGap),
                verticalArrangement = Arrangement.spacedBy(CardGap),
            ) {
                items(vm.brushLibraries.size) { i ->
                    val library = vm.brushLibraries[i]
                    androidx.compose.runtime.DisposableEffect(i) { onDispose { cardBounds.remove(i) } }
                    Box(
                        Modifier.onGloballyPositioned { cardBounds[i] = it.boundsInWindow() }.graphicsLayer {
                            val k = if (pinchedCard == i) cardZoom.coerceIn(1f, PINCH_TO_LIBRARY) else 1f
                            scaleX = k
                            scaleY = k
                        },
                    ) {
                        LibraryCard(vm, i, library, onOpen = { vm.openLibrary(i) }, onMenu = { cardMenu = i })
                        LibraryMenu(cardMenu == i, androidx.compose.ui.graphics.TransformOrigin(0.5f, 0f), MenuPlace { a, size -> IntOffset(a.left + (a.width - size.width) / 2, a.top + a.height / 4) }, onDismiss = { cardMenu = null }) {
                            MenuItem(PcIcons.MenuRename, "Rename") {
                                cardMenu = null
                                vm.renamingLibrary = i
                            }
                            MenuItem(PcIcons.MenuDuplicate, "Duplicate") {
                                cardMenu = null
                                vm.duplicateLibrary(i)
                            }
                            if (vm.brushLibraries.size > 1) {
                                MenuDivider(11.dp)
                                MenuItem(PcIcons.MenuTrash, "Delete", Pc.MenuDanger) {
                                    cardMenu = null
                                    vm.deletingLibrary = i
                                }
                            }
                        }
                    }
                }
            }
        } else {
            // a library with no sets yet
            Column(Modifier.fillMaxWidth().padding(horizontal = 32.dp, vertical = 48.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                Text("This library has no brushes yet", color = Pc.PanelTitle, fontSize = 17.sp, fontWeight = FontWeight.SemiBold, maxLines = 2)
                Text(
                    "Make a set with +, then brushes in it, or bring brushes in from your files.",
                    color = Pc.PanelGlyph,
                    fontSize = 14.sp,
                    modifier = Modifier.padding(top = 8.dp),
                )
            }
        }
        floating?.let { FloatingBottom(it) }
    }
    Alert(
        vm.deletingLibrary,
        title = { "Delete this library?" },
        message = { "Its sets and brushes go with it." },
        action = "Delete",
        onCancel = { vm.deletingLibrary = null },
        onAction = { vm.deleteLibrary(it) },
    )
}

/**
 * A library's card: its name over a small likeness of it, its sets down the left and the brushes
 * of one of them beside them; the one in use outlined in blue.
 */
@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
@Composable
private fun LibraryCard(vm: CanvasViewModel, i: Int, library: io.github.artcraftandroid.procraft.core.BrushLibraryInfo, onOpen: () -> Unit, onMenu: () -> Unit) {
    val tints = remember(vm.colorRgb(), vm.pluginLooks) { vm.previewTints() }
    val shape = RoundedCornerShape(CardCorner)
    Column(
        Modifier.fillMaxWidth().height(CardHeight).clip(shape).background(Pc.SetCell)
            .then(if (library.current) Modifier.border(2.dp, Pc.Accent, shape) else Modifier)
            .combinedClickable(onClick = onOpen, onLongClick = onMenu)
            .padding(10.dp),
    ) {
        if (vm.renamingLibrary == i) {
            InlineName(library.name, Pc.PanelTitle, 14.sp) { vm.renameLibrary(i, it) }
        } else {
            Text(library.name, color = Pc.PanelTitle, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        // the one in use's shown set; another's first with a brush chosen in it
        val chosen = if (library.current) vm.shownSet else library.sets.indexOfFirst { it.chosen != null }.coerceAtLeast(0)
        Row(Modifier.padding(top = 8.dp).fillMaxSize()) {
            Column(Modifier.weight(0.45f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                library.sets.forEachIndexed { s, set ->
                    Row(
                        Modifier.fillMaxWidth().height(CardSetRow).clip(RoundedCornerShape(5.dp))
                            .background(if (s == chosen) Pc.RowSelected else Pc.PopoverFill.copy(alpha = 0.35f)),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        val ink = if (s == chosen) Color.White else Pc.PanelTitle
                        Icon(PcIcons.SetIcons.getOrElse(set.icon.toInt()) { PcIcons.BrushSet }, contentDescription = null, tint = ink, modifier = Modifier.requiredSize(18.dp))
                        Text(set.name, color = ink, fontSize = 8.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
            Column(Modifier.weight(0.55f).padding(start = 6.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                library.sets.getOrNull(chosen)?.brushes?.take(CardBrushRows)?.forEach { b ->
                    // its name over its stroke, drawn small
                    val key = "$i-${b.set}-${b.index}"
                    val stroke = vm.cardPreviews[key]
                    LaunchedEffect(key, stroke == null) { if (stroke == null) vm.loadCardPreview(i, b.set.toInt(), b.index.toInt()) }
                    Box(Modifier.fillMaxWidth().height(CardBrushHeight)) {
                        stroke?.let { androidx.compose.foundation.Image(it, contentDescription = null, contentScale = androidx.compose.ui.layout.ContentScale.FillBounds, colorFilter = pluginFilter(tints.first), modifier = Modifier.fillMaxSize()) }
                        Text(b.name, color = Pc.PanelTitle, fontSize = 8.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(start = 3.dp, top = 1.dp))
                    }
                }
            }
        }
    }
}

/**
 * How much longer, milliseconds, a brush held still waits before its menu shows, the press seen
 * [sinceDown] milliseconds after the touch came down by its own clock (the core's rule: the menu
 * shows its measured hold after the finger landed).
 */
private fun brushMenuWaitMillis(sinceDown: Long): Long = (brushMenuWait(sinceDown / 1000.0) * 1000).roundToLong()

/**
 * The press [down] on a brush held still for [wait] milliseconds more: not lifted, not taken (the
 * list scrolling), not moved past the touch slop. Its last change then (where it took hold), else
 * null.
 */
private suspend fun AwaitPointerEventScope.awaitBrushHold(down: PointerInputChange, wait: Long): PointerInputChange? {
    var last = down
    val cut = withTimeoutOrNull(wait.coerceAtLeast(1L)) {
        var still = true
        while (still) {
            val change = awaitPointerEvent().changes.firstOrNull { it.id == down.id }
            still = change != null && change.pressed && !change.isConsumed &&
                (change.position - down.position).getDistance() <= viewConfiguration.touchSlop
            if (change != null && still) last = change
        }
        true
    }
    return if (cut == null) last else null
}

/**
 * What brushes dragged in the library are over at [at] (window pixels), for the core to say where
 * they land (`brushDropPlace`): a set's row of [sets] (by place); else, in the list [list] of the
 * shown set [shown] (none for the Recent set), which holds [count] brushes, the one of its [rows]
 * (by place in the set) nearest up or down, on its upper or lower half, or with none there the
 * list itself; else nothing.
 */
internal fun brushDropOver(at: Offset, sets: Map<Int, Rect>, shown: Int?, count: Int, list: Rect, rows: List<Pair<Int, Rect>>): io.github.artcraftandroid.procraft.core.BrushDropOver? {
    sets.entries.firstOrNull { it.value.contains(at) }?.let { return io.github.artcraftandroid.procraft.core.BrushDropOver.OnSet(it.key.toUInt()) }
    if (shown == null || shown < 0 || !list.contains(at)) return null
    // how far up or down from a row (none on it)
    fun off(r: Rect) = maxOf(r.top - at.y, at.y - r.bottom, 0f)
    val (index, row) = rows.minByOrNull { off(it.second) }
        ?: return io.github.artcraftandroid.procraft.core.BrushDropOver.InList(shown.toUInt(), count.coerceAtLeast(0).toUInt())
    return io.github.artcraftandroid.procraft.core.BrushDropOver.OnBrush(shown.toUInt(), index.toUInt(), at.y > row.center.y)
}

/**
 * The brushes dragged with [held]: those picked ([picked], by place "set-index") in its set and it,
 * in their order in its set ([brushes]), when it is one of them (the current brush, [selected],
 * counts as picked); else it alone.
 */
internal fun draggedTogether(held: BrushInfo, picked: List<String>, selected: Boolean, brushes: List<BrushInfo>): List<BrushInfo> {
    val key = "${held.set}-${held.index}"
    if (picked.isEmpty() || (key !in picked && !selected)) return listOf(held)
    val going = brushes.filter { b -> b.set == held.set && ("${b.set}-${b.index}" in picked || b.index == held.index) }
    return going.ifEmpty { listOf(held) }
}

/**
 * Where a brush's row slid sideways by [amount] comes to from [at] (pixels, left negative): open as
 * far as its buttons' [width], shut as far as the pick's [reach] to the right (none while [open]).
 */
internal fun brushSlide(at: Float, amount: Float, width: Float, reach: Float, open: Boolean): Float =
    (at + amount).coerceIn(-width, if (open) 0f else reach)

/**
 * A brush's row let go slid to [at]: whether it is picked (past half the [reach] to the right),
 * and whether it is open (past a third of its buttons' [width] to the left). A press that may not
 * slide it ([slides] false: its brush's menu showed) leaves it as it was ([open]), unpicked.
 */
internal fun brushSlideEnd(at: Float, width: Float, reach: Float, open: Boolean, slides: Boolean): Pair<Boolean, Boolean> {
    if (!slides) return false to open
    return (reach > 0f && at > reach / 2f) to (at < -width / 3f)
}

/**
 * How long a dragged brush is held over another set before it opens, to be let go among its
 * brushes (measured on the original: 1.76 s, from frames 0.1 s apart).
 */
internal const val SPRING_LOAD_MS = 1800L

/** Hanging from its button, the library's grabber tears it off into a window of its own, while a plugin lets it ([floating]: it floats). */
@Composable
private fun androidx.compose.foundation.layout.BoxScope.LibraryGrabber(vm: CanvasViewModel, floating: FloatingFrame?) {
    if (floating == null && io.github.artcraftandroid.procraft.core.FloatingPanel.BRUSHES in Pc.floatingPanels) {
        PanelGrabber(vm, io.github.artcraftandroid.procraft.core.FloatingPanel.BRUSHES)
    }
}
