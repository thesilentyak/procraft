import SwiftUI
import UniformTypeIdentifiers

private let libraryWidth: CGFloat = 440
private let setsWidth: CGFloat = 168
private let brushHeight: CGFloat = 80
private let brushGap: CGFloat = 8
private let namePill = Color(argb: 0x8000_0000)
/// The space the library's rows report their places in, and the drags follow.
private let librarySpace = "library"
/// A brush swiped left shows three buttons filling its place, this far apart.
private let swipeGap: CGFloat = 5.5
private let swipeFill = Color(argb: 0xFF3F_3F3F)
private let swipeInk = Color(argb: 0xFFC5_C5C5)
private let swipeDelete = Color(argb: 0xFFFF_3A46)
/// A brush slid right is picked, this fill over its row; it follows the finger this far.
private let pickedBrush = Color(argb: 0xFF2C_5281)
private let pickReach: CGFloat = 72
private let searchFill = Color(argb: 0x1F76_7680)
private let searchHint = Color(argb: 0x4DEB_EBF5)
private let resultFill = Color(argb: 0x14FF_FFFF)
/// The search field with the room above and below it, which scrolls away with the lists.
private let searchBlock: CGFloat = 60
/// Menus stay below the top bar.
private let menuTop: CGFloat = 57
/// A brush held goes on to follow the finger once it has moved this far, while it still may (the
/// core's brush_hold_move says until when).
private let heldSlop: CGFloat = 8
/// A brush's cell as the touch comes down: it shrinks to this (deepest about 0.09 s in) and is back
/// to its size about 0.2 s in, the finger still down, as measured (0.876 to 0.88).
private let pressShrink: CGFloat = 0.88
private let pressIn = 0.09
private let pressOut = 0.11
/// A brush held for its menu lifts, this much bigger, on a dark card (the column's grey 53 darkened
/// to 32, as measured), over about 0.15 s; a brush dragged is that card.
private let heldLift: CGFloat = 1.045
private let heldLiftTime = 0.15
private let heldCard = Color.black.opacity(0.4)
private var draggedCard: Color { Look.shared.light ? Color(argb: 0xFF86_8686) : Color(argb: 0xFF20_2020) }
/// A held brush's menu sits this far above it, their left edges in line, as measured.
private let brushMenuGap: CGFloat = 9
/// Lifted to be dragged, the brush's menu fades this quickly (about 3 frames at 31 a second).
private let brushMenuFade = 0.1
/// Two fingers pinched on a library and let go at this much of their spread at the start (or less)
/// go to every library's card; pinched out on a card past this much, that library opens. While they
/// pinch, a library's lists shrink with them no further than the least here, and grow no further
/// than the most.
let pinchToLibraries: CGFloat = 0.66
let pinchToLibrary: CGFloat = 2
private let pinchShownLeast: CGFloat = 0.6
private let pinchShownMost: CGFloat = 1.05
/// The set icon picker: how many of the icons it offers, its columns, how far left of the set it
/// starts, and the room it keeps below.
private let pickedSetIcons = 42
private let pickerColumns = 5
private let pickerInset: CGFloat = 11
private let pickerMargin: CGFloat = 25

/// A panel's title row: its title (with a chevron when it has a menu) at its left, `action` at its
/// right. With `rename`, the title is a field editing it, which takes the name once done.
struct PanelHeader<Action: View>: View {
    let title: String
    var start: CGFloat = 24
    var end: CGFloat = 18
    var height: CGFloat = 64
    var chevron = true
    var chevronOn = false
    var onTitle: (() -> Void)?
    var rename: ((String) -> Void)?
    @ViewBuilder let action: () -> Action

    var body: some View {
        ZStack(alignment: .topLeading) {
            HStack(spacing: 0) {
                if let rename {
                    InlineName(name: title, color: Pc.panelTitle, fontSize: 24, onDone: rename)
                        .fixedSize()
                } else {
                    Text(verbatim: title)
                        .font(.system(size: 24, weight: .bold))
                        .foregroundStyle(Pc.panelTitle)
                        .lineLimit(1)
                }
                if chevron {
                    ZStack {
                        Circle().fill(chevronOn ? Pc.rowSelected : Pc.chevronDisc)
                        PcIconView(PcIcons.chevronDown, size: 13, color: Pc.chevronGlyph)
                    }
                    .frame(width: 17, height: 17)
                    .padding(.leading, 10)
                    .offset(y: -1.5)
                }
            }
            .padding(.leading, start)
            .padding(.top, 19)
            .contentShape(Rectangle())
            .onTapGesture { onTitle?() }
            HStack {
                Spacer()
                action()
            }
            .padding(.trailing, end)
        }
        .frame(maxWidth: .infinity, minHeight: height, maxHeight: height, alignment: .topLeading)
    }
}

/// A brush held for its menu: which, whether it is pinned in the Recent set, whether the Recent set
/// shows it, and where its row was as the menu came up (the menu stays there while the list scrolls
/// under it).
private struct BrushMenuAsk {
    let brush: BrushInfo
    let pinned: Bool
    let recent: Bool
    let frame: CGRect
}

/// What the library's rows share while it is open: its menus, the search, and a brush or set held
/// and dragged, with where the rows are.
@Observable
private final class LibraryUi {
    var adding = false
    /// The title's menu: the library renamed or shared, or every library's card.
    var titleMenu = false
    var setMenu: Int?
    var recentMenu = false
    /// A brush held: its menu.
    var brushMenu: BrushMenuAsk?
    /// A brush is held down (its menu up, the brush perhaps about to follow the finger).
    var holding = false
    /// When a brush's hold was last over: the lift that ends a hold is no tap.
    @ObservationIgnored var holdEnded = Date.distantPast
    var iconPicker = false
    var importing = false
    var query = ""
    /// How far the search field (and the lists under it) has scrolled up out of sight: all of it
    /// when the library opens.
    var searchAway = searchBlock
    var dragged: BrushInfo?
    /// The brushes going with it (it among them, those picked in its set), in their order.
    var draggedWith: [BrushInfo] = []
    var draggedSet: Int?
    /// Where the finger is, and where on the row it took hold.
    var dragAt = CGPoint.zero
    var grab = CGPoint.zero
    /// The dragged brush's cell width (its row may go, as another set opens under the finger).
    var dragWidth: CGFloat = 0
    var setFrames: [Int: CGRect] = [:]
    var brushFrames: [String: CGRect] = [:]
    /// Where the shown set's brushes list is.
    var brushListFrame = CGRect.zero
    /// Follows the finger dragging brushes from the window, once their row has begun the drag.
    @ObservationIgnored weak var touches: LibraryTouches.Follower?
    /// Dragged brushes resting on another set's row open it.
    let spring = BrushSpring()

    /// The set the dragged brush or set is over, if any.
    var overSet: Int? {
        guard dragged != nil || draggedSet != nil else { return nil }
        return setFrames.first { $0.value.contains(dragAt) }?.key
    }

    var dragging: Bool {
        dragged != nil || draggedSet != nil
    }

    /// Whether `b` is being dragged, alone or with others.
    func isDragged(_ b: BrushInfo) -> Bool {
        if draggedWith.isEmpty {
            return dragged == b
        }
        return draggedWith.contains { $0.set == b.set && $0.index == b.index }
    }

    /// What the dragged brushes are over where the finger is: a set's row, or a brush of the set
    /// shown (which half), or its list; read from the library as it is now.
    func dropOver(_ model: CanvasModel) -> BrushDropOver? {
        let shown = model.shownSet
        let brushes = model.brushSets.indices.contains(shown) ? model.brushSets[shown].brushes : []
        let rows = brushes.compactMap { b in brushFrames[CanvasModel.brushKey(b)].map { (Int(b.index), $0) } }
        return brushDropOver(
            at: dragAt, sets: setFrames, shown: model.brushSets.indices.contains(shown) ? shown : nil,
            count: brushes.count, list: brushListFrame, rows: rows
        )
    }

    /// The places in their set of the brushes dragged (it, and those going with it).
    private var draggedIndices: [UInt32] {
        guard let b = dragged else { return [] }
        return (draggedWith.isEmpty ? [b] : draggedWith).filter { $0.set == b.set }.map(\.index)
    }

