import SwiftUI

private let sliderLeft: CGFloat = 5
private let sizeTrackTop: CGFloat = 6
private let opacityTrackTop: CGFloat = 248

/// A button lights up fast and goes back more slowly.
private func highlight(_ on: Bool) -> Animation {
    .linear(duration: on ? 0.067 : 0.15)
}

/// The bar across the top of the canvas: artwork actions on the left, painting tools on the right.
struct TopBar: View {
    let model: CanvasModel

    var body: some View {
        GeometryReader { bar in
            let gaps = Pc.barGaps(bar.size.width)
            ZStack {
                HStack(spacing: gaps.action) {
                    ActionButtons(model: model, upright: false)
                }
                .padding(.leading, Pc.barMargin)
                .frame(maxWidth: .infinity, alignment: .leading)
                HStack(spacing: gaps.tool) {
                    ToolButtons(model: model, upright: false)
                }
                .padding(.trailing, Pc.barMargin)
                .frame(maxWidth: .infinity, alignment: .trailing)
            }
            .frame(height: Pc.barHeight)
        }
        .frame(height: Pc.barHeight)
        // frosted: what the canvas has under it shows blurred
        .background {
            ZStack {
                // the interface's frost (dark unless Light interface) whatever the system's
                // appearance: a light one shows through the tint lighter than measured
                Rectangle().fill(.ultraThinMaterial)
                    .environment(\.colorScheme, Look.shared.light ? .light : .dark)
                Pc.bar
            }
        }
        .modifier(ScaledAcross(k: Look.shared.scale(.topBar), height: Pc.barHeight))
    }
}

/// Gallery and the actions: the top bar's left half, each button turned to stand in a column while
/// `upright`.
private struct ActionButtons: View {
    let model: CanvasModel
    let upright: Bool

    var body: some View {
        Text("Gallery")
            .font(.system(size: 18, weight: .medium))
            .foregroundStyle(Pc.barText)
            .lineLimit(1)
            .fixedSize()
            .offset(y: upright ? 0 : -1)
            .frame(width: Pc.galleryWidth, height: Pc.barButtonHeight, alignment: upright ? .center : .leading)
            // upright, the word reads up the column
            .rotationEffect(.degrees(upright ? -90 : 0))
            .frame(width: upright ? Pc.barButtonHeight : Pc.galleryWidth, height: upright ? Pc.galleryWidth : Pc.barButtonHeight)
            .contentShape(Rectangle())
            // (the model held weakly: the touch that leaves the artwork, with what its gesture's
            // action holds, may be kept by the system until the next touch, and with the model the
            // artwork's whole session would stay)
            .onTapGesture { [weak model] in model?.onClose() }
        ActionButton(icon: PcIcons.actions, selected: model.panel == .actions, upright: upright) { model.toggle(.actions) }
        ActionButton(icon: PcIcons.adjustments, selected: model.panel == .adjustments || model.adjusting || model.recoloring, upright: upright) {
            model.toggle(.adjustments)
        }
        ActionButton(icon: PcIcons.selection, selected: model.selecting, glyph: 33, upright: upright) {
            model.toggleSelecting()
        }
        ActionButton(icon: PcIcons.transform, selected: model.transforming, glyph: 44, upright: upright) {
            model.toggleTransform()
        }
    }
}

/// The tools, Layers and the colour: the top bar's right half, standing in a column while `upright`.
private struct ToolButtons: View {
    let model: CanvasModel
    let upright: Bool

    var body: some View {
        // the layers and colour panels take the highlight from the current tool while open
        let panelOpen = model.panel == .layers || model.panel == .color || model.transforming
            || model.selecting || model.adjusting
        ToolButton(icon: PcIcons.brush, selected: !panelOpen && model.tool == .paint, upright: upright, onHold: { model.toolHeld(.paint) }) {
            model.toolPressed(.paint)
        }
        ToolButton(icon: PcIcons.smudge, selected: !panelOpen && model.tool == .smudge, upright: upright, onHold: { model.toolHeld(.smudge) }) {
            model.toolPressed(.smudge)
        }
        ToolButton(icon: PcIcons.erase, selected: !panelOpen && model.tool == .erase, upright: upright, onHold: { model.toolHeld(.erase) }) {
            model.toolPressed(.erase)
        }
        ToolButton(icon: PcIcons.layers, selected: model.panel == .layers, upright: upright) { model.toggle(.layers) }
        ColorButton(model: model, upright: upright)
    }
}

/// Half of the top bar (`left`, else the right) where Edit layout moved it: on glass of its own, its
/// buttons in a row, or in a column while `upright` against a side; as long either way, laid out at
/// its own points and shown as big as the top bar shows.
struct TopBarHalf: View {
    let model: CanvasModel
    let left: Bool
    let upright: Bool
    let screenWidth: CGFloat
    /// Its sides against the window's edges, square; the rest round.
    var edges: BarSides? = nil

