import SwiftUI
import UIKit

/// How long the band offers Continue filling after a drop, seconds.
private let bandShown: TimeInterval = 2.5
/// How far a touch goes before it is a drag, points; how long it stays still to be a hold, seconds.
private let dragSlop: CGFloat = 8
private let holdWait: TimeInterval = 0.4
/// How long (ms) a plugin may finish a fill before the band says it is.
private let finishingSaid: Double = 500
/// The dragged colour's dot, across.
private let dotSize: CGFloat = 55.5

/// ColorDrop: the colour dragged from the colour button (or a swatch) and dropped on the canvas
/// fills the area around where it lands. The core says when it rests and what a slide does; this
/// pours, and keeps what the band shows.
@Observable
final class ColorDrop {
    /// What the band under the top bar shows.
    enum Band {
        case none, dragging, dropped, filling
    }

    private static let thresholdKey = "dropThreshold"

    @ObservationIgnored private unowned let model: CanvasModel

    private(set) var band = Band.none
    /// The dragged colour's dot (window points) and its colour (0xRRGGBB), while it is dragged.
    private(set) var dot: (at: CGPoint, rgb: UInt32)?
    /// The dragged colour as it is drawn: pulled out of the colour button, a drop that lets go and
    /// follows the pointer as a ball on a spring (the core works its outline out each frame).
    private(set) var blob: ColourBlob?
    /// The colour button's centre while its colour is dragged, over which the dot shrinks on a bar.
    @ObservationIgnored private var buttonCentre: CGPoint?
    /// The threshold the colour fills with (0..1), kept from one drop to the next.
    private(set) var threshold: Float
    /// The threshold is being slid: its row shows under the band.
    private(set) var sliding = false
    /// Where a dragged colour let go fills nothing (the colour panel, while open), window points.
    @ObservationIgnored var avoid: CGRect?

    /// The swatch's colour when a swatch was dropped (filling goes on with it); else the current one.
    @ObservationIgnored private var swatch: UInt32?
    @ObservationIgnored private var bandTimer: Timer?
    @ObservationIgnored private var restTimer: Timer?
    /// The colour being dragged: where it is and whether it has come to rest (the core's rules).
    @ObservationIgnored private var drag: ColourDrag?
    /// It came to rest and has begun filling (or tried to, and couldn't).
    @ObservationIgnored private var poured = false
    @ObservationIgnored private var pourRefused = false

    /// A fill's work, in order, off the main thread (a slid threshold redoes the whole fill).
    @ObservationIgnored private let queue = DispatchQueue(label: "pour", qos: .userInitiated)
    /// A plugin is finishing a fill let go, and the cover taking the touches meanwhile.
    @ObservationIgnored private var finishing = false
    @ObservationIgnored private var finishingTold = false
    @ObservationIgnored private var shield: UIView?
    /// The latest threshold waiting for that queue.
    @ObservationIgnored private let waiting = NSLock()
    @ObservationIgnored private var nextThreshold: Float?

    init(model: CanvasModel) {
        self.model = model
        threshold = UserDefaults.standard.object(forKey: Self.thresholdKey) as? Float ?? 0.5
    }

    private func keepThreshold() {
        if sliding {
            UserDefaults.standard.set(threshold, forKey: Self.thresholdKey)
        }
        sliding = false
    }

    private static func channels(_ rgb: UInt32) -> (r: Float, g: Float, b: Float) {
        (Float((rgb >> 16) & 0xFF) / 255, Float((rgb >> 8) & 0xFF) / 255, Float(rgb & 0xFF) / 255)
    }

    private static var now: Double {
        ProcessInfo.processInfo.systemUptime * 1000
    }

    private static func point(_ step: DragStep) -> CGPoint {
        CGPoint(x: CGFloat(step.dotX), y: CGFloat(step.dotY))
    }

    // MARK: the drag

