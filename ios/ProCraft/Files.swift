import Foundation
import os
import QuickLook
import UIKit

/// Handing files to other apps through the system's share sheet.
enum Sharing {
    /// A folder for files made to share, emptied by the system as it needs.
    static var exportsDir: URL {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("exports")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// A name that is safe as a file's: anything else in it goes, none at all is `otherwise`.
    static func safeName(_ name: String, otherwise: String) -> String {
        let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: " _-"))
        let safe = String(name.unicodeScalars.map { allowed.contains($0) ? Character($0) : "_" })
        return safe.trimmingCharacters(in: .whitespaces).isEmpty ? otherwise : safe
    }

    /// The share sheet for `files`, hanging from `from` (points on the screen), else from the middle
    /// of the screen.
    static func share(_ files: [URL], from: CGRect? = nil) {
        guard let top = topController() else { return }
        let sheet = UIActivityViewController(activityItems: files, applicationActivities: nil)
        if let popover = sheet.popoverPresentationController {
            popover.sourceView = top.view
            let bounds = top.view.bounds
            popover.sourceRect = from ?? CGRect(x: bounds.midX, y: bounds.midY, width: 0, height: 0)
            popover.permittedArrowDirections = from == nil ? [] : .any
        }
        top.present(sheet, animated: true)
    }

    /// The model in `usdz` shown by the system's viewer, which puts it in the room around the iPad.
    static func viewInRoom(_ usdz: URL) {
        guard let top = topController() else { return }
        let viewer = QLPreviewController()
        let shown = OneFile(usdz)
        // the viewer holds its data source weakly: it is kept here while it shows
        viewing = shown
        viewer.dataSource = shown
        top.present(viewer, animated: true)
    }

    /// The file the system's viewer is showing.
    private static var viewing: OneFile?

    /// What is shown on top now, for something to be presented over it.
    private static func topController() -> UIViewController? {
        let scene = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first
        var top = scene?.keyWindow?.rootViewController
        while let shown = top?.presentedViewController {
            top = shown
        }
        return top
    }
}

/// One file for the system's viewer.
private final class OneFile: NSObject, QLPreviewControllerDataSource {
    let url: URL

    init(_ url: URL) {
        self.url = url
    }

    func numberOfPreviewItems(in controller: QLPreviewController) -> Int {
        1
    }

    func previewController(_ controller: QLPreviewController, previewItemAt index: Int) -> QLPreviewItem {
        url as NSURL
    }
}

extension URL {
    /// The bytes of a file the system's picker gave, read with the access it gave.
    func readPicked() -> Data? {
        let scoped = startAccessingSecurityScopedResource()
        defer {
            if scoped {
                stopAccessingSecurityScopedResource()
            }
        }
        return try? Data(contentsOf: self)
    }

    /// A copy of a file the system's picker gave, in the app's temporary folder, made with the
    /// access it gave: the system copies it, so a file of gigabytes is never held in memory. None if
    /// it couldn't be copied.
    func copyPicked() -> URL? {
        let scoped = startAccessingSecurityScopedResource()
        defer {
            if scoped {
                stopAccessingSecurityScopedResource()
            }
        }
        let copy = FileManager.default.temporaryDirectory.appendingPathComponent("picked-\(UUID().uuidString)-\(lastPathComponent)")
        return (try? FileManager.default.copyItem(at: self, to: copy)) != nil ? copy : nil
    }
}

/// The memory the app may still take before the system ends it.
enum DeviceMemory {
    /// Told to the core before a plugin's brush engine imports a file, for what that may take.
    static func tell() {
        let available = os_proc_available_memory()
        if available > 0 {
            setMemoryAvailable(bytes: UInt64(available))
        }
    }
}
