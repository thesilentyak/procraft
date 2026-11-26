import ImageIO
import PhotosUI
import SwiftUI
import UIKit

// the dark bars are glass, like the panels: what the picture has under them shows through, blurred
// (33 over the window's own grey, 57 over a white canvas, as measured)
private var barFill: Color { Look.shared.light ? Color(argb: 0xFFED_EDED) : Pc.onDarkFrost(0xDE1E_1E1E) }
private var pictureFill: Color { Look.shared.light ? Color(argb: 0xFFF0_F0F0) : Color(argb: 0xFF33_3333) }
private var grabLine: Color { Look.shared.light ? Color(argb: 0xFF90_9090) : Color(argb: 0x80BF_BFBF) }
private var titleInk: Color { Look.shared.light ? Color(argb: 0xFF3A_3A3A) : Color(argb: 0xFFC5_C5C5) }
private var closeFill: Color { Look.shared.light ? Color(argb: 0xFFFA_FAFA) : Color(argb: 0x9418_1818) }
private var closeEdge: Color { Look.shared.light ? Color(argb: 0xFFE0_E0E0) : Color(argb: 0x8051_5151) }
private var closeInk: Color { Look.shared.light ? Color(argb: 0xFF59_5959) : Color(argb: 0xFFA5_A5A5) }
private var segmentTrack: Color { Look.shared.light ? Color(argb: 0xFFD7_D7D7) : Color(argb: 0x17FF_FFFF) }
private var importFill: Color { Look.shared.light ? Color(argb: 0xFFD7_D7D7) : Color(argb: 0xFF4D_4D4D) }
private var muted: Color { Look.shared.light ? Color(argb: 0xFF3B_3B3B) : Color(argb: 0xFFAA_AAAA) }
private let topBarHeight: CGFloat = 64
/// Showing a picture or a face the top bar is taller: what goes with them sits under the title.
private let tallTopBar: CGFloat = 74
private let bottomBarHeight: CGFloat = 45
private let windowCorner: CGFloat = 24
private let handleSide: CGFloat = 60
/// How far down from the window's top a drag moves it, the bars shown or not.
private let grabHeight: CGFloat = 16
/// How far above the screen's foot a window first has its top, whatever the canvas's shape: where
/// a screen-shaped canvas's (532 tall with its bars) has it, 50 above the foot, as measured.
private let firstTop: CGFloat = 50 + 532
private let bars = topBarHeight + bottomBarHeight
/// The side a model's picture is read at for the window.
private let modelPixels: UInt32 = 720
/// How long between reads of the canvas for the window (each at the size the window shows it: the
/// core's referenceCanvasSide).
private let readEvery = 0.3
/// The window the eyedropper shows, points across.
private let pickWindow: CGFloat = 54
/// How far down and right of the window it was opened from another opens.
private let nextStep: CGFloat = 24

/// What the window shows: on a model, the model itself through a camera of its own.
enum ReferenceMode {
    case canvas, image, face, model
}

/// The artwork's Reference windows: its own first (Reference in the Canvas tab shows and hides
/// it), then those opened besides it while a plugin lets more than one be open. Each is kept by the
/// core with the artwork: they come back as they were left.
@Observable
final class ReferenceTool {
    private unowned let model: CanvasModel
    /// Its own first, then those opened besides it.
    private(set) var windows: [ReferencePane]

    init(model: CanvasModel) {
        self.model = model
        let count = max(model.session.referenceWindowCount(), 1)
        windows = (0 ..< count).map { ReferencePane(model: model, index: $0) }
    }

    /// The artwork's own window shows.
    var on: Bool {
        windows.first?.on == true
    }

    /// Those showing, the last opened on top.
    var shown: [ReferencePane] {
        windows.filter(\.on)
    }

    /// Whether another may be opened: as many as the plugins say may be open (one without them).
    var canAdd: Bool {
        _ = PluginsModel.shared.changes
        return windows.count < max(Int(PluginsModel.shared.store.referenceWindows()), 1)
    }

    func setOn(_ on: Bool) {
        windows.first?.setOn(on)
    }