    /// A colour starts being dragged from the colour button, or `swatch` from the palette, by a pen
    /// or not, pulled out `from` the button's centre or where the swatch was touched.
    func start(at: CGPoint, swatch: UInt32?, pen: Bool, from: CGPoint) {
        if swatch == nil {
            model.closePanels()
        }
        bandTimer?.invalidate()
        self.swatch = swatch
        dot = (at, swatch ?? model.colorRgb())
        band = .dragging
        poured = false
        pourRefused = false
        drag = ColourDrag(x: Float(at.x), y: Float(at.y), ms: Self.now, pen: pen, threshold: threshold)
        // with Reduce Motion it neither stretches nor wobbles
        buttonCentre = swatch == nil ? from : nil
        blob = ColourBlob(
            x: Float(from.x), y: Float(from.y), fromButton: swatch == nil, pen: pen,
            reduceMotion: UIAccessibility.isReduceMotionEnabled, ms: Self.now
        )
        blobTo(at)
        waitForRest()
    }

    /// The drawn colour goes after `p`: over the top bar it shrinks the further it is from the
    /// colour button.
    private func blobTo(_ p: CGPoint) {
        let button = buttonCentre ?? p
        blob?.pointTo(x: Float(p.x), y: Float(p.y), overBar: p.y <= Pc.barHeight, buttonX: Float(button.x), buttonY: Float(button.y))
    }

    /// The dragged colour's pointer moved: the dot follows, or, resting, the threshold slides.
    func move(to at: CGPoint) {
        guard let drag else { return }
        let step = drag.moved(x: Float(at.x), y: Float(at.y), ms: Self.now)
        let shown = Self.point(step)
        dot = dot.map { (shown, $0.rgb) }
        blobTo(shown)
        if step.slid {
            if poured {
                pourThreshold(step.threshold)
            } else if !pourRefused {
                threshold = step.threshold
                sliding = true
                pourHere(shown)
            }
        }
        waitForRest()
    }

    private func pourHere(_ at: CGPoint) {
        poured = pourBegin(at)
        pourRefused = !poured
        if poured {
            dot = nil
        }
    }

    /// Held still a moment, the colour comes to rest.
    private func waitForRest() {
        restTimer?.invalidate()
        guard let due = drag?.restDue() else { return }
        restTimer = Timer.scheduledTimer(withTimeInterval: max(due - Self.now, 1) / 1000, repeats: false) { [weak self] _ in
            self?.rest()
            self?.waitForRest()
        }
    }

    /// The colour comes to rest if it is due, on the artwork off the bars, and fills there at once,
    /// the fill showing instead of the dot (a slide then sets the threshold).
    private func rest() {
        guard let drag else { return }
        let at = Self.point(drag.now())
        let pixels = model.pixels
        let lands = at.y > Pc.barHeight && avoid?.contains(at) != true
            && model.session.pourLands(x: Float(at.x * pixels), y: Float(at.y * pixels))
        guard drag.rest(ms: Self.now, droppable: lands), !poured, !pourRefused else { return }
        pourHere(at)
    }

    /// The dragged colour let go at `at` (nil: somewhere it doesn't fill, like the top bar).
    func end(at: CGPoint?) {
        let step = drag?.now()
        drag = nil
        blob = nil
        buttonCentre = nil
        restTimer?.invalidate()
        // come to rest, it fills where it rests (already, if its threshold was slid)
        if poured {
            poured = false
            dot = nil
            pourEnd(keep: true)
            offerToContinue()
            return
        }
        let rested = step.flatMap { $0.resting ? Self.point($0) : nil }
        let target = at.map { rested ?? $0 }
        dot = nil
        if let target, avoid?.contains(target) != true, pourBegin(target) {
            pourEnd(keep: true)
            offerToContinue()
        } else {
            band = .none
        }
    }

    /// The band offers to go on filling for a moment.
    private func offerToContinue() {
        band = .dropped
        bandTimer = Timer.scheduledTimer(withTimeInterval: bandShown, repeats: false) { [weak self] _ in
            if self?.band == .dropped {
                self?.band = .none
            }
        }
    }

    // MARK: filling

    /// Continue filling: each touch on the canvas fills, until `endFilling`.
    func continueFilling() {
        bandTimer?.invalidate()
        band = .filling
    }

    /// Filling ends, and Recolor with it.
    func endFilling() {
        endBand()
        model.endRecolor()
    }

