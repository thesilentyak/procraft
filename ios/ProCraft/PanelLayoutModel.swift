import Foundation
import SwiftUI

/// Where the bars always are depends on: the screen, the side of the side bar and its place down
/// that edge, and how big the top bar and the side bar show.
struct BarsKey: Equatable {
    let size: CGSize
    let rightHanded: Bool
    let centre: Float
    let topBar: CGFloat
    let sideBar: CGFloat
    let sideBarShown: CGFloat
}

/// What the floating panels' room depends on: the screen, the side bar they keep clear of (as big
/// as it shows), how far down the top bar reaches, and how big the floating windows show.
struct LayoutRoomKey: Equatable {
    let size: CGSize
    let sideBar: Bool
    let rightHanded: Bool
    let sideBarShown: CGFloat
    let barShown: CGFloat
    let floating: CGFloat
}

/// The floating panels' places (the Layers panel and the brush library torn off while a plugin
/// lets them float, the colour panel too while one lays the panels out): the core works them out
/// (PanelLayout), snapped to the screen's edges, each other and the Reference window while a plugin
/// lays the panels out (the `panel_layout` hook), folded to a tab, docked panels making room for
/// the canvas, and kept for each way the screen turns. With no plugin on that hook the colour
/// window goes where it always did.
extension CanvasModel {
    /// Reads the layout kept (the windows kept before there were layouts, for the first place of
    /// each), and how the plugins lay the panels out.
    func readLayout() {
        for key in [panelLayoutKey()] + FloatingPanels.panels.map({ floatingKey(panel: $0) }) {
            if let kept = UserDefaults.standard.string(forKey: key) {
                panelLayout.readKept(key: key, value: kept)
            }
        }
        if let kept = UserDefaults.standard.string(forKey: barLayoutKey()) {
            barLayout.readKept(kept: kept)
        }
        if let kept = UserDefaults.standard.string(forKey: namedLayoutsKey()) {
            namedLayouts.readKept(kept: kept)
        }
        followLayout()
    }

    /// The layout as it is now, kept for the next start.
    func keepLayout() {
        UserDefaults.standard.set(panelLayout.kept(), forKey: panelLayoutKey())
    }

    /// The plugins changed: how the panels are laid out.
    func followLayout() {
        panelLayout.follow(plugins: PluginsModel.shared.store)
        layoutLook = panelLayout.look()
        // (Collapse to edge turned on or off: the bars know at once, not only once they are told again)
        barLayout.setCollapsing(on: layoutLook.collapse)
        // the bars back where they always are as no plugin lets them move: no handles
        if !layoutLook.edit, editingLayout {
            doneEditLayout()
        }
        barChanges += 1
        applyFitInsets()
        // the colour panel kept floating comes back where it was
        if layoutLook.on, floatingColorsAt == nil, panelLayout.isOpen(piece: .colours) {
            floatingColorsAt = .zero
        }
        layoutChanged()
    }

    /// The layout changed: shown again, and what docked panels take from the drawing area told to
    /// the canvas.
    func layoutChanged() {
        layoutChanges += 1
        let insets = panelLayout.canvasInsets()
        let px = Float(pixels)
        session.setRoomInsets(left: insets.left * px, top: insets.top * px, right: insets.right * px, bottom: insets.bottom * px)
        if layoutLook.on, let shown = panelLayout.shown(piece: .colours) {
            floatingColorsAt = CGPoint(x: CGFloat(shown.x), y: CGFloat(shown.y))
        }
    }

    /// The room the floating panels go in now: the screen, clear of the side bar.
    func layoutRoom(_ key: LayoutRoomKey) {
        let clear = key.sideBar ? key.sideBarShown + Pc.popoverGap : 0
        let (w, h) = (Float(key.size.width), Float(key.size.height))
        let room = key.rightHanded
            ? FloatingRoom(left: 0, top: 0, right: w - Float(clear), bottom: h)
            : FloatingRoom(left: Float(clear), top: 0, right: w, bottom: h)
        panelLayout.setRoom(room: room, width: w, height: h, bar: Float(key.barShown), scale: Float(key.floating))
        layoutChanged()
    }

