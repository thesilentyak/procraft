import Observation
import SwiftUI
import UIKit

/// The gallery zooming about an artwork's cell into its canvas as it opens (0.4 s), and back out of
/// it as it closes (0.5 s), both easing in and out, as measured.
extension Motion {
    static let galleryOpen = easeInOut(0.4)
    static let galleryClose = easeInOut(0.5)
}

/// How much of the opening zoom's end the gallery fades over (the canvas showing through), and of
/// the closing zoom's start the canvas's picture does.
private let openFade = 0.35
private let closeFade = 0.3
/// The opening zoom's fade, starting where its eased progress has come `1 - openFade` of the way.
private let openFadeAfter = Duration.milliseconds(240)
private let openFadeTime = 0.16
/// A canvas not drawn when the gallery would fade keeps the grown artwork over it until it is, for
/// at most this long; then that fades.
private let drawnWait = Duration.seconds(4)
private let coverFade = 0.15
/// Closing, how long the gallery has to lay the artwork's cell out before the canvas's picture
/// just fades where it is.
private let cellWait = Duration.milliseconds(300)
private let poll = Duration.milliseconds(16)
/// How big the picture of the canvas fading away is, at most, pixels.
private let closingPictureSide: UInt32 = 768

/// Where a canvas is on the window, points: its middle, its size as drawn (upright, not turned), how
/// far its view turns it (radians, clockwise) and whether it shows mirrored.
struct CanvasPlace {
    let centre: CGPoint
    let size: CGSize
    let angle: CGFloat
    let mirrored: Bool

    /// The canvas `width` x `height` pixels as the view `m` shows it (as `viewMatrix` gives it, in
    /// the window's pixels, `pixels` to its point).
    init?(width: UInt32, height: UInt32, matrix m: [Float], pixels: CGFloat) {
        guard m.count >= 6, pixels > 0, width > 0, height > 0 else { return nil }
        let a = m.map { CGFloat($0) }
        let w = CGFloat(width)
        let h = CGFloat(height)
        // the canvas's down axis on the window gives its scale and turn whether or not it is
        // mirrored (a mirror turns its across axis round)
        let scale = hypot(a[2], a[3]) / pixels
        let x = (a[0] * w / 2 + a[2] * h / 2 + a[4]) / pixels
        let y = (a[1] * w / 2 + a[3] * h / 2 + a[5]) / pixels
        guard scale.isFinite, scale > 0, x.isFinite, y.isFinite else { return nil }
        centre = CGPoint(x: x, y: y)
        size = CGSize(width: w * scale, height: h * scale)
        angle = atan2(-a[2], a[3])
        mirrored = a[0] * a[3] - a[1] * a[2] < 0
    }
}

/// The square a gallery cell at `cell` fits an artwork's picture in, and the picture `ratio` (width
/// over height, as the gallery shows it) wide fitted in its middle: the cell's thumbnail box is 39
/// under its top and 82 shorter than it, the square a point short of the box's height.
func galleryPicture(in cell: CGRect, ratio: CGFloat) -> CGRect {
    let box = cell.height - 82
    let side = box - 1
    let square = CGRect(x: cell.midX - side / 2, y: cell.minY + 39 + (box - side) / 2, width: side, height: side)
    guard ratio > 0, ratio.isFinite else { return square }
    let width = ratio <= 1 ? side * ratio : side
    let height = width / ratio
    return CGRect(x: square.midX - width / 2, y: square.midY - height / 2, width: width, height: height)
}

/// Where the gallery is drawn part way between an artwork's cell and its canvas: its cell's middle
/// is at `centre`, the gallery `scale` times bigger, turned `angle` (radians, clockwise) and across
/// `across` (1, or -1 mirrored).
private struct ZoomPlace {
    let centre: CGPoint
    let scale: CGFloat
    let angle: CGFloat
    let across: CGFloat
}

/// The gallery zooming into an artwork as it opens, or out of it as it closes: the artwork's picture
/// in its cell and where its canvas is (window points); the cell shows the artwork turned `turns`
/// quarter turns clockwise. Closing, a picture of the canvas fades away over the cell, and the cell
/// is found once the gallery is laid out (the picture's width over height as it will show).
struct GalleryZoom {
    let id = UUID()
    let dir: String
    var cell: CGRect?
    let canvas: CanvasPlace
    let opening: Bool
    let turns: Int
    var picture: UIImage?
    var ratio: CGFloat = 1

    private var cellTurn: CGFloat {
        CGFloat(((turns % 4) + 4) % 4) * .pi / 2
    }