    /// Whether the dragged brush would be copied where the finger is now (onto another set, or a
    /// brush of another set), as the green plus on it says.
    func dropCopies(_ model: CanvasModel) -> Bool {
        guard let b = dragged, let over = dropOver(model) else { return false }
        return brushDropPlace(from: b.set, indices: draggedIndices, over: over)?.copies == true
    }

    /// A brush let go of, with those going with it: onto another set, copied to its top; onto a
    /// brush of the shown set not among them, just above it or just below it as the finger is on
    /// its upper or lower half (copied there from another set, moved along their own); into the
    /// list away from its brushes, after the last.
    func dropBrush(_ model: CanvasModel) {
        spring.stop()
        guard let b = dragged else { return }
        let moving = draggedWith.isEmpty ? [b] : draggedWith
        let over = dropOver(model)
        dragged = nil
        draggedWith = []
        if let over {
            model.dropBrushesOver(moving, over: over)
        }
    }

    /// The finger the window follows dragging brushes is at `at`: held a moment over another set's
    /// row, that set opens, the drag going on, to be let go among its brushes (the row the drag
    /// began on goes with its set, so only while the window follows the finger).
    func follow(_ at: CGPoint, _ model: CanvasModel) {
        dragAt = at
        let set = dragged == nil ? nil : brushSpringSet(at: at, sets: setFrames, shown: model.shownSet)
        spring.over(set) { [weak self, weak model] set in
            guard let self, let model, self.dragged != nil,
                  brushSpringSet(at: self.dragAt, sets: self.setFrames, shown: model.shownSet) == set else { return }
            model.showSet(set)
        }
    }

    /// The finger the window follows dragging brushes lifted at `at`: they land there. Taken away
    /// by the system (`cancelled`), they land nowhere.
    func letGo(_ at: CGPoint, cancelled: Bool, _ model: CanvasModel) {
        holding = false
        holdEnded = Date()
        spring.stop()
        if cancelled {
            dragged = nil
            draggedWith = []
            return
        }
        dragAt = at
        dropBrush(model)
    }

    /// A set dropped onto another takes its place.
    func dropSet(_ model: CanvasModel) {
        guard let from = draggedSet else { return }
        draggedSet = nil
        if let to = setFrames.first(where: { $0.value.contains(dragAt) })?.key {
            model.moveBrushSet(from: from, to: to)
        }
    }
}

private struct SetFrames: PreferenceKey {
    static let defaultValue: [Int: CGRect] = [:]
    static func reduce(value: inout [Int: CGRect], nextValue: () -> [Int: CGRect]) {
        value.merge(nextValue()) { $1 }
    }
}

private struct BrushFrames: PreferenceKey {
    static let defaultValue: [String: CGRect] = [:]
    static func reduce(value: inout [String: CGRect], nextValue: () -> [String: CGRect]) {
        value.merge(nextValue()) { $1 }
    }
}

private struct BrushListFrame: PreferenceKey {
    static let defaultValue = CGRect.zero
    static func reduce(value: inout CGRect, nextValue: () -> CGRect) {
        let next = nextValue()
        if next != .zero {
            value = next
        }
    }
}

/// What brushes dragged in the library are over at `at` (the library's space), for the core to say
/// where they land (`brushDropPlace`): a set's row of `sets` (by place; not the Recent set's);
/// else, in the list `list` of the shown set `shown` (none for the Recent set), which holds `count`
/// brushes, the one of its `rows` (by place in the set) nearest up or down, on its upper or lower
/// half, or with none there the list itself; else nothing.
func brushDropOver(
    at: CGPoint, sets: [Int: CGRect], shown: Int?, count: Int, list: CGRect, rows: [(Int, CGRect)]
) -> BrushDropOver? {
    if let set = sets.first(where: { $0.key >= 0 && $0.value.contains(at) })?.key {
        return .onSet(set: UInt32(set))
    }
    guard let shown, shown >= 0, list.contains(at) else { return nil }
    // how far up or down from a row (none on it)
    func off(_ r: CGRect) -> CGFloat { max(r.minY - at.y, at.y - r.maxY, 0) }
    guard let nearest = rows.filter({ $0.0 >= 0 }).min(by: { off($0.1) < off($1.1) }) else {
        return .inList(set: UInt32(shown), count: UInt32(max(count, 0)))
    }
    return .onBrush(set: UInt32(shown), index: UInt32(nearest.0), lower: at.y > nearest.1.midY)
}

/// The brush library hanging from the tool that opened it: the sets down its left, the shown
/// set's brushes (each its name over its sample stroke) on its right. Tap the chosen set for its
/// menu, swipe a brush left to share, duplicate or delete it, hold it for its menu (and drag it
/// straight on from there onto a set or to another place in its own; held a moment over another
/// set, that set opens to let it go among its brushes), pinch the lists in for every library's
/// card, and add brushes and sets with the plus.
struct BrushLibrary: View {
    let model: CanvasModel
    let arrowX: CGFloat
    /// The library's top left corner on the screen (its arrow's top), and the screen's size: the
    /// menus are kept on it.
    let origin: CGPoint
    let screen: CGSize
    /// Floating in a window of its own (a plugin lets it): its frame; none while it hangs.
    var floating: FloatingFrame?
    @State private var ui = LibraryUi()
    @FocusState private var searching: Bool
    /// Two fingers pinching on the lists: how far (their spread, a part of what it was). The lists
    /// follow, as far as the pinch can take them, and spring back when it isn't taken.
    @State private var pinched: CGFloat?

    var body: some View {
        let shown = model.shownSet == CanvasModel.recentSet || model.brushSets.indices.contains(model.shownSet)
        // every library's card, or a library with no sets yet: a panel of their own
        if model.showingLibraries || model.brushSets.isEmpty || !shown {
            LibrariesPanel(model: model, arrowX: arrowX, origin: origin, screen: screen, floating: floating)
        } else {
            library
        }
    }

    /// A word in the plus's place while brushes are picked.
    private func pickedAction(_ label: String, width: CGFloat, perform: @escaping () -> Void) -> some View {
        Text(verbatim: label)
            .font(.system(size: 15))
            .foregroundStyle(swipeInk)
            .lineLimit(1)
            .frame(width: width, height: 32)
            .contentShape(Rectangle())
            .onTapGesture(perform: perform)
    }

