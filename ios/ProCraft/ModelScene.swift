import SwiftUI
import UIKit

/// An environment the Lighting Studio lists: its name, its panorama (a file in the app's
/// environments folder) and its picture.
struct ModelEnvironment {
    let label: String
    let asset: String
    let icon: PcIcon
}

/// The environments to light a model by, as the Lighting Studio lists them.
let modelEnvironments = [
    ModelEnvironment(label: "Studio", asset: "studio", icon: PcIcons.envStudio),
    ModelEnvironment(label: "Coast", asset: "coast", icon: PcIcons.envCoast),
    ModelEnvironment(label: "Auditorium", asset: "auditorium", icon: PcIcons.envAuditorium),
    ModelEnvironment(label: "City", asset: "city", icon: PcIcons.envCity),
    ModelEnvironment(label: "Nightlife", asset: "nightlife", icon: PcIcons.envNightlife),
    ModelEnvironment(label: "Portside", asset: "portside", icon: PcIcons.envPortside),
    ModelEnvironment(label: "Industrial", asset: "industrial", icon: PcIcons.envIndustrial),
    ModelEnvironment(label: "Sunrise", asset: "sunrise", icon: PcIcons.envSunrise),
    ModelEnvironment(label: "Beach", asset: "beach", icon: PcIcons.envBeach),
    ModelEnvironment(label: "Mountain", asset: "mountain", icon: PcIcons.envMountain),
    ModelEnvironment(label: "Daytime", asset: "daytime", icon: PcIcons.envDaytime),
]

/// The environment a model is lit by unless another is chosen.
let defaultModelEnvironment = "mountain"

/// A model's turntable: its frame rate, and its sizes (720p, 1080p, 4K, Square).
let turntableFps = 30
let turntableSizes: [(width: Int, height: Int)] = [(1280, 720), (1920, 1080), (3840, 2160), (1080, 1080)]

/// The side the pictures of a model's meshes and channels are read at, in pixels.
private let modelThumbSide: UInt32 = 192

/// The model an artwork paints on, if it is one: its texture sets and meshes, the channel painted,
/// how it is lit, and a picture being placed on it. The core keeps all of it with the artwork;
/// this reads it for the interface and changes it through the core.
@Observable
final class ModelScene {
    private unowned let model: CanvasModel

    /// The artwork paints on a model: a finger turns it.
    private(set) var open = false
    /// The model's texture sets (by material), and the one painted on: its layers show.
    private(set) var textureSets: [String] = []
    private(set) var textureSet = 0
    /// The texture set painted on shows flat, as a canvas, in place of the model.
    private(set) var flat = false
    /// Strokes on the model reach every surface under them, not only the nearest.
    private(set) var paintThrough = false
    /// The model's meshes, each in its texture set; which is open in the layers panel; which are
    /// hidden, locked and picked (the picked alone take strokes; none: the whole texture set).
    private(set) var submeshes: [SubmeshInfo] = []
    private(set) var openSubmesh: Int?
    private(set) var hiddenMeshes: [Bool] = []
    private(set) var lockedMeshes: [Bool] = []
    private(set) var activeMeshes: [Int] = []
    private(set) var meshThumbnails: [Int: UIImage] = [:]
    /// The channel painted: 0 its colour, 1 roughness, 2 metalness.
    private(set) var channel = 0
    /// A channel's tile is chosen (blue, the layer's row not): painting goes to that channel of the
    /// current layer.
    private(set) var tileChosen = false
    /// Each channel's own colour (hue, saturation, brightness), kept while another is painted.
    @ObservationIgnored private var channelColors: [[Float]?] = [nil, nil, nil]
    /// The layer whose channels show under its row in the layers panel, if one's do; their pictures
    /// by "channel-layer".
    private(set) var trayFor: UInt64?
    private(set) var channelThumbnails: [String: UIImage] = [:]

