import SwiftUI
import UIKit
import UIKit.UIGestureRecognizerSubclass

/// How far past the layers panel's side a dragged row goes before it is lifted out of it.
let layersLiftReach: CGFloat = 24

/// Layers lifted out of the layers panel by the finger dragging their row, held while it goes on
/// (the other hand may go to the gallery and open another artwork) until it lets go: over the
/// layers panel of the artwork they came from they go among its rows; over the canvas they came
/// from they are copied to the top; over another artwork's canvas, above its current layer (one
/// layer ready to transform, as a picture brought in is); anywhere else (the gallery) nowhere.
/// The held row is a view on the window itself, so it outlives the screen it came from, and the
/// finger is followed from the window: a watcher there sees every touch from the moment the
/// layers panel first opened.
final class HeldLayers: NSObject, UIGestureRecognizerDelegate {
    static let shared = HeldLayers()

    /// What is held: the layers apart from the artwork they came from (its folder `from`, where
    /// they were the row of `layer`), the finger holding them, and where it holds the row (from
    /// its corner, window points).
    private struct Held {
        let bundle: LayerBundle
        let layer: UInt64
        let from: String
        let items: Int
        let touch: UITouch
        let grab: CGPoint
    }

    private var held: Held?
    /// The artwork last opened: where held layers land, while its canvas is on the window.
    weak var canvas: CanvasModel?
    /// The open layers panel's rows and list on the window, for a drop among them.
    var panelRows: [UInt64: CGRect] = [:]
    var panelBox: CGRect?
    private weak var watcher: TouchWatcher?
    private var rowView: HeldRowView?

    var holding: Bool {
        held != nil
    }

    /// Watches the touches on `window` from now on, for a finger that lifts layers out to be
    /// followed wherever it goes.
    func watch(_ window: UIWindow) {
        if let w = watcher, w.view === window {
            return
        }
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

    /// Row `l` of `model`'s layers panel dragged out past the panel's side by the finger at `at`
    /// (window points), the row's corner at `corner`: it and what a drag of it takes along (its
    /// mask, a group's layers, the others chosen with it) are lifted out to be held. Says whether
    /// they were (not on a model, nor a mask, nor with a finger the window's watcher didn't see
    /// come down).
    func lift(_ model: CanvasModel, _ l: LayerInfo, at: CGPoint, corner: CGPoint, thumbnail: UIImage?) -> Bool {
        guard held == nil, !model.scene.open, l.kind != .mask, let watcher, let window = watcher.view as? UIWindow,
              let touch = watcher.nearest(to: at, in: window),
              let bundle = model.session.liftLayers(id: l.id) else { return false }
        let made = Held(
            bundle: bundle, layer: l.id, from: model.dir, items: Int(bundle.items()), touch: touch,
            grab: CGPoint(x: at.x - corner.x, y: at.y - corner.y)
        )
        held = made
        let row = HeldRowView(name: bundle.name(), thumbnail: thumbnail, items: made.items)
        window.addSubview(row)
        rowView = row
        place(touch.location(in: window))
        return true
    }

    /// The held row under the finger at `at`, with the plus while letting go would land it.
    private func place(_ at: CGPoint) {
        guard let held, let row = rowView else { return }
        let k = Interface.shared.scale
        row.transform = CGAffineTransform(scaleX: k, y: k)
        row.center = CGPoint(
            x: at.x - held.grab.x + row.bounds.width * k / 2,
            y: at.y - held.grab.y + row.bounds.height * k / 2
        )
        row.lands = shownCanvas() != nil
    }

    private func moved(_ touch: UITouch) {
        guard let held, held.touch === touch, let window = watcher?.view else { return }
        place(touch.location(in: window))
    }

    private func ended(_ touch: UITouch, cancelled: Bool) {
        guard let held, held.touch === touch else { return }
        let at = touch.location(in: watcher?.view)
        self.held = nil
        rowView?.removeFromSuperview()
        rowView = nil
        if !cancelled {
            drop(held, at: at)
        }
    }

    /// The artwork whose canvas is on the window now, unless a model is open on it.
    private func shownCanvas() -> CanvasModel? {
        guard let canvas, !canvas.scene.open, let window = watcher?.view else { return nil }
        return Self.holds(window, canvas.session) ? canvas : nil
    }

    /// Whether `view` or a view in it is the canvas of `session`.
    private static func holds(_ view: UIView, _ session: CanvasSession) -> Bool {
        if let canvas = view as? CanvasView, canvas.session === session {
            return true
        }
        return view.subviews.contains { holds($0, session) }
    }

    /// `held` let go at `at` (window points).
    private func drop(_ held: Held, at: CGPoint) {
        guard let canvas = shownCanvas() else { return }
        let session = canvas.session
        let same = canvas.dir == held.from
        let overRows = canvas.layersShown && (panelBox?.contains(at) ?? false)
        let shown = panelRows.map { ShownRow(id: $0.key, top: Float($0.value.minY), bottom: Float($0.value.maxY)) }
        if same, overRows {
            // back among its own rows: moved there
            if let l = canvas.layers.first(where: { $0.id == held.layer }) {
                canvas.dropRow(l, canvas.rowDrop(l, rows: panelRows, y: at.y, held: nil))
            }
            return
        }
        let place: LayerDrop = overRows
            ? .row(place: session.rowDrop(id: nil, rows: shown, y: Float(at.y), held: nil))
            : same ? .top : .aboveCurrent
        if canvas.transforming {
            canvas.endTransform(commit: true)
        }
        guard session.dropLayers(bundle: held.bundle, place: place) != nil else {
            canvas.noRoomForLayers()
            return
        }
        canvas.refresh()
        canvas.refreshThumbnails()
        if place == .aboveCurrent, held.items == 1 {
            canvas.toggleTransform()
        }
    }
}

/// Sees every touch on the window without taking any: which are down and where, and the moves and
/// ends of each, for the one holding layers (or dragging brushes in the library).
final class TouchWatcher: UIGestureRecognizer {
    private var down: [UITouch] = []
    var onMove: (UITouch) -> Void = { _ in }
    var onEnd: (UITouch, Bool) -> Void = { _, _ in }

