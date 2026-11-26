import QuartzCore
import UIKit

/// The canvas: a Metal layer the core's render thread draws into, and every touch on it handed to
/// the core's touches (`crates/core/src/touch.rs`, the same as the Android app's), which paint,
/// move the canvas and say what the app should do. A touch carries each sample the system
/// coalesced since the last, in pixels, with the Pencil's force, angles and barrel roll. The
/// Pencil's hover, double-tap and squeeze go to the core's pen gestures (`touch/pen.rs`), with
/// the system's own Apple Pencil settings for them.
final class CanvasView: UIView {
    override class var layerClass: AnyClass { CAMetalLayer.self }
    /// No system editing gestures over the canvas (its controller says the same).
    override var editingInteractionConfiguration: UIEditingInteractionConfiguration { .none }

    let session: CanvasSession
    /// What the touches asked of the app (undo, redo, fit, ...).
    var onCommands: ([TouchCommand]) -> Void = { _ in }
    /// A touch is coming down with none on the canvas: the touches are told how things stand.
    var beforeTouches: () -> Void = {}
    /// The touches moved, or the last of them went.
    var afterTouches: (_ ended: Bool) -> Void = { _ in }
    /// Whether the canvas takes touches now (not while Edit Layout moves the bars).
    var takesTouches: () -> Bool = { true }
    /// Where one touch is on the canvas or the Pencil hovers (points), for Liquify's ring; none
    /// with no touch, or more than one.
    var ringAt: (CGPoint?) -> Void = { _ in }
    /// Tap Assistance (the system's Settings): for its first moments a lone finger's touch stays
    /// where it landed (its moves held back, its lift put back there), so a tap that drifts still
    /// lands where it began. More fingers, a touch held longer, or one gone farther than a drift go
    /// on as they are.
    var tapAssistance = false
    /// The finger held where it landed (points), and when it landed.
    private var heldTap: (touch: ObjectIdentifier, at: CGPoint, since: TimeInterval)?
    /// How long (seconds) and how far (points) a touch stays held at most.
    private static let tapHold: TimeInterval = 0.3
    private static let tapDrift: CGFloat = 24

    private var attached = false
    /// The window as last given to the session, and the way up it was at.
    private var windowWidth: UInt32 = 0
    private var windowHeight: UInt32 = 0
    private var windowTurn: UInt32 = 0
    /// Each touch's id for the core while it is down, by the touch.
    private var ids: [ObjectIdentifier: UInt64] = [:]
    private var nextId: UInt64 = 1
    /// The touch whose stroke is drawn ahead to where the system predicts it goes.
    private var strokeTouch: ObjectIdentifier?
    /// Where the Pencil last was over or on the canvas (its double-tap acts there).
    private var pencilAt: CGPoint?
    /// How far the hovering Pencil's outline turns past the brush's own rotation (radians), as the
    /// core last said.
    private var hoverTurn: CGFloat = 0
    /// The touches' next wait (a hold, a repeat) running out.
    private var tick: Timer?
    /// The brush's outline where the Pencil or the pointer hovers (shown while hovering only, as
    /// Brush cursor starts).
    private let cursor = BrushCursorView()
    /// The Pencil's touch and its samples on their way to the core, each held until its true force
    /// is known.
    private var penId: UInt64?
    private lazy var pencil = PencilHold(
        begin: { [weak self] samples in
            guard let self, let id = self.penId else { return }
            self.obey(self.session.touchDown(id: id, kind: .stylus, samples: samples))
        },
        move: { [weak self] samples in
            guard let self, let id = self.penId else { return }
            self.obey(self.session.touchMove(id: id, samples: samples))
        },
        ahead: { [weak self] samples in
            self?.session.strokePredict(samples: samples)
        },
        end: { [weak self] samples in
            guard let self, let id = self.penId else { return }
            self.penId = nil
            self.obey(self.session.touchUp(id: id, samples: samples))
        }
    )
    /// What Apple Pencil Pro feels.
    private lazy var feedback = PencilFeedback(view: self)

