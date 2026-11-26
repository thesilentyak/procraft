import AVFoundation
import CoreTransferable
import Foundation
import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// What the Share tab makes.
enum ShareKind {
    case psd, pdf, tiff, jpeg, png, layersPdf, layerPngs
    /// A plugin's artwork, its shapes kept.
    case svg
}

/// What the Share tab makes of a model: the artwork whole, the model as USDZ or OBJ, pictures of
/// it as shown, and its textures.
enum ModelShareKind {
    case artwork, usdz, obj, jpeg, png, tiff, textures
}

/// How a picture comes in from the Add tab.
enum InsertAsk {
    case file, photo, camera
}

/// The most frames a video brings in.
private let mostVideoFrames = 999
/// The frame rate a video is read at with no animation to take it from.
private let firstFps: UInt32 = 15
/// The most pixels a video's frame has on a side.
private let mostFramePixels: CGFloat = 4096

extension URL {
    /// Whether the file is a video (by its name's extension).
    var isMovie: Bool {
        UTType(filenameExtension: pathExtension)?.conforms(to: .movie) == true
    }
}

/// A video picked from the photos, copied to a file of its own for its frames to be read (it goes
/// once they have been).
struct PickedMovie: Transferable {
    let url: URL

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(contentType: .movie) { movie in
            SentTransferredFile(movie.url)
        } importing: { received in
            let kind = received.file.pathExtension.isEmpty ? "mov" : received.file.pathExtension
            let copy = FileManager.default.temporaryDirectory
                .appendingPathComponent("picked-\(UUID().uuidString)").appendingPathExtension(kind)
            try FileManager.default.copyItem(at: received.file, to: copy)
            return PickedMovie(url: copy)
        }
    }
}