    /// `t` of the way from the cell (0) to the canvas (1).
    fileprivate func place(_ t: CGFloat, cell: CGRect) -> ZoomPlace {
        let upright = turns % 2 == 1 ? CGSize(width: cell.height, height: cell.width) : cell.size
        let full = upright.height > 0 ? canvas.size.height / upright.height : 1
        // from the cell's turn to the canvas's, the shorter way round
        var angle = canvas.angle - cellTurn
        while angle > .pi {
            angle -= 2 * .pi
        }
        while angle < -.pi {
            angle += 2 * .pi
        }
        let centre = CGPoint(x: cell.midX + (canvas.centre.x - cell.midX) * t, y: cell.midY + (canvas.centre.y - cell.midY) * t)
        // a mirrored canvas: the cell turns over on the way
        return ZoomPlace(centre: centre, scale: 1 + (full - 1) * t, angle: angle * t, across: canvas.mirrored ? 1 - 2 * t : 1)
    }

    /// What draws the gallery `t` of the way to the canvas: its cell's middle on the place's,
    /// scaled, turned and mirrored about it. Nothing moves with no cell.
    fileprivate func transform(_ t: CGFloat) -> CGAffineTransform {
        guard let cell else { return .identity }
        let p = place(t, cell: cell)
        guard p.scale.isFinite, p.scale > 0 else { return .identity }
        return CGAffineTransform(translationX: p.centre.x, y: p.centre.y)
            .rotated(by: p.angle)
            .scaledBy(x: p.scale * p.across, y: p.scale)
            .translatedBy(x: -cell.midX, y: -cell.midY)
    }

    /// Closing, the canvas's picture `t` of the way from its canvas (1) to the cell (0): its
    /// middle, its size upright, its turn (with the cell's) and across, and how much shows (all of it
    /// at first, gone over the zoom's first part). With no cell it stays where the canvas was.
    fileprivate func pictureShown(_ t: CGFloat) -> (centre: CGPoint, size: CGSize, angle: CGFloat, across: CGFloat, alpha: Double) {
        let alpha = min(max(1 - (1 - Double(t)) / closeFade, 0), 1)
        guard let cell else {
            return (canvas.centre, canvas.size, canvas.angle, canvas.mirrored ? -1 : 1, alpha)
        }
        let p = place(t, cell: cell)
        let upright = turns % 2 == 1 ? CGSize(width: cell.height, height: cell.width) : cell.size
        return (p.centre, CGSize(width: upright.width * p.scale, height: upright.height * p.scale), p.angle + cellTurn, p.across, alpha)
    }
}

/// The gallery's zooms into an artwork and out of it: the one under way, how far it has come (0 the
/// cell, 1 the canvas) and how much of the gallery shows meanwhile.
@Observable
final class GalleryZoomer {
    static let shared = GalleryZoomer()

    private(set) var zoom: GalleryZoom?
    private(set) var at: CGFloat = 0
    private(set) var galleryShown: Double = 1

    /// The artwork in `dir` opens in `session` (not yet on the window), in a window `window` points
    /// big with `pixels` to its point: the gallery zooms about its cell into where its canvas will
    /// be, then fades, the canvas showing through. A canvas not drawn by then (a big artwork) stays
    /// covered by the grown artwork until it is. Nothing zooms for an artwork with no cell showing.
    func open(_ session: CanvasSession, dir: String, window: CGSize, pixels: CGFloat) {
        guard let shown = GalleryModel.shared.thumbAt[dir], window.width > 0, window.height > 0 else { return }
        let width = UInt32(max((window.width * pixels).rounded(), 1))
        let height = UInt32(max((window.height * pixels).rounded(), 1))
        let info = session.info()
        guard let canvas = CanvasPlace(
            width: info.width, height: info.height, matrix: session.openingViewMatrix(width: width, height: height), pixels: pixels
        ) else { return }
        let turns = GalleryModel.shared.artworks.first { $0.dir == dir }?.turns ?? session.shownTurns()
        let z = GalleryZoom(dir: dir, cell: Interface.shared.onWindow(shown), canvas: canvas, opening: true, turns: Int(turns))
        at = 0
        galleryShown = 1
        zoom = z
        withAnimation(Motion.galleryOpen) { at = 1 }
        // (the session looked at, not held: an artwork left before its zoom ends lets it go)
        Task { @MainActor [self, weak session] in
            try? await Task.sleep(for: openFadeAfter)
            guard zoom?.id == z.id else { return }
            if await Self.drawn(session) {
                withAnimation(Motion.easeOut(openFadeTime)) { galleryShown = 0 }
                try? await Task.sleep(for: .seconds(openFadeTime))
            } else {
                let until = ContinuousClock.now + drawnWait
                while ContinuousClock.now < until, zoom?.id == z.id {
                    try? await Task.sleep(for: poll)
                    if await Self.drawn(session) {
                        break
                    }
                }
                guard zoom?.id == z.id else { return }
                withAnimation(Motion.easeInOut(coverFade)) { galleryShown = 0 }
                try? await Task.sleep(for: .seconds(coverFade))
            }
            end(z)
        }
    }