    private var library: some View {
        let renaming = model.renamingLibrary == model.libraryPlace
        // floating in a window of its own: as wide as the window, no arrow, a strip over it and under it
        return Popover(width: floating?.width ?? libraryWidth, arrowX: arrowX, side: floating == nil ? .top : .none) {
            if let floating {
                FloatingTopStrip(frame: floating)
            }
            PanelHeader(
                title: model.libraryName, start: 20, end: 12, height: 52,
                onTitle: { withAnimation(Motion.menuOpen) { ui.titleMenu.toggle() } },
                rename: renaming ? { model.renameLibrary(model.libraryPlace, name: $0) } : nil
            ) {
                if model.pickedBrushes.isEmpty {
                    PcIconView(PcIcons.plusMedium, size: 41, color: Pc.panelGlyph)
                        .frame(width: 41, height: 40)
                        .contentShape(Rectangle())
                        .onTapGesture { withAnimation(Motion.menuOpen) { ui.adding = true } }
                        .padding(.top, 12)
                } else {
                    // brushes picked: Delete in the plus's place, and Combine beside it for one
                    // with the current brush
                    HStack(spacing: 4) {
                        if model.canCombinePicked {
                            pickedAction("Combine", width: 80) { model.combinePicked() }
                        }
                        pickedAction("Delete", width: 63) {
                            model.question = Question(title: "Delete these brushes?") { model.deletePickedBrushes() }
                        }
                    }
                    .padding(.top, 16)
                }
            }
            VStack(spacing: 0) {
                // the search field scrolls away with the lists: out of sight at first, it comes down
                // as a list is pulled down past its top, and goes again as the lists scroll up; what
                // is left of it shows its lower part, as if slid up under the header
                let away = searching || !ui.query.isEmpty ? 0 : ui.searchAway
                LibrarySearch(ui: ui, focus: $searching)
                    .frame(height: searchBlock - away, alignment: .bottom)
                    .clipped()
                    .contentShape(Rectangle())
                if searching || !ui.query.isEmpty {
                    SearchResults(model: model, query: ui.query) { b in
                        // as the original: the search ends, the field goes, and the brush shows in
                        // its own set, chosen there and scrolled to (the set column too, as it
                        // comes back)
                        ui.query = ""
                        searching = false
                        ui.searchAway = searchBlock
                        model.findBrush(b)
                    }
                } else {
                    HStack(spacing: 0) {
                        SetColumn(model: model, ui: ui)
                        BrushColumn(model: model, ui: ui)
                    }
                    .frame(maxHeight: .infinity)
                }
            }
            .frame(maxHeight: .infinity)
            // two fingers pinched in on them shrink them, and let go small enough show every
            // library's card
            .scaleEffect(min(max(pinched ?? 1, pinchShownLeast), pinchShownMost))
            .clipped()
            .simultaneousGesture(
                MagnifyGesture()
                    .onChanged { pinched = $0.magnification }
                    .onEnded { value in
                        withAnimation(Motion.swipeSettle) { pinched = nil }
                        if value.magnification <= pinchToLibraries {
                            ui.brushMenu = nil
                            model.renamingBrush = nil
                            model.showingLibraries = true
                        }
                    }
            )
            if floating != nil {
                FloatingBottomStrip()
            }
        }
        // hanging from its button, its grabber tears it off into a window of its own, while a
        // plugin lets it
        .overlay(alignment: .top) {
            if floating == nil, model.floating.allowed(.brushes) {
                PanelGrabber(model: model, panel: .brushes)
                    .padding(.top, Pc.arrowHeight)
            }
        }
        // under it all, filling it: where the window follows a finger dragging brushes from
        .background(LibraryTouches(model: model, ui: ui))
        .coordinateSpace(.named(librarySpace))
        .onPreferenceChange(SetFrames.self) { ui.setFrames = $0 }
        .onPreferenceChange(BrushFrames.self) { ui.brushFrames = $0 }
        .onPreferenceChange(BrushListFrame.self) { ui.brushListFrame = $0 }
        .overlay(alignment: .topLeading) { LibraryMenus(model: model, ui: ui, origin: origin, screen: screen) }
        .overlay(alignment: .topLeading) { DragGhost(model: model, ui: ui) }
        .fileImporter(isPresented: $ui.importing, allowedContentTypes: [.item]) { result in
            if case let .success(url) = result {
                model.importBrushes(from: url)
            }
        }
        .onDisappear {
            model.renamingBrush = nil
            model.libraryClosed()
        }
        #if DEBUG
        .onAppear {
            switch ScreenCapture.libraryShows {
            case "add":
                ui.adding = true
            case "title":
                ui.titleMenu = true
            case "set-menu":
                ui.setMenu = model.shownSet
            case "icons":
                ui.setMenu = model.shownSet
                ui.iconPicker = true
            case "search":
                ui.searchAway = 0
            case "recent":
                model.showSet(CanvasModel.recentSet)
            case "swipe":
                // once the row is up, as a finger would find it
                DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
                    model.swipedBrush = model.currentBrush.map { CanvasModel.brushKey($0) }
                }
            default:
                break
            }
        }
        #endif
    }
}

/// The library's search field: a magnifier, what is typed, and a button clearing it.
private struct LibrarySearch: View {
    let ui: LibraryUi
    let focus: FocusState<Bool>.Binding

    var body: some View {
        @Bindable var ui = ui
        HStack(spacing: 0) {
            Canvas { context, size in
                // a magnifier: a ring and its handle
                let c = CGPoint(x: size.width * 0.47, y: size.height * 0.46)
                let r: CGFloat = 7
                context.stroke(Path(ellipseIn: CGRect(x: c.x - r, y: c.y - r, width: r * 2, height: r * 2)), with: .color(searchHint), lineWidth: 2)
                let d = r * 0.7071
                var handle = Path()
                handle.move(to: CGPoint(x: c.x + d, y: c.y + d))
                handle.addLine(to: CGPoint(x: c.x + d + 5, y: c.y + d + 5))
                context.stroke(handle, with: .color(searchHint), style: StrokeStyle(lineWidth: 2.4, lineCap: .round))
            }
            .frame(width: 41, height: 40)
            ZStack(alignment: .leading) {
                if ui.query.isEmpty {
                    Text(verbatim: "Search")
                        .font(.system(size: 17))
                        .foregroundStyle(searchHint)
                        .lineLimit(1)
                }
                TextField(text: $ui.query) { EmptyView() }
                    .font(.system(size: 17))
                    .foregroundStyle(Color.white)
                    .tint(Pc.accent)
                    .autocorrectionDisabled()
                    .submitLabel(.search)
                    .focused(focus)
            }
            if !ui.query.isEmpty {
                // a filled circle with a cross cut out of it
                Canvas { context, size in
                    let c = CGPoint(x: size.width / 2, y: size.height / 2)
                    let r = min(size.width, size.height) / 2
                    context.fill(Path(ellipseIn: CGRect(x: c.x - r, y: c.y - r, width: r * 2, height: r * 2)), with: .color(searchHint))
                    let k: CGFloat = 3.5
                    for s: CGFloat in [1, -1] {
                        var line = Path()
                        line.move(to: CGPoint(x: c.x - k, y: c.y - k * s))
                        line.addLine(to: CGPoint(x: c.x + k, y: c.y + k * s))
                        context.stroke(line, with: .color(.black), style: StrokeStyle(lineWidth: 1.8, lineCap: .round))
                    }
                }
                .frame(width: 19, height: 18)
                .contentShape(Rectangle())
                .onTapGesture { ui.query = "" }
                .padding(.trailing, 5)
            }
        }
        .frame(height: 40)
        .background(RoundedRectangle(cornerRadius: 10, style: .circular).fill(searchFill))
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
    }
}

/// The brushes of every set whose name holds the search, two to a row; tapping one (`onPick`) ends
/// the search and shows it in its own set, chosen there.
private struct SearchResults: View {
    let model: CanvasModel
    let query: String
    let onPick: (BrushInfo) -> Void

    var body: some View {
        let wanted = query.trimmingCharacters(in: .whitespaces)
        let found = wanted.isEmpty ? [] : model.brushSets.flatMap(\.brushes).filter { $0.name.localizedCaseInsensitiveContains(wanted) }
        if found.isEmpty {
            Text(verbatim: "No results")
                .font(.system(size: 15, weight: .medium))
                .foregroundStyle(Pc.panelTitle)
                .padding(.top, 24)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        } else {
            ScrollView {
                let tints = model.previewTints()
                LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 6), count: 2), spacing: 6) {
                    ForEach(found, id: \.self) { b in
                        ResultCell(model: model, brush: b, tints: tints, onPick: onPick)
                    }
                }
                .padding(.horizontal, 12)
                .padding(.top, 11)
                .padding(.bottom, 12)
            }
            .scrollIndicators(.hidden)
            .scrollDismissesKeyboard(.immediately)
        }
    }
}

private struct ResultCell: View {
    let model: CanvasModel
    let brush: BrushInfo
    let tints: PreviewTints
    let onPick: (BrushInfo) -> Void

    var body: some View {
        let key = CanvasModel.brushKey(brush)
        let preview = model.previews[key]
        let selected = model.highlighted(brush)
        let shape = RoundedRectangle(cornerRadius: 12, style: .circular)
        ZStack(alignment: .topLeading) {
            shape.fill(selected ? Pc.rowSelected : resultFill)
            if let preview {
                SampleStroke(image: preview, tint: selected ? tints.chosen : tints.other)
            }
            BrushName(name: brush.name, selected: selected, shaded: model.shadedNames[key] == true)
                .padding(.leading, 14)
                .padding(.top, 11)
                .padding(.trailing, 10)
        }
        .frame(height: 81)
        .clipShape(shape)
        .contentShape(Rectangle())
        .onTapGesture { onPick(brush) }
        .task(id: preview == nil) {
            if preview == nil {
                model.loadPreview(brush)
            }
        }
    }
}

/// A brush's sample stroke filling its cell's height, centred, its ends past the cell's sides. It
/// lies over a clear view: wider than the cell, it would otherwise widen everything sized with it.
struct SampleStroke: View {
    let image: UIImage
    var tint: Color?