    func setMode(_ next: ReferenceMode) {
        windows.first?.setMode(next)
    }

    func toggleBars() {
        windows.first?.toggleBars()
    }

    /// Reads the canvas again for the windows that show it.
    func refresh() {
        windows.forEach { $0.refresh() }
    }

    /// Another window opened besides those open, a little down and right of `from`, showing the
    /// canvas.
    func add(from: ReferencePane) {
        guard canAdd else { return }
        var state = from.state()
        state.shown = true
        state.shows = .canvas
        if let at = from.offset {
            state.placed = true
            state.x = Float(at.x + nextStep)
            state.y = Float(at.y + nextStep)
        } else {
            state.placed = false
        }
        state.canvasView = ReferenceView(zoom: 1, x: 0, y: 0)
        state.imageView = ReferenceView(zoom: 1, x: 0, y: 0)
        guard let index = model.session.addReferenceWindow(state: state) else { return }
        let next = ReferencePane(model: model, index: index)
        withAnimation(Motion.referenceFadeIn) { windows.append(next) }
    }

    /// `window` closed: the artwork's own hides; one opened besides it goes, with its picture, and
    /// those after it take the place before theirs (as the core numbers them).
    func close(_ window: ReferencePane) {
        guard window.index > 0 else {
            window.setOn(false)
            return
        }
        model.session.removeReferenceWindow(index: window.index)
        for w in windows where w.index > window.index {
            w.index -= 1
        }
        withAnimation(Motion.referenceFadeOut) { windows.removeAll { $0 === window } }
    }
}

/// One Reference window: where it is, what it shows, and how each picture sits in it.
@Observable
final class ReferencePane: Identifiable {
    private unowned let model: CanvasModel
    /// Its place among the artwork's windows, as the core numbers them: 0 the artwork's own.
    fileprivate(set) var index: UInt32

    private(set) var on = false
    private(set) var mode = ReferenceMode.canvas
    /// The whole canvas as it is, kept up to date while the window shows it.
    private(set) var canvas: UIImage?
    /// `canvas` is the canvas at its own size (not made smaller): its pixels are the canvas's.
    private(set) var canvasWhole = false
    private(set) var image: UIImage?
    /// How the window shows the canvas and a picture imported, each zoomed and moved on its own.
    private(set) var canvasView = ReferenceView(zoom: 1, x: 0, y: 0)
    private(set) var imageView = ReferenceView(zoom: 1, x: 0, y: 0)
    /// Where the window is, none until it is first placed; and the size of its picture.
    private(set) var offset: CGPoint?
    private(set) var size = CGSize(width: 350, height: 423)
    /// The size of its picture until it is sized: 350 wide, the whole window the canvas's shape, so
    /// the canvas fills it (the core's).
    private let first: CGSize
    /// The window's bars are slid out of the way.
    private(set) var barsHidden = false
    /// The screen the window was last laid out on, none before it first is.
    private var room: CGSize?

    @ObservationIgnored private var reading = false
    @ObservationIgnored private var stale = false
    /// The side the canvas was last read at (0: not yet).
    @ObservationIgnored private var readSide: UInt32 = 0

    init(model: CanvasModel, index: UInt32) {
        self.model = model
        self.index = index
        let info = model.session.info()
        let first = referenceWindowFirstSize(canvasWidth: info.width, canvasHeight: info.height)
        self.first = CGSize(width: CGFloat(first.width), height: CGFloat(first.height))
        size = self.first
        restore()
    }

    /// The window as the artwork left it, or none if it never had one.
    private func restore() {
        let session = model.session
        guard let w = session.referenceWindowAt(index: index) else { return }
        mode = switch w.shows {
        case .image: .image
        case .face: .face
        case .canvas: .canvas
        // only the artwork's own window looks at the model
        case .model: index == 0 ? ReferenceMode.model : ReferenceMode.canvas
        }
        if w.sized {
            size = CGSize(width: CGFloat(w.width), height: CGFloat(w.height))
        }
        if w.placed {
            offset = CGPoint(x: CGFloat(w.x), y: CGFloat(w.y))
        }
        canvasView = w.canvasView
        imageView = w.imageView
        on = w.shown
        let index = index
        DispatchQueue.global(qos: .utility).async { [weak self] in
            let picture = session.referencePictureAt(index: index).flatMap { Self.decode($0) }
            DispatchQueue.main.async {
                guard let self, let picture, self.model.session === session, self.index == index else { return }
                self.image = picture
            }
        }
        refresh()
    }