    init(session: CanvasSession) {
        self.session = session
        super.init(frame: .zero)
        CanvasLife.made(.view)
        isMultipleTouchEnabled = true
        isOpaque = true
        backgroundColor = .black
        cursor.frame = bounds
        cursor.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        addSubview(cursor)
        // the Pencil held over the glass, and a trackpad's or mouse's pointer, each on its own
        let pencilHover = UIHoverGestureRecognizer(target: self, action: #selector(pencilHovered(_:)))
        pencilHover.allowedTouchTypes = [NSNumber(value: UITouch.TouchType.pencil.rawValue)]
        addGestureRecognizer(pencilHover)
        let pointerHover = UIHoverGestureRecognizer(target: self, action: #selector(pointerHovered(_:)))
        pointerHover.allowedTouchTypes = [NSNumber(value: UITouch.TouchType.indirectPointer.rawValue)]
        addGestureRecognizer(pointerHover)
        // what lies over the canvas (a side slider's notch) is felt through it too
        PencilFeedback.canvas = feedback
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("the canvas is made in code")
    }

    deinit {
        CanvasLife.gone(.view)
    }

    /// Pixels to a point on the screen the view is on.
    private var screenScale: CGFloat {
        window?.screen.scale ?? traitCollection.displayScale
    }

    /// Pixels to one of this view's own points: the interface's size scales it, and the canvas stays
    /// at the screen's pixels.
    private var scale: CGFloat {
        screenScale * Interface.shared.scale
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        if window == nil {
            detach()
        } else {
            setNeedsLayout()
        }
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        guard window != nil, bounds.width > 0, bounds.height > 0 else { return }
        contentScaleFactor = scale
        layer.contentsScale = scale
        let width = UInt32(max(1, (bounds.width * scale).rounded()))
        let height = UInt32(max(1, (bounds.height * scale).rounded()))
        let turn = quarterTurns
        if attached {
            // the canvas stays put on the glass as the bars turn upright
            if turn != windowTurn {
                session.followTurn(from: windowTurn, to: turn, width: windowWidth, height: windowHeight)
            }
            session.resizeWindow(width: width, height: height)
            remember(width: width, height: height, turn: turn)
        } else {
            attach(width: width, height: height)
            if attached {
                // turned while there was no window (the app away): the view follows now
                if windowWidth > 0, turn != windowTurn {
                    session.followTurn(from: windowTurn, to: turn, width: windowWidth, height: windowHeight)
                }
                remember(width: width, height: height, turn: turn)
            }
        }
    }

    /// The way up the interface is, as quarter turns of what is drawn clockwise from upright (the
    /// device turned the other way: the home edge on the right is one).
    private var quarterTurns: UInt32 {
        switch window?.windowScene?.interfaceOrientation {
        case .landscapeRight: 1
        case .portraitUpsideDown: 2
        case .landscapeLeft: 3
        default: 0
        }
    }

    private func remember(width: UInt32, height: UInt32, turn: UInt32) {
        _ = session.placeGrid(quarters: turn, width: width, height: height, pixelsPerPoint: Float(screenScale))
        windowWidth = width
        windowHeight = height
        windowTurn = turn
    }

    /// Hands the layer to the core, whose render thread draws into it from now on.
    private func attach(width: UInt32, height: UInt32) {
        guard let metal = layer as? CAMetalLayer else { return }
        session.setDisplayScale(pixelsPerPoint: Float(screenScale))
        // frames no more often than the screen shows them (120 a second on a ProMotion iPad)
        let fastest = window?.screen.maximumFramesPerSecond ?? 0
        session.setRefreshRate(hertz: Float(fastest > 0 ? fastest : 60))
        // the core takes a retain of its own, let go with the window
        let pointer = UInt64(UInt(bitPattern: Unmanaged.passRetained(metal).toOpaque()))
        let id = acquireLayer(layer: pointer)
        guard id != 0 else {
            AppLog.error("canvas", "the canvas's layer wasn't taken")
            return
        }
        do {
            try session.attachWindow(window: id, width: width, height: height)
            attached = true
        } catch {
            releaseLayer(id: id)
            AppLog.error("canvas", "attaching the canvas: \(error)")
        }
    }

    /// Stops drawing: returns once the core's render thread has let go of the layer.
    func detach() {
        tick?.invalidate()
        tick = nil
        _ = pencil.cancel()
        penId = nil
        guard attached else { return }
        session.detachWindow()
        attached = false
    }

    // MARK: touches

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        // (none while Edit Layout moves the bars: nothing picked, nothing painted)
        guard takesTouches() else { return }
        // painting, the hovering outline goes (Brush cursor may show one as it paints)
        cursor.hide()
        // a stroke may begin: the strokes' sound wakes if it rests
        StrokeSound.shared.wake()
        if ids.isEmpty {
            beforeTouches()
        }
        var out: [TouchCommand] = []
        for touch in touches {
            let key = ObjectIdentifier(touch)
            let id = nextId
            nextId &+= 1
            ids[key] = id
            if ids.count == 1 {
                strokeTouch = key
            }
            if touch.type == .pencil {
                pencil.down(touch)
                penId = id
                feedback.prepare()
                pencil.add(coalesced(touch, in: event), sample: sample)
                pencil.release()
            } else {
                out += session.touchDown(id: id, kind: kind(of: touch), samples: samples(of: touch, in: event))
            }
        }
        // Tap Assistance holds a lone finger where it landed for its first moments
        if tapAssistance, ids.count == 1, touches.count == 1, let touch = touches.first, touch.type == .direct {
            heldTap = (ObjectIdentifier(touch), touch.location(in: self), touch.timestamp)
        } else {
            heldTap = nil
        }
        obey(out)
        ringFollows(event)
    }

    /// Whether Tap Assistance still holds `touch` where it landed: not long down, nor gone farther
    /// than a finger drifts. Once it isn't, it goes on as it is.
    private func heldBack(_ touch: UITouch) -> Bool {
        guard let held = heldTap, held.touch == ObjectIdentifier(touch) else { return false }
        let at = touch.location(in: self)
        if touch.timestamp - held.since <= Self.tapHold, hypot(at.x - held.at.x, at.y - held.at.y) <= Self.tapDrift {
            return true
        }
        heldTap = nil
        return false
    }

    /// Liquify's ring under the touch on the canvas while there is just one.
    private func ringFollows(_ event: UIEvent?) {
        let down = ids.count == 1 ? event?.allTouches?.first(where: { ids[ObjectIdentifier($0)] != nil }) : nil
        ringAt(down?.location(in: self))
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        var out: [TouchCommand] = []
        // the fingers this event moves go together: the view goes by all of them at once, never by
        // a spread only some of them have got to
        var moves: [TouchMove] = []
        for touch in touches {
            guard let id = ids[ObjectIdentifier(touch)] else { continue }
            if pencil.holds(touch) {
                pencil.add(coalesced(touch, in: event), sample: sample)
                pencil.release(predicted: (event?.predictedTouches(for: touch) ?? []).map(sample))
            } else if !heldBack(touch) {
                moves.append(TouchMove(id: id, samples: samples(of: touch, in: event)))
            }
        }
        if !moves.isEmpty {
            out += session.touchMoves(moves: moves)
        }
        // the stroke drawn ahead to where it is predicted to go, until the next move
        if let key = strokeTouch, heldTap?.touch != key,
           let touch = touches.first(where: { ObjectIdentifier($0) == key }), !pencil.holds(touch),
           let predicted = event?.predictedTouches(for: touch), !predicted.isEmpty
        {
            session.strokePredict(samples: predicted.map(sample))
        }
        obey(out)
        ringFollows(event)
        afterTouches(false)
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        var out: [TouchCommand] = []
        for touch in touches {
            guard let id = ids.removeValue(forKey: ObjectIdentifier(touch)) else { continue }
            if pencil.holds(touch) {
                pencil.add(coalesced(touch, in: event), sample: sample)
                pencil.lift()
            } else if let held = heldTap, heldBack(touch) {
                // Tap Assistance: a tap that drifted lands where it began
                heldTap = nil
                let x = Float(held.at.x * scale)
                let y = Float(held.at.y * scale)
                let landed = samples(of: touch, in: event).map {
                    PenSample(x: x, y: y, pressure: $0.pressure, tilt: $0.tilt, orientation: $0.orientation, timeMs: $0.timeMs, roll: $0.roll)
                }
                out += session.touchUp(id: id, samples: landed)
            } else {
                out += session.touchUp(id: id, samples: samples(of: touch, in: event))
            }
        }
        if ids.isEmpty {
            strokeTouch = nil
        }
        obey(out)
        // a touch lifted: the ring goes, until one alone moves again
        ringAt(nil)
        afterTouches(ids.isEmpty)
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        var out: [TouchCommand] = []
        for touch in touches {
            guard let id = ids.removeValue(forKey: ObjectIdentifier(touch)) else { continue }
            if heldTap?.touch == ObjectIdentifier(touch) {
                heldTap = nil
            }
            // the system took the touch back (a palm, its own gesture): what it did is undone
            if pencil.holds(touch) {
                if pencil.cancel() {
                    out += session.touchCancelOne(id: id)
                }
                penId = nil
            } else {
                out += session.touchCancelOne(id: id)
            }
        }
        if ids.isEmpty {
            strokeTouch = nil
        }
        obey(out)
        // a touch lifted: the ring goes, until one alone moves again
        ringAt(nil)
        afterTouches(ids.isEmpty)
    }

    /// True values for samples given as estimates: the stroke waiting on them goes on.
    override func touchesEstimatedPropertiesUpdated(_ touches: Set<UITouch>) {
        pencil.update(touches, sample: sample)
        pencil.release()
    }

    // MARK: the Pencil's own gestures

    /// The Pencil held over the canvas: the core follows it (the brush's outline, Hover's touch, a
    /// squeeze held meanwhile), and the outline shows as strongly as the core says.
    @objc private func pencilHovered(_ hover: UIHoverGestureRecognizer) {
        switch hover.state {
        case .began, .changed:
            let at = hover.location(in: self)
            pencilAt = at
            // a spray can's sound (a plugin's) rattles shaken above the canvas
            if StrokeSound.shared.sounding {
                session.soundHover(x: Float(at.x * scale), y: Float(at.y * scale))
                StrokeSound.shared.wake()
            }
            obey(session.touchPenHover(hover: penHover(at, height: hover.zOffset, seconds: CACurrentMediaTime())))
            // the outline turned as a stamp laid there: the way the Pencil goes, as it is held
            hoverTurn = CGFloat(session.cursorHovers(x: Float(at.x * scale), y: Float(at.y * scale), pen: heldPencil(hover)))
            showHoverCursor()
            ringAt(at)
        default:
            obey(session.touchPenHover(hover: nil))
            session.cursorHoverGone()
            cursor.hide()
            ringAt(nil)
        }
    }

    /// A trackpad's or mouse's pointer over the canvas: the brush's outline follows it (it isn't
    /// the Pencil's hover, which sets the brush with a finger).
    @objc private func pointerHovered(_ hover: UIHoverGestureRecognizer) {
        switch hover.state {
        case .began, .changed:
            // a pointer has no height over the glass: one that has is the Pencil, followed above
            guard hover.zOffset == 0 else { return }
            let at = hover.location(in: self)
            let turn = CGFloat(session.cursorHovers(x: Float(at.x * scale), y: Float(at.y * scale), pen: nil))
            showCursor(at: at, hovering: true, turn: turn)
            ringAt(at)
        default:
            session.cursorHoverGone()
            cursor.hide()
            ringAt(nil)
        }
    }

    /// The outline under the hovering Pencil, as the core has it.
    private func showHoverCursor() {
        guard let shown = session.penHoverCursor() else {
            cursor.hide()
            return
        }
        // as big as the brush, as the other apps draw it: its stamp's fill shows what the estimated
        // pressure makes of it
        let at = CGPoint(x: CGFloat(shown.x) / scale, y: CGFloat(shown.y) / scale)
        showCursor(at: at, hovering: true, alpha: CGFloat(shown.alpha), turn: hoverTurn)
    }

    /// The brush's outline at `point` (points), as long as Brush cursor shows it when hovering (or
    /// painting); none, it goes. Hovering, it is turned `turn` past the brush's own rotation;
    /// painting, it is as big and turned as the newest stamp laid, as the core has it.
    func showCursor(at point: CGPoint?, hovering: Bool, alpha: CGFloat = 1, turn: CGFloat = 0) {
        let settings = AppSettings.shared
        let wanted = hovering ? settings.cursorShows != .painting : settings.cursorShows != .hovering
        guard let asked = point, settings.brushCursorOn, wanted else {
            // (hovering again, it goes on from where it comes back)
            if point == nil {
                session.cursorHoverGone()
            }
            cursor.hide()
            return
        }
        // where a stroke's sample there lands, as a plugin may move it: the cursor stands where the stroke goes
        let landed = session.cursorPoint(x: Float(asked.x * scale), y: Float(asked.y * scale))
        let point = landed.count == 2 ? CGPoint(x: CGFloat(landed[0]) / scale, y: CGFloat(landed[1]) / scale) : asked
        if hovering {
            cursor.show(at: point, alpha: alpha, session: session, pixels: scale, drawn: settings.cursorStyle, turn: turn)
        } else {
            // read without waiting on the canvas: the newest stamp's size (pressure and taper in)
            let radius = session.cursorRadius().map { CGFloat($0) / scale }
            let turned = CGFloat(session.cursorTurn())
            cursor.show(at: point, alpha: alpha, painting: true, painted: radius, session: session, pixels: scale, drawn: settings.cursorStyle, turn: turned)
        }
    }

    /// A shape drawn snapped into place: Apple Pencil Pro feels it, where the Pencil is.
    func shapeSnapped() {
        feedback.completed(at: pencilAt ?? CGPoint(x: bounds.midX, y: bounds.midY))
    }

    /// What is moved lined up with something (a transformed box snapping onto a guide): Apple
    /// Pencil Pro feels it, where the Pencil is.
    func linedUp() {
        feedback.aligned(at: pencilAt ?? CGPoint(x: bounds.midX, y: bounds.midY))
    }

    /// How the hovering Pencil is held, as the core takes it: the way its top leans, clockwise from
    /// up (as its samples' orientation), and its barrel (Apple Pencil Pro, iPadOS 17.5; 0 else).
    private func heldPencil(_ hover: UIHoverGestureRecognizer) -> CursorPen {
        var lean = Double(hover.azimuthAngle(in: self)) + Double.pi / 2
        lean = lean.truncatingRemainder(dividingBy: 2 * Double.pi)
        if lean < 0 {
            lean += 2 * Double.pi
        }
        var roll: Float = 0
        if #available(iOS 17.5, *) {
            roll = Float(hover.rollAngle)
        }
        return CursorPen(azimuth: Float(lean), roll: roll)
    }