    /// The Lighting Studio is open over the model.
    private(set) var studioOpen = false
    /// The environment the model is lit by, how bright, and whether it shows behind it.
    private(set) var environment = defaultModelEnvironment
    private(set) var exposure: Float = 1
    private(set) var environmentShown = true
    /// The lights the model is lit by, as the studio shows them.
    private(set) var lights: [ModelLight] = []
    /// The lighting as the studio found it, put back by Cancel.
    @ObservationIgnored private var kept: (environment: String, exposure: Float, shown: Bool, lights: [ModelLight])?

    /// A picture is being placed on the model.
    private(set) var placing = false
    /// What a model newly opened warns of, while its dialog shows.
    var warnings: [String] = []
    /// The model's turntable being set up to share, its settings and its size.
    var animatedExport: AnimatedFormat?
    var turntable = Turntable(swing: false, seconds: 2, zoom: 0.5, ease: 1, environment: true)
    var turntableSize = 0

    init(model: CanvasModel) {
        self.model = model
    }

    private var session: CanvasSession { model.session }

    /// The model shows (not its texture flat): what lands on the canvas lands on it.
    var shown: Bool { open && !flat }

    /// Reads the model as the artwork opened with it: its sets, meshes, channel and lighting.
    func start() {
        let info = session.modelInfo()
        open = info != nil
        textureSets = info?.textureSets ?? []
        textureSet = Int(session.textureSet())
        flat = session.modelFlat()
        paintThrough = session.paintThrough()
        submeshes = info?.submeshes ?? []
        readMeshes()
        openSubmesh = nil
        meshThumbnails = [:]
        session.setSelectionXRay(on: AppSettings.shared.selectionXRay)
        warnings = session.takeModelWarnings().map { modelWarningName(warning: $0) }
        channel = Int(session.modelChannel())
        tileChosen = false
        channelColors = [nil, nil, nil]
        trayFor = nil
        channelThumbnails = [:]
        exposure = session.modelExposure()
        environmentShown = session.modelEnvironmentShown()
        lights = session.modelLights()
        studioOpen = false
        // lit by the environment it was last lit by, else the default
        guard open else { return }
        let last = session.modelEnvironment()
        chooseEnvironment(modelEnvironments.contains { $0.asset == last } ? last ?? defaultModelEnvironment : defaultModelEnvironment)
    }

    // MARK: showing it

    func setFlat(_ on: Bool) {
        session.setModelFlat(on: on)
        flat = session.modelFlat()
        model.refreshView()
    }

    func setPaintThrough(_ on: Bool) {
        session.setPaintThrough(on: on)
        paintThrough = session.paintThrough()
    }

    /// Whether a selection drawn on the model reaches every surface under it, kept between artworks.
    func toggleXRay() {
        let on = !AppSettings.shared.selectionXRay
        AppSettings.shared.selectionXRay = on
        session.setSelectionXRay(on: on)
    }

    // MARK: meshes and texture sets

    /// Opens mesh `index` in the layers panel (its texture set's layers under it), or closes it.
    func openMesh(_ index: Int) {
        if openSubmesh == index {
            openSubmesh = nil
            return
        }
        guard submeshes.indices.contains(index) else { return }
        openSubmesh = index
        chooseTextureSet(Int(submeshes[index].textureSet))
        session.pickSubmesh(index: UInt32(index))
        readMeshes()
    }

    /// Hides mesh `index` (or shows it again), saying which.
    func toggleHidden(_ index: Int) {
        let hide = hiddenMeshes.indices.contains(index) ? !hiddenMeshes[index] : true
        session.setSubmeshHidden(index: UInt32(index), hidden: hide)
        readMeshes()
        model.showDone(hide ? "Hide Submesh" : "Show Submesh")
    }

    /// Locks mesh `index` (or unlocks it), saying which.
    func toggleLocked(_ index: Int) {
        let lock = lockedMeshes.indices.contains(index) ? !lockedMeshes[index] : true
        session.setSubmeshLocked(index: UInt32(index), locked: lock)
        readMeshes()
        model.showDone(lock ? "Lock mesh" : "Unlock mesh")
    }