    /// The window as it now is, for the core.
    func state() -> ReferenceWindowState {
        let shows: ReferenceShows = switch mode {
        case .canvas: .canvas
        case .image: .image
        case .face: .face
        case .model: .model
        }
        return ReferenceWindowState(
            shown: on, shows: shows, sized: size != first,
            width: Float(size.width), height: Float(size.height),
            placed: offset != nil, x: Float(offset?.x ?? 0), y: Float(offset?.y ?? 0),
            canvasView: canvasView, imageView: imageView
        )
    }

    /// The window as it now is, kept with the artwork.
    private func keep() {
        model.session.setReferenceWindowAt(index: index, state: state())
    }

    /// The picture's size as the window shows it: as big as it was sized, within the window's least
    /// and most (the core's), and no bigger than the screen.
    var shownSize: CGSize {
        fitted(size)
    }

    private func fitted(_ size: CGSize) -> CGSize {
        let r = referenceWindowSize(
            width: Float(size.width), height: Float(size.height),
            screenWidth: Float(room?.width ?? .nan), screenHeight: Float(room?.height ?? .nan)
        )
        return CGSize(width: CGFloat(r.width), height: CGFloat(r.height))
    }

    /// Where the window at `at` goes to be all on the screen it was last laid out on (its top left,
    /// if it is bigger).
    private func onScreen(_ at: CGPoint) -> CGPoint {
        guard let room else { return at }
        let shown = shownSize
        let p = referenceWindowFitted(
            x: Float(at.x), y: Float(at.y), width: Float(shown.width), tall: Float(shown.height + bars),
            screenWidth: Float(room.width), screenHeight: Float(room.height)
        )
        return CGPoint(x: CGFloat(p.x), y: CGFloat(p.y))
    }

    var shown: UIImage? {
        switch mode {
        case .canvas, .model: canvas
        case .image: image
        case .face: nil
        }
    }

    var view: ReferenceView {
        mode == .image ? imageView : canvasView
    }

    private func set(_ next: ReferenceView) {
        if mode == .image {
            imageView = next
        } else {
            canvasView = next
        }
    }

    /// Shown, it fades in where it is; hidden, it fades out.
    func setOn(_ on: Bool) {
        withAnimation(on ? Motion.referenceFadeIn : Motion.referenceFadeOut) { self.on = on }
        keep()
        if on {
            refresh()
        }
    }

    func setMode(_ next: ReferenceMode) {
        mode = next
        keep()
        refresh()
    }

    /// The window laid out on `screen`: the first time in the middle, in the lower half; then where
    /// it was put, all of it on the screen (an artwork may have left it on a screen of another shape,
    /// or the screen turned or split). What the artwork keeps changes only as it is moved or sized.
    func place(on screen: CGSize) {
        if room != screen {
            room = screen
        }
        guard let at = offset else {
            let shown = shownSize
            let tall = shown.height + bars
            offset = CGPoint(x: (screen.width - shown.width) / 2, y: max(min(screen.height - firstTop, screen.height - tall), 0))
            return
        }
        let kept = onScreen(at)
        if kept != at {
            offset = kept
        }
    }

    func move(by delta: CGSize) {
        guard let at = offset else { return }
        offset = onScreen(CGPoint(x: at.x + delta.width, y: at.y + delta.height))
        keep()
    }

    /// A drag on a size handle, from the size the window shows at; from the left one the window's
    /// left edge moves.
    func resize(by delta: CGSize, fromLeft: Bool) {
        let old = shownSize
        let next = fitted(CGSize(width: old.width + (fromLeft ? -delta.width : delta.width), height: old.height + delta.height))
        if fromLeft, let at = offset {
            offset = CGPoint(x: at.x + old.width - next.width, y: at.y)
        }
        size = next
        offset = offset.map { onScreen($0) }
        keep()
        sharpen()
    }