    /// Where `bar` shows while Edit layout moved it (interface points), while a plugin lets the
    /// bars be moved; else none: where it always is.
    func barMoved(_ bar: LayoutBar) -> BarPlace? {
        _ = barChanges
        guard layoutLook.edit, barLayout.isMoved(bar: bar) else { return nil }
        return barLayout.shown(bar: bar)
    }

    /// Which sides of `bar` lie against the window's edges where it shows now, as it is dragged too:
    /// square there, round elsewhere.
    func barTouching(_ bar: LayoutBar) -> BarSides? {
        _ = barChanges
        return barLayout.touching(bar: bar)
    }

    /// Collapse to edge: `bar` collapsed into its side (the side, its tab there, how far it slid),
    /// while it is.
    func barCollapsed(_ bar: LayoutBar) -> EdgeCollapse? {
        _ = barChanges
        return barLayout.collapsed(bar: bar)
    }

    /// Where `bar` goes when it collapses, collapsed or not (to slide it there and back); none while
    /// it can't.
    func barCollapsePlace(_ bar: LayoutBar) -> EdgeCollapse? {
        _ = barChanges
        return barLayout.collapsePlace(bar: bar)
    }

    /// Where `bar`'s collapse button goes, while it has one (interface points).
    func barCollapseButton(_ bar: LayoutBar) -> LayoutRect? {
        _ = barChanges
        return barLayout.collapseButton(bar: bar)
    }

    /// `bar` collapsed into its side, or brought back out, kept.
    func collapseBar(_ bar: LayoutBar, _ collapsed: Bool) {
        if barLayout.setCollapsed(bar: bar, collapsed: collapsed) {
            UserDefaults.standard.set(barLayout.kept(), forKey: barLayoutKey())
        }
        barChanges += 1
    }

    /// A drag from collapsed `bar`'s tab, `by` so far: out of the side far enough, it comes back out.
    func pullBarTab(_ bar: LayoutBar, by: CGSize) {
        if barLayout.pullTab(bar: bar, dx: Float(by.width), dy: Float(by.height)) {
            UserDefaults.standard.set(barLayout.kept(), forKey: barLayoutKey())
            barChanges += 1
        }
    }

    /// `piece`'s window collapsed into its side, while it is.
    func pieceCollapsed(_ piece: LayoutPiece) -> EdgeCollapse? {
        _ = layoutChanges
        return panelLayout.pieceCollapsed(piece: piece)
    }

    /// Where `piece`'s window goes when it collapses, collapsed or not; none while it can't.
    func pieceCollapsePlace(_ piece: LayoutPiece) -> EdgeCollapse? {
        _ = layoutChanges
        return panelLayout.pieceCollapsePlace(piece: piece)
    }

    /// The side `piece`'s window goes into at its collapse button, while it may.
    func pieceCollapseEdge(_ piece: LayoutPiece) -> WindowEdge? {
        _ = layoutChanges
        return panelLayout.pieceCollapseEdge(piece: piece)
    }

    /// `piece`'s window collapsed into its side, or brought back out, kept.
    func collapsePiece(_ piece: LayoutPiece, _ collapsed: Bool) {
        if panelLayout.setPieceCollapsed(piece: piece, collapsed: collapsed) {
            keepLayout()
        }
        layoutChanged()
    }

    /// A drag from collapsed `piece`'s tab, `by` so far: out of the side far enough, it comes back out.
    func pullPieceTab(_ piece: LayoutPiece, by: CGSize) {
        if panelLayout.pullPieceTab(piece: piece, dx: Float(by.width), dy: Float(by.height)) {
            keepLayout()
            layoutChanged()
        }
    }

    /// Whether any bar or window is collapsed into its side.
    func anyCollapsed() -> Bool {
        _ = barChanges
        _ = layoutChanges
        return barLayout.anyCollapsed() || panelLayout.anyCollapsed()
    }

