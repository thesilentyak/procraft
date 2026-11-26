import PhotosUI
import SwiftUI
import UIKit

private let layersWidth: CGFloat = 384
private let rowHeight: CGFloat = 69
/// A layer with a mask, and its mask: their rows sit closer.
private let maskedRowHeight: CGFloat = 65
private let rowGap: CGFloat = 1
private let listCorner: CGFloat = 8
private let groupHeight: CGFloat = 44
private let groupIndent: CGFloat = 21
/// A closed group's row is taller: its line (20 under the row's top and 20 over its bottom) opens
/// into a window 4 in from them, on its layers' rows drawn small; opening or closing, over this long.
private let groupClosedHeight: CGFloat = 82
private let groupLineInset: CGFloat = 20
private let groupWindowInset: CGFloat = 4
private let groupTurn = 0.15
/// The rows' width in the list, which a closed group's window draws its layers' rows to fit.
private let listWidth: CGFloat = layersWidth - 24
private let groupLine = Color(argb: 0xFF4E_4E4E)
private let maskOwnerRow = Color(argb: 0xFF2C_5281)
private let pickedRow = Color(argb: 0xFF2C_5281)
private let lockedLetter = Color(argb: 0xFF80_8080)
/// Blend modes shown by their first two letters (their first is another mode's).
private let twoLetterBlends: Set<String> = ["Darken", "Shade", "Lighten", "Subtract", "Divide", "Saturation"]
/// Swiping a row left shows these buttons, each this wide, this far apart.
private let swipeButton: CGFloat = 82
private let swipeGap: CGFloat = 2
/// The badge behind a plugin's layer's glyph on its thumbnail.
private let pluginBadgeBehind = Color(argb: 0x9900_0000)
/// A kind's badge as a menu row's glyph: its box 1.6 times a thumbnail's (18 by 14), its lines as
/// thick as the menu glyphs'.
private let menuBadgeWidth: CGFloat = 28.8
private let menuBadgeHeight: CGFloat = 22.4
private let menuBadgeLine: CGFloat = 2.4
private let swipeFill = Color(argb: 0xFF3F_3F3F)
private let swipeInk = Color(argb: 0xFFC1_C1C1)
private let swipeDelete = Color(argb: 0xFFFF_473D)
/// How far right a row slides to be picked.
private let pickReach: CGFloat = 72
/// The options never go higher than this from the screen's top (the top bar and a gap).
private let optionsTop: CGFloat = 57
/// The layer's blend list: nine rows showing, its glyphs a darker grey than its names.
private let blendRowHeight: CGFloat = 38.8
private let blendListHeight: CGFloat = 349
private let blendGlyphInk = Color(argb: 0xFF80_8080)
/// The opacity slider's thumb, a small white pill, and its value's ink, lighter than its label.
private let opacityThumb = CGSize(width: 16, height: 10)
private let opacityTouch: CGFloat = 30
private let opacityValueInk = Color(argb: 0xFFED_EDED)
/// How long the blend modes take to open and close, and the rows under them to come back.
private let blendOpen = 0.25
private let blendClose = 0.1
private let blendRowsWait = 0.1
private let blendRowsBack = 0.12
/// How much of its finger's way a pinched row moves, and how close two rows are pinched to merge
/// (of how far apart they were), as measured.
private let squeezeFollow: CGFloat = 0.3
private let pinchMerge: CGFloat = 0.65
/// A dragged row this near an end of the list scrolls it, at most this far a second.
private let dragEdge: CGFloat = 56
private let dragScroll: CGFloat = 480
/// A row's thumbnail (a group's line) held over by a dragged row long enough to take it.
private let heldOutline = Color.white
/// How long a new row takes to fade in, as measured.
private let newRowFade = 0.1
/// The rows standing aside for a dragged row, and coming back: a 0.3 s spring, damping 0.75.
private let rowAside = Animation.spring(duration: 0.3, bounce: 0.25)

/// Calls its tick once a frame with the seconds since the frame before, until the tick says to
/// stop or it is stopped.
private final class FrameLoop: NSObject {
    private var link: CADisplayLink?
    private var last: CFTimeInterval = 0
    private var tick: ((CFTimeInterval) -> Bool)?

    var running: Bool {
        link != nil
    }

    func start(_ tick: @escaping (CFTimeInterval) -> Bool) {
        stop()
        self.tick = tick
        last = CACurrentMediaTime()
        let made = CADisplayLink(target: self, selector: #selector(step))
        made.add(to: .main, forMode: .common)
        link = made
    }

    func stop() {
        link?.invalidate()
        link = nil
        tick = nil
    }

    @objc private func step() {
        let now = CACurrentMediaTime()
        let seconds = now - last
        last = now
        if tick?(seconds) != true {
            stop()
        }
    }
}

/// How far row `id` stands aside while the rows of `unit` are dragged: the rows between where they
/// were and where they land (`place`) take the room they left, so the gap shows where they will
/// land; held onto a row, or over their own place, none moves.
private func makingWay(_ id: UInt64, unit: Set<UInt64>, place: RowDrop?, rows: [UInt64: CGRect], gap: CGFloat) -> CGFloat {
    guard !unit.contains(id), let row = rows[id] else { return 0 }
    let moved = unit.compactMap { rows[$0] }
    guard let top = moved.map(\.minY).min(), let bottom = moved.map(\.maxY).max() else { return 0 }
    let room = bottom - top + gap
    // the line between the rows they land between
    let slot: CGFloat
    switch place {
    case let .above(target)?:
        guard let r = rows[target] else { return 0 }
        slot = r.minY
    case let .below(target)?:
        guard let r = rows[target] else { return 0 }
        slot = r.maxY
    case .bottom?:
        slot = .infinity
    default:
        return 0
    }
    if row.maxY <= top, row.minY >= slot {
        return room
    }
    if row.minY >= bottom, row.maxY <= slot {
        return -room
    }
    return 0
}

/// Two rows being pinched: each moved a share of its finger's way, the upper on top.
private struct Squeeze {
    let upper: UInt64
    let lower: UInt64
    let upperBy: CGFloat
    let lowerBy: CGFloat

    func offset(_ id: UInt64) -> CGFloat? {
        id == upper ? upperBy * squeezeFollow : id == lower ? lowerBy * squeezeFollow : nil
    }
}

/// What the panel's rows share while it is open.
@Observable
private final class LayersUi {
    /// The layer whose options are open, and whose blend modes.
    var options: UInt64?
    var blendFor: UInt64?
    /// The row the blend modes open under (-1 when nothing is moving), and how far the rows under
    /// it have come back.
    var blendAt = -1
    var belowShown = 1.0
    var renaming: UInt64?
    var swiped: UInt64?
    var titleMenu = false
    /// The plus's menu, with the kinds of layer the plugins on add.
    var addKinds: [PluginKind]?
    /// The filter layers' list its Filter Layer opens (interface 17), while open.
    var filterList: [PluginLayerFilter]?
    /// A row held and dragged, and how far (on the window), with the rows that go with it (its
    /// mask, a group's layers).
    var dragId: UInt64?
    var dragOffset: CGFloat = 0
    var dragUnit: Set<UInt64> = []
    /// How far each other row stands aside to show where the dragged one lands, and the row it is
    /// held over long enough to go onto (its thumbnail outlined).
    var aside: [UInt64: CGFloat] = [:]
    var heldTarget: UInt64?
    var squeeze: Squeeze?
    /// Where each row is on the screen.
    @ObservationIgnored var rows: [UInt64: CGRect] = [:]
    /// The list on the screen, and the scroll view under it.
    @ObservationIgnored var listBox: CGRect = .zero
    @ObservationIgnored weak var scroller: UIScrollView?
    /// The finger's way since the row was picked up, and how far the list has scrolled under it.
    @ObservationIgnored private var fingerWay: CGFloat = 0
    @ObservationIgnored private var scrolled: CGFloat = 0
    /// The row under the dragged one's middle and since when (held there a moment, what is let go
    /// goes onto it), and the row held over long enough.
    @ObservationIgnored private var over: (id: UInt64, since: CFTimeInterval)?
    @ObservationIgnored private var heldOver: UInt64?
    /// Where it was last placed: placed again only when it moves or a hold comes due.
    @ObservationIgnored private var placedFor: (y: CGFloat, held: UInt64?)?
    @ObservationIgnored private let dragLoop = FrameLoop()
    @ObservationIgnored private var blendJob: DispatchWorkItem?
    /// The dragged row was lifted out of the panel: the finger holds its layers now.
    @ObservationIgnored private var lifted = false

    /// The layer whose row is at `y` on the screen.
    func row(at y: CGFloat) -> UInt64? {
        rows.first { y >= $0.value.minY && y <= $0.value.maxY }?.key
    }

    /// Blend modes opening under a layer: the rows under it go at once and the panel grows over
    /// where they were; closing, it shrinks back to them and they come back a moment later.
    func changeBlend(_ id: UInt64?, shown: [LayerInfo]) {
        guard id != blendFor else { return }
        let places = [blendFor, id].compactMap { $0 }.compactMap { b in shown.firstIndex { $0.id == b } }
        let opening = id != nil
        blendAt = places.min() ?? -1
        belowShown = 0
        withAnimation(Motion.easeInOut(opening ? blendOpen : blendClose)) {
            blendFor = id
        }
        blendJob?.cancel()
        let job = DispatchWorkItem { [weak self] in
            withAnimation(Motion.easeInOut(blendRowsBack)) {
                self?.belowShown = 1
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + blendRowsBack) {
                self?.blendAt = -1
            }
        }
        blendJob = job
        DispatchQueue.main.asyncAfter(deadline: .now() + (opening ? blendOpen : blendClose + blendRowsWait), execute: job)
    }

    /// `l`'s row held long enough to drag: what goes with it is taken along, and each frame from
    /// now on places it and scrolls the list while it is near an end.
    func pickUp(_ model: CanvasModel, _ l: LayerInfo) {
        dragId = l.id
        dragOffset = 0
        dragUnit = model.movingWith(l).union([l.id])
        aside = [:]
        heldTarget = nil
        swiped = nil
        fingerWay = 0
        scrolled = 0
        over = nil
        heldOver = nil
        placedFor = nil
        lifted = false
        dragLoop.start { [weak self, weak model] seconds in
            guard let self, let model else { return false }
            return self.dragFrame(model, l, seconds)
        }
    }