    /// A picture read at the size the core keeps a window's picture at (no bigger: its longer side
    /// within the core's most), turned upright.
    private static func decode(_ data: Data) -> UIImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any]
        let width = (properties?[kCGImagePropertyPixelWidth] as? NSNumber)?.uint32Value ?? .max
        let height = (properties?[kCGImagePropertyPixelHeight] as? NSNumber)?.uint32Value ?? .max
        let kept = referencePictureSize(width: max(width, 1), height: max(height, 1))
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: Int(max(kept.width, kept.height)),
        ]
        return CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary).map { UIImage(cgImage: $0) }
    }

    func importPicture(_ data: Data?) {
        guard let picture = data.flatMap(Self.decode) else {
            model.showNotice("This picture can't be opened.", warning: true)
            return
        }
        image = picture
        imageView = ReferenceView(zoom: 1, x: 0, y: 0)
        keep()
        // kept with the artwork as a PNG, whatever kind of file it came from
        let session = model.session
        let index = index
        DispatchQueue.global(qos: .utility).async {
            if let png = picture.pngData() {
                session.setReferencePictureAt(index: index, png: png)
            }
        }
    }

    func clear() {
        image = nil
        imageView = ReferenceView(zoom: 1, x: 0, y: 0)
        model.session.clearReferencePictureAt(index: index)
        keep()
    }

    /// A tap on the window's picture: its bars slide away, or back.
    func toggleBars() {
        withAnimation(Motion.bars) { barsHidden.toggle() }
    }

    /// Two taps on the window's picture: it fits the window again, the bars out of the way.
    func fit() {
        set(ReferenceView(zoom: 1, x: 0, y: 0))
        withAnimation(Motion.bars) { barsHidden = true }
        keep()
    }

    /// Fingers on the window's picture moved from `from` to `to` (their middle), `spread` times as
    /// far apart: the picture zooms and moves with them.
    func pinch(_ sizes: ReferenceSizes, from: CGPoint, to: CGPoint, spread: CGFloat) {
        // the model turns under the finger, and doesn't move in the window
        if mode == .model {
            model.session.referenceModelTurn(dx: Float(to.x - from.x), dy: Float(to.y - from.y), side: sizes.windowWidth)
            refresh()
            return
        }
        set(referenceViewPinched(
            view: view, sizes: sizes, fromX: Float(from.x), fromY: Float(from.y),
            toX: Float(to.x), toY: Float(to.y), spread: Float(spread)
        ))
        keep()
        sharpen()
    }

    /// The side to read the canvas at for the window as it shows now: a pixel of it for each pixel
    /// of the screen it covers, zoomed as it is (the core's referenceCanvasSide).
    private func canvasSide() -> UInt32 {
        let shown = shownSize
        let info = model.session.info()
        return referenceCanvasSide(
            windowWidth: Float(shown.width * model.pixels), windowHeight: Float((shown.height + bars) * model.pixels),
            zoom: canvasView.zoom, canvasWidth: info.width, canvasHeight: info.height
        )
    }

    /// The canvas read again when the window, zoomed or sized, shows it at another size than it was
    /// read at.
    private func sharpen() {
        if on, mode == .canvas, readSide != 0, canvasSide() != readSide {
            refresh()
        }
    }

    /// The colour of one pixel of `image`.
    private static func rgb(of image: CGImage, x: Int, y: Int) -> UInt32? {
        guard let one = image.cropping(to: CGRect(x: x, y: y, width: 1, height: 1)) else { return nil }
        var pixel: [UInt8] = [0, 0, 0, 0]
        let space = CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB()
        let drawn: Bool = pixel.withUnsafeMutableBytes { bytes in
            guard let context = CGContext(
                data: bytes.baseAddress, width: 1, height: 1, bitsPerComponent: 8, bytesPerRow: 4,
                space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
            ) else { return false }
            context.draw(one, in: CGRect(x: 0, y: 0, width: 1, height: 1))
            return true
        }
        guard drawn else { return nil }
        return UInt32(pixel[0]) << 16 | UInt32(pixel[1]) << 8 | UInt32(pixel[2])
    }

    /// The eyedropper over the picture at `place` (`referencePicturePoint`'s: its point, and how many
    /// of its pixels a point spans), the finger at `at` on the screen: the colour there and what is
    /// around it.
    func pick(_ picture: UIImage, _ place: [Float], at: CGPoint, pen: Bool) {
        guard let image = picture.cgImage, place.count >= 3 else { return }
        let x = min(max(Int(place[0]), 0), image.width - 1)
        let y = min(max(Int(place[1]), 0), image.height - 1)
        // as much of the picture as the loupe's window shows, around the point
        let n = min(max(Int((pickWindow * CGFloat(place[2])).rounded()), 1), min(image.width, image.height))
        let left = min(max(x - n / 2, 0), image.width - n)
        let top = min(max(y - n / 2, 0), image.height - n)
        let around = image.cropping(to: CGRect(x: left, y: top, width: n, height: n)).map { UIImage(cgImage: $0) }
        model.pickFromReference(around, rgb: Self.rgb(of: image, x: x, y: y), at: at, pen: pen)
    }

    /// Reads the canvas again for the window, at most every so often while changes come.
    func refresh() {
        guard on, mode == .canvas || mode == .model else { return }
        if reading {
            stale = true
            return
        }
        reading = true
        read()
    }

    private func read() {
        stale = false
        let session = model.session
        // as sharp as the window shows it, zoomed as it is (not a few hundred pixels blown up); a
        // model through its own camera. Windows showing the canvas at one size share one read.
        let side = canvasSide()
        let ofModel = mode == .model
        DispatchQueue.global(qos: .utility).async { [weak self] in
            let read = ofModel ? session.referenceModelPicture(side: modelPixels) : session.referenceCanvas(maxSide: side)
            let picture = read.flatMap { CanvasModel.image($0) }
            let whole = read?.width == session.info().width
            DispatchQueue.main.async {
                guard let self else { return }
                if let picture {
                    self.canvas = picture
                    self.canvasWhole = whole
                    self.readSide = side
                }
                DispatchQueue.main.asyncAfter(deadline: .now() + readEvery) { [weak self] in
                    guard let self else { return }
                    if self.stale {
                        self.read()
                    } else {
                        self.reading = false
                    }
                }
            }
        }
    }
}