    /// Collapse All (`collapsed`) or Open All: every bar and window against a side, kept.
    func collapseAll(_ collapsed: Bool) {
        if barLayout.setAllCollapsed(collapsed: collapsed) {
            UserDefaults.standard.set(barLayout.kept(), forKey: barLayoutKey())
        }
        barChanges += 1
        if panelLayout.setAllCollapsed(collapsed: collapsed) {
            keepLayout()
        }
        layoutChanged()
    }

    /// The bars where they always are as `key` says, told to Edit layout to move them from.
    func tellBars(_ key: BarsKey) {
        let size = key.size
        let (w, h) = (Float(size.width), Float(size.height))
        let landscape = size.width >= size.height
        barLayout.setWindow(width: w, height: h, snap: max(layoutLook.snap, 16))
        barLayout.setCollapsing(on: layoutLook.collapse)
        // the top bar's halves as big as it shows: laid out at its own points across the window
        let kb = key.topBar
        let gaps = Pc.barGaps(size.width / kb)
        let left = topBarHalfLength(left: true, gaps: gaps) * kb
        let right = topBarHalfLength(left: false, gaps: gaps) * kb
        let end = (Pc.barMargin - topBarHalfEnd) * kb
        let thick = Float(Pc.barHeight * kb)
        barLayout.setBar(bar: .topLeft, own: BarPlace(x: Float(end), y: 0, vertical: false), length: Float(left), thickness: thick)
        barLayout.setBar(bar: .topRight, own: BarPlace(x: w - Float(end + right), y: 0, vertical: false), length: Float(right), thickness: thick)
        // the side bar at its place down its edge, as big as it shows
        let sideX = key.rightHanded ? size.width - key.sideBarShown : 0
        let sideY = sideBarPlace(height: size.height).top
        barLayout.setBar(
            bar: .sideBar, own: BarPlace(x: Float(sideX), y: sideY, vertical: true),
            length: Float(Pc.sideBarHeight * key.sideBar), thickness: Float(key.sideBarShown)
        )
        if landscape != barsLandscape {
            barsLandscape = landscape
        }
        barChanges += 1
    }

    /// The named layouts' names, and the one last saved or chosen.
    func layoutNames() -> [String] {
        _ = namedChanges
        return namedLayouts.names()
    }

    func chosenLayout() -> String? {
        _ = namedChanges
        return namedLayouts.chosen()
    }

    private func keepNamed() {
        UserDefaults.standard.set(namedLayouts.kept(), forKey: namedLayoutsKey())
        namedChanges += 1
    }

    /// The bars and panels where they are now, saved as `name` (in place of one of that name).
    func saveLayout(_ name: String) {
        if namedLayouts.save(name: name, bars: barLayout, panels: panelLayout) != nil {
            keepNamed()
        }
    }

    /// The layout named `name` brought back: the bars and panels where it had them, kept.
    func chooseLayout(_ name: String) {
        guard namedLayouts.choose(name: name, bars: barLayout, panels: panelLayout) else { return }
        keepNamed()
        UserDefaults.standard.set(barLayout.kept(), forKey: barLayoutKey())
        barChanges += 1
        keepLayout()
        layoutChanged()
        applyFitInsets()
    }

    func renameLayout(_ from: String, _ to: String) {
        if namedLayouts.rename(from: from, to: to) != nil {
            keepNamed()
        }
    }

    func deleteLayout(_ name: String) {
        if namedLayouts.delete(name: name) {
            keepNamed()
        }
    }

    /// Edit layout on (from the Canvas tab): the panels close, and the bars show handles.
    func startEditLayout() {
        closePanels()
        barLayout.setEditing(editing: true)
        editingLayout = true
    }

    /// Edit layout's Done: the bars stay where they are, kept.
    func doneEditLayout() {
        barLayout.setEditing(editing: false)
        editingLayout = false
        UserDefaults.standard.set(barLayout.kept(), forKey: barLayoutKey())
    }

    /// Edit layout's Reset: every bar back where it always is.
    func resetLayout() {
        barLayout.reset()
        UserDefaults.standard.set(barLayout.kept(), forKey: barLayoutKey())
        barChanges += 1
        applyFitInsets()
    }