    /// The band goes, and Continue filling with it.
    func endBand() {
        bandTimer?.invalidate()
        band = .none
        sliding = false
    }

    /// Continue filling: the colour and threshold the canvas's touches fill with; none when not
    /// filling.
    func touchFill() -> Fill? {
        guard band == .filling else { return nil }
        let c = Self.channels(swatch ?? model.colorRgb())
        return Fill(r: c.r, g: c.g, b: c.b, threshold: threshold)
    }

    /// The touches' fill was let go of (kept, or taken back by a second finger).
    func filled() {
        keepThreshold()
        model.refresh()
    }

    /// Continue filling's touch landed where it fills: `fill` poured from window pixel (`x`, `y`)
    /// on the fill's queue, after what was asked of it before; the touch goes on meanwhile.
    func tapFill(x: Float, y: Float, fill: Fill) {
        let session = model.session
        queue.async {
            _ = session.pourBegin(x: x, y: y, r: fill.r, g: fill.g, b: fill.b, threshold: fill.threshold)
        }
    }

    /// Continue filling's touch's fill ends on the fill's queue, after a threshold slid before it is
    /// redone: `keep` makes it an undo step, else it is taken back. What it changed is read once
    /// it is made.
    func tapFillEnd(keep: Bool) {
        let session = model.session
        queue.async { [weak self] in
            session.pourEnd(keep: keep, name: nil)
            DispatchQueue.main.async {
                self?.model.refresh()
                self?.watchFinishing()
            }
        }
    }

    /// Fills from window point `p` with the dropped colour, spreading in from there; false if
    /// nothing was filled.
    private func pourBegin(_ p: CGPoint) -> Bool {
        guard model.currentTakesChanges(paint: true) else { return false }
        // text takes no colour until it is rasterized
        if model.layers.first(where: { $0.current })?.text == true {
            model.askRasterize = true
            return false
        }
        let c = Self.channels(swatch ?? model.colorRgb())
        let session = model.session
        let x = Float(p.x * model.pixels)
        let y = Float(p.y * model.pixels)
        let t = threshold
        // made at once, and shown spreading in from where it landed
        return queue.sync {
            session.pourDrop(x: x, y: y, r: c.r, g: c.g, b: c.b, threshold: t)
        }
    }

    /// The fill under way redone with `value`, which is kept for the next.
    func pourThreshold(_ value: Float) {
        let t = min(max(value, 0), 1)
        threshold = t
        sliding = true
        let session = model.session
        let first = waiting.withLock {
            let first = nextThreshold == nil
            nextThreshold = t
            return first
        }
        guard first else { return }
        queue.async { [weak self] in
            guard let self else { return }
            let latest = waiting.withLock {
                let latest = self.nextThreshold
                self.nextThreshold = nil
                return latest
            }
            if let latest {
                session.pourThreshold(threshold: latest)
            }
        }
    }

    /// The fill under way ends: `keep` makes it an undo step, else it is taken back.
    private func pourEnd(keep: Bool) {
        let session = model.session
        queue.sync {
            session.pourEnd(keep: keep, name: nil)
        }
        keepThreshold()
        model.refresh()
        watchFinishing()
    }

    /// While a plugin finishes a fill let go (on a thread of its own), nothing is touched until it
    /// is done: a cover over the window takes the touches, and after a while the band says why.
    private func watchFinishing() {
        let session = model.session
        guard !finishing, session.pourFinishing() else { return }
        finishing = true
        let window = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap(\.windows)
            .first(where: \.isKeyWindow)
        if let window {
            let cover = UIView(frame: window.bounds)
            cover.backgroundColor = .clear
            cover.autoresizingMask = [.flexibleWidth, .flexibleHeight]
            window.addSubview(cover)
            shield = cover
        }
        let since = Self.now
        finishingTold = false
        Timer.scheduledTimer(withTimeInterval: 1.0 / 60, repeats: true) { [weak self] timer in
            guard let self else {
                timer.invalidate()
                return
            }
            if session.pourFinishing() {
                if !finishingTold, Self.now - since >= finishingSaid {
                    finishingTold = true
                    model.showNotice("Finishing the fill…")
                }
                return
            }
            timer.invalidate()
            shield?.removeFromSuperview()
            shield = nil
            finishing = false
            model.refresh()
        }
    }
}