    /// The Pencil's double-tap, where it hovers (`hovering`, else where it last was): what Gesture
    /// controls set it to do, else what the system's own setting for it asks (`action`); that set
    /// to off, nothing.
    func pencilDoubleTapped(at hovering: CGPoint?, action: UIPencilPreferredAction) {
        if let hovering {
            pencilAt = hovering
        }
        let at = hovering ?? pencilAt ?? CGPoint(x: bounds.midX, y: bounds.midY)
        obey(session.touchPenDoubleTap(preferred: penPreferred(action), x: Float(at.x * scale), y: Float(at.y * scale)))
    }

    /// The Pencil squeezed (Apple Pencil Pro): what Gesture controls set it to do, held (the
    /// eyedropper's loupe and Layer Select follow the hovering Pencil), else what the system's own
    /// setting for a squeeze asks (`action`).
    @available(iOS 17.5, *)
    func pencilSqueezed(_ squeeze: UIPencilInteraction.Squeeze, action: UIPencilPreferredAction) {
        let phase: SqueezePhase
        switch squeeze.phase {
        case .began:
            phase = .began
        case .changed:
            phase = .changed
        case .ended:
            phase = .ended
        case .cancelled:
            phase = .cancelled
        @unknown default:
            phase = .cancelled
        }
        var hover: PenHover?
        if let pose = squeeze.hoverPose {
            pencilAt = pose.location
            hover = penHover(pose.location, height: pose.zOffset, seconds: squeeze.timestamp)
        }
        obey(session.touchPenSqueeze(phase: phase, preferred: penPreferred(action), hover: hover, timeMs: squeeze.timestamp * 1000))
        // the eyedropper's loupe stands in for the outline while it shows
        showHoverCursor()
    }