    /// `bar` dragged by its handle, pressed `from` and `by` all told since (interface points).
    func dragBar(_ bar: LayoutBar, from: CGPoint, by: CGSize) {
        if barLayout.editing(), barLayout.holding() != bar {
            heldAt = from
            _ = barLayout.hold(bar: bar, x: Float(from.x), y: Float(from.y))
        }
        _ = barLayout.drag(x: Float(heldAt.x + by.width), y: Float(heldAt.y + by.height))
        barChanges += 1
    }

    /// The bar held let go, kept where it is.
    func letGoBar() {
        if barLayout.letGo() {
            UserDefaults.standard.set(barLayout.kept(), forKey: barLayoutKey())
        }
        barChanges += 1
        // the side bar off its edge, or back against it: what fitting keeps clear of
        applyFitInsets()
    }

    /// The layout's piece for `panel`.
    func piece(_ panel: FloatingPanel) -> LayoutPiece {
        panel == .layers ? .layers : .brushes
    }

    /// `panel`'s window while it shows: torn off and not closed, and a plugin lets it float.
    func floatingWindow(_ panel: FloatingPanel) -> PieceShown? {
        shownPiece(piece(panel))
    }

    /// `panel`'s grabber dragged out past its start, the finger `at` (interface points): its window
    /// follows it, as big as it was last left, its grabber under the finger.
    func undockPanel(_ panel: FloatingPanel, at: CGPoint) {
        if floating.undocking != panel {
            floating.undocking = panel
            _ = panelLayout.openAt(piece: piece(panel), x: Float(at.x), y: Float(at.y), grab: Float(floatingGrabY))
        } else {
            _ = panelLayout.drag(x: Float(at.x), y: Float(at.y))
        }
        layoutChanged()
    }

    /// `panel`'s grabber let go: whether it was being dragged out, so it floats where it is (kept),
    /// and the panel that hung from its button is to close.
    func undockedPanel(_ panel: FloatingPanel) -> Bool {
        guard floating.undocking == panel else { return false }
        floating.undocking = nil
        if panelLayout.letGo() {
            keepLayout()
        }
        layoutChanged()
        return true
    }

    /// `piece`'s strip or tab (or, `sizing`, its corner) pressed: it moves (or is sized) with the
    /// finger until let go.
    func holdPiece(_ piece: LayoutPiece, sizing: Bool, at: CGPoint) {
        heldAt = at
        _ = panelLayout.hold(piece: piece, sizing: sizing, x: Float(at.x), y: Float(at.y))
    }

    /// The piece held dragged `by` all told since it was pressed (interface points).
    func dragPiece(by: CGSize) {
        _ = panelLayout.drag(x: Float(heldAt.x + by.width), y: Float(heldAt.y + by.height))
        layoutChanged()
    }

    /// The piece held let go, kept where it is.
    func letGoPiece() {
        if panelLayout.letGo() {
            keepLayout()
        }
        layoutChanged()
    }

    /// `piece` folded to its tab, or unfolded.
    func foldPiece(_ piece: LayoutPiece, _ folded: Bool) {
        panelLayout.setFolded(piece: piece, folded: folded)
        keepLayout()
        layoutChanged()
    }

    /// `panel`'s window closed by its Close: where it was and its size kept for when it floats again.
    func closeFloating(_ panel: FloatingPanel) {
        panelLayout.close(piece: piece(panel))
        keepLayout()
        layoutChanged()
    }

    /// Whether the plugins let `piece` float now.
    func mayFloat(_ piece: LayoutPiece) -> Bool {
        _ = layoutChanges
        return panelLayout.mayFloat(piece: piece)
    }

    /// `piece` (not the Layers panel or the library) dragged out by its grabber past its start, the
    /// finger `at` (interface points): its window follows it, its strip's middle `grab` down.
    func undockPiece(_ piece: LayoutPiece, at: CGPoint, grab: CGFloat) {
        if floating.undockingPiece != piece {
            floating.undockingPiece = piece
            _ = panelLayout.openAt(piece: piece, x: Float(at.x), y: Float(at.y), grab: Float(grab))
        } else {
            _ = panelLayout.drag(x: Float(at.x), y: Float(at.y))
        }
        layoutChanged()
    }