    /// The dragged row's finger gone `way` down since it was picked up, at `at` on the window: the
    /// row stays under it, however far the list has scrolled. Taken out past the panel's side, the
    /// row (with what goes with it) is lifted out, held by the finger from then on wherever it goes.
    func dragged(_ way: CGFloat, at: CGPoint, _ model: CanvasModel, _ l: LayerInfo) {
        guard !lifted else { return }
        let reach = layersLiftReach * Interface.shared.scale
        if at.x < listBox.minX - reach || at.x > listBox.maxX + reach, let frame = rows[l.id] {
            let corner = CGPoint(x: frame.minX, y: frame.minY + dragOffset)
            if HeldLayers.shared.lift(model, l, at: at, corner: corner, thumbnail: model.thumbnails[l.id]) {
                lifted = true
                // the rows settle back as they were
                withAnimation(Motion.rowMove) { stopDrag() }
                return
            }
        }
        fingerWay = way
        dragOffset = fingerWay + scrolled
    }

    /// A frame of `l`'s drag: the row its middle is over (held there a moment, it goes onto it),
    /// where it lands and the rows making way for it; held near an end of the list it scrolls,
    /// faster the nearer, the row staying under the finger. Says whether the drag goes on.
    private func dragFrame(_ model: CanvasModel, _ l: LayerInfo, _ seconds: CFTimeInterval) -> Bool {
        guard dragId == l.id else { return false }
        guard let frame = rows[l.id] else { return true }
        let y = frame.midY + dragOffset
        let now = CACurrentMediaTime()
        let under = rows.first { !dragUnit.contains($0.key) && y >= $0.value.minY && y < $0.value.maxY }?.key
        if under != over?.id {
            over = under.map { ($0, now) }
        }
        heldOver = over.flatMap { now - $0.since >= Double(rowHoldSeconds()) ? $0.id : nil }
        if placedFor?.y != y || placedFor?.held != heldOver {
            placedFor = (y, heldOver)
            let place = model.rowDrop(l, rows: rows, y: y, held: heldOver)
            var way: [UInt64: CGFloat] = [:]
            for id in rows.keys {
                let by = makingWay(id, unit: dragUnit, place: place, rows: rows, gap: rowGap * Interface.shared.scale)
                if by != 0 {
                    way[id] = by
                }
            }
            if way != aside {
                withAnimation(rowAside) { aside = way }
            }
            var onto: UInt64?
            if case let .onto(target) = place {
                onto = target
            }
            if onto != heldTarget {
                heldTarget = onto
            }
        }
        // the list's own points are the interface's
        let scale = Interface.shared.scale
        let edge = dragEdge * scale
        let towards: CGFloat = if y < listBox.minY + edge {
            (y - listBox.minY - edge) / edge
        } else if y > listBox.maxY - edge {
            (y - listBox.maxY + edge) / edge
        } else {
            0
        }
        if towards != 0, let list = scroller {
            let inset = list.adjustedContentInset
            let least = -inset.top
            let most = max(list.contentSize.height + inset.bottom - list.bounds.height, least)
            let from = list.contentOffset.y
            let to = min(max(from + min(max(towards, -1), 1) * dragScroll * CGFloat(seconds), least), most)
            if to != from {
                list.contentOffset.y = to
                // the rows move up the screen as far, until the list says where they are
                let moved = (to - from) * scale
                scrolled += moved
                dragOffset = fingerWay + scrolled
                rows = rows.mapValues { $0.offsetBy(dx: 0, dy: -moved) }
            }
        }
        return true
    }

    /// Lets `l` go where its row's middle came to rest, as the shared core places it: just above
    /// or below a row, into a group only among its layers, or onto a row held over a moment. The
    /// rows settle into their new places.
    func drop(_ model: CanvasModel, _ l: LayerInfo) {
        dragLoop.stop()
        // lifted out, where it lands is the held layers' to say
        if lifted {
            lifted = false
            return
        }
        let place = rows[l.id].map { model.rowDrop(l, rows: rows, y: $0.midY + dragOffset, held: heldOver) }
        withAnimation(Motion.rowMove) {
            dragId = nil
            dragOffset = 0
            dragUnit = []
            aside = [:]
            heldTarget = nil
        }
        over = nil
        heldOver = nil
        placedFor = nil
        if let place {
            model.dropRow(l, place)
        }
    }

    /// The panel gone mid-drag: nothing moves.
    func stopDrag() {
        dragLoop.stop()
        dragId = nil
        dragOffset = 0
        dragUnit = []
        aside = [:]
        heldTarget = nil
    }
}

/// Finds the scroll view of the list it is put in, for a dragged row to scroll it.
private struct ListScroller: UIViewRepresentable {
    let ui: LayersUi

    final class Finder: UIView {
        fileprivate weak var ui: LayersUi?

        override func didMoveToWindow() {
            super.didMoveToWindow()
            guard let window else { return }
            // from now on a finger taking a row out of the panel can be followed anywhere
            HeldLayers.shared.watch(window)
            var under = superview
            while let view = under, !(view is UIScrollView) {
                under = view.superview
            }
            ui?.scroller = under as? UIScrollView
        }
    }

    func makeUIView(context: Context) -> Finder {
        let view = Finder()
        view.isUserInteractionEnabled = false
        view.ui = ui
        return view
    }

    func updateUIView(_ view: Finder, context: Context) {
        view.ui = ui
    }
}

private struct LayerFrames: PreferenceKey {
    static let defaultValue: [UInt64: CGRect] = [:]
    static func reduce(value: inout [UInt64: CGRect], nextValue: () -> [UInt64: CGRect]) {
        value.merge(nextValue()) { $1 }
    }
}

/// The layers panel: top layer first (a closed group's left out), each its thumbnail, name, blend
/// mode's letters and visibility box, and the background colour's row under them. Tap a layer to
/// make it current, tap the current one for its options, tap the blend letter for blend mode and
/// opacity, hold and drag to reorder, swipe left for lock, duplicate and delete, swipe right to
/// pick it to act on with the current one (the title's menu then groups, merges, shares,
/// duplicates or deletes them). Two fingers swiped right on a row set its alpha lock, held there
/// select what is on it, and pinching two rows merges them and every layer between.
struct LayersPanel: View {
    let model: CanvasModel
    let arrowX: CGFloat
    /// The most the panel may be tall: its list scrolls past that.
    let maxHeight: CGFloat
    /// Floating in a window of its own (a plugin lets it): its frame; none while it hangs.
    var floating: FloatingFrame?
    /// How tall the rows are together.
    @State private var listHeight: CGFloat = 0
    @State private var ui = LayersUi()

    var body: some View {
        let shown = model.layers.filter { !model.hidden($0) }
        let ids = shown.map(\.id)
        // floating in a window of its own: as wide as the window, no arrow, a strip over it and under it
        Popover(width: floating?.width ?? layersWidth, arrowX: arrowX, side: floating == nil ? .top : .none) {
            if let floating {
                FloatingTopStrip(frame: floating)
            }
            PanelHeader(
                title: "Layers",
                chevronOn: model.anyPicked,
                onTitle: { withAnimation(Motion.menuOpen) { ui.titleMenu.toggle() } }
            ) {
                PcIconView(PcIcons.plusMedium, size: 41, color: Pc.panelGlyph)
                    .frame(width: 40, height: 41)
                    .contentShape(Rectangle())
                    .onTapGesture {
                        // with plugins' kinds of layer or filter layers on, a menu of them under a
                        // plain layer
                        let kinds = model.layerKinds()
                        if kinds.isEmpty && model.layerFilters().isEmpty {
                            model.addLayer()
                        } else {
                            withAnimation(Motion.menuOpen) { ui.addKinds = kinds }
                        }
                    }
                    .padding(.top, 12)
            }
            ScrollViewReader { reader in
                ScrollView {
                    VStack(spacing: rowGap) {
                        if model.scene.open {
                            ModelRows(model: model, ui: ui, shown: shown)
                        } else {
                            ForEach(Array(shown.enumerated()), id: \.element.id) { index, l in
                                LayerItem(model: model, ui: ui, layer: l, index: index, shown: shown)
                                    .id(l.id)
                                    // a new row fades in quickly as the rows under it slide down
                                    .transition(.opacity.animation(.easeOut(duration: newRowFade)))
                            }
                            VStack(spacing: 0) {
                                BackgroundRow(model: model)
                                LayersPluginRows(model: model)
                            }
                            .padding(.top, 10)
                            .opacity(ui.blendAt >= 0 ? ui.belowShown : 1)
                        }
                    }
                    .animation(Motion.rowMove, value: ids)
                    .padding(.horizontal, 12)
                    .padding(.bottom, 12)
                    .background {
                        GeometryReader { rows in
                            Color.clear
                                .onAppear { listHeight = rows.size.height }
                                .onChange(of: rows.size.height) { _, height in listHeight = height }
                        }
                        ListScroller(ui: ui)
                    }
                }
                .scrollIndicators(.hidden)
                // a dragged row scrolls it, near an end
                .scrollDisabled(ui.dragId != nil)
                // a layer added at the top of what shows comes in there
                .onChange(of: ids) { before, now in
                    if let added = now.first(where: { !before.contains($0) }) {
                        withAnimation(Motion.rowMove) { reader.scrollTo(added, anchor: .top) }
                    }
                }
            }
            // as tall as its rows, up to what the screen leaves under the header; floating, it fills
            // its window
            .frame(height: floating.map { max(0, $0.height - floatingStrips - 64) } ?? max(0, min(listHeight, maxHeight - 64 - Pc.arrowHeight)))
            .background {
                GeometryReader { list in
                    Color.clear
                        .onAppear {
                            ui.listBox = list.frame(in: .global)
                            HeldLayers.shared.panelBox = ui.listBox
                        }
                        .onChange(of: list.frame(in: .global)) { _, box in
                            ui.listBox = box
                            HeldLayers.shared.panelBox = box
                        }
                }
            }
            if floating != nil {
                FloatingBottomStrip()
            }
        }
        // hanging from its button, its grabber tears it off into a window of its own, while a plugin
        // lets it
        .overlay(alignment: .top) {
            if floating == nil, model.floating.allowed(.layers) {
                PanelGrabber(model: model, panel: .layers)
                    .padding(.top, Pc.arrowHeight)
            }
        }
        .background(RowFingers(model: model, ui: ui).frame(width: 0, height: 0))
        .onDisappear {
            ui.stopDrag()
            HeldLayers.shared.panelBox = nil
        }
        .onPreferenceChange(LayerFrames.self) {
            ui.rows = $0
            HeldLayers.shared.panelRows = $0
        }
        // a model's meshes are pictured as the model is seen when the panel opens
        .onAppear { model.scene.refreshMeshThumbnails() }
        .overlay(alignment: .topLeading) { LayersMenus(model: model, ui: ui, maxHeight: maxHeight) }
        .sheet(isPresented: Binding(get: { ui.filterList != nil }, set: { if !$0 { ui.filterList = nil } })) {
            FilterLayerList(filters: ui.filterList ?? [], onPick: { id in
                ui.filterList = nil
                model.addFilterLayer(id)
            }, onClose: { ui.filterList = nil })
        }
        #if DEBUG
        .onAppear {
            guard let current = model.layers.first(where: { $0.current }) else { return }
            switch ScreenCapture.layersShows {
            case "swipe":
                DispatchQueue.main.asyncAfter(deadline: .now() + 1) { ui.swiped = current.id }
            case "options":
                DispatchQueue.main.asyncAfter(deadline: .now() + 1) { ui.options = current.id }
            case "blend":
                ui.changeBlend(current.id, shown: shown)
            case "menu":
                ui.titleMenu = true
            case "add":
                ui.addKinds = model.layerKinds()
            case "rename":
                ui.renaming = current.id
            default:
                break
            }
        }
        #endif
    }
}

/// A row of the list: the layer or group, what it swipes to show, and the blend modes opening
/// under it.
private struct LayerItem: View {
    let model: CanvasModel
    let ui: LayersUi
    let layer: LayerInfo
    let index: Int
    let shown: [LayerInfo]
    /// On a model: the row under its mesh, indented, its own corners rounded or not, and what it
    /// says under its name ("Shared" by the meshes of its texture set).
    var indent: CGFloat = 0
    var corners: (top: Bool, bottom: Bool)?
    var subtitle: String?