    /// Adds mesh `index` to those picked, or takes it away; meshes picked share a texture set.
    func pickMore(_ index: Int) {
        if session.pickMoreSubmeshes(index: UInt32(index)) {
            readMeshes()
        } else {
            model.showNotice("Those meshes aren't in the same texture set", warning: true)
        }
    }

    /// What is hidden, locked and picked, as the core has it (after undo and redo too).
    func readMeshes() {
        hiddenMeshes = session.submeshesHidden()
        lockedMeshes = session.submeshesLocked()
        activeMeshes = session.activeSubmeshes().map { Int($0) }
    }

    /// Layer Select at window pixel (`x`, `y`) on the model: the mesh there is picked and opened.
    func pickAt(x: Float, y: Float) {
        guard let said = session.pickSubmeshAt(x: x, y: y) else { return }
        readMeshes()
        openSubmesh = activeMeshes.first
        textureSet = Int(session.textureSet())
        model.refresh()
        model.showNotice(said)
    }

    /// Paints on texture set `index` from now on: the layers become its layers.
    func chooseTextureSet(_ index: Int) {
        guard index != textureSet, textureSets.indices.contains(index) else { return }
        // a texture set is opened on its colour, no tile chosen
        chooseChannel(0)
        trayFor = nil
        session.chooseTextureSet(index: UInt32(index))
        tileChosen = session.channelTileChosen()
        textureSet = index
        model.refresh()
        model.refreshThumbnails()
    }