    /// `piece`'s grabber let go: it floats where it is, kept; a panel that hung from its button closes.
    func undockedPiece(_ piece: LayoutPiece) {
        guard floating.undockingPiece == piece else { return }
        floating.undockingPiece = nil
        if panelLayout.letGo() {
            keepLayout()
        }
        layoutChanged()
        if piece == .actions || piece == .adjustments {
            closePanels()
        }
    }

    /// `piece`'s window closed: a panel goes back to its button, a toolbar or an adjustment's
    /// controls to where they always are.
    func closePiece(_ piece: LayoutPiece) {
        panelLayout.close(piece: piece)
        keepLayout()
        layoutChanged()
    }

    /// `piece`'s window hidden (from its menu), or shown again.
    func hidePiece(_ piece: LayoutPiece, _ hidden: Bool = true) {
        guard panelLayout.isHidden(piece: piece) != hidden else { return }
        panelLayout.setHidden(piece: piece, hidden: hidden)
        keepLayout()
        layoutChanged()
    }

    /// Every hidden window shown again.
    func showHiddenPieces() {
        panelLayout.showHidden()
        keepLayout()
        layoutChanged()
    }

    /// The windows hidden now.
    func hiddenPieces() -> [LayoutPiece] {
        _ = layoutChanges
        return panelLayout.hiddenPieces()
    }

    /// `piece`'s window pinned over the others, or not.
    func pinPiece(_ piece: LayoutPiece, _ pinned: Bool) {
        panelLayout.setPinned(piece: piece, pinned: pinned)
        keepLayout()
        layoutChanged()
    }

    /// Whether `piece`'s window is pinned over the others.
    func pinnedPiece(_ piece: LayoutPiece) -> Bool {
        _ = layoutChanges
        return panelLayout.isPinned(piece: piece)
    }

    /// `piece`'s window back where it first goes.
    func resetPiece(_ piece: LayoutPiece) {
        _ = panelLayout.resetPlace(piece: piece)
        keepLayout()
        layoutChanged()
    }

    /// The panels `piece`'s window holds as tabs, the first holding its place; none while alone.
    func tabsOf(_ piece: LayoutPiece) -> [LayoutPiece] {
        _ = layoutChanges
        return panelLayout.tabsOf(piece: piece)
    }

    /// `piece`, a tab of its window, shown there.
    func pickTab(_ piece: LayoutPiece) {
        panelLayout.setActiveTab(piece: piece)
        keepLayout()
        layoutChanged()
    }

    /// `piece`, a tab of its window, torn out with the finger `at` (interface points): it follows
    /// the finger.
    func tearTab(_ piece: LayoutPiece, at: CGPoint) {
        _ = panelLayout.tearTab(piece: piece, x: Float(at.x), y: Float(at.y), grab: Float(floatingGrabY))
        layoutChanged()
    }

    /// The tab torn out follows the finger `at` (interface points).
    func followTorn(_ at: CGPoint) {
        _ = panelLayout.drag(x: Float(at.x), y: Float(at.y))
        layoutChanged()
    }

    /// The Tool Belt as it shows, while a plugin says so.
    func beltShown() -> BeltShown? {
        _ = layoutChanges
        return panelLayout.beltShown()
    }

    /// Where the Tool Belt shows (interface points): moved where Edit layout left it (a bar), else
    /// where `belt` says; told to the layout, which parks what is let go on it.
    func beltPlace(_ belt: BeltShown) -> BarPlace {
        let k = belt.scale
        let (length, thickness) = (belt.width * k, belt.height * k)
        let own = BarPlace(x: belt.x, y: belt.y, vertical: false)
        barLayout.setBar(bar: .belt, own: own, length: length, thickness: thickness)
        _ = barChanges
        let place = barLayout.shown(bar: .belt) ?? own
        let (w, h) = place.vertical ? (thickness, length) : (length, thickness)
        panelLayout.setBeltPlace(rect: LayoutRect(x: place.x, y: place.y, width: w, height: h))
        return place
    }