    private func tapped() {
        let l = layer
        ui.swiped = nil
        let scene = model.scene
        if scene.open, scene.tileChosen, l.kind != .group {
            // on a model the row means the layer itself: its colour, no channel's tile
            scene.leaveTiles()
            if !l.current {
                model.selectLayer(l.id)
            }
        } else if model.anyPicked {
            // picks give way to the layer tapped, the current one too
            model.selectLayer(l.id)
            ui.options = nil
        } else if l.current {
            withAnimation(Motion.menuOpen) {
                ui.options = ui.options == l.id ? nil : l.id
            }
            ui.changeBlend(nil, shown: shown)
        } else {
            model.selectLayer(l.id)
            ui.options = nil
        }
    }

    var body: some View {
        let l = layer
        // dragged, or going with the row dragged (its mask, a group's layers)
        let dragging = ui.dragId != nil && ui.dragUnit.contains(l.id)
        // held over by a dragged row long enough to take it
        let held = ui.heldTarget == l.id
        // the list's ends are rounded
        let first = corners?.top ?? (index == 0)
        let last = corners?.bottom ?? (index == shown.count - 1)
        let shape = UnevenRoundedRectangle(
            topLeadingRadius: first ? listCorner : 0, bottomLeadingRadius: last ? listCorner : 0,
            bottomTrailingRadius: last ? listCorner : 0, topTrailingRadius: first ? listCorner : 0,
            style: .circular
        )
        let renaming = ui.renaming == l.id
        let onRenamed: (String) -> Void = {
            model.rename(l, $0)
            ui.renaming = nil
        }
        VStack(spacing: 0) {
            LayerSwipe(
                model: model, layer: l, open: ui.swiped == l.id, enabled: ui.dragId == nil,
                onOpen: { ui.swiped = $0 ? l.id : nil },
                onPick: { model.pickLayer(l) }
            ) {
                if l.kind == .group {
                    GroupRow(
                        model: model, layer: l, shape: shape, renaming: renaming, onRenamed: onRenamed, onTap: tapped, held: held,
                        closed: l.collapsed ? 1 : 0
                    )
                    .animation(.linear(duration: groupTurn), value: l.collapsed)
                } else {
                    let scene = model.scene
                    LayerRow(
                        model: model, layer: l, shape: shape,
                        // the current mask's layer, or the current layer's mask
                        owner: model.layers.contains {
                            $0.current && (($0.kind == .mask && $0.parent == l.id) || (l.kind == .mask && l.parent == $0.id))
                        },
                        renaming: renaming, onRenamed: onRenamed, onTap: tapped,
                        onBlend: {
                            ui.changeBlend(ui.blendFor == l.id ? nil : l.id, shown: shown)
                            ui.options = nil
                        },
                        indent: indent, subtitle: subtitle,
                        // on a model: its channels, and the row giving its blue to the channel painted
                        onCube: scene.open && l.kind != .mask ? { scene.toggleTray(l.id) } : nil,
                        cubeLit: scene.open && l.current && scene.tileChosen,
                        selected: l.current && !(scene.open && scene.tileChosen),
                        held: held
                    )
                }
            }
            // held by the row itself: a hold on its channels or blend modes is theirs
            .modifier(RowDrag(
                enabled: l.kind != .mask,
                began: { ui.pickUp(model, l) },
                moved: { ui.dragged($0, at: $1, model, l) },
                ended: { ui.drop(model, l) }
            ))
            if model.scene.open, model.scene.trayFor == l.id {
                ChannelTray(model: model, layer: l)
            }
            if ui.blendFor == l.id {
                BlendEditor(model: model, layer: l)
                    .transition(.opacity)
            }
        }
        // (the drag, the rows aside and the squeeze go by the window; the rows by the interface)
        .offset(y: (dragging ? ui.dragOffset : (ui.squeeze?.offset(l.id) ?? 0) + (ui.aside[l.id] ?? 0)) / Interface.shared.scale)
        .opacity(ui.blendAt >= 0 && ui.blendAt < index ? ui.belowShown : 1)
        .background(GeometryReader { g in
            Color.clear.preference(key: LayerFrames.self, value: [l.id: g.frame(in: .global)])
        })
        .zIndex(dragging || ui.squeeze?.upper == l.id ? 1 : 0)
    }
}

/// A row held still, then dragged up or down: `ended` hears that it is over, let go or taken away
/// by the system alike (a gesture's state goes back either way).
private struct RowDrag: ViewModifier {
    let enabled: Bool
    let began: () -> Void
    /// How far down the finger has gone, and where it is on the window.
    let moved: (CGFloat, CGPoint) -> Void
    let ended: () -> Void

    @GestureState private var held = false
    @State private var started = false

    func body(content: Content) -> some View {
        content
            .simultaneousGesture(
                LongPressGesture(minimumDuration: 0.5)
                    .sequenced(before: DragGesture(minimumDistance: 0, coordinateSpace: .global))
                    .updating($held) { value, state, _ in
                        if case .second(true, _) = value {
                            state = true
                        }
                    }
                    .onChanged { value in
                        if case let .second(true, drag?) = value {
                            if !started {
                                started = true
                                began()
                            }
                            moved(drag.translation.height, drag.location)
                        }
                    },
                including: enabled ? .all : .subviews
            )
            .onChange(of: held) { _, now in
                if !now, started {
                    started = false
                    ended()
                }
            }
    }
}

/// A row that slides left to show Lock, Duplicate (not for a mask) and Delete, or a little right to
/// pick it, springing back.
private struct LayerSwipe<Content: View>: View {
    let model: CanvasModel
    let layer: LayerInfo
    let open: Bool
    let enabled: Bool
    let onOpen: (Bool) -> Void
    let onPick: () -> Void
    @ViewBuilder let content: () -> Content

    @State private var offset: CGFloat = 0
    /// The touch is going along the row, not down the list.
    @State private var sliding = false

    /// How far it slides open: 6 past the buttons.
    private var reach: CGFloat {
        let buttons: CGFloat = layer.kind == .mask ? 2 : 3
        return swipeButton * buttons + swipeGap * (buttons - 1) + 6
    }

    private func button(_ label: String, fill: Color, ink: Color, action: @escaping () -> Void) -> some View {
        Text(verbatim: label)
            .font(.system(size: 15, weight: .medium))
            .foregroundStyle(ink)
            .lineLimit(1)
            .frame(width: swipeButton)
            .frame(maxHeight: .infinity)
            .background(RoundedRectangle(cornerRadius: 8, style: .circular).fill(fill))
            .contentShape(Rectangle())
            .onTapGesture {
                action()
                onOpen(false)
            }
    }

    var body: some View {
        let l = layer
        // the only layer can't go: it is cleared instead
        let alone = model.layers.filter { $0.kind != .mask }.count == 1
        ZStack(alignment: .trailing) {
            if offset < 0 {
                HStack(spacing: swipeGap) {
                    button(l.locked ? "Unlock" : "Lock", fill: swipeFill, ink: swipeInk) { model.toggleLocked(l) }
                    if l.kind != .mask {
                        button("Duplicate", fill: swipeFill, ink: swipeInk) { model.duplicate(l) }
                    }
                    button(alone ? "Clear" : "Delete", fill: swipeDelete, ink: .white) {
                        if alone {
                            model.clear(l)
                        } else {
                            model.deleteAsking(l)
                        }
                    }
                }
            }
            content()
                .offset(x: offset)
                .simultaneousGesture(
                    DragGesture(minimumDistance: 14)
                        .onChanged { drag in
                            guard enabled else { return }
                            if !sliding, abs(drag.translation.width) > abs(drag.translation.height) {
                                sliding = true
                            }
                            if sliding {
                                // closed, it goes right as far as picking takes
                                offset = min(max((open ? -reach : 0) + drag.translation.width, -reach), open ? 0 : pickReach)
                            }
                        }
                        .onEnded { _ in
                            guard sliding else { return }
                            sliding = false
                            if offset > pickReach / 2 {
                                onPick()
                                withAnimation(Motion.swipeSettle) { offset = 0 }
                                return
                            }
                            let opened = offset < -reach / 2
                            if opened == open {
                                withAnimation(Motion.swipeSettle) { offset = open ? -reach : 0 }
                            } else {
                                onOpen(opened)
                            }
                        }
                )
        }
        // the list only clips top and bottom; a slid row stops at its edge
        .clipped()
        .onChange(of: open, initial: true) { _, now in
            withAnimation(Motion.swipeSettle) { offset = now ? -reach : 0 }
        }
    }
}

/// Two fingers on the rows. These are UIKit's: a SwiftUI gesture over the rows would take their
/// one-finger touches, which these leave alone (they watch from the window, for as long as the
/// panel is up).
private struct RowFingers: UIViewRepresentable {
    let model: CanvasModel
    let ui: LayersUi