    /// The Pencil over the canvas as the core takes it: pixels, its height (0..1 of how high it is
    /// sensed), and the time in milliseconds of the clock the touches run on.
    private func penHover(_ at: CGPoint, height: CGFloat, seconds: TimeInterval) -> PenHover {
        PenHover(x: Float(at.x * scale), y: Float(at.y * scale), height: Float(height), timeMs: seconds * 1000)
    }

    /// The system's own setting for a Pencil gesture (Settings, Apple Pencil), as the core takes it.
    private func penPreferred(_ action: UIPencilPreferredAction) -> PenPreferred {
        switch action {
        case .ignore:
            return .ignore
        case .switchEraser:
            return .switchEraser
        case .switchPrevious:
            return .switchPrevious
        case .showColorPalette:
            return .showColours
        case .showInkAttributes:
            return .showInkAttributes
        default:
            // the newer choices (iPadOS 17.5) by number: a palette where the Pencil is, and a
            // shortcut the system runs itself; one still newer is the system's own too
            return action.rawValue == 5 ? .showContextualPalette : .runSystemShortcut
        }
    }

    private func kind(of touch: UITouch) -> PointerKind {
        switch touch.type {
        case .pencil:
            return .stylus
        case .indirectPointer:
            return .mouse
        default:
            return .finger
        }
    }