/// What takes the touches on a colour that can be dragged out: its own so a pen is told from a
/// finger.
final class DropTouchView: UIView {
    var drop: ColorDrop?
    var swatch: UInt32?
    var onHold: (() -> Void)?
    var onTap: () -> Void = {}
    /// Pressed, or let go.
    var onPress: (Bool) -> Void = { _ in }

    private var touch: UITouch?
    private var start = CGPoint.zero
    private var dragging = false
    private var held = false
    private var holdTimer: Timer?

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard touch == nil, let first = touches.first else { return }
        touch = first
        start = Interface.shared.point(first.location(in: nil))
        dragging = false
        held = false
        onPress(true)
        guard onHold != nil else { return }
        holdTimer = Timer.scheduledTimer(withTimeInterval: holdWait, repeats: false) { [weak self] _ in
            self?.held = true
            self?.onHold?()
        }
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let touch, touches.contains(touch), !held else { return }
        let at = Interface.shared.point(touch.location(in: nil))
        if !dragging {
            guard hypot(at.x - start.x, at.y - start.y) > dragSlop else { return }
            holdTimer?.invalidate()
            dragging = true
            // out of the colour button's centre, or where the swatch was touched
            let centre = Interface.shared.point(convert(CGPoint(x: bounds.midX, y: bounds.midY), to: nil))
            drop?.start(at: start, swatch: swatch, pen: touch.type == .pencil, from: swatch == nil ? centre : start)
        }
        drop?.move(to: at)
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let touch, touches.contains(touch) else { return }
        let at = Interface.shared.point(touch.location(in: nil))
        finish {
            // let go on the top bar, it is put back
            drop?.end(at: at.y > Pc.barHeight ? at : nil)
        } otherwise: {
            if !held {
                onTap()
            }
        }
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let touch, touches.contains(touch) else { return }
        finish {
            drop?.end(at: nil)
        } otherwise: {}
    }

    private func finish(dragged: () -> Void, otherwise: () -> Void) {
        holdTimer?.invalidate()
        touch = nil
        onPress(false)
        if dragging {
            dragged()
        } else {
            otherwise()
        }
        dragging = false
    }
}

/// A colour that can be dragged out onto the canvas to fill there (the colour button, or a swatch
/// giving its colour): tapped it does `onTap`, held still `onHold`; dragged, its colour goes with
/// the finger as a dot and fills where it is let go below the top bar.
struct DropSource: UIViewRepresentable {
    let model: CanvasModel
    var swatch: UInt32?
    var onHold: (() -> Void)?
    var onPress: (Bool) -> Void = { _ in }
    let onTap: () -> Void

    func makeUIView(context: Context) -> DropTouchView {
        DropTouchView()
    }

    func updateUIView(_ view: DropTouchView, context: Context) {
        view.drop = model.drop
        view.swatch = swatch
        view.onHold = onHold
        view.onTap = onTap
        view.onPress = onPress
    }
}

/// The colour being dragged out, as the core's `ColourBlob` works it out each frame: a drop pulled
/// out of the colour button that lets go and follows the finger as a ball, springing and wobbling
/// on its own (a swatch's grows out of nothing).
struct ColorDropDot: View {
    let drop: ColorDrop