    final class Anchor: UIView, UIGestureRecognizerDelegate {
        var model: CanvasModel?
        fileprivate var ui: LayersUi?
        private var watching: [UIGestureRecognizer] = []
        /// The two rows pinched, where their fingers went down, and how far apart.
        private var ends: (upper: UInt64, lower: UInt64)?
        private var starts: (upper: CGFloat, lower: CGFloat) = (0, 0)
        private var apart: CGFloat = 0
        private var pinched = false

        override func willMove(toWindow window: UIWindow?) {
            super.willMove(toWindow: window)
            watching.forEach { $0.view?.removeGestureRecognizer($0) }
            watching = []
            guard let window else { return }
            let swipe = UISwipeGestureRecognizer(target: self, action: #selector(swiped(_:)))
            swipe.direction = .right
            swipe.numberOfTouchesRequired = 2
            let hold = UILongPressGestureRecognizer(target: self, action: #selector(held(_:)))
            hold.numberOfTouchesRequired = 2
            hold.minimumPressDuration = 0.5
            let pinch = UIPinchGestureRecognizer(target: self, action: #selector(pinching(_:)))
            let tap = UITapGestureRecognizer(target: self, action: #selector(tapped(_:)))
            tap.numberOfTouchesRequired = 2
            for recognizer in [swipe, hold, pinch, tap] {
                recognizer.cancelsTouchesInView = false
                recognizer.delaysTouchesBegan = false
                recognizer.delaysTouchesEnded = false
                recognizer.delegate = self
                window.addGestureRecognizer(recognizer)
                watching.append(recognizer)
            }
        }

        func gestureRecognizer(_ g: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
            true
        }

        /// Only on the rows: elsewhere two fingers are the canvas's.
        override func gestureRecognizerShouldBegin(_ g: UIGestureRecognizer) -> Bool {
            ui?.row(at: g.location(in: nil).y) != nil && insidePanel(g.location(in: nil).x)
        }

        private func insidePanel(_ x: CGFloat) -> Bool {
            guard let rows = ui?.rows.values, let any = rows.first else { return false }
            return x >= any.minX && x <= any.maxX
        }

        /// The paint layer under a two-finger touch.
        private func paintLayer(_ g: UIGestureRecognizer) -> LayerInfo? {
            guard let id = ui?.row(at: g.location(in: nil).y) else { return nil }
            return model?.layers.first { $0.id == id && $0.kind == .paint }
        }

        @objc private func swiped(_ g: UISwipeGestureRecognizer) {
            if let l = paintLayer(g) {
                model?.toggleAlphaLock(l)
            }
        }

        /// Two fingers tapped on a layer: its opacity, dragged across the canvas.
        @objc private func tapped(_ g: UITapGestureRecognizer) {
            if let l = paintLayer(g) {
                model?.selectLayer(l.id)
                model?.startLayerOpacity()
            }
        }

        @objc private func held(_ g: UILongPressGestureRecognizer) {
            if g.state == .began, let l = paintLayer(g) {
                model?.selectLayerContents(l)
            }
        }

        @objc private func pinching(_ g: UIPinchGestureRecognizer) {
            guard let ui else { return }
            switch g.state {
            case .began:
                guard g.numberOfTouches >= 2 else { return }
                let a = g.location(ofTouch: 0, in: nil).y
                let b = g.location(ofTouch: 1, in: nil).y
                let upper = min(a, b)
                let lower = max(a, b)
                pinched = false
                ends = nil
                if let top = ui.row(at: upper), let bottom = ui.row(at: lower), top != bottom {
                    ends = (top, bottom)
                    starts = (upper, lower)
                    apart = lower - upper
                }
            case .changed:
                guard let ends, g.numberOfTouches >= 2 else { return }
                let a = g.location(ofTouch: 0, in: nil).y
                let b = g.location(ofTouch: 1, in: nil).y
                let upper = min(a, b)
                let lower = max(a, b)
                if lower - upper <= apart * pinchMerge {
                    pinched = true
                }
                ui.squeeze = Squeeze(upper: ends.upper, lower: ends.lower, upperBy: upper - starts.upper, lowerBy: lower - starts.lower)
            default:
                ui.squeeze = nil
                // as a finger lets go
                if g.state == .ended, pinched, let ends {
                    model?.mergeBetween(ends.upper, ends.lower)
                }
                ends = nil
            }
        }
    }

    func makeUIView(context: Context) -> Anchor {
        let view = Anchor()
        view.isUserInteractionEnabled = false
        view.model = model
        view.ui = ui
        return view
    }

    func updateUIView(_ view: Anchor, context: Context) {}
}

/// The panel's menus, over it: the title's and the current layer's options.
private struct LayersMenus: View {
    let model: CanvasModel
    let ui: LayersUi
    let maxHeight: CGFloat

    private func closeAll() {
        withAnimation(Motion.menuClose) {
            ui.titleMenu = false
            ui.addKinds = nil
            ui.options = nil
        }
    }

    /// The plus with plugins' kinds of layer on: a plain layer, or one of each kind, named as the
    /// plugin names it beside its badge, as its layers' thumbnails show it (a plug when it draws
    /// none). It hangs from the plus, their right edges in line.
    private func addMenu(_ kinds: [PluginKind]) -> some View {
        // the plus's lower right corner: the header's end in from the right, its 41 under the
        // header's first 12 (the header starting under the arrow)
        let plus = CGPoint(x: layersWidth - 18, y: Pc.arrowHeight + 12 + 41)
        return GlassMenu(overPanel: true) {
            MenuRow(icon: PcIcons.menuNewLayer, label: "Layer") {
                closeAll()
                model.addLayer()
            }
            ForEach(kinds, id: \.key) { kind in
                GlyphMenuRow(label: kind.label, action: {
                    closeAll()
                    model.addPluginLayer(kind.key)
                }) {
                    PluginBadgeView(badge: pluginKindBadge(kind: kind), ink: Pc.menuInk, width: menuBadgeLine)
                        .frame(width: menuBadgeWidth, height: menuBadgeHeight)
                }
            }
            // the filter layers the plugins offer, in a list of their own
            let filters = model.layerFilters()
            if !filters.isEmpty {
                GlyphMenuRow(label: "Filter Layer", action: {
                    closeAll()
                    ui.filterList = filters
                }) {
                    PluginBadgeView(badge: filterLayerBadge(), ink: Pc.menuInk, width: menuBadgeLine)
                        .frame(width: menuBadgeWidth, height: menuBadgeHeight)
                }
            }
        }
        .offset(x: plus.x - glassMenuWidth, y: plus.y)
        .menuTransition(.topTrailing)
    }

    var body: some View {
        ZStack(alignment: .topLeading) {
            if ui.titleMenu {
                MenuDismissal { closeAll() }
                titleMenu
            }
            if let kinds = ui.addKinds {
                MenuDismissal { closeAll() }
                addMenu(kinds)
            }
            if let id = ui.options, let l = model.layers.first(where: { $0.id == id }), let row = ui.rows[id] {
                MenuDismissal { closeAll() }
                LayerOptionsMenu(model: model, ui: ui, layer: l, rowY: row.midY, maxHeight: maxHeight) { closeAll() }
            }
        }
    }

    /// Under the title: Select All; Group and Merge (with layers picked); Share and Duplicate;
    /// Delete. Shaped as the system's own menus are: frosted, the panel showing through blurred,
    /// its lines 11 clear of the rows.
    private var titleMenu: some View {
        GlassMenu(system: true) {
            MenuRow(icon: PcIcons.menuSelectAll, label: "Select All") {
                closeAll()
                model.pickAllLayers()
            }
            MenuLine(gap: 11)
            if model.anyPicked {
                MenuRow(icon: PcIcons.menuGroup, label: "Group") {
                    closeAll()
                    model.groupPicked()
                }
                MenuRow(icon: PcIcons.menuMerge, label: "Merge") {
                    closeAll()
                    model.mergePicked()
                }
                MenuLine(gap: 11)
            }
            MenuRow(icon: PcIcons.menuShare, label: "Share") {
                closeAll()
                model.shareChosenLayers()
            }
            MenuRow(icon: PcIcons.menuDuplicate, label: "Duplicate") {
                closeAll()
                model.duplicatePicked()
            }
            MenuLine(gap: 11)
            MenuRow(icon: PcIcons.menuTrash, label: "Delete", ink: Pc.menuDanger) {
                closeAll()
                model.question = Question(title: "Delete these layers?", message: "Undo brings them back.") {
                    // as measured, a locked one among them stops it, said why
                    if model.chosenLocked() {
                        model.lockedDeleteTold = true
                    } else {
                        model.deletePicked()
                    }
                }
            }
        }
        // under the title row, whose foot is 60 down with the arrow above it; its left edge 43 left
        // of the panel's, as measured
        .offset(x: -43, y: 60)
        .menuTransition(.top)
    }
}

/// The current layer's options, left of the panel and centred on its row, kept below the top bar.
/// What doesn't apply to it is left out; lock, duplicate and delete are on the row's swipe.
private struct LayerOptionsMenu: View {
    let model: CanvasModel
    let ui: LayersUi
    let layer: LayerInfo
    /// The row's middle on the screen.
    let rowY: CGFloat
    let maxHeight: CGFloat
    let onDone: () -> Void

    private func act(_ option: LayerOption) {
        let l = layer
        switch option {
        case .rename:
            ui.renaming = l.id
        case .ungroup:
            model.ungroup(l)
        case .flatten:
            model.flattenGroup(l)
        case .select:
            model.selectLayerContents(l)
        case .copy:
            model.copyLayerContents(l)
        case .fill:
            model.fillLayer(l)
        case .clear:
            model.clear(l)
        case .alphaLock:
            model.toggleAlphaLock(l)
        case .mask:
            model.addMask(l)
        case .clippingMask:
            model.toggleClipped(l)
        case .drawingAssist:
            model.setGuideAssisted(!l.assisted)
        case .invert:
            model.invertLayer(l)
        case .reference:
            model.toggleReference(l)
        case .editText:
            model.text.edit(l)
        case .filterSettings:
            model.startFilterLayerSettings(l)
        case .rasterize:
            model.rasterize(l)
        case .recolour:
            model.restyleStrokes(l, colour: true)
        case .applyBrush:
            model.restyleStrokes(l, colour: false)
        case .mergeDown:
            model.mergeDown(l)
        case .combineDown:
            model.combineDown(l)
        case .mergeMask:
            model.mergeMask(l)
        }
    }

    var body: some View {
        let rows = model.layerOptions(layer)
        // a plugin's actions for the layer, after the app's own
        let actions = model.panelPluginActions(.layer)
        let shape = RoundedRectangle(cornerRadius: 24, style: .circular)
        let tall = max(CGFloat(rows.count + actions.count) * 38 + 14, 64)
        // the panel hangs from the bar: its own top is the bar's height down the screen
        let top = optionsTop - Pc.barHeight
        let y = min(max(rowY - Pc.barHeight - tall / 2, top), max(maxHeight - tall, top))
        VStack(spacing: 0) {
            ForEach(rows, id: \.option) { r in
                let ink = r.enabled ? Pc.menuInk : Pc.menuDisabled
                HStack(spacing: 0) {
                    // 15 pt, as measured
                    Text(verbatim: r.label)
                        .font(.system(size: 15))
                        .foregroundStyle(ink)
                        .lineLimit(1)
                    Spacer(minLength: 0)
                    if r.switchOn == true {
                        PcIconView(PcIcons.tick, size: 18, color: ink)
                    }
                }
                .padding(.leading, 19)
                .padding(.trailing, 16)
                .frame(height: 38)
                .contentShape(Rectangle())
                .onTapGesture {
                    if r.enabled {
                        act(r.option)
                        onDone()
                    }
                }
            }
            ForEach(actions, id: \.key) { action in
                Text(verbatim: action.label)
                    .font(.system(size: 15))
                    .foregroundStyle(Pc.menuInk)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.leading, 19)
                    .padding(.trailing, 16)
                    .frame(height: 38)
                    .contentShape(Rectangle())
                    .onTapGesture {
                        onDone()
                        model.runPluginAction(action.key)
                    }
            }
        }
        .padding(.vertical, 7)
        .frame(width: 194)
        .frame(minHeight: 64)
        .background {
            // empty (a locked group's or mask's), as measured, the frost alone, untinted
            ZStack {
                shape.fill(.ultraThinMaterial)
                    .environment(\.colorScheme, Look.shared.light ? .light : .dark)
                if !rows.isEmpty || !actions.isEmpty {
                    shape.fill(Pc.glassMenu)
                }
            }
        }
        .clipShape(shape)
        .shadow(color: .black.opacity(0.14), radius: 15, y: 6)
        .offset(x: -183, y: y)
        .menuTransition(.trailing)
    }
}

/// How the blend list is moving.
private enum BlendMotion {
    /// At rest on a row.
    case resting
    /// Held: pressed with the list where a pull of `start` (unstretched) takes it, the finger then
    /// at `from` down the list; `caught` when it was moving as it was pressed (let go without
    /// dragging, it only stops).
    case held(start: CGFloat, caught: Bool, from: CGFloat)
    /// Let go at `since`, moving on as `coast` says.
    case coasting(BlendCoast, since: CFTimeInterval)
    /// A tapped mode on its way to the band since `since`: the list takes no touches and the
    /// modes it passes don't show on the layer.
    case tapped(from: Float, to: Float, since: CFTimeInterval)
}

/// The blend list's modes turning under a band fixed in its middle: how far it is scrolled (0
/// with the first mode in the band, a row further for each mode after; past an end while
/// stretched) and what moves it, as the shared core works it out. Each mode reaching the band as
/// it moves shows on the layer, the whole movement one undo step.
@Observable
private final class BlendWheel {
    var position: CGFloat = 0
    @ObservationIgnored private weak var model: CanvasModel?
    @ObservationIgnored private var layer: LayerInfo?
    @ObservationIgnored private var modes: [BlendModeInfo] = []
    /// The mode last shown on the layer.
    @ObservationIgnored private var shownMode: UInt32 = 0
    @ObservationIgnored private var motion = BlendMotion.resting
    @ObservationIgnored private let loop = FrameLoop()
    /// The finger's last places down the list and when, for how fast it was going as it let go.
    @ObservationIgnored private var trail: [(at: CFTimeInterval, y: CGFloat)] = []

    private var rows: UInt32 {
        UInt32(modes.count)
    }

    /// Under a finger or moving on after it.
    private var moving: Bool {
        switch motion {
        case .held, .coasting: true
        default: false
        }
    }

    /// Opened under `l`: its mode in the band.
    func open(_ model: CanvasModel, _ l: LayerInfo) {
        self.model = model
        modes = model.blendModeList
        follow(l)
        shownMode = l.blend
        position = place(of: l.blend)
    }

    /// The layer read again: undo or redo may have changed its mode, which comes to the band.
    func follow(_ l: LayerInfo) {
        layer = l
        guard case .resting = motion, l.blend != shownMode else { return }
        shownMode = l.blend
        position = place(of: l.blend)
    }

    private func place(of blend: UInt32) -> CGFloat {
        CGFloat(modes.firstIndex { $0.id == blend } ?? 0) * blendRowHeight
    }

    /// Pressed with the finger `y` down the list: it follows the finger from where it is, caught
    /// if it was moving.
    func pressed(at y: CGFloat) {
        if case .tapped = motion {
            return
        }
        let caught = moving
        loop.stop()
        let start = blendPullFor(position: Float(position), row: Float(blendRowHeight), rows: rows, height: Float(blendListHeight))
        motion = .held(start: CGFloat(start), caught: caught, from: y)
        trail = [(CACurrentMediaTime(), y)]
    }

    /// The finger at `y`: the list there between its ends, stretched past them.
    func dragged(to y: CGFloat) {
        guard case let .held(start, _, from) = motion else { return }
        let now = CACurrentMediaTime()
        trail.append((now, y))
        trail.removeAll { now - $0.at > 0.1 }
        position = CGFloat(blendPulled(pull: Float(start + from - y), row: Float(blendRowHeight), rows: rows, height: Float(blendListHeight)))
        showCentred()
    }

    /// Let go `y` down the list: a tap (`tap`) on a row away from the band makes it the mode at
    /// once and scrolls it there; else the list moves on as it was going, slowing onto a row (a
    /// tap on a moving list only stops it).
    func letGo(at y: CGFloat, tap: Bool) {
        guard case let .held(_, caught, _) = motion else { return }
        let now = CACurrentMediaTime()
        let band = (blendListHeight - blendRowHeight) / 2
        let row = ((y - band + position) / blendRowHeight).rounded(.down)
        if tap, !caught, row >= 0, row < CGFloat(rows),
           let to = blendTap(tapped: UInt32(row), position: Float(position), row: Float(blendRowHeight), rows: rows)
        {
            model?.finishBlend()
            let mode = modes[Int(row)].id
            shownMode = mode
            if let layer {
                model?.setBlend(layer, mode)
            }
            motion = .tapped(from: Float(position), to: to, since: now)
        } else {
            trail.append((now, y))
            trail.removeAll { now - $0.at > 0.1 }
            var speed: CGFloat = 0
            if !tap, let first = trail.first, let last = trail.last, last.at - first.at > 0.005 {
                // down the list: the finger going up
                speed = -(last.y - first.y) / CGFloat(last.at - first.at)
            }
            motion = .coasting(blendCoast(from: Float(position), speed: Float(speed), row: Float(blendRowHeight), rows: rows), since: now)
        }
        trail = []
        loop.start { [weak self] _ in self?.advance() ?? false }
    }

    /// A frame of the list moving by itself; at rest, what it showed is done. Says whether it
    /// moves on.
    private func advance() -> Bool {
        let now = CACurrentMediaTime()
        let step: BlendStep
        switch motion {
        case let .coasting(coast, since):
            step = blendCoastAt(coast: coast, seconds: Float(now - since))
            position = CGFloat(step.position)
            showCentred()
        case let .tapped(from, to, since):
            step = blendTapAt(from: from, to: to, seconds: Float(now - since))
            position = CGFloat(step.position)
        default:
            return false
        }
        guard step.done else { return true }
        let coasted = moving
        motion = .resting
        if coasted {
            model?.finishBlend()
        }
        return false
    }

    /// The mode in the band shows on the layer.
    private func showCentred() {
        let i = Int(blendCentred(position: Float(position), row: Float(blendRowHeight), rows: rows))
        guard modes.indices.contains(i), modes[i].id != shownMode, let layer else { return }
        shownMode = modes[i].id
        model?.previewBlend(layer, shownMode)
    }

    /// Closed: it stops where it is, what it showed done.
    func close() {
        loop.stop()
        motion = .resting
        model?.finishBlend()
    }
}

/// The blend list's touches: pressed, moved and let go. They are taken as the finger goes down,
/// so the panel's list doesn't scroll with them.
private struct BlendTouches: UIViewRepresentable {
    let wheel: BlendWheel

    final class Coordinator: NSObject, UIGestureRecognizerDelegate {
        fileprivate var wheel: BlendWheel?
        private var start: CGPoint = .zero
        private var far: CGFloat = 0

        @objc func touched(_ g: UILongPressGestureRecognizer) {
            guard let wheel, let view = g.view else { return }
            let p = g.location(in: view)
            switch g.state {
            case .began:
                start = p
                far = 0
                wheel.pressed(at: p.y)
            case .changed:
                far = max(far, hypot(p.x - start.x, p.y - start.y))
                wheel.dragged(to: p.y)
            case .ended:
                wheel.letGo(at: p.y, tap: far < 10)
            default:
                wheel.letGo(at: p.y, tap: false)
            }
        }

        /// A scroll view round it waits for these touches to be someone else's.
        func gestureRecognizer(_ g: UIGestureRecognizer, shouldBeRequiredToFailBy other: UIGestureRecognizer) -> Bool {
            other.view is UIScrollView
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeUIView(context: Context) -> UIView {
        let view = UIView()
        view.backgroundColor = .clear
        let press = UILongPressGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.touched(_:)))
        press.minimumPressDuration = 0
        press.allowableMovement = .greatestFiniteMagnitude
        press.delegate = context.coordinator
        view.addGestureRecognizer(press)
        context.coordinator.wheel = wheel
        return view
    }

    func updateUIView(_ view: UIView, context: Context) {
        context.coordinator.wheel = wheel
    }
}

/// A layer's opacity and blend mode, under its row. The modes are a picker: the rows turn under a
/// band fixed in the list's middle, the row there the layer's mode as it moves (one undo step a
/// movement); let go, the list slows straight onto a row; a tap on another row makes it the mode
/// at once and scrolls it to the band. The layer's mode is in the band when it opens.
private struct BlendEditor: View {
    let model: CanvasModel
    let layer: LayerInfo

    @State private var opacity: Float = 1
    @State private var wheel = BlendWheel()

    private func slider(_ width: CGFloat) -> some View {
        let part = CGFloat(min(max(opacity, 0), 1))
        let half = opacityThumb.width / 2
        let x = min(max(width * part, half), max(width - half, half))
        return ZStack(alignment: .leading) {
            Capsule().fill(Pc.sliderRest).frame(height: 3)
            Capsule().fill(Pc.accent).frame(width: width * part, height: 3)
            Capsule().fill(Color.white)
                .frame(width: opacityThumb.width, height: opacityThumb.height)
                .offset(x: x - half)
        }
        .frame(width: width, height: opacityThumb.height)
        // touched over the slider's whole 30 height
        .frame(height: opacityTouch)
        .contentShape(Rectangle())
        .gesture(
            DragGesture(minimumDistance: 0)
                .onChanged { drag in
                    opacity = Float(min(max(drag.location.x / max(width, 1), 0), 1))
                    model.setLayerOpacity(layer, opacity, done: false)
                }
                .onEnded { _ in model.setLayerOpacity(layer, opacity, done: true) }
        )
    }

    var body: some View {
        let modes = model.blendModeList
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                Text(verbatim: "Opacity")
                    .foregroundStyle(Pc.panelTitle)
                Spacer(minLength: 0)
                // the value lighter than its label, as measured
                Text(verbatim: opacity >= 0.995 ? "Max" : "\(wholeNumber(opacity * 100))%")
                    .foregroundStyle(Look.shared.light ? Pc.panelTitle : opacityValueInk)
            }
            .font(.system(size: 17))
            .padding(.horizontal, 13)
            GeometryReader { g in
                slider(g.size.width)
            }
            .frame(height: opacityTouch)
            .padding(.horizontal, 13)
            // the track's middle 6.5 under the label's line, the touch reaching up over it
            .padding(.top, 6.5 - opacityTouch / 2)
            let band = (blendListHeight - blendRowHeight) / 2
            ZStack(alignment: .top) {
                Rectangle()
                    .fill(Pc.rowSelected)
                    .frame(height: blendRowHeight)
                    .padding(.top, band)
                // grey outside the band and white inside it: a name half under it is half and half
                names(modes, ink: Pc.panelTitle, glyphInk: blendGlyphInk)
                    .mask(alignment: .top) {
                        VStack(spacing: 0) {
                            Rectangle().frame(height: band)
                            Color.clear.frame(height: blendRowHeight)
                            Rectangle()
                        }
                    }
                names(modes, ink: .white, glyphInk: .white)
                    .mask(alignment: .top) {
                        Rectangle()
                            .frame(height: blendRowHeight)
                            .padding(.top, band)
                    }
            }
            .frame(maxWidth: .infinity)
            .frame(height: blendListHeight, alignment: .top)
            .background(Pc.layerRow)
            .clipShape(RoundedRectangle(cornerRadius: 12, style: .circular))
            .overlay(BlendTouches(wheel: wheel))
            // the list 29.5 under the track's middle
            .padding(.top, 29.5 - opacityTouch / 2)
        }
        .padding(.top, 22)
        .padding(.bottom, 13)
        .onAppear {
            opacity = layer.opacity
            wheel.open(model, layer)
        }
        .onChange(of: layer.blend) { wheel.follow(layer) }
        .onDisappear { wheel.close() }
    }