    var body: some View {
        let k = Look.shared.scale(.topBar)
        let gaps = Pc.barGaps(screenWidth / k)
        let length = topBarHalfLength(left: left, gaps: gaps)
        let gap = left ? gaps.action : gaps.tool
        let w = upright ? Pc.barHeight : length
        let h = upright ? length : Pc.barHeight
        Group {
            if upright {
                VStack(spacing: gap) {
                    if left { ActionButtons(model: model, upright: true) } else { ToolButtons(model: model, upright: true) }
                }
            } else {
                HStack(spacing: gap) {
                    if left { ActionButtons(model: model, upright: false) } else { ToolButtons(model: model, upright: false) }
                }
            }
        }
        .frame(width: w, height: h)
        .background {
            ZStack {
                Rectangle().fill(.ultraThinMaterial)
                    .environment(\.colorScheme, Look.shared.light ? .light : .dark)
                Pc.bar
            }
            .clipShape(UnevenRoundedRectangle(cornerRadii: barCorners(edges, radius: Pc.barHeight / 2), style: .circular))
            .animation(barCornerEase, value: edges)
        }
        .scaleEffect(k, anchor: .topLeading)
        .frame(width: w * k, height: h * k, alignment: .topLeading)
    }
}

/// A bar laid out at its own points `height` tall across the room it is given less its size, and
/// shown `k` times as big from its top left (a plugin sizing that part of the interface): it takes
/// the room it shows in.
private struct ScaledAcross: ViewModifier {
    let k: CGFloat
    let height: CGFloat

    @ViewBuilder
    func body(content: Content) -> some View {
        if k == 1 {
            content
        } else {
            GeometryReader { room in
                content
                    .frame(width: room.size.width / k, height: height)
                    .scaleEffect(k, anchor: .topLeading)
            }
            .frame(height: height * k)
        }
    }
}

/// An action button: its glyph on a dark disc, which turns blue while its panel is open.
private struct ActionButton: View {
    let icon: PcIcon
    let selected: Bool
    var glyph: CGFloat = 36
    var upright = false
    let action: () -> Void

    var body: some View {
        ZStack {
            Circle()
                .fill(selected ? Pc.barGlyphSelected : Pc.actionDisc)
                .frame(width: Pc.actionDiscSize, height: Pc.actionDiscSize)
            // the glyphs are drawn in a 44 frame, shown `glyph` across (most 36)
            PcIconView(icon, size: glyph, color: selected ? .white : Pc.actionGlyph)
        }
        .animation(highlight(selected), value: selected)
        .frame(width: upright ? Pc.barButtonHeight : Pc.barButtonWidth, height: upright ? Pc.barButtonWidth : Pc.barButtonHeight)
        .contentShape(Rectangle())
        .onTapGesture(perform: action)
    }
}

/// A tool button: tapped, the tool (or its brushes); held (`onHold`), it takes the current brush.
private struct ToolButton: View {
    let icon: PcIcon
    let selected: Bool
    var upright = false
    var onHold: (() -> Void)?
    let action: () -> Void

    var body: some View {
        // a 44 frame shown as it is, over the 38 tall button
        let button = PcIconView(icon, size: 44, color: selected ? Pc.barGlyphSelected : Pc.barGlyph)
            .animation(highlight(selected), value: selected)
            .frame(width: upright ? Pc.barButtonHeight : Pc.barButtonWidth, height: upright ? Pc.barButtonWidth : Pc.barButtonHeight)
            .contentShape(Rectangle())
            .onTapGesture(perform: action)
        if let onHold {
            button.onLongPressGesture(perform: onHold)
        } else {
            button
        }
    }
}

/// The colour button: the colour painted with, ringed only where it would fade into the bar.
private struct ColorButton: View {
    let model: CanvasModel
    var upright = false

    /// The brightness (HSB) under which it is ringed on the dark bar.
    private static let darkRingBelow: Float = 0.35
    /// The brightness over which it is ringed on the light bar.
    private static let lightRingAbove: Float = 0.85

    var body: some View {
        let ringed = Look.shared.light ? model.brightness > Self.lightRingAbove : model.brightness < Self.darkRingBelow
        Circle()
            .fill(CanvasSpace.shared.color(model.colorRgb()))
            .overlay {
                if ringed {
                    Circle().strokeBorder(Pc.colorRing, lineWidth: 1)
                }
            }
            .frame(width: Pc.colorDot, height: Pc.colorDot)
            .frame(width: upright ? Pc.barButtonHeight : Pc.barButtonWidth, height: upright ? Pc.barButtonWidth : Pc.barButtonHeight)
            // held, the colour painted with before; dragged out onto the canvas, it fills there
            .overlay {
                DropSource(model: model, onHold: { model.previousColor() }, onTap: { model.toggle(.color) })
            }
    }
}