    var body: some View {
        Color.clear
            .overlay {
                if let tint {
                    Image(uiImage: image)
                        .renderingMode(.template)
                        .resizable()
                        .scaledToFill()
                        .foregroundStyle(tint)
                } else {
                    Image(uiImage: image)
                        .resizable()
                        .scaledToFill()
                }
            }
            .clipped()
    }
}

/// A brush's name on its cell: on a dark pill where its sample stroke runs under it.
private struct BrushName: View {
    let name: String
    let selected: Bool
    let shaded: Bool

    var body: some View {
        Text(verbatim: name)
            .font(.system(size: 14, weight: .semibold))
            .foregroundStyle(selected ? Color.white : Pc.panelTitle)
            .lineLimit(1)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background {
                if shaded {
                    Capsule().fill(namePill)
                }
            }
            .offset(x: -6, y: -2)
    }
}

/// A row's touch held still, then dragged, in the library's own space: `began` hears where it took
/// hold, and `ended` that it is over, let go or taken away by the system alike (a gesture's state
/// goes back either way, where its `onEnded` is only told of the first).
private struct HeldDrag: ViewModifier {
    let ui: LibraryUi
    let began: (CGPoint) -> Void
    let ended: () -> Void
    @GestureState private var held = false

    func body(content: Content) -> some View {
        content
            .simultaneousGesture(
                LongPressGesture(minimumDuration: 0.5)
                    .sequenced(before: DragGesture(minimumDistance: 0, coordinateSpace: .named(librarySpace)))
                    .updating($held) { value, state, _ in
                        if case .second(true, _) = value {
                            state = true
                        }
                    }
                    .onChanged { value in
                        if case let .second(true, drag?) = value {
                            if !ui.dragging {
                                began(drag.startLocation)
                            }
                            ui.dragAt = drag.location
                        }
                    }
            )
            .onChange(of: held) { _, now in
                if !now {
                    ended()
                }
            }
    }
}

/// A brush's touch: as it comes down the cell shrinks a moment and comes back; held still the core's
/// brush menu time (0.33 s, as measured), its menu shows (`held`) while the finger is still down,
/// the brush lifting on a dark card (let go before, it was a tap). Moved on within the core's drag
/// window after that, the menu goes and the brush follows the finger (`began` hears where it took
/// hold, in the library's own space); moved on later, the hold is over and the list has the touch,
/// scrolling under the menu, which stays; let go still, the menu stays and nothing is chosen. Once
/// it follows the finger the window follows it, past this row's life (another set may open under
/// it), and lets the brush go; when it can't, the row does, and `ended` hears that the touch is
/// over, let go or taken away by the system alike.
private struct HeldBrush: ViewModifier {
    let ui: LibraryUi
    /// It may be lifted out (the Recent set's brushes stay where they are).
    let movable: Bool
    let held: () -> Void
    let began: (CGPoint) -> Void
    let ended: () -> Void
    @GestureState private var touching = false
    @GestureState private var pressed = false
    /// When the hold was made (its menu showing), and whether a move has had its say since: the
    /// brush lifted, or the list given the touch.
    @State private var heldAt = Date.distantPast
    @State private var decided = false
    @State private var shrink: CGFloat = 1

    func body(content: Content) -> some View {
        // held, its menu up and the finger still on it (not dragged on): lifted on its dark card
        let lifted = pressed && !decided
        return content
            .background {
                RoundedRectangle(cornerRadius: 12, style: .circular)
                    .fill(heldCard)
                    .opacity(lifted ? 1 : 0)
            }
            .scaleEffect(shrink * (lifted ? heldLift : 1))
            .animation(.easeOut(duration: heldLiftTime), value: lifted)
            .simultaneousGesture(
                LongPressGesture(minimumDuration: brushMenuHoldSeconds())
                    .sequenced(before: DragGesture(minimumDistance: 0, coordinateSpace: .named(librarySpace)))
                    .updating($touching) { _, state, _ in
                        state = true
                    }
                    .updating($pressed) { value, state, _ in
                        if case .second(true, _) = value {
                            state = true
                        }
                    }
                    .onChanged { value in
                        guard case let .second(true, drag) = value else { return }
                        if !ui.holding, !ui.dragging {
                            ui.holding = true
                            heldAt = Date()
                            decided = false
                            held()
                        }
                        guard let drag else { return }
                        if !decided, !ui.dragging {
                            let moved = Float(hypot(drag.translation.width, drag.translation.height))
                            switch brushHoldMove(moved: moved, slop: Float(heldSlop), sinceMenu: Date().timeIntervalSince(heldAt), movable: movable) {
                            case .stays:
                                break
                            case .drags:
                                decided = true
                                began(drag.startLocation)
                                if ui.dragged != nil {
                                    ui.touches?.take(near: drag.location)
                                }
                            case .leaves:
                                // too late to lift it: the list has the touch, the menu staying
                                decided = true
                            }
                        }
                        if ui.touches?.following != true {
                            ui.dragAt = drag.location
                        }
                    }
            )
            .onChange(of: touching) { _, now in
                guard now else { return }
                // the press: in and back out, the finger still down
                withAnimation(.easeOut(duration: pressIn)) {
                    shrink = pressShrink
                } completion: {
                    withAnimation(.easeInOut(duration: pressOut)) { shrink = 1 }
                }
            }
            .onChange(of: pressed) { _, now in
                if !now {
                    ui.holding = false
                    ui.holdEnded = Date()
                    // the window lets go of what it follows
                    if ui.touches?.following != true {
                        ended()
                    }
                }
            }
    }
}

/// Follows, from the window, the finger dragging brushes in the library: once their row has begun
/// the drag (`take`), on past the row's own life (as another set opens under the finger, the shown
/// set's rows give way to its), and lets them go where it lifts. Its view lies under the library,
/// filling it, so a touch's place in it is its place in the library's space.
private struct LibraryTouches: UIViewRepresentable {
    let model: CanvasModel
    let ui: LibraryUi

    func makeUIView(context: Context) -> Follower {
        let view = Follower()
        view.isUserInteractionEnabled = false
        view.model = model
        view.ui = ui
        ui.touches = view
        return view
    }

    func updateUIView(_ view: Follower, context: Context) {
        view.model = model
        view.ui = ui
        ui.touches = view
    }

    final class Follower: UIView, UIGestureRecognizerDelegate {
        weak var model: CanvasModel?
        weak var ui: LibraryUi?
        /// Sees every touch on the window from the time the library is on it, taking none.
        private var watcher: TouchWatcher?
        /// The finger dragging brushes, and how far its place in the library's space is from its
        /// place here (nothing, as they lie one on the other, but measured as it is taken).
        private var held: UITouch?
        private var shift = CGSize.zero

        /// Whether a finger dragging brushes is followed.
        var following: Bool {
            held != nil
        }

        override func didMoveToWindow() {
            super.didMoveToWindow()
            if let watcher, watcher.view !== window {
                watcher.view?.removeGestureRecognizer(watcher)
                self.watcher = nil
                held = nil
            }
            guard let window, watcher == nil else { return }
            let w = TouchWatcher()
            w.cancelsTouchesInView = false
            w.delaysTouchesBegan = false
            w.delaysTouchesEnded = false
            w.delegate = self
            w.onMove = { [weak self] touch in self?.moved(touch) }
            w.onEnd = { [weak self] touch, cancelled in self?.ended(touch, cancelled: cancelled) }
            window.addGestureRecognizer(w)
            watcher = w
        }

        func gestureRecognizer(_ g: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
            true
        }

        /// The finger at `at` (the library's space) has begun dragging brushes: followed from now
        /// on, if the watcher saw it come down.
        func take(near at: CGPoint) {
            guard held == nil, let watcher, let touch = watcher.nearest(to: at, in: self) else { return }
            let here = touch.location(in: self)
            held = touch
            shift = CGSize(width: at.x - here.x, height: at.y - here.y)
        }

        /// Where `touch` is in the library's space.
        private func place(_ touch: UITouch) -> CGPoint {
            let here = touch.location(in: self)
            return CGPoint(x: here.x + shift.width, y: here.y + shift.height)
        }

        private func moved(_ touch: UITouch) {
            guard let held, held === touch, let ui, let model else { return }
            ui.follow(place(touch), model)
        }