    /// The modes' names and glyphs (each glyph's 26 frame centred 31.5 in from the right), turned
    /// as far as the list is, over the list's height.
    private func names(_ modes: [BlendModeInfo], ink: Color, glyphInk: Color) -> some View {
        VStack(spacing: 0) {
            ForEach(modes, id: \.id) { m in
                HStack(spacing: 0) {
                    Text(verbatim: m.name)
                        .font(.system(size: 15))
                        .foregroundStyle(ink)
                        .lineLimit(1)
                    Spacer(minLength: 0)
                    if let glyph = PcIcons.blendGlyphs.first(where: { $0.0 == m.name })?.1 {
                        PcIconView(glyph, size: 26, color: glyphInk)
                    }
                }
                .padding(.leading, 12)
                .padding(.trailing, 18.5)
                .frame(height: blendRowHeight)
            }
        }
        .fixedSize(horizontal: false, vertical: true)
        .offset(y: (blendListHeight - blendRowHeight) / 2 - wheel.position)
        .frame(maxWidth: .infinity)
        .frame(height: blendListHeight, alignment: .top)
        .allowsHitTesting(false)
    }
}

/// A layer's row.
private struct LayerRow: View {
    let model: CanvasModel
    let layer: LayerInfo
    let shape: UnevenRoundedRectangle
    /// The layer whose mask is the current one: a darker blue.
    let owner: Bool
    let renaming: Bool
    let onRenamed: (String) -> Void
    let onTap: () -> Void
    let onBlend: () -> Void
    /// On a model: how far in it sits under its mesh, what it says under its name, the button that
    /// shows its channels (blue while one of them is painted), and whether it shows chosen.
    var indent: CGFloat = 0
    var subtitle: String?
    var onCube: (() -> Void)?
    var cubeLit = false
    var selected: Bool?
    /// Held over by a dragged row long enough to take it: its thumbnail outlined.
    var held = false