/// The side bar against the left edge (the right one, `right`-handed): brush size, the modify
/// button, opacity, undo and redo. On the right everything is mirrored but the glyphs. Its modify
/// button dragged out moves it along its edge in a window `windowHeight` tall; stretched `stretch`
/// toward the finger as it goes, it is placed that far out by its caller and its glass reaches
/// back as far, its edge side staying against the window's edge.
struct SideBar: View {
    let model: CanvasModel
    var right = false
    var windowHeight: CGFloat = 0
    var stretch: CGFloat = 0
    /// Moved by Edit layout: its sides against the window's edges (its own way up), square; the rest
    /// round. None: against its edge as it always is.
    var edges: BarSides? = nil

    /// A control's left edge, mirrored on the right.
    private func x(_ left: CGFloat, _ width: CGFloat) -> CGFloat {
        right ? Pc.sideBarWidth - left - width : left
    }

    var body: some View {
        let fill = Pc.sideBar
        let r = Pc.sideBarCorner
        let corners = edges.map { barCorners($0, radius: r) } ?? (right
            ? RectangleCornerRadii(topLeading: r, bottomLeading: r, bottomTrailing: 0, topTrailing: 0)
            : RectangleCornerRadii(topLeading: 0, bottomLeading: 0, bottomTrailing: r, topTrailing: r))
        // its glass reaches back to the window's edge however far it is stretched
        let reach = max(stretch, 0)
        let back: CGFloat = right ? 0 : -reach
        // the slider tracks' windows in the glass, where it is laid (from `back`)
        let windows = [sizeTrackTop, opacityTrackTop].map {
            CGRect(x: x(sliderLeft, Pc.sliderWidth) - back, y: $0, width: Pc.sliderWidth, height: Pc.sliderHeight)
        }
        ZStack(alignment: .topLeading) {
            // frosted, as the top bar is; the tracks show what is behind them barely softened (not
            // frosted), unless a plugin has them frosted as the bar is
            SideBarGlass(corners: corners, windows: Look.shared.tracksFrosted ? [] : windows)
                .fill(.ultraThinMaterial, style: FillStyle(eoFill: true))
                .environment(\.colorScheme, Look.shared.light ? .light : .dark)
                .frame(width: Pc.sideBarWidth + reach, height: Pc.sideBarHeight)
                .offset(x: back)
                .animation(barCornerEase, value: edges)
                .allowsHitTesting(false)
            // translucent, with windows where the slider tracks are: they show what is behind,
            // each with a darker hairline round it
            SideBarGlass(corners: corners, windows: windows)
                .fill(fill, style: FillStyle(eoFill: true))
                .frame(width: Pc.sideBarWidth + reach, height: Pc.sideBarHeight)
                .offset(x: back)
                .animation(barCornerEase, value: edges)
                .allowsHitTesting(false)
            Canvas { context, size in
                let edge = Pc.sliderEdgeWidth
                for w in windows {
                    let r = Pc.sliderCorner
                    var ring = Path()
                    ring.addRoundedRect(in: w.insetBy(dx: -edge, dy: -edge), cornerSize: CGSize(width: r + edge, height: r + edge), style: .circular)
                    ring.addRoundedRect(in: w, cornerSize: CGSize(width: r, height: r), style: .circular)
                    // darker down the sides than across the ends
                    let shade = Pc.sliderEnds(Pc.sliderEdgeEnd, Pc.sliderEdgeSide, height: w.height)
                    context.fill(
                        ring, with: .linearGradient(shade, startPoint: CGPoint(x: 0, y: w.minY), endPoint: CGPoint(x: 0, y: w.maxY)),
                        style: FillStyle(eoFill: true)
                    )
                }
            }
            .frame(width: Pc.sideBarWidth + reach, height: Pc.sideBarHeight)
            .offset(x: back)
            .allowsHitTesting(false)

            BrushSlider(
                value: model.size,
                notches: model.sizeNotches,
                panel: hooks(.size),
                onChange: model.changeSize
            )
            .offset(x: x(sliderLeft, Pc.sliderWidth), y: sizeTrackTop)

            ModifyButton(model: model, windowHeight: windowHeight)
                .offset(x: x(5, 40), y: 191)

            BrushSlider(
                value: model.opacity,
                notches: model.opacityNotches,
                panel: hooks(.opacity),
                onChange: model.changeOpacity
            )
            .offset(x: x(sliderLeft, Pc.sliderWidth), y: opacityTrackTop)

            SideButton(icon: PcIcons.undo, enabled: model.canUndo) { model.undo() }
                .offset(x: x(5, 40), y: 430.5)
            SideButton(icon: PcIcons.redo, enabled: model.canRedo) { model.redo() }
                .offset(x: x(5, 40), y: 472.5)

            // beside the slider it belongs to, a little above it, outside the bar
            if let which = model.sliderPanel {
                let top = which == .size ? sizeTrackTop : opacityTrackTop
                SliderPanelView(model: model, which: which)
                    .offset(x: x(sliderLeft + Pc.sliderWidth + sliderPanelGap, sliderPanelSide), y: top - sliderPanelRise)
            }
        }
        .frame(width: Pc.sideBarWidth, height: Pc.sideBarHeight, alignment: .topLeading)
        .modifier(ScaledSideBar(k: Look.shared.scale(.sideBar), right: right))
    }