        private func ended(_ touch: UITouch, cancelled: Bool) {
            guard let held, held === touch else { return }
            self.held = nil
            guard let ui, let model else { return }
            ui.letGo(place(touch), cancelled: cancelled, model)
        }
    }
}

/// The library's sets, the shown one lit, the Recent set above them. It opens scrolled as little as
/// brings the shown set into sight (the brushes in use may be in a set far down the list).
private struct SetColumn: View {
    let model: CanvasModel
    let ui: LibraryUi

    var body: some View {
        ScrollViewReader { reader in
            ScrollView {
                VStack(spacing: 0) {
                    SearchScroll(ui: ui)
                        .frame(height: 0)
                    LazyVStack(spacing: 12) {
                        RecentRow(model: model, ui: ui)
                            .id(CanvasModel.recentSet)
                        ForEach(Array(model.brushSets.enumerated()), id: \.offset) { i, set in
                            SetRow(model: model, ui: ui, index: i, set: set)
                                .id(i)
                        }
                    }
                    .padding(12)
                }
            }
            .onAppear { reader.scrollTo(model.shownSet) }
        }
        .scrollIndicators(.hidden)
        .scrollDisabled(ui.dragging || ui.holding)
        .frame(width: setsWidth)
        .background(Pc.setColumn)
    }
}

/// Gives the search field above the lists its share of their scrolling, as if the field and the
/// list were one column: a list scrolled up moves the field away first, and one pulled down past
/// its top by a finger (not flung there) brings the field back first. At the top of a list's
/// content, it finds the scroll view holding it.
private struct SearchScroll: UIViewRepresentable {
    let ui: LibraryUi

    func makeUIView(context: Context) -> Sharer {
        let view = Sharer()
        view.ui = ui
        return view
    }

    func updateUIView(_ view: Sharer, context: Context) {
        view.ui = ui
    }

    final class Sharer: UIView {
        weak var ui: LibraryUi?
        private weak var list: UIScrollView?
        private var watch: NSKeyValueObservation?
        /// Where the list was scrolled to when last seen.
        private var last: CGFloat = 0
        /// Set while the list is moved back by what the field took.
        private var giving = false

        override func didMoveToWindow() {
            super.didMoveToWindow()
            guard window != nil else {
                watch = nil
                list = nil
                return
            }
            var under = superview
            while let view = under, !(view is UIScrollView) {
                under = view.superview
            }
            guard let found = under as? UIScrollView, found !== list else { return }
            list = found
            last = found.contentOffset.y
            watch = found.observe(\.contentOffset, options: [.new]) { [weak self] list, _ in
                MainActor.assumeIsolated { self?.moved(list) }
            }
        }

        private func moved(_ list: UIScrollView) {
            guard !giving, let ui else { return }
            let top = -list.adjustedContentInset.top
            let y = list.contentOffset.y
            let step = y - last
            last = y
            if step > 0, y > top, ui.searchAway < searchBlock {
                // scrolling up, the field goes first
                let taken = min(step, searchBlock - ui.searchAway, y - top)
                ui.searchAway += taken
                give(list, to: y - taken)
            } else if y < top, list.isTracking, ui.searchAway > 0 {
                // pulled down past the top, the field comes back first
                let taken = min(top - y, ui.searchAway)
                ui.searchAway -= taken
                give(list, to: y + taken)
            }
        }

        private func give(_ list: UIScrollView, to y: CGFloat) {
            giving = true
            list.contentOffset.y = y
            giving = false
            last = y
        }
    }
}

/// The brushes last painted with, and those pinned there, above the sets.
private struct RecentRow: View {
    let model: CanvasModel
    let ui: LibraryUi

    var body: some View {
        let recent = model.shownSet == CanvasModel.recentSet
        let ink = recent ? Color.white : Pc.panelTitle
        HStack(spacing: 0) {
            PcIconView(PcIcons.recentSet, size: 40, color: ink)
                .padding(.leading, 4)
            Text(verbatim: "Recent")
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(ink)
                .lineLimit(1)
                .padding(.leading, 1)
                .padding(.trailing, 8)
            Spacer(minLength: 0)
        }
        .frame(height: 50)
        .background(RoundedRectangle(cornerRadius: 12, style: .circular).fill(recent ? Pc.rowSelected : Pc.setCell))
        .background(GeometryReader { g in
            Color.clear.preference(key: SetFrames.self, value: [CanvasModel.recentSet: g.frame(in: .named(librarySpace))])
        })
        .contentShape(Rectangle())
        .onTapGesture {
            if recent {
                withAnimation(Motion.menuOpen) { ui.recentMenu = true }
            } else {
                model.showSet(CanvasModel.recentSet)
            }
        }
    }
}

/// A set: tapped, it shows (tapped again, its menu); held, it lifts and goes where it is dragged,
/// faint where it was.
private struct SetRow: View {
    let model: CanvasModel
    let ui: LibraryUi
    let index: Int
    let set: BrushSetInfo

    var body: some View {
        let selected = index == model.shownSet
        let ink = selected ? Color.white : Pc.panelTitle
        let fill = ui.overSet == index ? Pc.accent : selected ? Pc.rowSelected : Pc.setCell
        HStack(spacing: 0) {
            // a 40 frame centred 24 in
            PcIconView(model.setIcon(set), size: 40, color: ink)
                .padding(.leading, 4)
            if model.renamingSet == index {
                InlineName(name: set.name, color: ink, fontSize: 14) { model.renameBrushSet(index, name: $0) }
                    .padding(.leading, 1)
                    .padding(.trailing, 8)
            } else {
                Text(verbatim: set.name)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(ink)
                    .lineLimit(2)
                    .padding(.leading, 1)
                    .padding(.trailing, 8)
            }
            Spacer(minLength: 0)
        }
        .frame(height: 50)
        .background(RoundedRectangle(cornerRadius: 12, style: .circular).fill(fill))
        .opacity(ui.draggedSet == index ? 0.3 : 1)
        .background(GeometryReader { g in
            Color.clear.preference(key: SetFrames.self, value: [index: g.frame(in: .named(librarySpace))])
        })
        .contentShape(Rectangle())
        .onTapGesture {
            if selected {
                withAnimation(Motion.menuOpen) { ui.setMenu = index }
            } else {
                model.showSet(index)
            }
        }
        .modifier(HeldDrag(
            ui: ui,
            began: { at in
                ui.setMenu = nil
                let frame = ui.setFrames[index] ?? .zero
                ui.grab = CGPoint(x: at.x - frame.minX, y: at.y - frame.minY)
                ui.draggedSet = index
            },
            ended: { ui.dropSet(model) }
        ))
    }
}

/// A brush the list shows, and whether it is pinned in the Recent set.
private struct ListedBrush: Identifiable {
    var id: String { CanvasModel.brushKey(brush) }
    let brush: BrushInfo
    let pinned: Bool
}

/// The shown set's brushes (or the Recent set's, each marked if pinned).
private struct BrushColumn: View {
    let model: CanvasModel
    let ui: LibraryUi

    private var listed: [ListedBrush] {
        if model.shownSet == CanvasModel.recentSet {
            return model.recentBrushes.compactMap { r -> ListedBrush? in
                guard model.brushSets.indices.contains(Int(r.set)) else { return nil }
                let brushes = model.brushSets[Int(r.set)].brushes
                guard brushes.indices.contains(Int(r.index)) else { return nil }
                return ListedBrush(brush: brushes[Int(r.index)], pinned: r.pinned)
            }
        }
        guard model.brushSets.indices.contains(model.shownSet) else { return [] }
        return model.brushSets[model.shownSet].brushes.map { ListedBrush(brush: $0, pinned: false) }
    }

    /// Whether `b` is lit as its row shows it: in the Recent set the current brush alone.
    private func litInList(_ b: BrushInfo) -> Bool {
        if model.shownSet == CanvasModel.recentSet {
            return model.currentBrush.map { $0.set == b.set && $0.index == b.index } == true
        }
        return model.highlighted(b)
    }