/// A reference window: the whole canvas as it is, or a picture to work from, floating over the
/// canvas. Its grab line drags it; the round button closes it; handles over its bottom corners size
/// it. Its bars lie over the picture, and a tap on it slides them away and back. While a plugin lets
/// more than one be open, + opens another.
struct ReferenceWindow: View {
    let model: CanvasModel
    let window: ReferencePane
    let screen: CGSize

    @State private var choosing = false
    @State private var photo: PhotosPickerItem?
    /// How far the drag under way has been told.
    @State private var dragged = CGSize.zero

    private var tool: ReferencePane {
        window
    }

    /// The picture's area and `picture`'s size, for the core's view of it.
    private static func sizes(_ area: CGSize, _ picture: UIImage) -> ReferenceSizes {
        ReferenceSizes(
            windowWidth: Float(area.width), windowHeight: Float(area.height),
            pictureWidth: Float(picture.size.width * picture.scale), pictureHeight: Float(picture.size.height * picture.scale)
        )
    }

    /// What a drag has moved since last told, in the interface's points.
    private func step(_ drag: DragGesture.Value) -> CGSize {
        let k = Interface.shared.scale
        let now = CGSize(width: drag.translation.width / k, height: drag.translation.height / k)
        let delta = CGSize(width: now.width - dragged.width, height: now.height - dragged.height)
        dragged = now
        return delta
    }