    /// `piece`, parked on the Tool Belt, taken off: back where it was.
    func unparkPiece(_ piece: LayoutPiece) {
        _ = panelLayout.unpark(piece: piece)
        keepLayout()
        layoutChanged()
    }

    /// `piece`, parked on the Tool Belt, dragged off with the finger `at` (interface points): it
    /// follows the finger.
    func unparkAt(_ piece: LayoutPiece, at: CGPoint) {
        _ = panelLayout.unparkAt(piece: piece, x: Float(at.x), y: Float(at.y), grab: Float(floatingGrabY))
        layoutChanged()
    }

    /// The floating windows in the order they are drawn, from the bottom up.
    func pieceOrder() -> [LayoutPiece] {
        _ = layoutChanges
        return panelLayout.order()
    }

    /// How big `piece` is as it is laid out now (its own points): for one that isn't sized. The same
    /// size again tells nothing.
    func tellPieceSize(_ piece: LayoutPiece, width: CGFloat, height: CGFloat) {
        let now = CGSize(width: width, height: height)
        guard floating.sizesTold[piece] != now else { return }
        floating.sizesTold[piece] = now
        panelLayout.setSize(piece: piece, width: Float(width), height: Float(height))
        layoutChanged()
    }

    /// The layout's piece for a panel that may float from its button, if it is one.
    func floatingPiece(_ p: Panel) -> LayoutPiece? {
        switch p {
        case .layers: .layers
        case .brushes: .brushes
        case .actions: .actions
        case .adjustments: .adjustments
        default: nil
        }
    }

    /// `piece` as it shows now (interface points), while it does.
    func shownPiece(_ piece: LayoutPiece) -> PieceShown? {
        _ = layoutChanges
        return panelLayout.shown(piece: piece)
    }

    /// The floating colour panel as it shows now, `width` by `height` as its mode has it.
    func shownColours(width: CGFloat, height: CGFloat) -> PieceShown? {
        panelLayout.setColoursSize(width: Float(width), height: Float(height))
        return shownPiece(.colours)
    }

    /// The colour panel's grabber dragged out to `at` (interface points), the window's grabber
    /// `grab` under it.
    func undockColours(at: CGPoint, grab: CGPoint) {
        guard layoutLook.on else {
            floatingColorsAt = CGPoint(x: at.x - grab.x, y: at.y - grab.y)
            return
        }
        if panelLayout.holding() != .colours {
            _ = panelLayout.openAt(piece: .colours, x: Float(at.x), y: Float(at.y), grab: Float(grab.y))
        } else {
            _ = panelLayout.drag(x: Float(at.x), y: Float(at.y))
        }
        layoutChanged()
    }

    /// The floating colour panel dragged by its grabber, `by` all told since it was pressed
    /// (interface points); `from` where it was then, for the window as the app has it.
    func moveColours(by: CGSize, from: CGPoint) {
        guard layoutLook.on else {
            floatingColorsAt = CGPoint(x: from.x + by.width, y: from.y + by.height)
            return
        }
        if panelLayout.holding() != .colours {
            _ = panelLayout.hold(piece: .colours, sizing: false, x: 0, y: 0)
        }
        _ = panelLayout.drag(x: Float(by.width), y: Float(by.height))
        layoutChanged()
    }

    /// The colour panel's grabber let go: kept where it is.
    func letGoColours() {
        if layoutLook.on, panelLayout.letGo() {
            keepLayout()
        }
        layoutChanged()
    }

    /// The floating colour panel folded to its tab, or unfolded.
    func foldColours(_ folded: Bool) {
        panelLayout.setFolded(piece: .colours, folded: folded)
        keepLayout()
        layoutChanged()
    }

    /// The floating colour panel closed: kept closed where it was, while the layout keeps it.
    func closeColours() {
        floatingColorsAt = nil
        if layoutLook.on {
            panelLayout.close(piece: .colours)
            keepLayout()
            layoutChanged()
        }
    }
}

/// Fold: a disc with a bar across it, the panel going to its tab.
struct FloatingFold: View {
    let onFold: () -> Void
    var height: CGFloat = 40