/// The Actions panel's work on the open artwork: what it shares, and what it brings in.
extension CanvasModel {
    /// Runs `work`, an export by the core, off the main thread with the export card showing how far
    /// it has come and a check once made; what it made goes to the share sheet. Cancelled, nothing
    /// is shared; made nothing, `failed` is told.
    private func exporting(
        failed: @escaping @autoclosure () -> String, progress: (() -> Float)? = nil,
        _ work: @escaping (CanvasSession) -> [URL]
    ) {
        guard exportShown == nil else { return }
        let session = session
        session.resetExportProgress()
        exportCancelled = false
        exportShown = ProgressShown(progress: 0)
        let poll = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { [weak self] _ in
            self?.exportShown = ProgressShown(progress: progress?() ?? session.exportProgress())
        }
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let files = work(session)
            DispatchQueue.main.async {
                poll.invalidate()
                guard let self else { return }
                if self.exportCancelled {
                    self.exportShown = nil
                } else if files.isEmpty {
                    self.exportShown = nil
                    self.showNotice(failed(), warning: true)
                } else {
                    self.exportShown = ProgressShown(progress: 1, done: true)
                    DispatchQueue.main.asyncAfter(deadline: .now() + 0.45) {
                        self.exportShown = nil
                        Sharing.share(files)
                    }
                }
            }
        }
    }

    /// The time-lapse as a video to share: all of it, or `seconds` long; its card shows how far it has
    /// come. Which of three things went wrong, if it can't be made, is told: they are different
    /// faults. Frames it couldn't read (the one before each in their place) are told too.
    func shareTimeLapse(seconds: Int?) {
        let lapse = timeLapse.lapse
        guard lapse.count > 0 else {
            saying = ("Time-lapse", "There is no time-lapse to export yet.")
            return
        }
        let meter = ExportMeter()
        exporting(failed: lapse.whyNotMade(), progress: { meter.share }) { [weak self] session in
            let out = Sharing.exportsDir.appendingPathComponent(exportFileName(name: session.name(), package: false) + ".mp4")
            let made = lapse.export(to: out, seconds: seconds) { share in
                meter.share = share
                return self?.exportCancelled != true
            }
            let unread = lapse.unread
            // a copy for the folder a plugin keeps them in, if one does
            if made {
                _ = PluginsModel.shared.store.videoExported(path: out.path, name: session.name())
            }
            if made, unread > 0 {
                // frames left in their place, not left out silently
                DispatchQueue.main.async {
                    self?.showNotice(
                        "\(unread) frames of the time-lapse can't be read on this device: the one before each shows in its place.",
                        warning: true
                    )
                }
            }
            return made ? [out] : []
        }
    }

    /// The layered package, named with `suffix`.
    func sharePackage(suffix: String) {
        let lapse = timeLapse.lapse
        exporting(failed: "The artwork couldn't be exported.") { session in
            let base = exportFileName(name: session.name(), package: true)
            let out = Sharing.exportsDir.appendingPathComponent(base + suffix)
            let made = (try? session.exportPackage(path: out.path, seed: UInt64.random(in: 0 ... UInt64.max), timeLapse: lapse.forPackage())) != nil
            return made ? [out] : []
        }
    }

    func share(_ kind: ShareKind, quality: UInt8 = PdfQuality.best) {
        exporting(failed: "The artwork couldn't be exported.") { session in
            let dir = Sharing.exportsDir
            let base = exportFileName(name: session.name(), package: false)
            func file(_ suffix: String) -> URL {
                dir.appendingPathComponent(base + suffix)
            }
            do {
                switch kind {
                case .psd:
                    try session.exportPsd(path: file(".psd").path)
                    return [file(".psd")]
                case .tiff:
                    try session.exportTiff(path: file(".tif").path)
                    return [file(".tif")]
                case .jpeg:
                    try session.exportJpeg(path: file(".jpg").path)
                    return [file(".jpg")]
                case .png:
                    try session.exportPng(path: file(".png").path)
                    return [file(".png")]
                case .svg:
                    try session.exportSvg(path: file(".svg").path)
                    return [file(".svg")]
                case .pdf:
                    try session.exportPdf(path: file(".pdf").path, layers: false, quality: quality)
                    return [file(".pdf")]
                case .layersPdf:
                    try session.exportPdf(path: file(".pdf").path, layers: true, quality: quality)
                    return [file(".pdf")]
                case .layerPngs:
                    let folder = dir.appendingPathComponent(base)
                    try? FileManager.default.removeItem(at: folder)
                    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
                    return try session.exportLayerPngs(dir: folder.path).map { URL(fileURLWithPath: $0) }
                }
            } catch {
                return []
            }
        }
    }

    /// The model, a picture of it or its textures, to share.
    func shareModel(_ kind: ModelShareKind) {
        let dir = dir
        exporting(failed: "The model couldn't be exported.") { session in
            let exports = Sharing.exportsDir
            let base = exportFileName(name: session.name(), package: false)
            func file(_ suffix: String) -> URL {
                exports.appendingPathComponent(base + suffix)
            }
            do {
                switch kind {
                case .artwork:
                    let out = exports.appendingPathComponent(exportFileName(name: session.name(), package: true) + ".procraft")
                    try session.exportModelArtwork(dir: dir, path: out.path)
                    return [out]
                case .usdz:
                    try session.exportModelUsdz(path: file(".usdz").path)
                    return [file(".usdz")]
                case .obj:
                    try session.exportModelObj(path: file(".zip").path)
                    return [file(".zip")]
                case .jpeg:
                    try session.exportModelJpeg(path: file(".jpg").path)
                    return [file(".jpg")]
                case .png:
                    try session.exportModelRender(path: file(".png").path, transparent: false)
                    return [file(".png")]
                case .tiff:
                    try session.exportModelTiff(path: file(".tif").path)
                    return [file(".tif")]
                case .textures:
                    let folder = exports.appendingPathComponent(base)
                    try? FileManager.default.removeItem(at: folder)
                    return try session.exportModelTextures(dir: folder.path).map { URL(fileURLWithPath: $0) }
                }
            } catch {
                return []
            }
        }
    }

    /// The model in the room around the iPad: written as USDZ, then shown by the system's viewer.
    func viewModelInAR() {
        let session = session
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let out = Sharing.exportsDir.appendingPathComponent(exportFileName(name: session.name(), package: false) + ".usdz")
            let made = (try? session.exportModelUsdz(path: out.path)) != nil
            DispatchQueue.main.async {
                if made {
                    Sharing.viewInRoom(out)
                } else {
                    self?.showNotice("The model couldn't be exported.", warning: true)
                }
            }
        }
    }

    /// The model's turntable as `format` in the exports folder, its settings and size as Share's
    /// screen has them; `meter` hears how far a video has come; it stops when `cancelled` says so.
    /// Slow: off the main thread.
    static func exportTurntable(
        _ session: CanvasSession, _ format: AnimatedFormat, _ t: Turntable, size: (width: Int, height: Int),
        meter: ExportMeter? = nil, cancelled: @escaping () -> Bool = { false }
    ) -> URL? {
        let url = Sharing.exportsDir.appendingPathComponent(exportFileName(name: session.name(), package: false) + "." + format.fileExtension)
        let fps = Float(turntableFps)
        do {
            switch format {
            case .gif:
                try session.exportModelGif(path: url.path, turntable: t, width: UInt32(size.width), height: UInt32(size.height), fps: fps)
            case .png:
                try session.exportModelApng(path: url.path, turntable: t, width: UInt32(size.width), height: UInt32(size.height), fps: fps)
            case .mp4, .hevc:
                let (width, height) = VideoWriter.fit(width: size.width, height: size.height, maxSide: max(size.width, size.height))
                let count = max(Int((t.seconds * fps).rounded()), 1)
                let made = VideoWriter.write(
                    to: url, width: width, height: height, count: count, hevc: format == .hevc, bitsPerPixel: 4, fps: fps
                ) { n, context in
                    guard !cancelled() else { return false }
                    meter?.share = Float(n) / Float(count)
                    guard let frame = session.modelTurntablePicture(
                        index: UInt32(n), count: UInt32(count), turntable: t, width: UInt32(width), height: UInt32(height), bgra: false
                    ), let picture = CanvasModel.image(frame)?.cgImage else { return false }
                    context.interpolationQuality = .high
                    context.draw(picture, in: CGRect(x: 0, y: 0, width: width, height: height))
                    return true
                }
                if !made {
                    return nil
                }
            }
            return url
        } catch {
            return nil
        }
    }

    /// The model's turntable made and shared, with the export card showing how far it has come.
    func shareTurntable(_ format: AnimatedFormat, done: @escaping () -> Void) {
        guard exportShown == nil else { return }
        let session = session
        let t = scene.turntable
        let size = scene.turntablePixels
        session.resetExportProgress()
        exportCancelled = false
        exportShown = ProgressShown(progress: 0)
        let meter = ExportMeter()
        let poll = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { [weak self] _ in
            self?.exportShown = ProgressShown(progress: format.video ? meter.share : session.exportProgress())
        }
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let url = Self.exportTurntable(session, format, t, size: size, meter: meter) { self?.exportCancelled == true }
            DispatchQueue.main.async {
                poll.invalidate()
                guard let self else { return }
                if self.exportCancelled {
                    self.exportShown = nil
                } else if let url {
                    self.exportShown = ProgressShown(progress: 1, done: true)
                    DispatchQueue.main.asyncAfter(deadline: .now() + 0.45) {
                        self.exportShown = nil
                        done()
                        Sharing.share([url])
                    }
                } else {
                    self.exportShown = nil
                    self.showNotice("The animation could not be exported.", warning: true)
                }
            }
        }
    }

    /// The file a plugin makes of the artwork's layers of a kind it adds (`e`), to share.
    func sharePlugin(_ e: PluginExport) {
        exporting(failed: "The artwork couldn't be exported.") { session in
            let base = exportFileName(name: session.name(), package: false)
            let out = Sharing.exportsDir.appendingPathComponent(base).appendingPathExtension(e.`extension`)
            let made = (try? session.exportPlugin(key: e.key, path: out.path)) != nil
            return made ? [out] : []
        }
    }

    /// The bytes of a picture on a new layer, fitted to the canvas (a moving GIF's frames on the
    /// timeline instead), with `transform` ready to transform; a message if it couldn't be, and
    /// false.
    @discardableResult
    func insertPicture(_ data: Data?, name: String?, isPrivate: Bool, transform: Bool = true) -> Bool {
        closePanels()
        guard let data else {
            showNotice("This picture can't be opened.", warning: true)
            return false
        }
        // a moving GIF: its frames (a still one is a picture)
        if name?.lowercased().hasSuffix(".gif") == true, let done = session.insertGifFrames(bytes: data, private: isPrivate) {
            if done.added == 0 {
                showNotice(done.full ? noRoomSaid() : "This picture can't be opened.", warning: true)
                return false
            }
            refresh()
            showDone("Frames saved to timeline")
            if done.full {
                showNotice(noRoomSaid(), warning: true)
            }
            return !done.full
        }
        guard let image = GalleryModel.decode(data, p3: CanvasSpace.shared.p3)?.image else {
            showNotice("This picture can't be opened.", warning: true)
            return false
        }
        // on a model it is placed on the surface, not in the texture
        if scene.shown {
            scene.beginPicture(image, straight: true)
            return true
        }
        guard session.insertImage(image: image, private: isPrivate) != nil else {
            noRoomForLayers()
            return false
        }
        refresh()
        if transform {
            toggleTransform()
        }
        return true
    }

    /// A video's frames as layers above the current one, each fitted to the canvas, at the
    /// animation's frame rate (or its first one's), for as many as there is room; Animation Assist
    /// comes on with them in its timeline. Read off the main thread; `done` hears whether they all
    /// came in, once the file has been read. On a model, where a picture is placed on the surface,
    /// a video has no place.
    func insertVideo(_ url: URL, isPrivate: Bool, done: @escaping (Bool) -> Void = { _ in }) {
        closePanels()
        guard !scene.open else {
            showNotice("A video can't be placed on a model.", warning: true)
            done(false)
            return
        }
        let session = session
        let fps = min(max(animationAssist.settings?.fps ?? firstFps, 1), 60)
        let p3 = CanvasSpace.shared.p3
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let read = Self.videoFrames(url, fps: fps, p3: p3) { session.insertImage(image: $0, private: isPrivate) != nil }
            DispatchQueue.main.async {
                guard let self else {
                    done(false)
                    return
                }
                guard read.added > 0 else {
                    self.showNotice(read.full ? self.noRoomSaid() : "This video can't be opened.", warning: true)
                    done(false)
                    return
                }
                let settings = self.animationAssist.settings
                if settings == nil || settings?.pages == true {
                    self.animationAssist.setAssist(true, pages: false)
                } else {
                    self.refresh()
                }
                self.showDone("Frames saved to timeline")
                if read.full {
                    self.showNotice(self.noRoomSaid(), warning: true)
                }
                done(!read.full)
            }
        }
    }

    /// The frames of the video at `url`, `fps` a second from its start (at most `mostVideoFrames`,
    /// at most `mostFramePixels` on a side, in Display P3 if `p3`, else sRGB), each handed to
    /// `take` until it says there is no more room: how many it took, and whether room ran out.
    /// Slow: off the main thread.
    private static func videoFrames(_ url: URL, fps: UInt32, p3: Bool, take: (ThumbnailImage) -> Bool) -> (added: Int, full: Bool) {
        let asset = AVURLAsset(url: url)
        let seconds = CMTimeGetSeconds(asset.duration)
        guard seconds.isFinite, !asset.tracks(withMediaType: .video).isEmpty,
              let space = CGColorSpace(name: p3 ? CGColorSpace.displayP3 : CGColorSpace.sRGB) else { return (0, false) }
        let generator = AVAssetImageGenerator(asset: asset)
        // upright as it plays, each frame as it is
        generator.appliesPreferredTrackTransform = true
        generator.requestedTimeToleranceBefore = .zero
        generator.requestedTimeToleranceAfter = .zero
        generator.maximumSize = CGSize(width: mostFramePixels, height: mostFramePixels)
        let count = min(max(Int(seconds * Double(fps)), 1), mostVideoFrames)
        var added = 0
        for i in 0 ..< count {
            let taken: Bool? = autoreleasepool {
                let at = CMTime(value: Int64(i), timescale: Int32(fps))
                guard let frame = try? generator.copyCGImage(at: at, actualTime: nil),
                      let image = straightPixels(frame, space: space) else { return nil }
                return take(image)
            }
            // a frame that can't be read is left out
            guard let taken else { continue }
            guard taken else { return (added, true) }
            added += 1
        }
        return (added, false)
    }

    /// A frame's straight RGBA pixels in `space`.
    private static func straightPixels(_ frame: CGImage, space: CGColorSpace) -> ThumbnailImage? {
        let width = frame.width
        let height = frame.height
        guard width > 0, height > 0 else { return nil }
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let drawn: Bool = pixels.withUnsafeMutableBytes { bytes in
            guard let context = CGContext(
                data: bytes.baseAddress, width: width, height: height, bitsPerComponent: 8,
                bytesPerRow: width * 4, space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
            ) else { return false }
            context.draw(frame, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard drawn else { return nil }
        // most frames are opaque, so premultiplied and straight are the same; one with alpha is
        // made straight, as the core takes it
        let opaque: [CGImageAlphaInfo] = [.none, .noneSkipFirst, .noneSkipLast]
        if !opaque.contains(frame.alphaInfo) {
            for i in stride(from: 0, to: pixels.count, by: 4) {
                let alpha = Int(pixels[i + 3])
                guard alpha != 0, alpha != 255 else { continue }
                for k in 0 ..< 3 {
                    pixels[i + k] = UInt8(min(255, (Int(pixels[i + k]) * 255 + alpha / 2) / alpha))
                }
            }
        }
        return ThumbnailImage(width: UInt32(width), height: UInt32(height), rgba: Data(pixels))
    }

    /// A file from the picker, read with the access it gave: a video's frames, or a picture.
    func insertFile(_ url: URL, isPrivate: Bool) {
        guard url.isMovie else {
            insertPicture(url.readPicked(), name: url.lastPathComponent, isPrivate: isPrivate)
            return
        }
        let scoped = url.startAccessingSecurityScopedResource()
        insertVideo(url, isPrivate: isPrivate) { _ in
            if scoped {
                url.stopAccessingSecurityScopedResource()
            }
        }
    }

    func insertPhoto(_ picture: UIImage?, isPrivate: Bool) {
        insertPicture(picture?.jpegData(compressionQuality: 1), name: nil, isPrivate: isPrivate)
    }

    func changeRapidUndoDelay(_ share: Float) {
        let range = AppSettings.rapidUndoRange
        AppSettings.shared.rapidUndoDelay = range.lowerBound + share * (range.upperBound - range.lowerBound)
    }

    func changeSelectionVisibility(_ share: Float) {
        AppSettings.shared.selectionVisibility = share
        session.setSelectionVisibility(visibility: share)
    }

    /// Project canvas: the picture on a second display while there is one.
    func changeProjectCanvas(_ on: Bool) {
        AppSettings.shared.projectCanvas = on
        Projection.shared.update()
    }

    func changeBrushScaling(onScreen: Bool) {
        AppSettings.shared.brushOnScreen = onScreen
        session.setBrushScaling(onScreen: onScreen)
    }
}