    var body: some View {
        let listed = listed
        let tints = model.previewTints()
        ScrollViewReader { reader in
            ScrollView {
                VStack(spacing: 0) {
                    SearchScroll(ui: ui)
                        .frame(height: 0)
                    LazyVStack(spacing: brushGap) {
                        ForEach(listed) { item in
                            BrushRow(model: model, ui: ui, brush: item.brush, pinned: item.pinned, tints: tints)
                                .id(item.id)
                        }
                    }
                    .padding(.leading, 12)
                    .padding(.trailing, 12)
                    .padding(.top, 9.75)
                    .padding(.bottom, 12)
                }
            }
            // a brush found from the Recent set is scrolled to in its own; else the set's lit
            // brush is brought into sight as little as it takes
            .onChange(of: model.foundBrush, initial: true) { _, found in
                if let found {
                    reader.scrollTo(found, anchor: .center)
                    model.foundBrush = nil
                } else if let lit = listed.first(where: { litInList($0.brush) }) {
                    reader.scrollTo(lit.id)
                }
            }
        }
        .background(GeometryReader { g in
            Color.clear.preference(key: BrushListFrame.self, value: g.frame(in: .named(librarySpace)))
        })
        // another set's brushes replace these at once
        .id(model.shownSet)
        .scrollIndicators(.hidden)
        // still while a brush is dragged; a brush held and moved on too late to lift it, the list
        // has the touch and scrolls under its menu
        .scrollDisabled(ui.dragging)
        .frame(maxWidth: .infinity)
        .background(Pc.brushColumn)
    }
}

/// A button a swiped brush shows.
private struct SwipeAction: Identifiable {
    var id: String { label }
    let icon: PcIcon
    let label: String
    var fill = swipeFill
    var ink = swipeInk
    let perform: () -> Void
}

/// A brush that slides away to the left, showing its `actions` in its place: Share, Duplicate and
/// Delete; in the Recent set Find, Pin (or Unpin) and Clear. Slid right past half of `pickReach`
/// it is picked (or no longer), if it can be (`onPick`), and slides back. Open, any slide right
/// closes it, quick or slow and let go wherever (the slide is read on the whole row: open, the
/// brush itself is out of it, the buttons filling it). While it is `held` (its menu up, or it
/// dragged on from there), it stays where it is.
private struct SwipeRow<Content: View>: View {
    let open: Bool
    let onOpen: (Bool) -> Void
    let actions: [SwipeAction]
    var onPick: (() -> Void)?
    var held: () -> Bool = { false }
    @ViewBuilder let content: () -> Content

    @State private var width: CGFloat = 0
    @State private var offset: CGFloat = 0
    /// The touch is going along the row, not down the list.
    @State private var sliding = false

    private var rest: CGFloat {
        open ? -width : 0
    }

    var body: some View {
        ZStack {
            if offset < 0 {
                HStack(spacing: swipeGap) {
                    ForEach(actions) { a in
                        VStack(spacing: 2) {
                            // 19 across, as measured
                            PcIconView(a.icon, size: 19, color: a.ink)
                            Text(verbatim: a.label)
                                .font(.system(size: 14, weight: .medium))
                                .foregroundStyle(a.ink)
                                .lineLimit(1)
                        }
                        .padding(.top, 5)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .background(RoundedRectangle(cornerRadius: 16, style: .circular).fill(a.fill))
                        .contentShape(Rectangle())
                        .onTapGesture {
                            onOpen(false)
                            a.perform()
                        }
                    }
                }
            }
            content()
                .offset(x: offset)
        }
        .simultaneousGesture(
            DragGesture(minimumDistance: 14)
                .onChanged { drag in
                    // a hold has the rest of the touch: no buttons show under a dragged brush
                    if held() {
                        return
                    }
                    if !sliding, abs(drag.translation.width) > abs(drag.translation.height) {
                        sliding = true
                    }
                    if sliding {
                        let right: CGFloat = open || onPick == nil ? 0 : pickReach
                        offset = min(max(rest + drag.translation.width, -width), right)
                    }
                }
                .onEnded { drag in
                    guard sliding else { return }
                    sliding = false
                    if !open, offset > pickReach / 2, let onPick {
                        onPick()
                    }
                    // open, a slide right closes it however far it went; closed, a slide left past a
                    // third of the way opens it
                    let opened = open ? drag.translation.width <= 0 : offset < -width / 3
                    if opened == open {
                        withAnimation(Motion.swipeSettle) { offset = rest }
                    } else {
                        onOpen(opened)
                    }
                }
        )
        .background(GeometryReader { g in
            Color.clear
                .onAppear { width = g.size.width }
                .onChange(of: g.size.width) { _, w in width = w }
        })
        .clipped()
        .onChange(of: open) { _, _ in
            withAnimation(Motion.swipeSettle) { offset = rest }
        }
    }
}

/// A brush in the library: its sample stroke filling the cell's height, its name over it (on a
/// dark pill where the stroke runs under it), lit when it is the current brush or its set's last.
private struct BrushRow: View {
    let model: CanvasModel
    let ui: LibraryUi
    let brush: BrushInfo
    let pinned: Bool
    let tints: PreviewTints

    private var actions: [SwipeAction] {
        let b = brush
        let model = model
        if model.shownSet == CanvasModel.recentSet {
            return [
                SwipeAction(icon: PcIcons.swipeFind, label: "Find") { model.findBrush(b) },
                pinned
                    ? SwipeAction(icon: PcIcons.swipeUnpin, label: "Unpin") { model.pinRecentBrush(b, pinned: false) }
                    : SwipeAction(icon: PcIcons.swipePin, label: "Pin") { model.pinRecentBrush(b, pinned: true) },
                SwipeAction(icon: PcIcons.swipeClear, label: "Clear") { model.clearRecentBrush(b) },
            ]
        }
        return [
            SwipeAction(icon: PcIcons.swipeShare, label: "Share") { model.shareBrush(b) },
            SwipeAction(icon: PcIcons.swipeDuplicate, label: "Duplicate") { model.duplicateBrush(b) },
            SwipeAction(icon: PcIcons.swipeTrash, label: "Delete", fill: swipeDelete, ink: .white) {
                model.question = Question(title: "Delete this brush?") { model.deleteBrush(b) }
            },
        ]
    }

    var body: some View {
        let key = CanvasModel.brushKey(brush)
        let recent = model.shownSet == CanvasModel.recentSet
        // the current brush, or the brush last chosen in this set (in the Recent set the current
        // brush alone)
        let selected = recent
            ? model.currentBrush.map { $0.set == brush.set && $0.index == brush.index } == true
            : model.highlighted(brush)
        // a plugin may colour the sample strokes by the colour now; the samples are drawn white,
        // dark on the light interface's cells otherwise
        let tint = (selected ? tints.chosen : tints.other) ?? (Look.shared.light && !selected ? .black : nil)
        let preview = model.previews[key]
        let shape = RoundedRectangle(cornerRadius: 12, style: .circular)
        let picked = model.pickedBrushes.contains(key)
        // the current brush counts as picked already
        let pick: (() -> Void)? = recent || selected ? nil : { model.togglePickedBrush(brush) }
        SwipeRow(
            open: model.swipedBrush == key, onOpen: { model.swipedBrush = $0 ? key : nil }, actions: actions, onPick: pick,
            held: { ui.holding || ui.dragging }
        ) {
            ZStack(alignment: .topLeading) {
                shape.fill(selected ? Pc.rowSelected : picked ? pickedBrush : Color.clear)
                if let preview {
                    SampleStroke(image: preview, tint: tint)
                }
                if model.renamingBrush == key {
                    InlineName(name: brush.name, color: selected || picked ? Color.white : Pc.panelTitle, fontSize: 14) {
                        model.renameBrush(brush, name: $0)
                    }
                    .padding(.leading, 18)
                    .padding(.top, 11.75)
                    .padding(.trailing, 12)
                } else {
                    BrushName(name: brush.name, selected: selected || picked, shaded: model.shadedNames[key] == true)
                        .padding(.leading, 18)
                        .padding(.top, 11.75)
                }
                // a pinned brush's mark, at the cell's top right
                if pinned {
                    PcIconView(PcIcons.pinMark, size: 16, color: selected ? .white : Pc.panelTitle)
                        .padding(.top, 12.5)
                        .padding(.trailing, 11)
                        .frame(maxWidth: .infinity, alignment: .trailing)
                }
            }
            .frame(maxWidth: .infinity, minHeight: brushHeight, maxHeight: brushHeight, alignment: .topLeading)
            .clipShape(shape)
            .opacity(ui.isDragged(brush) ? 0.3 : 1)
            .background(GeometryReader { g in
                Color.clear.preference(key: BrushFrames.self, value: [key: g.frame(in: .named(librarySpace))])
            })
            .contentShape(Rectangle())
            .onTapGesture {
                // the lift ending a hold is no tap: its menu (or the drag) has it
                if ui.holding || Date().timeIntervalSince(ui.holdEnded) < 0.3 {
                    return
                }
                // with a brush swiped open, a tap only closes it
                if model.swipedBrush != nil {
                    model.swipedBrush = nil
                    return
                }
                // and with brushes picked, only lets them go
                if !model.pickedBrushes.isEmpty {
                    model.pickedBrushes = []
                    return
                }
                model.chooseBrush(brush)
                // a highlighted brush opens in the studio
                if selected {
                    model.studio.open()
                }
            }
            .modifier(HeldBrush(
                ui: ui,
                movable: !recent,
                held: {
                    model.swipedBrush = nil
                    let frame = ui.brushFrames[key] ?? .zero
                    withAnimation(Motion.menuOpen) {
                        ui.brushMenu = BrushMenuAsk(brush: brush, pinned: pinned, recent: recent, frame: frame)
                    }
                },
                began: { at in
                    // the Recent set's brushes stay where they are
                    guard !recent else { return }
                    // the menu fades as the brush lifts
                    withAnimation(.easeOut(duration: brushMenuFade)) { ui.brushMenu = nil }
                    let frame = ui.brushFrames[key] ?? .zero
                    ui.grab = CGPoint(x: at.x - frame.minX, y: at.y - frame.minY)
                    ui.dragWidth = frame.width
                    // the brushes picked in its set go with it, when it is one of them
                    let set = model.brushSets.indices.contains(Int(brush.set)) ? model.brushSets[Int(brush.set)].brushes : []
                    ui.draggedWith = draggedTogether(brush, picked: model.pickedBrushes, selected: selected, brushes: set)
                    ui.dragged = brush
                },
                ended: { ui.dropBrush(model) }
            ))
        }
        // a brush's preview goes by place, and is drawn again when the library changes
        .task(id: preview == nil) {
            if preview == nil {
                model.loadPreview(brush)
            }
        }
    }
}