    private func hooks(_ which: SliderPanel) -> SliderPanelHooks {
        SliderPanelHooks(
            show: { model.sliderPanel = which },
            hide: { model.sliderPanel = nil },
            up: { model.sliderPanel != nil }
        )
    }
}

/// The side bar shown `k` times as big from its top corner against the screen's edge (`right`),
/// as a plugin sizes it: it takes the room it shows in.
private struct ScaledSideBar: ViewModifier {
    let k: CGFloat
    let right: Bool

    func body(content: Content) -> some View {
        content
            .scaleEffect(k, anchor: right ? .topTrailing : .topLeading)
            .frame(width: Pc.sideBarWidth * k, height: Pc.sideBarHeight * k, alignment: right ? .topTrailing : .topLeading)
    }
}

/// The side bar's glass: its body, square against the screen's edge and round on the far side
/// (`corners`), with `windows` cut out of it (the slider tracks).
private struct SideBarGlass: Shape {
    var corners: RectangleCornerRadii
    let windows: [CGRect]

    // (its corners ease between square and round as a moved bar comes to an edge or leaves it)
    var animatableData: RectangleCornerRadii.AnimatableData {
        get { corners.animatableData }
        set { corners.animatableData = newValue }
    }

    func path(in rect: CGRect) -> Path {
        var shape = Path()
        shape.addRoundedRect(in: rect, cornerRadii: corners, style: .circular)
        for w in windows {
            shape.addRoundedRect(in: w, cornerSize: CGSize(width: Pc.sliderCorner, height: Pc.sliderCorner), style: .circular)
        }
        return shape
    }
}

/// A bar's corners as Edit layout leaves it: `radius` round all round, but square on each side of it
/// against the window's edges (`sides`, the bar's own way up; none: free).
func barCorners(_ sides: BarSides?, radius r: CGFloat) -> RectangleCornerRadii {
    guard let s = sides else {
        return RectangleCornerRadii(topLeading: r, bottomLeading: r, bottomTrailing: r, topTrailing: r)
    }
    return RectangleCornerRadii(
        topLeading: s.left || s.top ? 0 : r,
        bottomLeading: s.bottom || s.left ? 0 : r,
        bottomTrailing: s.right || s.bottom ? 0 : r,
        topTrailing: s.top || s.right ? 0 : r
    )
}

/// `sides` of a bar as the window has them, as the bar's own: for one drawn lying down by turning it
/// a quarter back about its top-left (its own left lies at the window's bottom, its top at the left,
/// and so on round).
func turnedSides(_ s: BarSides) -> BarSides {
    BarSides(left: s.bottom, top: s.left, right: s.top, bottom: s.right)
}

/// How a moved bar's corners turn square or round as it comes to an edge or leaves it, so none snaps.
let barCornerEase = Animation.easeOut(duration: 0.15)

/// One touch on the modify button: the core's drag, which says when it moves the side bar instead;
/// whether the button is still pressed; and where the button stood on the window as it began.
private final class ModifyTouch {
    let drag: SideBarDrag
    let button: CGRect
    var pressing = true

    init(drag: SideBarDrag, button: CGRect) {
        self.drag = drag
        self.button = button
    }
}

/// The modify button: held, it changes what a canvas touch does; tapped, it does what Tap □ is
/// set to (at first: the next touch on the canvas picks a colour). Dragged out from the bar's edge
/// and then up or down, it moves the side bar along its edge (in a window `windowHeight` tall)
/// instead, the press let go untapped; slid off without moving the bar, the press is let go
/// untapped too.
private struct ModifyButton: View {
    let model: CanvasModel
    let windowHeight: CGFloat
    @State private var touch: ModifyTouch?
    /// A touch is down on it: gone as it lifts, or as the system takes it back (no end comes then).
    @GestureState private var pressing = false