    private var picture: some View {
        let shown = tool.shown
        let view = tool.view
        let crispable = tool.mode == .canvas && tool.canvasWhole
        return Canvas { context, size in
            guard let shown else { return }
            let r = referencePictureRect(view: view, sizes: Self.sizes(size, shown))
            guard r.count >= 4 else { return }
            let rect = CGRect(x: CGFloat(r[0]), y: CGFloat(r[1]), width: CGFloat(r[2]), height: CGFloat(r[3]))
            // the canvas's pixels crisp from 100%, as on the canvas
            let crisp = crispable && rect.width >= shown.size.width * shown.scale * 0.999
            context.draw(Image(uiImage: shown).interpolation(crisp ? .none : .low), in: rect)
        }
    }

    private func touches(at: CGPoint) -> some View {
        ReferenceTouches(
            hold: Double(Gestures.shared.delay(.eyedropper)),
            canPick: tool.shown != nil,
            onTap: { tool.toggleBars() },
            onDoubleTap: { tool.fit() },
            onMove: { from, to, spread, area in
                if let shown = tool.shown {
                    tool.pinch(Self.sizes(area, shown), from: from, to: to, spread: spread)
                }
            },
            onPick: { point, area, pen in
                guard let shown = tool.shown else { return }
                let place = referencePicturePoint(
                    view: tool.view, sizes: Self.sizes(area, shown), x: Float(point.x), y: Float(point.y)
                )
                if let place {
                    tool.pick(shown, place, at: CGPoint(x: at.x + point.x, y: at.y + point.y), pen: pen)
                }
            },
            onPickEnd: {
                if model.picking != nil {
                    model.finishPick()
                }
            }
        )
    }

    @ViewBuilder private var empty: some View {
        switch tool.mode {
        case .canvas, .model:
            EmptyView()
        case .image:
            if tool.image == nil {
                Text(verbatim: "Import Image")
                    .font(.system(size: 16))
                    .foregroundStyle(muted)
                    .frame(width: 180, height: 60)
                    .background(Capsule().fill(importFill))
                    .contentShape(Capsule())
                    .onTapGesture { choosing = true }
            }
        case .face:
            Text(verbatim: "Not available on this device")
                .font(.system(size: 14))
                .foregroundStyle(muted)
                .allowsHitTesting(false)
        }
    }

    private var glass: some View {
        ZStack {
            // the interface's frost whatever the system's appearance, as the panels'
            Rectangle().fill(.ultraThinMaterial)
                .environment(\.colorScheme, Look.shared.light ? .light : .dark)
            barFill
        }
    }

    private func word(_ label: String, action: @escaping () -> Void) -> some View {
        Text(verbatim: label)
            .font(.system(size: 15))
            .foregroundStyle(Pc.accent)
            .lineLimit(1)
            .padding(.horizontal, 3)
            .frame(height: 30)
            .contentShape(Rectangle())
            .onTapGesture(perform: action)
    }

    /// The top bar's height: taller for a picture or a face.
    private var topTall: CGFloat {
        tool.mode == .image || tool.mode == .face ? tallTopBar : topBarHeight
    }

    private var topBar: some View {
        ZStack(alignment: .topLeading) {
            Text(verbatim: "Reference")
                .font(.system(size: 23, weight: .bold))
                .foregroundStyle(titleInk)
                .lineLimit(1)
                .padding(.leading, 20)
                .padding(.top, 10)
            // the picture's Clear and Import under the title
            if tool.mode == .image, tool.image != nil {
                HStack(spacing: 0) {
                    word("Clear") { tool.clear() }
                    word("Import") { choosing = true }
                }
                .padding(.leading, 12)
                .padding(.top, 39.5)
            }
            HStack(spacing: 0) {
                Spacer(minLength: 0)
                // another window, while a plugin lets more be open
                if model.reference.canAdd {
                    PcIconView(PcIcons.add, size: 28, color: closeInk)
                        .frame(width: 43, height: 43)
                        .background(Circle().fill(closeFill))
                        .overlay(Circle().strokeBorder(closeEdge, lineWidth: 1))
                        .contentShape(Circle())
                        .onTapGesture { model.reference.add(from: tool) }
                        .padding(.trailing, 8)
                }
                PcIconView(PcIcons.cross, size: 20, color: closeInk)
                    .frame(width: 43, height: 43)
                    .background(Circle().fill(closeFill))
                    .overlay(Circle().strokeBorder(closeEdge, lineWidth: 1))
                    .contentShape(Circle())
                    .onTapGesture { model.reference.close(tool) }
            }
            .padding(.trailing, 10)
            .frame(height: topBarHeight)
        }
        .frame(height: topTall, alignment: .top)
        .background(glass)
        // touches here go no further: not to the picture under it
        .contentShape(Rectangle())
        .onTapGesture {}
    }