    var body: some View {
        Canvas { context, size in
            let o = CGPoint(x: size.width / 2, y: size.height / 2)
            let r: CGFloat = 8.5
            context.fill(Path(ellipseIn: CGRect(x: o.x - r, y: o.y - r, width: r * 2, height: r * 2)), with: .color(.white.opacity(0.15)))
            var bar = Path()
            bar.move(to: CGPoint(x: o.x - 3.6, y: o.y))
            bar.addLine(to: CGPoint(x: o.x + 3.6, y: o.y))
            context.stroke(bar, with: .color(.black.opacity(0.3)), lineWidth: 1.3)
        }
        .frame(width: 40, height: height)
        .contentShape(Rectangle())
        .onTapGesture(perform: onFold)
        .accessibilityLabel("Fold")
    }
}

/// A floating panel folded to its tab: its name and an unfold mark, where `shown` says. A tap
/// unfolds it; dragged, it moves as the panel would, snapping as it does.
struct FloatingTab: View {
    let shown: PieceShown
    let title: String
    let onUnfold: () -> Void
    /// Dragged: pressed at a point, and moved all told since (interface points).
    let onDrag: (CGPoint, CGSize) -> Void
    let onLetGo: () -> Void

    var body: some View {
        Popover(width: CGFloat(shown.width), arrowX: 0, side: .none) {
            HStack(spacing: 0) {
                Text(verbatim: title)
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(Pc.panelTitle)
                    .lineLimit(1)
                Spacer(minLength: 0)
                // the unfold mark: a chevron opening downward
                Canvas { context, size in
                    var mark = Path()
                    mark.move(to: CGPoint(x: 0, y: size.height * 0.3))
                    mark.addLine(to: CGPoint(x: size.width / 2, y: size.height * 0.72))
                    mark.addLine(to: CGPoint(x: size.width, y: size.height * 0.3))
                    context.stroke(mark, with: .color(Pc.panelTitle), style: StrokeStyle(lineWidth: 1.6, lineCap: .round, lineJoin: .round))
                }
                .frame(width: 14, height: 14)
            }
            .padding(.horizontal, 14)
            .frame(height: CGFloat(shown.height))
        }
        .frame(height: CGFloat(shown.height))
        .contentShape(Rectangle())
        .onTapGesture(perform: onUnfold)
        .gesture(
            DragGesture(minimumDistance: 1, coordinateSpace: .global)
                .onChanged { drag in
                    let k = Interface.shared.scale
                    let from = Interface.shared.point(drag.startLocation)
                    onDrag(from, CGSize(width: drag.translation.width / k, height: drag.translation.height / k))
                }
                .onEnded { _ in onLetGo() }
        )
        .accessibilityLabel("\(title), folded")
        // at the floating windows' size
        .scaleEffect(CGFloat(shown.scale), anchor: .topLeading)
        .offset(x: CGFloat(shown.x), y: CGFloat(shown.y))
    }
}

/// The room at either end of half of the top bar moved by Edit layout, past its first and last buttons.
let topBarHalfEnd: CGFloat = 10

/// How long half of the top bar is (`left`, else the right), laid in a row with the bar's `gaps`.
func topBarHalfLength(left: Bool, gaps: (tool: CGFloat, action: CGFloat)) -> CGFloat {
    left
        ? topBarHalfEnd * 2 + Pc.galleryWidth + (Pc.barButtonWidth + gaps.action) * 4
        : topBarHalfEnd * 2 + Pc.barButtonWidth * 5 + gaps.tool * 4
}

/// Edit layout's handles: an outline round each bar that drags it, snapping to the screen's edges and
/// the other bars, a bar against a side standing upright; Reset and Done at the bottom.
struct EditLayoutHandles: View {
    let model: CanvasModel
    let screen: CGSize
    /// A name being asked for: a new layout's (`renaming` none), or one renamed.
    @State private var asking = false
    @State private var renaming: String?
    @State private var name = ""