    /// The touch down nearest `at` in `view`.
    func nearest(to at: CGPoint, in view: UIView) -> UITouch? {
        down.min { a, b in
            let p = a.location(in: view)
            let q = b.location(in: view)
            return hypot(p.x - at.x, p.y - at.y) < hypot(q.x - at.x, q.y - at.y)
        }
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        down.append(contentsOf: touches)
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
        touches.forEach(onMove)
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        down.removeAll { touches.contains($0) }
        touches.forEach { onEnd($0, false) }
        settle()
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
        down.removeAll { touches.contains($0) }
        touches.forEach { onEnd($0, true) }
        settle()
    }

    /// Never recognized: with every touch gone it gives up, to start again with the next.
    private func settle() {
        if down.isEmpty {
            state = .failed
        }
    }

    override func reset() {
        super.reset()
        down = []
    }
}

/// The row held out of the layers panel, as its row looks: blue as the current row, its thumbnail
/// and name, a count if it holds several, and a green plus while letting go would land it.
private final class HeldRowView: UIView {
    private let plus = HeldRowView.badge(UIColor(red: 0x30 / 255, green: 0xD1 / 255, blue: 0x58 / 255, alpha: 1), "+")

    var lands = false {
        didSet { plus.isHidden = !lands }
    }

    init(name: String, thumbnail: UIImage?, items: Int) {
        super.init(frame: CGRect(x: 0, y: 0, width: 360, height: 69))
        isUserInteractionEnabled = false
        let row = UIView(frame: bounds)
        row.backgroundColor = UIColor(red: 0, green: 0x76 / 255, blue: 0xFD / 255, alpha: 0.95)
        row.layer.cornerRadius = 8
        row.layer.shadowColor = UIColor.black.cgColor
        row.layer.shadowOpacity = 0.35
        row.layer.shadowRadius = 12
        row.layer.shadowOffset = CGSize(width: 0, height: 4)
        addSubview(row)
        let thumb = UIImageView(frame: CGRect(x: 4, y: 4, width: 93, height: 61))
        thumb.backgroundColor = UIColor(white: 0x4F / 255, alpha: 1)
        thumb.layer.cornerRadius = 4
        thumb.clipsToBounds = true
        thumb.contentMode = .scaleAspectFit
        thumb.layer.magnificationFilter = .nearest
        thumb.image = thumbnail
        row.addSubview(thumb)
        let label = UILabel(frame: CGRect(x: 4 + 93 + 29, y: 0, width: 360 - 140, height: 69))
        label.text = name
        label.font = .systemFont(ofSize: 17)
        label.textColor = .white
        label.lineBreakMode = .byTruncatingTail
        row.addSubview(label)
        if items > 1 {
            let count = Self.badge(UIColor(red: 1, green: 0x3B / 255, blue: 0x30 / 255, alpha: 1), "\(items)")
            count.center = CGPoint(x: 4, y: 4)
            addSubview(count)
        }
        plus.center = CGPoint(x: 356, y: 4)
        plus.isHidden = true
        addSubview(plus)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    private static func badge(_ fill: UIColor, _ text: String) -> UILabel {
        let badge = UILabel(frame: CGRect(x: 0, y: 0, width: 24, height: 24))
        badge.backgroundColor = fill
        badge.layer.cornerRadius = 12
        badge.clipsToBounds = true
        badge.text = text
        badge.textAlignment = .center
        badge.font = .systemFont(ofSize: 15)
        badge.textColor = .white
        return badge
    }
}
