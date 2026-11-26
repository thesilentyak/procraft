import Foundation
import ImageIO
import UIKit
import UniformTypeIdentifiers
import WidgetKit

/// The home screen's Recent artwork widget, from the app's side (the widget is ios/RecentWidget):
/// the picture of the artwork worked on last, kept where the widget reads it, and a tap on the
/// widget opening that artwork. The shared folder exists only in a build signed with the app group
/// (ios/widget.yml); without it nothing is kept and nothing changes.
enum RecentArtwork {
    /// The app group the app and its widget share.
    static let group = "group.io.github.artcraftandroid.procraft"
    /// The picture's file in the group's folder.
    static let pictureName = "recent.png"
    /// The longest side the picture is kept at: the home screen holds it in memory.
    private static let pictureMost = 720

    /// Where the picture is worked out and written, off the main thread, one at a time.
    private static let work = DispatchQueue(label: "recent artwork", qos: .utility)
    /// The artwork the kept picture is of, and when it was changed then: kept again only as these
    /// change (only touched on `work`).
    private static var shown: (dir: String, modifiedMs: Int64)?

    /// Whether `url` is the widget's tap.
    static func isOpenRecent(_ url: URL) -> Bool {
        url.scheme?.lowercased() == "procraft" && url.host?.lowercased() == "recent"
    }

    /// Follows the artworks being saved, for the widget to show the newest; once, as the app starts.
    static func start() {
        NotificationCenter.default.addObserver(forName: Gallery.keptNotification, object: nil, queue: .main) { _ in
            refresh()
        }
        refresh()
    }

    /// Keeps the picture of the artwork worked on last for the widget, and has the widget show it,
    /// when that changed.
    static func refresh() {
        guard let folder = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group) else { return }
        work.async {
            let file = folder.appendingPathComponent(pictureName)
            let recent = Gallery.store.mostRecent()
            if let recent, let shown, shown.dir == recent.dir, shown.modifiedMs == recent.modifiedMs,
               FileManager.default.fileExists(atPath: file.path) {
                return
            }
            if let recent {
                let thumbnail = URL(fileURLWithPath: (recent.dir as NSString).appendingPathComponent("thumbnail.png"))
                if let bytes = shrunk(thumbnail) {
                    try? bytes.write(to: file, options: .atomic)
                } else {
                    try? FileManager.default.removeItem(at: file)
                }
                shown = (recent.dir, recent.modifiedMs)
            } else {
                try? FileManager.default.removeItem(at: file)
                shown = nil
            }
            WidgetCenter.shared.reloadAllTimelines()
        }
    }

    /// The widget was tapped: the artwork worked on last opens, unless one is open already.
    static func openMostRecent(unlessOpen open: Bool) {
        guard !open else { return }
        work.async {
            guard let recent = Gallery.store.mostRecent() else { return }
            DispatchQueue.main.async {
                GalleryModel.shared.open(recent)
            }
        }
    }

    /// The picture at `url` as a PNG at most `pictureMost` across.
    private static func shrunk(_ url: URL) -> Data? {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: pictureMost,
        ]
        guard let picture = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { return nil }
        let bytes = NSMutableData()
        guard let out = CGImageDestinationCreateWithData(bytes as CFMutableData, UTType.png.identifier as CFString, 1, nil) else {
            return nil
        }
        CGImageDestinationAddImage(out, picture, nil)
        return CGImageDestinationFinalize(out) ? bytes as Data : nil
    }
}