    var body: some View {
        let l = layer
        let mask = l.kind == .mask
        let masked = model.hasMask(l)
        let selected = self.selected ?? l.current
        let ink = selected || l.picked ? Color.white : Pc.panelTitle
        let fill = selected ? Pc.rowSelected : l.picked ? pickedRow : owner ? maskOwnerRow : Pc.layerRow
        HStack(spacing: 0) {
            if l.clipped {
                PcIconView(PcIcons.clip, size: 16, color: ink)
            }
            // a mask's thumbnail and its layer's join into one, rounded only at the outer corners
            Thumb(model: model, layer: l, top: !masked, bottom: !mask, held: held)
                .padding(.leading, 4)
                .offset(y: mask ? 2 : masked ? -2 : 0)
            ZStack(alignment: .leading) {
                LockedName(layer: l, ink: ink, renaming: renaming, onRenamed: onRenamed)
                if let tag = tag(l) {
                    Text(verbatim: tag)
                        .font(.system(size: 13))
                        .foregroundStyle(ink)
                        .lineLimit(1)
                        .padding(.leading, 29)
                        .offset(y: 17.5)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
            // a model's layer: laid out narrower than it is touched (36 from the blend letter), so
            // the name keeps its room up to the icon
            if let onCube {
                Spacer().frame(width: 10)
                PcIconView(PcIcons.tab3D, size: 48, color: cubeLit ? Pc.accent : ink)
                    .frame(width: 44, height: 47)
                    .contentShape(Rectangle())
                    .onTapGesture(perform: onCube)
                    .frame(width: 12, height: 47)
            }
            // the blend and visibility buttons overlap by 8, as measured; a mask has no blend
            if mask {
                Spacer().frame(width: 36)
            } else {
                Text(verbatim: blendLetters(model, l.blend))
                    .font(.system(size: 14, weight: .semibold))
                    // a locked layer's mode is greyed
                    .foregroundStyle(l.locked && !l.current && !l.picked ? lockedLetter : ink)
                    .frame(width: 44, height: 47)
                    .contentShape(Rectangle())
                    .onTapGesture(perform: onBlend)
                    .offset(x: 8)
            }
            VisibilityBox(model: model, layer: l, fill: selected || l.picked ? Color.white : Pc.checkFill, height: 47)
            Spacer().frame(width: 8)
        }
        .padding(.leading, indent + (l.clipped ? 18 : 0) + groupIndent * CGFloat(l.depth))
        .frame(maxWidth: .infinity)
        .frame(height: mask || masked ? maskedRowHeight : rowHeight)
        .background(RowFill(shape: shape, fill: fill, layer: l))
        .contentShape(Rectangle())
        .onTapGesture(perform: onTap)
    }

    /// What the layer is, under its name: the reference layer, one drawing assist is on for, a
    /// private one, a model's layer shared by its texture set's meshes.
    private func tag(_ l: LayerInfo) -> String? {
        if l.reference && l.assisted {
            return "Assisted Reference"
        }
        if l.reference {
            return subtitle.map { "\($0) Reference" } ?? "Reference"
        }
        if let subtitle {
            return subtitle
        }
        if l.assisted {
            return "Assisted"
        }
        if l.private {
            return "Private"
        }
        return nil
    }
}

/// The letters on a layer's row for its mode: a two-word name's initials (the second small), a
/// one-word name's first letter, or its first two where one letter is already taken.
private func blendLetters(_ model: CanvasModel, _ blend: UInt32) -> String {
    guard let name = model.blendModeList.first(where: { $0.id == blend })?.name else { return "N" }
    if twoLetterBlends.contains(name) {
        return String(name.prefix(2))
    }
    let words = name.split(separator: " ")
    if words.count == 1 {
        return String(name.prefix(1))
    }
    return String(words[0].prefix(1)) + words[1].prefix(1).lowercased()
}

/// A layer's visibility box: tapped, shown or hidden (or, with a layer shown alone, every layer
/// again); held, shown alone (or every layer again). A layer another's solo hides shows unticked.
private struct VisibilityBox: View {
    let model: CanvasModel
    let layer: LayerInfo
    let fill: Color
    let height: CGFloat

    var body: some View {
        CheckBox(checked: layer.visible && !layer.soloedOut, fill: fill)
            .frame(width: 44, height: height)
            .contentShape(Rectangle())
            .onTapGesture { model.toggleVisible(layer) }
            .onLongPressGesture { model.toggleSolo(layer) }
    }
}

/// A group's row: where a thumbnail would be, a line while it is open or, closed (the row taller),
/// a window on its layers' rows drawn small; its name, its open or closed arrow and its box. Held
/// over by a dragged row long enough to take it (`held`), the line or window is outlined.
private struct GroupRow: View, Animatable {
    let model: CanvasModel
    let layer: LayerInfo
    let shape: UnevenRoundedRectangle
    let renaming: Bool
    let onRenamed: (String) -> Void
    let onTap: () -> Void
    let held: Bool
    /// 0 open, 1 closed, between as it opens or closes.
    var closed: CGFloat

    var animatableData: CGFloat {
        get { closed }
        set { closed = newValue }
    }

    var body: some View {
        let l = layer
        let ink = l.current || l.picked ? Color.white : Pc.panelTitle
        let height = groupHeight + (groupClosedHeight - groupHeight) * closed
        // the line's 20 above and below become 4, the line the window between
        let inset = groupLineInset + (groupWindowInset - groupLineInset) * closed
        let window = RoundedRectangle(cornerRadius: 2 + 2 * closed, style: .circular)
        HStack(spacing: 0) {
            window
                .fill(groupLine)
                .frame(width: 93, height: max(height - inset * 2, 0))
                .overlay(alignment: .topLeading) {
                    if closed > 0 {
                        GroupContents(model: model, group: l)
                    }
                }
                .clipShape(window)
                .overlay {
                    if held {
                        window.strokeBorder(heldOutline, lineWidth: 2)
                    }
                }
                .padding(.leading, 4)
            LockedName(layer: l, ink: ink, renaming: renaming, onRenamed: onRenamed)
                .frame(maxWidth: .infinity, alignment: .leading)
            PcIconView(l.collapsed ? PcIcons.chevronRight : PcIcons.chevronDown, size: 16, color: ink)
                .frame(width: 44, height: groupHeight)
                .contentShape(Rectangle())
                .onTapGesture { model.toggleCollapsed(l) }
                .offset(x: 8)
            VisibilityBox(model: model, layer: l, fill: l.current || l.picked ? Color.white : Pc.checkFill, height: groupHeight)
            Spacer().frame(width: 8)
        }
        .padding(.leading, groupIndent * CGFloat(l.depth))
        .frame(maxWidth: .infinity)
        .frame(height: height)
        .background(RowFill(shape: shape, fill: l.current ? Pc.rowSelected : l.picked ? pickedRow : Pc.layerRow, layer: l))
        .contentShape(Rectangle())
        .onTapGesture(perform: onTap)
    }
}

/// A closed group's layers' rows as the panel would list them open, drawn small to fit its
/// window's width less 1 on each side, from its top: their fills, thumbnails, names, blend letters
/// and boxes.
private struct GroupContents: View {
    let model: CanvasModel
    let group: LayerInfo

    var body: some View {
        VStack(spacing: rowGap) {
            ForEach(model.shownInside(group), id: \.id) { l in
                MiniRow(model: model, layer: l, depth: max(Int(l.depth) - Int(group.depth) - 1, 0))
            }
        }
        .frame(width: listWidth)
        .fixedSize(horizontal: false, vertical: true)
        .scaleEffect((93 - 2) / listWidth, anchor: .topLeading)
        .offset(x: 1, y: 1)
        .allowsHitTesting(false)
    }
}

/// A row of a closed group's window, as its row in the list looks, without its controls.
private struct MiniRow: View {
    let model: CanvasModel
    let layer: LayerInfo
    /// How many groups in from the closed one it is.
    let depth: Int

    var body: some View {
        let l = layer
        let mask = l.kind == .mask
        let masked = model.hasMask(l)
        let group = l.kind == .group
        let chosen = l.current || l.picked
        let ink = chosen ? Color.white : Pc.panelTitle
        HStack(spacing: 0) {
            if group {
                RoundedRectangle(cornerRadius: 2, style: .circular)
                    .fill(groupLine)
                    .frame(width: 93, height: 4)
                    .padding(.leading, 4)
            } else {
                Thumb(model: model, layer: l, top: !masked, bottom: !mask)
                    .padding(.leading, 4)
                    .offset(y: mask ? 2 : masked ? -2 : 0)
            }
            Text(verbatim: l.name)
                .font(.system(size: 17))
                .foregroundStyle(ink)
                .lineLimit(1)
                .padding(.leading, 29)
                .padding(.trailing, 6)
                .frame(maxWidth: .infinity, alignment: .leading)
            if group {
                PcIconView(l.collapsed ? PcIcons.chevronRight : PcIcons.chevronDown, size: 16, color: ink)
                    .frame(width: 44, height: groupHeight)
                    .offset(x: 8)
            } else if mask {
                Spacer().frame(width: 36)
            } else {
                Text(verbatim: blendLetters(model, l.blend))
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(l.locked && !chosen ? lockedLetter : ink)
                    .frame(width: 44, height: 47)
                    .offset(x: 8)
            }
            CheckBox(checked: l.visible && !l.soloedOut, fill: chosen ? Color.white : Pc.checkFill)
                .frame(width: 44, height: 47)
            Spacer().frame(width: 8)
        }
        .padding(.leading, groupIndent * CGFloat(depth))
        .frame(maxWidth: .infinity)
        .frame(height: group ? groupHeight : mask || masked ? maskedRowHeight : rowHeight)
        .background(RowFill(shape: Rectangle(), fill: l.current ? Pc.rowSelected : l.picked ? pickedRow : Pc.layerRow, layer: l))
    }
}

/// A layer's name, renamed in place, with a padlock before it when it is locked (the glyph ends 12
/// short of the name).
private struct LockedName: View {
    let layer: LayerInfo
    let ink: Color
    let renaming: Bool
    let onRenamed: (String) -> Void

    var body: some View {
        ZStack(alignment: .leading) {
            if layer.locked {
                PcIconView(PcIcons.lockRow, size: 17, color: ink)
                    .offset(x: 3)
            }
            Group {
                if renaming {
                    InlineName(name: layer.name, color: ink, fontSize: 17, onDone: onRenamed)
                } else {
                    Text(verbatim: layer.name)
                        .font(.system(size: 17))
                        .foregroundStyle(ink)
                        .lineLimit(1)
                }
            }
            .padding(.leading, 29)
            .padding(.trailing, 6)
        }
    }
}

/// A layer's thumbnail, 93 x 61: what is drawn, fitted (small drawings show their pixels), on
/// grey, white for a mask, a checkerboard with alpha lock; a text layer's is its glyph. `top` and
/// `bottom` say which corners are rounded.
private struct Thumb: View {
    let model: CanvasModel
    let layer: LayerInfo
    let top: Bool
    let bottom: Bool
    /// Outlined: held over by a dragged row long enough to take it.
    var held = false

    var body: some View {
        let corner: CGFloat = 4
        let shape = UnevenRoundedRectangle(
            topLeadingRadius: top ? corner : 0, bottomLeadingRadius: bottom ? corner : 0,
            bottomTrailingRadius: bottom ? corner : 0, topTrailingRadius: top ? corner : 0,
            style: .circular
        )
        ZStack {
            if layer.alphaLock {
                Checker()
            } else {
                Rectangle().fill(layer.kind == .mask ? Color.white : Pc.thumbBackground)
            }
            if layer.text {
                PcIconView(PcIcons.textLayer, size: 61, color: Pc.textLayerGlyph)
            } else if let picture = model.thumbnails[layer.id] {
                Image(uiImage: picture)
                    .resizable()
                    .interpolation(.none)
                    .scaledToFit()
            }
        }
        .frame(width: 93, height: 61)
        .overlay(alignment: .bottomTrailing) {
            // a plugin's layer: its kind's badge in its corner, as its manifest draws it
            if let badge = layer.pluginBadge {
                ZStack {
                    RoundedRectangle(cornerRadius: 4, style: .circular).fill(pluginBadgeBehind)
                    PluginBadgeView(badge: badge, ink: .white, width: 1.5)
                }
                .frame(width: 18, height: 14)
                .padding(3)
            }
        }
        .overlay {
            // 2 inside its edge, the rest clipped
            if held {
                shape.stroke(heldOutline, lineWidth: 4)
            }
        }
        .clipShape(shape)
    }
}

/// The transparency checkerboard behind thumbnails.
private struct Checker: View {
    var body: some View {
        Canvas { context, size in
            let cell: CGFloat = 6
            context.fill(Path(CGRect(origin: .zero, size: size)), with: .color(.white))
            var y = 0
            while CGFloat(y) * cell < size.height {
                var x = 0
                while CGFloat(x) * cell < size.width {
                    if (x + y) % 2 == 1 {
                        context.fill(Path(CGRect(x: CGFloat(x) * cell, y: CGFloat(y) * cell, width: cell, height: cell)), with: .color(Color(argb: 0xFFD6_D6D6)))
                    }
                    x += 1
                }
                y += 1
            }
        }
    }
}

/// A visibility box: filled with the tick cut out of it (the row showing through), or an outline.
struct CheckBox: View {
    let checked: Bool
    let fill: Color

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: 5, style: .circular)
        ZStack {
            if checked {
                shape.fill(fill)
                // the tick is cut out of the box
                PcIconView(PcIcons.tick, size: 14, color: .black)
                    .blendMode(.destinationOut)
            } else {
                shape.strokeBorder(fill, lineWidth: 1.5)
            }
        }
        .frame(width: 18, height: 18)
        .compositingGroup()
    }
}

/// The background colour's row: its swatch (none shown while hidden) and its visibility box.
private struct BackgroundRow: View {
    let model: CanvasModel