    private func segment(_ mode: ReferenceMode, _ label: String) -> some View {
        let on = tool.mode == mode
        return Text(verbatim: label)
            .font(.system(size: 15))
            .foregroundStyle(on ? Color.white : titleInk)
            .lineLimit(1)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Capsule().fill(on ? Pc.accent : Color.clear))
            .contentShape(Capsule())
            .onTapGesture { tool.setMode(mode) }
    }

    private var bottomBar: some View {
        HStack(spacing: 11) {
            segment(.canvas, "Canvas")
            segment(.image, "Image")
            // on a model the third is the model itself, in the artwork's own window
            if model.scene.open, tool.index == 0 {
                segment(.model, "3D")
            } else {
                segment(.face, "Face")
            }
        }
        .padding(2)
        .frame(width: 270, height: 35)
        .background(Capsule().fill(segmentTrack))
        .frame(maxWidth: .infinity)
        .frame(height: bottomBarHeight)
        .background(glass)
        .contentShape(Rectangle())
        .onTapGesture {}
    }

    /// The grab line stays when the bars go, and dragging by it moves the window.
    private var grab: some View {
        Capsule()
            .fill(grabLine)
            .frame(width: 40, height: 4)
            .padding(.top, 4)
            .frame(maxWidth: .infinity)
            .frame(height: grabHeight, alignment: .top)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 1, coordinateSpace: .global)
                    .onChanged { tool.move(by: step($0)) }
                    .onEnded { _ in dragged = .zero }
            )
    }

    /// A size handle, unseen, centred 12 in from the window's bottom corner and 15 up.
    private func handle(left: Bool, width: CGFloat, tall: CGFloat) -> some View {
        Color.clear
            .frame(width: handleSide, height: handleSide)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 1, coordinateSpace: .global)
                    .onChanged { tool.resize(by: step($0), fromLeft: left) }
                    .onEnded { _ in dragged = .zero }
            )
            .offset(x: left ? 12 - handleSide / 2 : width - 12 - handleSide / 2, y: tall - 15 - handleSide / 2)
    }

    var body: some View {
        let shown = tool.shownSize
        let width = shown.width
        let tall = bars + shown.height
        let hidden = tool.barsHidden
        let at = tool.offset ?? .zero
        let shape = RoundedRectangle(cornerRadius: windowCorner, style: .circular)
        ZStack(alignment: .topLeading) {
            ZStack(alignment: .top) {
                pictureFill
                picture
                touches(at: at)
                empty
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                topBar
                    .offset(y: hidden ? -topTall : 0)
                    .allowsHitTesting(!hidden)
                bottomBar
                    .offset(y: hidden ? bottomBarHeight + 1 : 0)
                    .frame(maxHeight: .infinity, alignment: .bottom)
                    .allowsHitTesting(!hidden)
                grab
            }
            .frame(width: width, height: tall)
            .clipShape(shape)
            .background(shape.fill(Color.black).shadow(color: Pc.popoverShadow, radius: 18))
            handle(left: true, width: width, tall: tall)
            handle(left: false, width: width, tall: tall)
        }
        .frame(width: width, height: tall, alignment: .topLeading)
        .offset(x: at.x, y: at.y)
        .onAppear { tool.place(on: screen) }
        .onChange(of: screen) { _, screen in tool.place(on: screen) }
        .photosPicker(isPresented: $choosing, selection: $photo, matching: .images)
        .onChange(of: photo) { _, item in
            guard let item else { return }
            photo = nil
            Task { @MainActor in
                tool.importPicture(try? await item.loadTransferable(type: Data.self))
            }
        }
    }
}