    /// The named layouts: each chosen, renamed or deleted from its own menu, and Save Layout.
    private var layouts: some View {
        Menu {
            let chosen = model.chosenLayout()
            ForEach(model.layoutNames(), id: \.self) { layout in
                Menu(layout == chosen ? "\u{2713} \(layout)" : layout) {
                    Button("Choose") { model.chooseLayout(layout) }
                    Button("Rename") {
                        renaming = layout
                        name = layout
                        asking = true
                    }
                    Button("Delete", role: .destructive) { model.deleteLayout(layout) }
                }
            }
            Button("Save Layout") {
                renaming = nil
                name = model.namedLayouts.nextName()
                asking = true
            }
        } label: {
            Text(verbatim: "Layouts")
                .font(.system(size: 17))
                .foregroundStyle(Pc.accent)
                .padding(.horizontal, 16)
                .frame(height: 44)
        }
    }

    var body: some View {
        let _ = model.barChanges
        ZStack(alignment: .topLeading) {
            // (the Tool Belt's while it shows)
            ForEach([LayoutBar.topLeft, .topRight, .sideBar] + (model.beltShown() == nil ? [] : [.belt]), id: \.self) { bar in
                BarOutline(model: model, bar: bar)
            }
            HStack(spacing: 0) {
                layouts
                // the windows hidden from their menus come back by Show Hidden
                let hidden: [(String, () -> Void)] = model.hiddenPieces().isEmpty ? [] : [("Show Hidden", { model.showHiddenPieces() })]
                // Collapse to edge: every bar and window against a side into it, or back out
                let any = model.anyCollapsed()
                let collapse: [(String, () -> Void)] =
                    model.layoutLook.collapse ? [(any ? "Open All" : "Collapse All", { model.collapseAll(!any) })] : []
                ForEach(hidden + collapse + [("Reset", { model.resetLayout() }), ("Done", { model.doneEditLayout() })], id: \.0) { label, act in
                    Text(verbatim: label)
                        .font(.system(size: 17))
                        .foregroundStyle(Pc.accent)
                        .padding(.horizontal, 16)
                        .frame(height: 44)
                        .contentShape(Rectangle())
                        .onTapGesture(perform: act)
                }
            }
            .padding(.horizontal, 8)
            .background(Capsule().fill(Pc.popoverFill))
            .frame(width: screen.width, height: screen.height - 28, alignment: .bottom)
        }
        .frame(width: screen.width, height: screen.height, alignment: .topLeading)
        // the name asked for in the app's own alert
        .overlay {
            AlertView(
                content: asking
                    ? AlertContent(title: renaming == nil ? "Save Layout" : "Rename Layout", action: renaming == nil ? "Save" : "Rename", destructive: false)
                    : nil,
                onCancel: { asking = false },
                onAction: {
                    if let from = renaming {
                        model.renameLayout(from, name)
                    } else {
                        model.saveLayout(name)
                    }
                    asking = false
                },
                field: $name
            )
        }
    }
}

/// A bar's outline while Edit layout is on, where the bar is now (it follows each step of a drag,
/// and Reset): dragged, it moves the bar.
private struct BarOutline: View {
    let model: CanvasModel
    let bar: LayoutBar

    var body: some View {
        // (read here, so each outline shows again as its bar moves)
        let _ = model.barChanges
        if let r = model.barLayout.rect(bar: bar) {
            RoundedRectangle(cornerRadius: 12, style: .circular)
                .strokeBorder(Pc.accent, lineWidth: 2)
                .contentShape(Rectangle())
                .frame(width: CGFloat(r.width), height: CGFloat(r.height))
                .gesture(
                    DragGesture(minimumDistance: 1, coordinateSpace: .global)
                        .onChanged { drag in
                            let k = Interface.shared.scale
                            // the finger's place: where the bar turns and goes decides by it
                            let from = Interface.shared.point(drag.startLocation)
                            model.dragBar(bar, from: from, by: CGSize(width: drag.translation.width / k, height: drag.translation.height / k))
                        }
                        .onEnded { _ in model.letGoBar() }
                )
                .onTapGesture {}
                .accessibilityLabel("Move")
                .offset(x: CGFloat(r.x), y: CGFloat(r.y))
        }
    }
}