    var body: some View {
        let bg = model.background
        // hidden, the swatch shows no colour: a faint white over the row
        let swatch = bg.visible ? Color(.sRGB, red: Double(bg.r), green: Double(bg.g), blue: Double(bg.b)) : Color.white.opacity(0.06)
        HStack(spacing: 0) {
            HStack(spacing: 0) {
                RoundedRectangle(cornerRadius: 14, style: .circular)
                    .fill(swatch)
                    .frame(width: 92, height: 42)
                    .padding(.leading, 4)
                Text(verbatim: "Background color")
                    .font(.system(size: 17))
                    .foregroundStyle(Pc.panelTitle)
                    .lineLimit(1)
                    .padding(.leading, 14)
                Spacer(minLength: 0)
            }
            .contentShape(Rectangle())
            .onTapGesture { model.pickBackground() }
            CheckBox(checked: bg.visible, fill: Pc.checkFill)
                .frame(width: 44, height: 41)
                .contentShape(Rectangle())
                .onTapGesture { model.toggleBackground() }
            Spacer().frame(width: 8)
        }
        .frame(height: 50)
        .background(RoundedRectangle(cornerRadius: 17, style: .circular).fill(Pc.backgroundRow))
    }
}

/// The actions the plugins on put in the Layers panel, under the background's row: a row each, by
/// its label, drawn as the background's is. A tap runs it, as one anywhere else does.
private struct LayersPluginRows: View {
    let model: CanvasModel