    /// The touch's samples in this event: those the system coalesced since the last, then the
    /// latest.
    private func samples(of touch: UITouch, in event: UIEvent?) -> [PenSample] {
        coalesced(touch, in: event).map(sample)
    }

    private func coalesced(_ touch: UITouch, in event: UIEvent?) -> [UITouch] {
        let all = event?.coalescedTouches(for: touch) ?? []
        return all.isEmpty ? [touch] : all
    }

    private func sample(_ touch: UITouch) -> PenSample {
        if touch.type == .pencil {
            pencilAt = touch.preciseLocation(in: self)
        }
        return penSample(touch, in: self, scale: scale)
    }

    /// Hands what the touches asked to the app, and waits for the touches' next deadline.
    private func obey(_ out: [TouchCommand]) {
        if !out.isEmpty {
            onCommands(out)
        }
        tick?.invalidate()
        tick = nil
        guard let due = session.touchDeadline() else { return }
        let wait = max(0, due / 1000 - CACurrentMediaTime())
        tick = Timer.scheduledTimer(withTimeInterval: wait, repeats: false) { [weak self] _ in
            guard let self else { return }
            self.obey(self.session.touchTick(nowMs: CACurrentMediaTime() * 1000))
        }
    }
}

/// One sample, as the core takes it: pixels; pressure 0..1 (the force against the most the device
/// senses; a touch with none presses fully); the lean from upright and the direction the Pencil's
/// top leans, clockwise from up (its azimuth is from the view's x axis); the time in milliseconds
/// of the clock the core's waits run on (`CACurrentMediaTime`'s); and how far the barrel is turned
/// (Apple Pencil Pro, iPadOS 17.5; 0 for the others).
func penSample(_ touch: UITouch, in view: UIView, scale: CGFloat) -> PenSample {
    let at = touch.preciseLocation(in: view)
    let pencil = touch.type == .pencil
    let force = touch.maximumPossibleForce > 0 ? touch.force / touch.maximumPossibleForce : 1
    let tilt = pencil ? Double.pi / 2 - Double(touch.altitudeAngle) : 0
    var lean = pencil ? Double(touch.azimuthAngle(in: view)) + Double.pi / 2 : 0
    lean = lean.truncatingRemainder(dividingBy: 2 * Double.pi)
    if lean < 0 {
        lean += 2 * Double.pi
    }
    var roll: Float = 0
    if #available(iOS 17.5, *), pencil {
        roll = Float(touch.rollAngle)
    }
    return PenSample(
        x: Float(at.x * scale),
        y: Float(at.y * scale),
        pressure: Float(min(max(force, 0), 1)),
        tilt: Float(tilt),
        orientation: Float(lean),
        timeMs: touch.timestamp * 1000,
        roll: roll
    )
}