    /// Draws each mesh's picture again, as the model is seen now, off the main thread.
    func refreshMeshThumbnails() {
        guard open else { return }
        let session = session
        let count = submeshes.count
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            var read: [Int: UIImage] = [:]
            for i in 0 ..< count {
                if let t = session.submeshThumbnail(index: UInt32(i), side: modelThumbSide), let image = CanvasModel.image(t) {
                    read[i] = image
                }
            }
            DispatchQueue.main.async {
                guard let self, self.model.session === session else { return }
                self.meshThumbnails.merge(read) { _, new in new }
            }
        }
    }

    /// The name a dialog about the meshes strokes are meant for gives them.
    var meantMeshName: String {
        session.meantMeshName() ?? ""
    }

    /// Whether deleting a layer takes it from several meshes (they share the texture set's layers).
    var layersShared: Bool {
        open && submeshes.filter { Int($0.textureSet) == textureSet }.count > 1
    }

    // MARK: channels

    /// Paints on `channel` of the texture set from now on (its layers stay the layers), with that
    /// channel's own colour: roughness and metalness start black.
    func chooseChannel(_ next: Int) {
        guard next != channel, (0 ... 2).contains(next) else { return }
        session.setModelChannel(channel: UInt32(next))
        follow()
    }

    /// Chooses a layer's channel tile: `next` painted from now on (an undo step).
    func chooseTile(_ next: Int, of l: LayerInfo) {
        guard (0 ... 2).contains(next) else { return }
        if !l.current {
            model.selectLayer(l.id)
        }
        session.chooseChannelTile(channel: UInt32(next))
        follow()
    }

    /// A layer's row tapped: the layer itself (its colour) is painted, no tile chosen (an undo step).
    func leaveTiles() {
        session.leaveChannelTiles()
        follow()
    }

    /// Shows layer `id`'s channels under its row (or hides them), drawn afresh.
    func toggleTray(_ id: UInt64) {
        trayFor = trayFor == id ? nil : id
        if trayFor == id {
            refreshChannelThumbnails(id)
        }
    }

    /// Layer `l`'s content in `next` replaced by a picture, stretched over the texture.
    func replaceMap(_ l: LayerInfo, channel next: Int, with data: Data?) {
        guard let data, let image = GalleryModel.decode(data, p3: CanvasSpace.shared.p3)?.image else {
            model.showNotice("This picture can't be opened.", warning: true)
            return
        }
        if session.replaceMaterialMap(id: l.id, channel: UInt32(next), image: image) {
            model.showDone("Replace Material Map Content")
        }
        follow()
        refreshChannelThumbnails(l.id)
        refreshMeshThumbnails()
        model.refresh()
        model.refreshThumbnails()
    }

    /// The channel and tile as the core has them now (a tile chosen, or its choice undone or
    /// redone), the channel's own colour shown.
    func follow() {
        guard open else { return }
        tileChosen = session.channelTileChosen()
        let now = Int(session.modelChannel())
        guard now != channel else { return }
        if channelColors.indices.contains(channel) {
            channelColors[channel] = [model.hue, model.saturation, model.brightness]
        }
        channel = now
        let kept = channelColors.indices.contains(now) ? channelColors[now] : nil
        // roughness and metalness start black
        if let c = kept ?? (now == 0 ? nil : [0, 0, 0]) {
            model.setColor(hue: c[0], saturation: c[1], brightness: c[2])
        }
        model.refresh()
        model.refreshThumbnails()
    }

    /// What undo or redo changed on the model: the meshes, the channel, and the pictures the
    /// layers panel shows of them.
    func followStep() {
        guard open else { return }
        follow()
        readMeshes()
        guard model.layersShown else { return }
        if let id = trayFor {
            refreshChannelThumbnails(id)
        }
        refreshMeshThumbnails()
    }

    private func refreshChannelThumbnails(_ id: UInt64) {
        let session = session
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            var read: [String: UIImage] = [:]
            for c in 0 ..< 3 {
                if let t = session.channelLayerThumbnail(channel: UInt32(c), id: id, maxSide: modelThumbSide),
                   let image = CanvasModel.image(t) {
                    read["\(c)-\(id)"] = image
                }
            }
            DispatchQueue.main.async {
                guard let self, self.model.session === session else { return }
                self.channelThumbnails.merge(read) { _, new in new }
            }
        }
    }

    func channelThumbnail(_ c: Int, _ id: UInt64) -> UIImage? {
        channelThumbnails["\(c)-\(id)"]
    }

    // MARK: the Lighting Studio

    func openStudio() {
        guard open, !flat else { return }
        model.closePanels()
        lights = session.modelLights()
        kept = (environment, exposure, environmentShown, lights)
        session.setLightingStudio(open: true)
        studioOpen = true
    }

    /// Leaves the studio, keeping what was changed or (`keep` false) putting the lighting back.
    func closeStudio(keep: Bool) {
        if !keep, let kept {
            if kept.environment != environment {
                chooseEnvironment(kept.environment)
            }
            setExposure(kept.exposure)
            setEnvironmentShown(kept.shown)
            session.setModelLights(lights: kept.lights)
        }
        session.setLightingStudio(open: false)
        lights = session.modelLights()
        kept = nil
        studioOpen = false
    }

    /// Adds a light above the model; at the most there can be, says so instead.
    func addLight() {
        if !session.addModelLight() {
            model.showNotice("A model can have four lights at most.")
        }
        lights = session.modelLights()
    }

    func duplicateLight(_ index: Int) {
        if !session.duplicateModelLight(index: UInt32(index)) {
            model.showNotice("A model can have four lights at most.")
        }
        lights = session.modelLights()
    }

    func deleteLight(_ index: Int) {
        session.deleteModelLight(index: UInt32(index))
        lights = session.modelLights()
    }

    /// Light `index` changed to `light` (its colour or intensity).
    func changeLight(_ index: Int, _ light: ModelLight) {
        session.setModelLight(index: UInt32(index), light: light)
        lights = session.modelLights()
    }

    /// The light whose cube is under window point `at` in the studio, if one is.
    func lightAt(_ at: CGPoint) -> Int? {
        let p = model.pixels
        return session.modelLightAt(x: Float(at.x * p), y: Float(at.y * p)).map { Int($0) }
    }

    /// Light `index` in the studio: its middle on the window and how far its cube reaches (points).
    func lightOnScreen(_ index: Int) -> (at: CGPoint, reach: CGFloat)? {
        guard let s = session.modelLightOnScreen(index: UInt32(index)), s.count >= 3 else { return nil }
        let p = model.pixels
        return (CGPoint(x: CGFloat(s[0]) / p, y: CGFloat(s[1]) / p), CGFloat(s[2]) / p)
    }

    /// Drags light `index` by `by` points across the studio.
    func moveLight(_ index: Int, by: CGSize) {
        let p = model.pixels
        session.moveModelLight(index: UInt32(index), dx: Float(by.width * p), dy: Float(by.height * p))
        lights = session.modelLights()
    }

    /// A drag across the studio (points): its camera goes round the model and up or down.
    func studioTurn(by: CGSize) {
        let p = model.pixels
        session.studioTurn(dx: Float(by.width * p), dy: Float(by.height * p))
    }

    func studioZoom(_ scale: CGFloat) {
        session.studioZoom(scale: Float(scale))
    }

    /// Lights the model by the environment `name`, its panorama read off the main thread.
    func chooseEnvironment(_ name: String) {
        environment = name
        let session = session
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let url = Bundle.main.url(forResource: name, withExtension: "hdr", subdirectory: "environments"),
                  let hdr = try? Data(contentsOf: url) else { return }
            DispatchQueue.main.async {
                guard let self, self.model.session === session, self.environment == name else { return }
                _ = session.setModelEnvironment(name: name, hdr: hdr)
            }
        }
    }

    func setExposure(_ value: Float) {
        session.setModelExposure(exposure: value)
        exposure = session.modelExposure()
    }

    func setEnvironmentShown(_ on: Bool) {
        session.setModelEnvironmentShown(shown: on)
        environmentShown = session.modelEnvironmentShown()
    }

    // MARK: a picture placed on the model

    /// Starts placing `image` (`straight` or premultiplied) on the shown model; no room for its
    /// layer is said.
    func beginPicture(_ image: ThumbnailImage, straight: Bool) {
        model.closePanels()
        if session.modelPictureBegin(image: image, straight: straight) {
            placing = true
        } else {
            model.noRoomForLayers()
        }
        model.refresh()
    }

    /// The picture's corners on the window, in points, clockwise from its top left.
    func pictureCorners() -> [CGPoint] {
        let p = model.pixels
        let flat = session.modelPictureBox() ?? []
        return stride(from: 0, to: flat.count - 1, by: 2).map { CGPoint(x: CGFloat(flat[$0]) / p, y: CGFloat(flat[$0 + 1]) / p) }
    }

    func flipPicture(horizontal: Bool) {
        session.modelPictureFlip(horizontal: horizontal)
    }

    /// Turns the picture being placed by an eighth of a turn.
    func turnPicture() {
        session.modelPictureMove(dx: 0, dy: 0, scale: 1, turn: .pi / 4)
    }

    func resetPicture() {
        session.modelPictureReset()
    }

    /// Ends placing the picture: it stays (`keep`) or goes, and its layer with it.
    func endPicture(keep: Bool) {
        session.modelPictureEnd(keep: keep)
        placing = false
        model.refresh()
    }

    // MARK: sharing

    /// The turntable's picture size: video's 16:9 sizes across, or a square.
    var turntablePixels: (width: Int, height: Int) {
        turntableSizes.indices.contains(turntableSize) ? turntableSizes[turntableSize] : turntableSizes[0]
    }

    /// Share's turntable opens over everything, the panel fading away.
    func openAnimatedExport(_ format: AnimatedFormat) {
        model.fadePanels()
        withAnimation(Motion.studioIn) { animatedExport = format }
    }
}

/// What the core said went wrong (a `MobileError`'s reason after its "what was being done: "), if
/// it said.
func mobileErrorReason(_ error: Error) -> String? {
    guard let failed = error as? MobileError, case let .File(reason) = failed,
          let colon = reason.range(of: ": ") else { return nil }
    let after = String(reason[colon.upperBound...])
    return after.isEmpty ? nil : after
}