    /// A drag's time as the core's drag takes it (ms).
    private static func ms(_ drag: DragGesture.Value) -> Double {
        drag.time.timeIntervalSinceReferenceDate * 1000
    }

    /// The touch at `drag` (on the window), in the bar's own points (a plugin may size it).
    private static func at(_ location: CGPoint) -> (x: Float, y: Float) {
        let p = Interface.shared.point(location)
        let k = Look.shared.scale(.sideBar)
        return (Float(p.x / k), Float(p.y / k))
    }

    /// The touch moved: the bar moves if the core says so; moving it, or off the button, the press
    /// is let go untapped.
    private func moved(_ t: ModifyTouch, _ drag: DragGesture.Value) {
        let p = Self.at(drag.location)
        let shown = t.drag.moved(x: p.x, y: p.y, ms: Self.ms(drag))
        model.sideBarDragged(shown)
        if t.pressing, shown.state == .moving || !t.button.contains(drag.location) {
            t.pressing = false
            model.modifyReleased(cancelled: true)
        }
    }

    /// The touch is over (`cancelled`: taken back, or let go off the button).
    private func letGo(_ t: ModifyTouch, cancelled: Bool) {
        touch = nil
        model.sideBarLetGo(t.drag.letGo())
        if t.pressing {
            t.pressing = false
            model.modifyReleased(cancelled: cancelled)
        }
    }

    var body: some View {
        GeometryReader { place in
            RoundedRectangle(cornerRadius: Pc.modifyCorner - Pc.modifyStroke / 2, style: .circular)
                .stroke(model.pickNext ? Pc.accent : Pc.sideGlyph, lineWidth: Pc.modifyStroke)
                .padding(Pc.modifyStroke / 2)
                .frame(width: Pc.modifySize, height: Pc.modifySize)
                .frame(width: 40, height: 41)
                .contentShape(Rectangle())
                // on the window: the bar may move under the finger
                .gesture(
                    DragGesture(minimumDistance: 0, coordinateSpace: .global)
                        .updating($pressing) { _, down, _ in down = true }
                        .onChanged { drag in
                            if let t = touch {
                                moved(t, drag)
                                return
                            }
                            model.modifyPressed()
                            let start = Self.at(drag.startLocation)
                            let t = ModifyTouch(
                                drag: model.sideBarDrag(x: start.x, y: start.y, ms: Self.ms(drag), windowHeight: windowHeight),
                                button: place.frame(in: .global)
                            )
                            touch = t
                            if drag.location != drag.startLocation {
                                moved(t, drag)
                            }
                        }
                        .onEnded { drag in
                            guard let t = touch else { return }
                            letGo(t, cancelled: !t.button.contains(drag.location))
                        }
                )
        }
        .frame(width: 40, height: 41)
        // taken back by the system, no end comes: let go of it all the same (after an end that
        // may yet come)
        .onChange(of: pressing) { _, down in
            guard !down, let t = touch else { return }
            DispatchQueue.main.async {
                if touch === t {
                    letGo(t, cancelled: true)
                }
            }
        }
    }
}

extension CanvasModel {
    /// A touch on the modify button at (`x`, `y`) at `ms` (the side bar's points, on the window) in
    /// a window `windowHeight` tall: the core's drag, which says when it moves the bar.
    func sideBarDrag(x: Float, y: Float, ms: Double, windowHeight: CGFloat) -> SideBarDrag {
        // (in the bar's own points, as a plugin may size it)
        let k = Look.shared.scale(.sideBar)
        return SideBarDrag(
            x: x, y: y, ms: ms, right: AppSettings.shared.rightHanded, height: Float(windowHeight / k),
            topBar: Float(Pc.barShown / k), centre: sideBarCentre
        )
    }

    /// Where the side bar goes down a window `height` tall, at its kept place: on the window, as
    /// big as it shows.
    func sideBarPlace(height: CGFloat) -> SideBarFit {
        let k = Float(Look.shared.scale(.sideBar))
        let fit = sideBarFit(height: Float(height) / k, topBar: Float(Pc.barShown) / k, centre: sideBarCentre)
        return SideBarFit(top: fit.top * k, track: fit.track * k, highest: fit.highest * k, lowest: fit.lowest * k)
    }

    /// The modify button's drag moved: the bar follows while it moves it (on the window, as big as
    /// it shows).
    func sideBarDragged(_ shown: SideBarShown) {
        if shown.state == .moving {
            let k = Float(Look.shared.scale(.sideBar))
            sideBarMoving = SideBarShown(state: shown.state, top: shown.top * k, stretch: shown.stretch * k, centre: shown.centre)
        }
    }