/// The brushes dragged with `held`: those picked (`picked`, by place "set-index") in its set and
/// it, in their order in its set (`brushes`), when it is one of them (the current brush,
/// `selected`, counts as picked); else it alone.
private func draggedTogether(_ held: BrushInfo, picked: [String], selected: Bool, brushes: [BrushInfo]) -> [BrushInfo] {
    if picked.isEmpty || (!picked.contains(CanvasModel.brushKey(held)) && !selected) {
        return [held]
    }
    let going = brushes.filter { $0.set == held.set && (picked.contains(CanvasModel.brushKey($0)) || $0.index == held.index) }
    return going.isEmpty ? [held] : going
}

/// The dragged brush or set, following the finger.
private struct DragGhost: View {
    let model: CanvasModel
    let ui: LibraryUi

    var body: some View {
        let at = CGPoint(x: ui.dragAt.x - ui.grab.x, y: ui.dragAt.y - ui.grab.y)
        let shape = RoundedRectangle(cornerRadius: 12, style: .circular)
        if let d = ui.dragged {
            let key = CanvasModel.brushKey(d)
            ZStack(alignment: .topLeading) {
                // lifted on its dark card, as held
                shape.fill(draggedCard)
                if let preview = model.previews[key] {
                    SampleStroke(image: preview, tint: model.previewTints().chosen)
                }
                Text(verbatim: d.name)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(Color.white)
                    .lineLimit(1)
                    .padding(.leading, 18)
                    .padding(.top, 11)
            }
            .frame(width: ui.dragWidth, height: brushHeight)
            .clipShape(shape)
            // several going together: how many
            .overlay(alignment: .topTrailing) {
                if ui.draggedWith.count > 1 {
                    Text(verbatim: "\(ui.draggedWith.count)")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(Pc.rowSelected)
                        .lineLimit(1)
                        .frame(width: 24, height: 24)
                        .background(Circle().fill(Color.white))
                        .padding(.top, 8)
                        .padding(.trailing, 8)
                }
            }
            // over another set it would be copied into: a green plus on its corner
            .overlay(alignment: .topTrailing) {
                if ui.dropCopies(model) {
                    Text(verbatim: "+")
                        .font(.system(size: 15))
                        .foregroundStyle(Color.white)
                        .frame(width: 24, height: 24)
                        .background(Circle().fill(Color(argb: 0xFF30_D158)))
                        .offset(x: 12, y: -12)
                }
            }
            .offset(x: at.x, y: at.y)
            .allowsHitTesting(false)
        } else if let i = ui.draggedSet, model.brushSets.indices.contains(i) {
            let set = model.brushSets[i]
            HStack(spacing: 0) {
                PcIconView(model.setIcon(set), size: 40, color: .white)
                    .padding(.leading, 4)
                Text(verbatim: set.name)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(Color.white)
                    .lineLimit(2)
                    .padding(.leading, 1)
                    .padding(.trailing, 8)
                Spacer(minLength: 0)
            }
            .frame(width: ui.setFrames[i]?.width ?? 0, height: 50)
            .background(shape.fill(Pc.rowSelected.opacity(0.9)))
            .offset(x: at.x, y: at.y)
            .allowsHitTesting(false)
        }
    }
}

/// The library's menus: the plus's, hanging below it with their right edges in line; a set's, left
/// of it, 6 clear and centred on it; the Recent set's, as a set's, of one item. Each is kept on the
/// screen, and all but the plus's below the top bar.
private struct LibraryMenus: View {
    let model: CanvasModel
    let ui: LibraryUi
    /// The library's top left corner on the screen, and the screen's size.
    let origin: CGPoint
    let screen: CGSize

    /// `p`, a menu `size` big's place in the library, moved as little as keeps the menu on the
    /// screen and its top at least `top` down it.
    private func kept(_ p: CGPoint, _ size: CGSize, below top: CGFloat = 0) -> CGPoint {
        CGPoint(
            x: min(max(origin.x + p.x, 0), max(screen.width - size.width, 0)) - origin.x,
            y: min(max(origin.y + p.y, top), max(screen.height - size.height, top)) - origin.y
        )
    }

    /// Where a menu `height` tall goes for a row: left of it, 6 clear, centred on it.
    private func beside(_ frame: CGRect, height: CGFloat) -> CGPoint {
        kept(CGPoint(x: frame.minX - glassMenuWidth - 6, y: frame.midY - height / 2), CGSize(width: glassMenuWidth, height: height), below: menuTop)
    }

    var body: some View {
        ZStack(alignment: .topLeading) {
            if ui.adding {
                MenuDismissal { closeAll() }
                addMenu
            }
            if ui.titleMenu {
                MenuDismissal { closeAll() }
                let at = kept(libraryTitleMenuAt, CGSize(width: glassMenuWidth, height: LibraryTitleMenu.height), below: menuTop)
                LibraryTitleMenu(model: model) { closeAll() }
                    .offset(x: at.x, y: at.y)
                    .menuTransition(.topLeading)
            }
            if ui.recentMenu, let frame = ui.setFrames[CanvasModel.recentSet] {
                MenuDismissal { closeAll() }
                let at = beside(frame, height: 65)
                GlassMenu {
                    MenuRow(icon: PcIcons.menuClear, label: "Clear") {
                        closeAll()
                        model.clearRecentBrushes()
                    }
                    .padding(.vertical, 5.5)
                }
                .offset(x: at.x, y: at.y)
                .menuTransition(.trailing)
            }
            if let i = ui.setMenu, let frame = ui.setFrames[i], model.brushSets.indices.contains(i) {
                MenuDismissal { closeAll() }
                setMenu(i, frame: frame)
            }
            if let ask = ui.brushMenu {
                MenuDismissal { closeAll() }
                brushMenu(ask, frame: ask.frame)
            }
        }
    }

    private func closeAll() {
        withAnimation(Motion.menuClose) {
            ui.adding = false
            ui.titleMenu = false
            ui.recentMenu = false
            ui.setMenu = nil
            ui.iconPicker = false
            ui.brushMenu = nil
        }
    }