    var body: some View {
        if let dot = drop.dot {
            if let blob = drop.blob {
                TimelineView(.animation) { _ in
                    Canvas { context, _ in
                        let shape = blob.shape(ms: ProcessInfo.processInfo.systemUptime * 1000)
                        // the outlines filled together, where they cross too
                        var path = Path()
                        for outline in shape.outlines {
                            let p = outline.points
                            guard p.count >= 6 else { continue }
                            path.move(to: CGPoint(x: CGFloat(p[0]), y: CGFloat(p[1])))
                            var i = 2
                            while i + 1 < p.count {
                                path.addLine(to: CGPoint(x: CGFloat(p[i]), y: CGFloat(p[i + 1])))
                                i += 2
                            }
                            path.closeSubpath()
                        }
                        context.fill(path, with: .color(CanvasSpace.shared.color(dot.rgb)))
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .allowsHitTesting(false)
            } else {
                Circle()
                    .fill(CanvasSpace.shared.color(dot.rgb))
                    .frame(width: dotSize, height: dotSize)
                    .offset(x: dot.at.x - dotSize / 2, y: dot.at.y - dotSize / 2)
                    .allowsHitTesting(false)
            }
        }
    }
}

/// ColorDrop's band under the top bar, sliding down from behind it: while a colour is dragged and
/// while filling goes on, with the threshold as a blue line across the screen under it; after a
/// drop, for a moment, offering to continue filling. Sliding the threshold adds a row reading it.
struct ColorDropBand: View {
    let drop: ColorDrop
    let width: CGFloat

    /// How far down it is, and what it showed, kept while it slides away.
    @State private var shown: CGFloat = 0
    @State private var kept = ColorDrop.Band.none

    private var light: Bool {
        Look.shared.light
    }

    private var blue: Color {
        Color(argb: light ? 0xFF0A_8CFD : 0xFF00_76FD)
    }

    private var ink: Color {
        Color(argb: light ? 0xFF22_2222 : 0xFFC5_C5C5)
    }

    private var tick: some View {
        Canvas { context, size in
            let u = size.width / 20
            var path = Path()
            path.move(to: CGPoint(x: 5.8 * u, y: 10.1 * u))
            path.addLine(to: CGPoint(x: 8.6 * u, y: 12.6 * u))
            path.addLine(to: CGPoint(x: 14 * u, y: 7.2 * u))
            context.stroke(
                path, with: .color(.white),
                style: StrokeStyle(lineWidth: 2 * u, lineCap: .round, lineJoin: .round)
            )
        }
        .frame(width: 20, height: 20)
        .background(Circle().fill(blue))
        .contentShape(Circle())
        .onTapGesture { drop.endFilling() }
    }

    private var continueButton: some View {
        Text(verbatim: "Continue filling")
            .font(.system(size: 11.7))
            .foregroundStyle(Color.white)
            .lineLimit(1)
            .frame(width: 101, height: 20)
            .background(RoundedRectangle(cornerRadius: 10, style: .circular).fill(blue))
            .contentShape(Rectangle())
            .onTapGesture { drop.continueFilling() }
    }

    var body: some View {
        let line = kept == .dragging || kept == .filling
        let height: CGFloat = 32 + (drop.sliding ? 24 : 0) + (line ? 4 : 0)
        let opening = drop.band != .none
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 13) {
                Text(verbatim: "ColorDrop")
                    .font(.system(size: 16.5, weight: .bold))
                    .foregroundStyle(ink)
                    .lineLimit(1)
                if kept == .filling {
                    // the tick ends filling
                    tick
                } else {
                    continueButton
                }
            }
            .frame(width: width, height: 32)
            .background(
                Rectangle()
                    .fill(Color(argb: light ? 0xD6EE_EEEE : 0xD63B_3B3B))
                    .allowsHitTesting(false)
            )
            if drop.sliding {
                Text(verbatim: "Threshold \(wholeNumber(drop.threshold * 100))%")
                    .font(.system(size: 14.4))
                    .foregroundStyle(ink)
                    .lineLimit(1)
                    .frame(width: width, height: 24)
                    .background(Rectangle().fill(Color(argb: light ? 0xCCDC_DCDC : 0xCC4A_4A4A)))
                    .allowsHitTesting(false)
            }
            if line {
                Rectangle()
                    .fill(blue)
                    .frame(width: width * CGFloat(drop.threshold), height: 4)
                    .allowsHitTesting(false)
            }
        }
        .offset(y: -(1 - shown) * height)
        .frame(width: width, height: height, alignment: .top)
        .clipped()
        .offset(y: Pc.barHeight)
        .opacity(shown > 0 ? 1 : 0)
        .allowsHitTesting(opening)
        .onChange(of: drop.band, initial: true) { _, band in
            if band != .none {
                kept = band
            }
            withAnimation(.timingCurve(0.42, 0, 0.58, 1, duration: 0.3)) {
                shown = band == .none ? 0 : 1
            }
        }
    }
}