    /// The modify button let go: a bar it moved settles back to its width, inside its room, and
    /// its place is kept.
    func sideBarLetGo(_ shown: SideBarShown) {
        guard shown.state == .moving else {
            sideBarMoving = nil
            return
        }
        UserDefaults.standard.set(shown.centre, forKey: sideBarCentreKey())
        withAnimation(sideBarSettle) {
            sideBarCentre = shown.centre
            sideBarMoving = nil
        }
    }
}

/// The side bar settling into its width and place once its drag is let go.
private let sideBarSettle = Motion.easeOut(0.15)

private struct SideButton: View {
    let icon: PcIcon
    let enabled: Bool
    let action: () -> Void

    var body: some View {
        // a 50 frame shown 36 across
        PcIconView(icon, size: 36, color: enabled ? Pc.sideGlyph : Pc.sideGlyphDisabled)
            .frame(width: 40, height: 41)
            .contentShape(Rectangle())
            .onTapGesture {
                if enabled {
                    action()
                }
            }
    }
}

/// What a slider's panel does: shown while it is dragged (`held` false), kept up when it is held
/// still.
/// What a slider does with its panel: `show` it (dragged or held, it comes up and stays up when
/// the slider is let go, for its + to be tapped), `hide` it, and whether a slider's panel is `up`.
struct SliderPanelHooks {
    let show: () -> Void
    let hide: () -> Void
    let up: () -> Bool
}

private let sliderPanelSide: CGFloat = 190
private let sliderPanelGap: CGFloat = 8
private let sliderPanelRise: CGFloat = 10
// dark glass over the canvas (what is behind it blurred), or light
private var sliderPanelFill: Color { Look.shared.light ? Color(argb: 0xFFE7_E7E7) : Pc.onDarkFrost(0xC93E_3E3E) }
private var sliderPanelInk: Color { Look.shared.light ? Color(argb: 0xFF39_3939) : Color(argb: 0xFFF0_F0F0) }
// the dot is drawn in the interface's ink, not the brush's colour
private var sliderPanelDot: Color { Look.shared.light ? .black : .white }

/// A slider's panel: its name and value ("Size 8%"), a button saving the value as a notch on the
/// slider ("+"; "-" takes away the notch it is on), and in the middle the brush's shape (a dot
/// for a round tip) as it would paint now, its size on the screen (and at its opacity).
private struct SliderPanelView: View {
    let model: CanvasModel
    let which: SliderPanel
    var body: some View {
        let size = which == .size
        let value = size ? model.size : model.opacity
        // read again as the size changes
        let diameter = CGFloat(model.brushScreenDiameter()) / model.pixels
        let shape = model.brushOutline()
        let onNotch = model.notchAt(size: size, value: value) != nil
        let ink = sliderPanelDot.opacity(size ? 1 : Double(min(max(value, 0.02), 1)))
        let shape22 = RoundedRectangle(cornerRadius: 22, style: .circular)
        ZStack(alignment: .topLeading) {
            Canvas { context, area in
                let centre = CGPoint(x: area.width / 2, y: area.height / 2)
                let r = min(max(diameter / 2, 0.75), min(area.width, area.height) / 2 - 8)
                if shape.isEmpty {
                    context.fill(Path(ellipseIn: CGRect(x: centre.x - r, y: centre.y - r, width: r * 2, height: r * 2)), with: .color(ink))
                } else {
                    // its loops across the diameter, a hole where one lies inside another
                    var path = Path()
                    for ring in shape {
                        for i in 0 ..< ring.count / 2 {
                            let p = CGPoint(x: centre.x + CGFloat(ring[i * 2]) * r * 2, y: centre.y + CGFloat(ring[i * 2 + 1]) * r * 2)
                            if i == 0 {
                                path.move(to: p)
                            } else {
                                path.addLine(to: p)
                            }
                        }
                        path.closeSubpath()
                    }
                    context.fill(path, with: .color(ink), style: FillStyle(eoFill: true))
                }
            }
            // the size as its share of the brush's size range, the opacity as it is (the core's)
            (Text(verbatim: size ? "Size" : "Opacity").bold() + Text(verbatim: " \(sideSliderShownPercent(size: size, value: value))%"))
                .font(.system(size: 19))
                .foregroundStyle(sliderPanelInk)
                .lineLimit(1)
                .fixedSize()
                .frame(height: 32)
                .offset(x: 16, y: 6)
            if onNotch || model.canSaveNotch(size: size) {
                PcIconView(onNotch ? PcIcons.notchRemove : PcIcons.notchAdd, size: 24, color: sliderPanelInk)
                    .frame(width: 32, height: 32)
                    .contentShape(Rectangle())
                    .onTapGesture { model.toggleNotch(size: size) }
                    .offset(x: 150, y: 6)
            }
        }
        .frame(width: sliderPanelSide, height: sliderPanelSide, alignment: .topLeading)
        .background {
            if Look.shared.light {
                shape22.fill(sliderPanelFill)
            } else {
                shape22.fill(.ultraThinMaterial)
                    .environment(\.colorScheme, .dark)
                    .overlay(shape22.fill(sliderPanelFill))
            }
        }
        // a hairline rim, as measured
        .overlay(shape22.strokeBorder(Pc.glassMenuEdge, lineWidth: 1))
        .clipShape(shape22)
    }
}