    /// A brush held: its menu above it, their left edges in line, growing from the brush's corner
    /// (under it where there is no room above). In a set Rename, Duplicate, Share and Delete; in the
    /// Recent set Find, Pin or Unpin, Share and Clear.
    private func brushMenu(_ ask: BrushMenuAsk, frame: CGRect) -> some View {
        let b = ask.brush
        // two rows, Share between lines, and Delete or Clear
        let height: CGFloat = 14 + 80 + 21 + 40 + 21 + 40
        let above = CGPoint(x: frame.minX, y: frame.minY - brushMenuGap - height)
        let fits = origin.y + above.y >= menuTop
        let at = kept(fits ? above : CGPoint(x: frame.minX, y: frame.maxY), CGSize(width: glassMenuWidth, height: height), below: menuTop)
        return GlassMenu(overPanel: true) {
            if ask.recent {
                MenuRow(icon: PcIcons.swipeFind, label: "Find", iconSize: 26) {
                    closeAll()
                    model.findBrush(b)
                }
                MenuRow(icon: ask.pinned ? PcIcons.swipeUnpin : PcIcons.swipePin, label: ask.pinned ? "Unpin" : "Pin", iconSize: 26) {
                    closeAll()
                    model.pinRecentBrush(b, pinned: !ask.pinned)
                }
            } else {
                MenuRow(icon: PcIcons.menuRename, label: "Rename") {
                    closeAll()
                    model.renamingBrush = CanvasModel.brushKey(b)
                }
                MenuRow(icon: PcIcons.menuDuplicate, label: "Duplicate") {
                    closeAll()
                    model.duplicateBrush(b)
                }
            }
            MenuLine()
            MenuRow(icon: PcIcons.menuShare, label: "Share") {
                closeAll()
                model.shareBrush(b)
            }
            MenuLine()
            if ask.recent {
                MenuRow(icon: PcIcons.menuClear, label: "Clear") {
                    closeAll()
                    model.clearRecentBrush(b)
                }
            } else {
                MenuRow(icon: PcIcons.menuTrash, label: "Delete", ink: Pc.menuDanger) {
                    closeAll()
                    model.question = Question(title: "Delete this brush?") { model.deleteBrush(b) }
                }
            }
        }
        .offset(x: at.x, y: at.y)
        .menuTransition(fits ? .bottomLeading : .topLeading)
    }

    private var addMenu: some View {
        let recent = model.shownSet == CanvasModel.recentSet
        // its rows, and Import under a line 11 clear of them
        let height: CGFloat = 14 + (recent ? 40 : 80) + 23 + 40
        // the plus's lower right corner: 12 in from the header's right, its 40 under the header's
        // first 12 (the header starting under the arrow)
        let plus = CGPoint(x: libraryWidth - 12, y: Pc.arrowHeight + 12 + 40)
        let spot = kept(CGPoint(x: plus.x - glassMenuWidth, y: plus.y), CGSize(width: glassMenuWidth, height: height))
        return GlassMenu(overPanel: true) {
            // the Recent set holds no brushes of its own to add one to
            if !recent {
                MenuRow(icon: PcIcons.menuNewBrush, label: "Create new brush") {
                    closeAll()
                    model.newBrush()
                }
            }
            MenuRow(icon: PcIcons.menuNewSet, label: "Create new set") {
                closeAll()
                model.newBrushSet()
            }
            MenuLine(gap: 11)
            MenuRow(icon: PcIcons.menuFolder, label: "Import from Files") {
                closeAll()
                ui.importing = true
            }
        }
        .offset(x: spot.x, y: spot.y)
        .menuTransition(.topTrailing)
    }

    @ViewBuilder
    private func setMenu(_ i: Int, frame: CGRect) -> some View {
        let set = model.brushSets[i]
        let canDelete = model.brushSets.count > 1
        // four rows and a line, and Delete under a line of its own
        let height: CGFloat = 14 + 160 + 21 + (canDelete ? 61 : 0)
        let at = beside(frame, height: height)
        GlassMenu {
            MenuRow(icon: PcIcons.menuRename, label: "Rename") {
                closeAll()
                model.renamingSet = i
            }
            MenuRow(icon: PcIcons.menuDuplicate, label: "Duplicate") {
                closeAll()
                model.duplicateBrushSet(i)
            }
            MenuRow(icon: model.setIcon(set), label: "Customize icon", trailing: PcIcons.menuChevron) {
                withAnimation(Motion.menuOpen) { ui.iconPicker.toggle() }
            }
            MenuLine()
            MenuRow(icon: PcIcons.menuShare, label: "Share") {
                closeAll()
                model.shareBrush(nil)
            }
            if canDelete {
                MenuLine()
                MenuRow(icon: PcIcons.menuTrash, label: "Delete", ink: Pc.menuDanger) {
                    closeAll()
                    model.question = Question(title: "Delete this set and its brushes?") { model.deleteBrushSet(i) }
                }
            }
        }
        .offset(x: at.x, y: at.y)
        .menuTransition(.trailing)
        if ui.iconPicker {
            iconPicker(i, set: set, frame: frame, rowTop: at.y + 7 + 80)
        }
    }

    /// The icons to choose from, right of the menu over the library: under the Customize icon row,
    /// or over it when there is no room below; scrolled to the set's own icon.
    private func iconPicker(_ i: Int, set: BrushSetInfo, frame: CGRect, rowTop: CGFloat) -> some View {
        let tall: CGFloat = 325
        let below = rowTop + 40
        let room = screen.height - pickerMargin - origin.y
        let spot = kept(CGPoint(x: frame.minX - pickerInset, y: below + tall <= room ? below : rowTop - tall), CGSize(width: 250, height: tall), below: menuTop)
        let shape = RoundedRectangle(cornerRadius: 26, style: .circular)
        return ScrollViewReader { reader in
            ScrollView {
                LazyVGrid(columns: Array(repeating: GridItem(.fixed(48), spacing: 0), count: pickerColumns), spacing: 0) {
                    ForEach(0 ..< pickedSetIcons, id: \.self) { k in
                        ZStack {
                            RoundedRectangle(cornerRadius: 21, style: .circular)
                                .fill(k == Int(set.icon) ? Pc.accent : Color.clear)
                            PcIconView(PcIcons.setIcons[k], size: 40, color: .white)
                        }
                        .frame(width: 48, height: 48)
                        .contentShape(Rectangle())
                        .onTapGesture {
                            model.changeBrushSetIcon(i, icon: k)
                            closeAll()
                        }
                        .id(k)
                    }
                }
            }
            .scrollIndicators(.hidden)
            .onAppear {
                if Int(set.icon) < pickedSetIcons {
                    reader.scrollTo(Int(set.icon), anchor: .center)
                }
            }
        }
        .padding(3)
        .frame(width: 250, height: tall)
        .background(shape.fill(Color(argb: 0xFA30_3335)))
        .overlay(shape.strokeBorder(Color(argb: 0xFF5A_5D5F), lineWidth: 1))
        .clipShape(shape)
        .offset(x: spot.x, y: spot.y)
        .menuTransition(.topLeading)
    }
}

/// The library's place under the bar: its width and which tool button it hangs from (0 is the
/// colour button, counting leftwards).
func libraryButton(_ tool: ToolKind) -> Int {
    switch tool {
    case .erase: 2
    case .smudge: 3
    case .paint: 4
    }
}

/// The library's width.
let brushLibraryWidth = libraryWidth

extension CanvasModel {
    /// What the library's sample strokes are tinted with as a plugin colours them by the colour
    /// now: each brush's (`other`), and the brush in use's on the accent (`chosen`); none, as the
    /// interface draws them. Read again as the colour or the plugins change.
    func previewTints() -> PreviewTints {
        _ = colorRgb()
        _ = PluginsModel.shared.changes
        let tint = { (chosen: Bool) -> Color? in
            self.session.brushPreviewColour(chosen: chosen).map { Color(argb: 0xFF00_0000 | $0) }
        }
        return PreviewTints(other: tint(false), chosen: tint(true))
    }
}

/// The colours a plugin gives the library's sample strokes: each brush's, and the brush in use's.
struct PreviewTints {
    var other: Color?
    var chosen: Color?
}