/// Touches on the window's picture: a tap, then two; a finger moves it and two zoom and move it; a
/// finger held still for the eyedropper's delay picks a colour from it, following the finger until
/// it lets go. They never reach the canvas under the window.
private struct ReferenceTouches: UIViewRepresentable {
    let hold: Double
    let canPick: Bool
    let onTap: () -> Void
    let onDoubleTap: () -> Void
    /// Their middle from, to, how many times as far apart, and the picture's area.
    let onMove: (CGPoint, CGPoint, CGFloat, CGSize) -> Void
    /// Where it picks, the picture's area, and whether the Pencil (not a finger) holds it.
    let onPick: (CGPoint, CGSize, Bool) -> Void
    let onPickEnd: () -> Void

    final class Coordinator: NSObject, UIGestureRecognizerDelegate {
        var touches: ReferenceTouches
        let view = UIView()
        private let held = UILongPressGestureRecognizer()
        private let pinch = UIPinchGestureRecognizer()
        private let pan = UIPanGestureRecognizer()
        private var last = CGPoint.zero
        private var fingers = 0
        /// The hold for the eyedropper is the Pencil's.
        private var pencil = false

        init(_ touches: ReferenceTouches) {
            self.touches = touches
            super.init()
            let two = UITapGestureRecognizer(target: self, action: #selector(Coordinator.tappedTwice))
            two.numberOfTapsRequired = 2
            let one = UITapGestureRecognizer(target: self, action: #selector(Coordinator.tapped))
            // it waits to see whether a second comes
            one.require(toFail: two)
            pan.addTarget(self, action: #selector(Coordinator.moved(_:)))
            pinch.addTarget(self, action: #selector(Coordinator.moved(_:)))
            held.addTarget(self, action: #selector(Coordinator.picked(_:)))
            pan.delegate = self
            pinch.delegate = self
            held.delegate = self
            for recognizer in [two, one, pan, pinch, held] as [UIGestureRecognizer] {
                view.addGestureRecognizer(recognizer)
            }
        }

        func update(_ next: ReferenceTouches) {
            touches = next
            held.minimumPressDuration = max(next.hold, 0.05)
            held.isEnabled = next.canPick
        }

        /// Whether the touch the hold takes is the Pencil's, for the loupe to sit on its tip.
        func gestureRecognizer(_ g: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
            if g === held {
                pencil = touch.type == .pencil
            }
            return true
        }

        /// A pan and a pinch go together; nothing else does.
        func gestureRecognizer(_ a: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith b: UIGestureRecognizer) -> Bool {
            (a === pan && b === pinch) || (a === pinch && b === pan)
        }

        @objc func tapped() {
            touches.onTap()
        }

        @objc func tappedTwice() {
            touches.onDoubleTap()
        }

        @objc func moved(_ recognizer: UIGestureRecognizer) {
            let at = recognizer.location(in: view)
            switch recognizer.state {
            case .began:
                last = at
                fingers = recognizer.numberOfTouches
            case .changed:
                // a finger came or went: their middle jumps, which nothing moved
                if recognizer.numberOfTouches != fingers {
                    fingers = recognizer.numberOfTouches
                    last = at
                }
                let spread = pinch.state == .changed ? pinch.scale : 1
                pinch.scale = 1
                touches.onMove(last, at, spread, view.bounds.size)
                last = at
            default:
                break
            }
        }

        @objc func picked(_ recognizer: UILongPressGestureRecognizer) {
            switch recognizer.state {
            case .began, .changed:
                touches.onPick(recognizer.location(in: view), view.bounds.size, pencil)
            case .ended, .cancelled, .failed:
                touches.onPickEnd()
            default:
                break
            }
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    func makeUIView(context: Context) -> UIView {
        context.coordinator.update(self)
        return context.coordinator.view
    }

    func updateUIView(_: UIView, context: Context) {
        context.coordinator.update(self)
    }
}