    /// The canvas of `session` (folder `dir`) is being left, `pixels` to the window's point: read
    /// while it is still on the window, where it is and a picture of it. Once the gallery shows the
    /// artwork's cell it zooms out of it, the picture fading away over its first part; with no cell
    /// (an artwork scrolled away) the picture just fades where it is.
    func close(_ session: CanvasSession, dir: String, pixels: CGFloat) {
        let info = session.info()
        guard let canvas = CanvasPlace(width: info.width, height: info.height, matrix: session.viewMatrix(), pixels: pixels) else {
            zoom = nil
            return
        }
        let turns = Int(session.shownTurns())
        let upright = CGFloat(info.width) / CGFloat(max(info.height, 1))
        var z = GalleryZoom(dir: dir, cell: nil, canvas: canvas, opening: false, turns: turns)
        z.ratio = turns % 2 == 1 ? 1 / upright : upright
        z.picture = session.canvasThumbnail(maxSide: closingPictureSide).flatMap { CanvasModel.image($0) }
        at = 1
        galleryShown = 0
        zoom = z
        Task { @MainActor [self] in
            // the gallery laid out (and scrolled back where it was) before its cell is looked for
            let until = ContinuousClock.now + cellWait
            var cell: CGRect?
            while ContinuousClock.now < until, zoom?.id == z.id {
                try? await Task.sleep(for: poll)
                if let shown = GalleryModel.shared.cellAt[dir] {
                    cell = galleryPicture(in: Interface.shared.onWindow(shown), ratio: z.ratio)
                    break
                }
            }
            guard zoom?.id == z.id else { return }
            zoom?.cell = cell
            if cell != nil {
                galleryShown = 1
                withAnimation(Motion.galleryClose) { at = 0 }
            } else {
                withAnimation(Motion.galleryClose) {
                    at = 0
                    galleryShown = 1
                }
            }
            try? await Task.sleep(for: .milliseconds(500))
            end(z)
        }
    }

    private func end(_ z: GalleryZoom) {
        guard zoom?.id == z.id else { return }
        zoom = nil
        at = 0
        galleryShown = 1
    }

    /// Whether the canvas of `session` has drawn a frame (or can't, or is gone), asked off the main
    /// thread: its first frame may hold the session a while.
    private static func drawn(_ session: CanvasSession?) async -> Bool {
        guard let session else { return true }
        return await withCheckedContinuation { done in
            DispatchQueue.global(qos: .userInitiated).async {
                let info = session.info()
                done.resume(returning: info.frames > 0 || info.error != nil)
            }
        }
    }
}

/// The gallery as a zoom draws it, `t` of the way from its cell (0) to the canvas (1).
struct ZoomedGallery: ViewModifier, Animatable {
    var t: CGFloat
    let zoom: GalleryZoom?

    var animatableData: CGFloat {
        get { t }
        set { t = newValue }
    }

    func body(content: Content) -> some View {
        // always the same modifier, so the gallery keeps what it holds whether or not it zooms
        content.transformEffect(zoom?.transform(t) ?? .identity)
    }
}

/// Closing, the canvas's picture fading away as it shrinks into its cell, `t` of the way from the
/// cell (0) to the canvas (1).
struct ClosingPicture: View, Animatable {
    var t: CGFloat
    let zoom: GalleryZoom

    var animatableData: CGFloat {
        get { t }
        set { t = newValue }
    }

    var body: some View {
        let shown = zoom.pictureShown(t)
        ZStack(alignment: .topLeading) {
            if let picture = zoom.picture {
                Image(uiImage: picture)
                    .resizable()
                    .frame(width: shown.size.width, height: shown.size.height)
                    .scaleEffect(x: shown.across, y: 1)
                    .rotationEffect(.radians(shown.angle))
                    .position(shown.centre)
                    .opacity(shown.alpha)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .allowsHitTesting(false)
    }
}