/// Where a slider's touch has got to.
private final class SliderTouch {
    var began = false
    var lastY: CGFloat = 0
    /// Where the finger has taken the value, before a notch pulls it in; the value it began at,
    /// and whether a slider's panel was up as it landed.
    var raw: Float = 0
    var from: Float = 0
    var panelWasUp = false
    var moved = false
    var held = false
    var hold: DispatchWorkItem?
    /// The notch the thumb was last pulled onto, if it is on one.
    var notch: Float?
    /// Where the slider is on the window, as last laid out.
    var frame = CGRect.zero

    func placed(_ frame: CGRect) {
        self.frame = frame
    }
}

/// An upright slider moved by dragging anywhere on it: the thumb follows the finger (relative,
/// not jump-to-touch), finer the farther out to the side the finger goes, 1 at the top. Moved or
/// held still, its `panel` comes up, and stays up when the finger lifts; its thumb tapped brings
/// the panel up, or takes it away; a tap elsewhere on the track takes the thumb there (and the
/// panel away), as the core says. Its `notches` (saved values) show as marks on the track; a value
/// dragged near one is pulled onto it, and a tap on one goes there. On a notch, the thumb shows a
/// blue bar.
struct BrushSlider: View {
    let value: Float
    var notches: [Float] = []
    var panel: SliderPanelHooks?
    let onChange: (Float) -> Void

    @State private var touch = SliderTouch()
    /// A touch is down on the slider: gone as it lifts, or as the system takes it back (no end
    /// comes then, and the next touch must not go on from where this one was).
    @GestureState private var pressing = false

    private static let notchWidth: CGFloat = 20
    private static let notchHeight: CGFloat = 4
    /// How the thumb's shadow is drawn, the widest step first: worked out once, the same on either
    /// look.
    private static let thumbShadow = sideThumbShadow()
    /// A touch held this long without moving brings the panel up (seconds).
    private static let holdSeconds = 0.5
    /// A touch let go within this far of where it went down (points) is a tap: a finger's wobble
    /// as it lands and lifts doesn't make it a drag (what it moved the thumb is taken back).
    static let tapSlop: CGFloat = 4

    private var travel: CGFloat {
        Pc.sliderHeight - Pc.thumbHeight - Pc.thumbInset * 2
    }

    private func near(_ v: Float, reach: Float = CanvasModel.notchNear) -> Float? {
        notches.filter { abs($0 - v) < reach }.min { abs($0 - v) < abs($1 - v) }
    }

