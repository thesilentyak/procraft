import Foundation

/// The folders picked with the system's document picker for plugins' folders, each kept as a
/// bookmark (its text the bookmark's bytes in base64), as the core reaches them: their files listed,
/// a file written in whole, a file read out whole. Nothing here takes a file of the person's away;
/// what goes where is the core's (crates/core folders.rs). A folder whose bookmark no longer
/// resolves, or that the app may no longer reach, is unreachable.
final class SystemFolders: FolderAccess, @unchecked Sendable {
    /// The text a folder picked at `url` is kept by, and the name it shows by; none if it can't be
    /// kept.
    static func kept(_ url: URL) -> (location: String, name: String)? {
        let reaching = url.startAccessingSecurityScopedResource()
        defer {
            if reaching {
                url.stopAccessingSecurityScopedResource()
            }
        }
        guard let bookmark = try? url.bookmarkData(options: .minimalBookmark, includingResourceValuesForKeys: nil, relativeTo: nil) else {
            return nil
        }
        return (bookmark.base64EncodedString(), url.lastPathComponent)
    }

    /// Where a file named `name` is written before it is whole: hidden, beside it.
    static func partialName(_ name: String) -> String {
        ".\(name).partial"
    }

    /// Whether `name` is one file's name in a folder: nothing that reaches out of it.
    static func plainName(_ name: String) -> Bool {
        !name.isEmpty && name != "." && name != ".." && !name.contains("/") && !name.contains("\0")
    }

    func list(location: String) throws -> [FolderEntry] {
        try reaching(location) { folder in
            let keys: [URLResourceKey] = [.fileSizeKey, .contentModificationDateKey, .isDirectoryKey]
            let found: [URL]
            do {
                found = try FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: keys, options: [.skipsHiddenFiles])
            } catch {
                throw FolderError.Unreachable(reason: error.localizedDescription)
            }
            return found.compactMap { url in
                let values = try? url.resourceValues(forKeys: Set(keys))
                guard values?.isDirectory != true else { return nil }
                return Self.entry(url, values: values)
            }
        }
    }

    func writeWhole(location: String, name: String, from: String) throws -> FolderEntry {
        guard Self.plainName(name) else { throw FolderError.Failed(reason: "\(name) isn't a file's name") }
        return try reaching(location) { folder in
            let target = folder.appendingPathComponent(name)
            let partial = folder.appendingPathComponent(Self.partialName(name))
            var failure: Error?
            var coordinated: NSError?
            NSFileCoordinator().coordinate(writingItemAt: target, options: .forReplacing, error: &coordinated) { url in
                do {
                    let files = FileManager.default
                    try? files.removeItem(at: partial)
                    try files.copyItem(at: URL(fileURLWithPath: from), to: partial)
                    if files.fileExists(atPath: url.path) {
                        _ = try files.replaceItemAt(url, withItemAt: partial)
                    } else {
                        try files.moveItem(at: partial, to: url)
                    }
                } catch {
                    try? FileManager.default.removeItem(at: partial)
                    failure = error
                }
            }
            if let failed = coordinated ?? failure {
                throw FolderError.Failed(reason: failed.localizedDescription)
            }
            let values = try? target.resourceValues(forKeys: [.fileSizeKey, .contentModificationDateKey])
            guard let entry = Self.entry(target, values: values) else { throw FolderError.Failed(reason: "\(name) wasn't written") }
            return entry
        }
    }

    func readWhole(location: String, name: String, to: String) throws {
        guard Self.plainName(name) else { throw FolderError.Failed(reason: "\(name) isn't a file's name") }
        try reaching(location) { folder in
            let source = folder.appendingPathComponent(name)
            let target = URL(fileURLWithPath: to)
            let beside = target.deletingLastPathComponent().appendingPathComponent(".\(target.lastPathComponent).partial")
            var failure: Error?
            var coordinated: NSError?
            NSFileCoordinator().coordinate(readingItemAt: source, options: [], error: &coordinated) { url in
                do {
                    let files = FileManager.default
                    try? files.removeItem(at: beside)
                    try files.copyItem(at: url, to: beside)
                    try? files.removeItem(at: target)
                    try files.moveItem(at: beside, to: target)
                } catch {
                    try? FileManager.default.removeItem(at: beside)
                    failure = error
                }
            }
            if let failed = coordinated ?? failure {
                throw FolderError.Failed(reason: failed.localizedDescription)
            }
        }
    }

    private static func entry(_ url: URL, values: URLResourceValues?) -> FolderEntry? {
        guard let values else { return nil }
        let modified = values.contentModificationDate.map { Int64(($0.timeIntervalSince1970 * 1000).rounded()) } ?? 0
        return FolderEntry(name: url.lastPathComponent, size: UInt64(max(values.fileSize ?? 0, 0)), modifiedMs: modified)
    }

    /// `work` on the folder kept as `location`, while the app may reach it.
    private func reaching<T>(_ location: String, _ work: (URL) throws -> T) throws -> T {
        guard let bookmark = Data(base64Encoded: location) else {
            throw FolderError.Unreachable(reason: "the folder isn't kept as one")
        }
        var stale = false
        let folder: URL
        do {
            folder = try URL(resolvingBookmarkData: bookmark, options: [], relativeTo: nil, bookmarkDataIsStale: &stale)
        } catch {
            throw FolderError.Unreachable(reason: error.localizedDescription)
        }
        let reached = folder.startAccessingSecurityScopedResource()
        defer {
            if reached {
                folder.stopAccessingSecurityScopedResource()
            }
        }
        var directory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: folder.path, isDirectory: &directory), directory.boolValue else {
            throw FolderError.Unreachable(reason: "the folder is gone")
        }
        return try work(folder)
    }
}