    var body: some View {
        ForEach(model.panelPluginActions(.layers), id: \.key) { action in
            Text(verbatim: action.label)
                .font(.system(size: 17))
                .foregroundStyle(Pc.panelTitle)
                .lineLimit(1)
                .padding(.horizontal, 18)
                .frame(maxWidth: .infinity, alignment: .leading)
                .frame(height: 50)
                .background(RoundedRectangle(cornerRadius: 17, style: .circular).fill(Pc.backgroundRow))
                .contentShape(Rectangle())
                .onTapGesture { model.runPluginAction(action.key) }
                .padding(.top, 10)
        }
    }
}

/// How wide the colour a plugin labels a layer with shows, down the left edge of its row.
private let labelWidth: CGFloat = 5

/// A row's fill in its shape, and the colour a plugin labels its layer with, if one does, down its
/// left edge (under what is on the row).
private struct RowFill<S: Shape>: View {
    let shape: S
    let fill: Color
    let layer: LayerInfo

    var body: some View {
        ZStack(alignment: .leading) {
            shape.fill(fill)
            if let label = layer.label {
                Color(argb: 0xFF00_0000 | label)
                    .frame(width: labelWidth)
            }
        }
        .clipShape(shape)
    }
}

/// The layers panel's width.
let layersPanelWidth = layersWidth

/// A model's mesh opened in the layers panel: its row darkened blue.
private let submeshOpen = Color(argb: 0xFF25_344A)
private let submeshTile = Color(argb: 0xFF43_4246)
private let trayFill = Color(argb: 0xFF30_3032)
private let trayTile = Color(argb: 0xFF3B_3B3D)

/// On a model: each texture set headed by its name, its meshes under it; the open mesh's texture
/// set's layers under that, shared by every mesh in the set.
private struct ModelRows: View {
    let model: CanvasModel
    let ui: LayersUi
    let shown: [LayerInfo]

    var body: some View {
        let scene = model.scene
        ForEach(Array(scene.textureSets.enumerated()), id: \.offset) { setIndex, name in
            let meshes = Array(scene.submeshes.enumerated().filter { Int($0.element.textureSet) == setIndex })
            if !meshes.isEmpty {
                SetHeader(name: name, first: setIndex == 0)
                ForEach(Array(meshes.enumerated()), id: \.offset) { k, mesh in
                    let lastMesh = k == meshes.count - 1
                    let open = scene.openSubmesh == mesh.offset && setIndex == scene.textureSet && !shown.isEmpty
                    SubmeshRow(
                        model: model, index: mesh.offset, name: mesh.element.name, open: open,
                        roundTop: k == 0, roundBottom: lastMesh && !open
                    )
                    if open {
                        ForEach(Array(shown.enumerated()), id: \.element.id) { index, l in
                            LayerItem(
                                model: model, ui: ui, layer: l, index: index, shown: shown, indent: groupIndent,
                                corners: (false, lastMesh && index == shown.count - 1),
                                subtitle: meshes.count > 1 ? "Shared" : nil
                            )
                            .id(l.id)
                        }
                    }
                }
            }
        }
    }
}

/// A model's texture set's name, heading its meshes: 41 tall, 16 under the set before it.
private struct SetHeader: View {
    let name: String
    let first: Bool

    var body: some View {
        Text(verbatim: name)
            .font(.system(size: 14, weight: .semibold))
            .foregroundStyle(Pc.panelTitle)
            .lineLimit(1)
            .padding(.leading, 16)
            .padding(.top, 12)
            .frame(maxWidth: .infinity, alignment: .topLeading)
            .frame(height: 41, alignment: .top)
            .padding(.top, first ? 0 : 16)
    }
}

/// One of a model's meshes: its picture, its name, the arrow that opens it (its texture set's
/// layers under it) and its box (shown or hidden). A tap anywhere but the box opens or closes it;
/// slid left it shows Lock (or Unlock); a little right, it joins the meshes picked.
private struct SubmeshRow: View {
    let model: CanvasModel
    let index: Int
    let name: String
    let open: Bool
    let roundTop: Bool
    let roundBottom: Bool

    @State private var offset: CGFloat = 0
    @State private var opened = false
    /// The touch is going along the row, not down the list.
    @State private var sliding = false

    var body: some View {
        let scene = model.scene
        let hidden = scene.hiddenMeshes.indices.contains(index) && scene.hiddenMeshes[index]
        let locked = scene.lockedMeshes.indices.contains(index) && scene.lockedMeshes[index]
        let picked = scene.activeMeshes.contains(index)
        let reach = swipeButton + 6
        let shape = UnevenRoundedRectangle(
            topLeadingRadius: roundTop ? listCorner : 0, bottomLeadingRadius: roundBottom ? listCorner : 0,
            bottomTrailingRadius: roundBottom ? listCorner : 0, topTrailingRadius: roundTop ? listCorner : 0,
            style: .circular
        )
        ZStack(alignment: .trailing) {
            if offset < 0 {
                Text(verbatim: locked ? "Unlock" : "Lock")
                    .font(.system(size: 15, weight: .medium))
                    .foregroundStyle(swipeInk)
                    .lineLimit(1)
                    .frame(width: swipeButton)
                    .frame(maxHeight: .infinity)
                    .background(RoundedRectangle(cornerRadius: 8, style: .circular).fill(swipeFill))
                    .contentShape(Rectangle())
                    .onTapGesture {
                        scene.toggleLocked(index)
                        opened = false
                        withAnimation(Motion.swipeSettle) { offset = 0 }
                    }
            }
            HStack(spacing: 0) {
                ZStack {
                    RoundedRectangle(cornerRadius: 4, style: .circular).fill(submeshTile)
                    if let picture = scene.meshThumbnails[index] {
                        Image(uiImage: picture)
                            .resizable()
                            .scaledToFit()
                            .padding(4)
                    }
                }
                .frame(width: 93, height: 61)
                .clipShape(RoundedRectangle(cornerRadius: 4, style: .circular))
                .padding(.leading, 4)
                Text(verbatim: name)
                    .font(.system(size: 17))
                    .foregroundStyle(Pc.panelTitle)
                    .lineLimit(1)
                    .padding(.leading, 29)
                    .padding(.trailing, 6)
                    .frame(maxWidth: .infinity, alignment: .leading)
                if locked {
                    PcIconView(PcIcons.pageLocked, size: 16, color: Pc.panelTitle)
                }
                PcIconView(open ? PcIcons.chevronDown : PcIcons.chevronRight, size: 16, color: Pc.panelTitle)
                    .frame(width: 44, height: 51)
                    .contentShape(Rectangle())
                    .onTapGesture { scene.openMesh(index) }
                    .offset(x: 8)
                CheckBox(checked: !hidden, fill: Pc.checkFill)
                    .frame(width: 44, height: 47)
                    .contentShape(Rectangle())
                    .onTapGesture { scene.toggleHidden(index) }
                Spacer().frame(width: 8)
            }
            .frame(maxWidth: .infinity)
            .frame(height: rowHeight)
            .background(shape.fill(open || picked ? submeshOpen : Pc.layerRow))
            .contentShape(Rectangle())
            .onTapGesture { scene.openMesh(index) }
            .offset(x: offset)
            .simultaneousGesture(
                DragGesture(minimumDistance: 14)
                    .onChanged { drag in
                        if !sliding, abs(drag.translation.width) > abs(drag.translation.height) {
                            sliding = true
                        }
                        if sliding {
                            offset = min(max((opened ? -reach : 0) + drag.translation.width, -reach), opened ? 0 : pickReach)
                        }
                    }
                    .onEnded { _ in
                        guard sliding else { return }
                        sliding = false
                        if offset > pickReach / 2 {
                            scene.pickMore(index)
                            opened = false
                        } else {
                            opened = offset < -reach / 2
                        }
                        withAnimation(Motion.swipeSettle) { offset = opened ? -reach : 0 }
                    }
            )
        }
        .clipped()
    }
}

/// A model's layer's channels under its row: Color, Roughness and Metallic, each its picture of
/// the layer; the one painted on (of the current layer) blue. A tap paints on it; held, a picture
/// replaces what the channel holds.
private struct ChannelTray: View {
    let model: CanvasModel
    let layer: LayerInfo

    @State private var asking = false
    @State private var channelAsked = 0
    @State private var photo: PhotosPickerItem?

    var body: some View {
        let scene = model.scene
        HStack(spacing: 6) {
            ForEach(Array(["Color", "Roughness", "Metallic"].enumerated()), id: \.offset) { c, label in
                let chosen = layer.current && scene.tileChosen && scene.channel == c
                VStack(alignment: .leading, spacing: 0) {
                    ZStack {
                        Rectangle().fill(Pc.thumbBackground)
                        if let picture = scene.channelThumbnail(c, layer.id) {
                            Image(uiImage: picture)
                                .resizable()
                                .scaledToFit()
                        }
                    }
                    .frame(maxWidth: .infinity)
                    .frame(height: 102)
                    .clipShape(RoundedRectangle(cornerRadius: 3, style: .circular))
                    Text(verbatim: label)
                        .font(.system(size: 15))
                        .foregroundStyle(chosen ? Color.white : Pc.panelTitle)
                        .lineLimit(1)
                        .padding(.leading, 2)
                        .padding(.top, 9)
                }
                .padding(6)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                .background(RoundedRectangle(cornerRadius: 6, style: .circular).fill(chosen ? Pc.accent : trayTile))
                .contentShape(Rectangle())
                .onTapGesture { scene.chooseTile(c, of: layer) }
                .onLongPressGesture(minimumDuration: 0.5) {
                    channelAsked = c
                    asking = true
                }
            }
        }
        .padding(4)
        .frame(maxWidth: .infinity)
        .frame(height: 159)
        .background(trayFill)
        .photosPicker(isPresented: $asking, selection: $photo, matching: .images)
        .onChange(of: photo) { _, item in
            guard let item else { return }
            photo = nil
            let channel = channelAsked
            let l = layer
            Task { @MainActor in
                let data = try? await item.loadTransferable(type: Data.self)
                scene.replaceMap(l, channel: channel, with: data)
            }
        }
    }
}