    var body: some View {
        let track = Pc.sliderTrack
        let thumbColour = Pc.sliderThumb
        let accent = Pc.accent
        Canvas { context, size in
            let inset = Pc.thumbInset
            let thumb = CGSize(width: Pc.thumbWidth, height: Pc.thumbHeight)
            let travel = size.height - thumb.height - inset * 2
            let at = CGPoint(x: (size.width - thumb.width) / 2, y: inset + travel * CGFloat(1 - value))
            let knobRect = CGRect(origin: at, size: thumb)
            let knob = Path(roundedRect: knobRect, cornerRadius: thumb.height / 2, style: .circular)
            // the track has a window where the thumb is, so the thumb shows over the bar alone
            let corner = Pc.sliderCorner
            var window = Path(roundedRect: CGRect(origin: .zero, size: size), cornerRadius: corner, style: .circular)
            window.addPath(knob)
            context.fill(window, with: .color(track), style: FillStyle(eoFill: true))
            // its ends catch the light, a half-point band at a time in from the edge, round the
            // corners and not down the sides
            for (i, white) in Pc.sliderShine.enumerated() {
                let d = CGFloat(i) * 0.5 + 0.25
                let band = Path(
                    roundedRect: CGRect(origin: .zero, size: size).insetBy(dx: d, dy: d),
                    cornerRadius: max(corner - d, 0), style: .circular
                )
                let shine = Pc.sliderEnds(Color.white.opacity(white), Color.white.opacity(0), height: size.height)
                context.stroke(
                    band, with: .linearGradient(shine, startPoint: .zero, endPoint: CGPoint(x: 0, y: size.height)), lineWidth: 0.5
                )
            }
            // the notches: short bars across the track; on one, the thumb shows a blue bar
            let bar = CGSize(width: Self.notchWidth, height: Self.notchHeight)
            for n in notches {
                let y = inset + thumb.height / 2 + travel * CGFloat(1 - n)
                let mark = CGRect(x: (size.width - bar.width) / 2, y: y - bar.height / 2, width: bar.width, height: bar.height)
                context.fill(Path(roundedRect: mark, cornerRadius: bar.height / 2, style: .circular), with: .color(Pc.notchMark))
            }
            // the thumb's soft shadow, a point below it: under the thumb, only its rim shows
            for step in Self.thumbShadow {
                let grow = CGFloat(step.grow)
                let shade = knobRect.insetBy(dx: -grow, dy: -grow).offsetBy(dx: 0, dy: CGFloat(step.down))
                let r = max(thumb.height / 2 + grow, 0)
                context.fill(Path(roundedRect: shade, cornerRadius: r, style: .circular), with: .color(Color.black.opacity(Double(step.alpha))))
            }
            context.fill(knob, with: .color(thumbColour))
            if notches.contains(where: { abs($0 - value) < 1e-4 }) {
                let mark = CGRect(x: (size.width - bar.width) / 2, y: at.y + (thumb.height - bar.height) / 2, width: bar.width, height: bar.height)
                context.fill(Path(roundedRect: mark, cornerRadius: bar.height / 2, style: .circular), with: .color(accent))
            }
        }
        .frame(width: Pc.sliderWidth, height: Pc.sliderHeight)
        // where it is on the window, for a notch to be felt where it is
        .background {
            GeometryReader { place in
                let _ = touch.placed(place.frame(in: .global))
                Color.clear
            }
        }
        .contentShape(Rectangle())
        .gesture(
            DragGesture(minimumDistance: 0)
                .updating($pressing) { _, down, _ in down = true }
                .onChanged { drag in
                    let t = touch
                    if !t.began {
                        t.began = true
                        t.lastY = drag.startLocation.y
                        t.raw = value
                        t.from = value
                        t.panelWasUp = panel?.up() ?? false
                        t.moved = false
                        t.held = false
                        t.notch = notches.first { abs($0 - value) < 1e-4 }
                        let hold = DispatchWorkItem { [panel] in
                            if t.began, !t.moved {
                                t.held = true
                                panel?.show()
                            }
                        }
                        t.hold = hold
                        DispatchQueue.main.asyncAfter(deadline: .now() + Self.holdSeconds, execute: hold)
                    }
                    let dy = drag.location.y - t.lastY
                    if dy != 0 {
                        if !t.moved {
                            panel?.show()
                        }
                        t.moved = true
                        t.lastY = drag.location.y
                        // the farther out to the side the finger is, the finer the thumb follows
                        let across = Float(drag.location.x - Pc.sliderWidth / 2)
                        t.raw = min(max(t.raw - Float(dy / travel) * sideSliderDragShare(across: across), 0), 1)
                        let notch = near(t.raw)
                        onChange(notch ?? t.raw)
                        // pulled onto a notch, it is felt there (Apple Pencil Pro)
                        if let notch, notch != t.notch {
                            let y = Pc.thumbInset + Pc.thumbHeight / 2 + travel * CGFloat(1 - notch)
                            PencilFeedback.canvas?.aligned(onWindow: CGPoint(x: t.frame.midX, y: t.frame.minY + y))
                        }
                        t.notch = notch
                    }
                }
                .onEnded { drag in
                    let t = touch
                    t.hold?.cancel()
                    t.hold = nil
                    t.began = false
                    // let go after a drag or a hold, the panel stays up for its +
                    let travelled = hypot(drag.location.x - drag.startLocation.x, drag.location.y - drag.startLocation.y)
                    guard !t.held, travelled <= Self.tapSlop else { return }
                    // a tap: what a finger's wobble moved is taken back; then on the thumb it brings
                    // up the panel (or takes it away), elsewhere the thumb goes there (or onto a
                    // notch's mark there) and the panel goes, as the core says
                    if t.moved {
                        onChange(t.from)
                    }
                    let at = 1 - Float((drag.startLocation.y - Pc.thumbInset - Pc.thumbHeight / 2) / travel)
                    let thumb = Float(Pc.thumbHeight / 2 / travel)
                    switch sideSliderTap(at: at, value: t.from, thumb: thumb, notches: notches, panelUp: t.panelWasUp) {
                    case .panel:
                        panel?.show()
                    case .closePanel:
                        panel?.hide()
                    case let .jump(value):
                        onChange(value)
                        panel?.hide()
                    }
                }
        )
        // taken back by the system: the touch is over all the same
        .onChange(of: pressing) { _, down in
            if !down, touch.began {
                touch.hold?.cancel()
                touch.hold = nil
                touch.began = false
            }
        }
    }
}
