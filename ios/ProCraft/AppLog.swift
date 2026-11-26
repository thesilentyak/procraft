import Foundation
import MetricKit
import os
import UIKit

/// The app's log, kept in a file as well so that it can be shared (the core keeps it: procraft.log
/// in the app's own folder, the run before's beside it): a header about the app and the device, the
/// interface's warnings and errors, which go through `warn` and `error`, and an exception nothing
/// caught. Help's Share log hands it on (`toShare`).
enum AppLog {
    /// The app's own folder, out of the person's sight: where the log is kept.
    private static var folder: URL {
        let dir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// Begins the log's file with its header, the run before's kept beside it; once a run.
    static func start() {
        startLogFile(dir: folder.path, header: header())
        NSSetUncaughtExceptionHandler { exception in
            let stack = exception.callStackSymbols.joined(separator: "\n")
            AppLog.error("crash", "uncaught \(exception.name.rawValue): \(exception.reason ?? "")\n\(stack)")
        }
        // how the last runs ended, as the system tells it later
        RunReports.start()
    }

    /// The log to share, as one text file among the files made to share (see the core's
    /// write_log_to_share); none if it couldn't be written. Off the main thread.
    static func toShare() -> URL? {
        let file = Sharing.exportsDir.appendingPathComponent("procraft-log.txt")
        return writeLogToShare(dir: folder.path, to: file.path) ? file : nil
    }

    /// What happened, to the file only.
    static func info(_ tag: String, _ message: String) {
        logLine(level: .info, tag: tag, message: message)
    }

    /// A warning, to the system's log and the file.
    static func warn(_ tag: String, _ message: String) {
        NSLog("ProCraft: %@: %@", tag, message)
        logLine(level: .warn, tag: tag, message: message)
    }

    /// An error, to the system's log and the file.
    static func error(_ tag: String, _ message: String) {
        NSLog("ProCraft: %@: %@", tag, message)
        logLine(level: .error, tag: tag, message: message)
    }

    /// What the log says first: the app's version, the device, its system and its memory, and the
    /// screen.
    private static func header() -> String {
        let bundle = Bundle.main
        let version = bundle.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "?"
        let build = bundle.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "?"
        var system = utsname()
        uname(&system)
        let machine = withUnsafeBytes(of: &system.machine) { raw in
            String(decoding: raw.prefix { $0 != 0 }, as: UTF8.self)
        }
        let device = UIDevice.current
        let gb = { (bytes: Double) in String(format: "%.1f GB", bytes / 1e9) }
        let screen = UIScreen.main
        let started = ISO8601DateFormatter().string(from: Date())
        return [
            "ProCraft \(version) (build \(build)), started \(started)",
            "device: \(machine) (\(device.model)), \(device.systemName) \(device.systemVersion)",
            "memory: \(gb(Double(ProcessInfo.processInfo.physicalMemory))); the app may use \(gb(Double(os_proc_available_memory()))) now",
            "screen: \(Int(screen.nativeBounds.width))×\(Int(screen.nativeBounds.height)), \(screen.nativeScale)x",
        ].joined(separator: "\n") + "\n"
    }
}

/// How the app's last runs ended, as the system reports it a while after (MetricKit): a crash's or
/// a hang's report (the signal, the exception, the call stacks by address) and, once a day, how
/// many runs ended each way (closed for memory, by the watchdog, crashed, quit). Each goes into the
/// log as the system's own text, cut short past a length.
final class RunReports: NSObject, MXMetricManagerSubscriber {
    private static let shared = RunReports()
    private static let most = 16000

    static func start() {
        MXMetricManager.shared.add(shared)
    }

    private static func text(_ json: Data) -> String {
        let said = String(decoding: json, as: UTF8.self)
        return said.count > most ? String(said.prefix(most)) + "…" : said
    }

    func didReceive(_ payloads: [MXMetricPayload]) {
        for payload in payloads {
            guard let exits = payload.applicationExitMetrics else { continue }
            AppLog.warn("runs", "how runs ended, \(payload.timeStampBegin) to \(payload.timeStampEnd): \(Self.text(exits.jsonRepresentation()))")
        }
    }

    func didReceive(_ payloads: [MXDiagnosticPayload]) {
        for payload in payloads {
            let crashes = payload.crashDiagnostics?.count ?? 0
            let hangs = payload.hangDiagnostics?.count ?? 0
            let when = "\(payload.timeStampBegin) to \(payload.timeStampEnd)"
            AppLog.error("runs", "an earlier run's report (\(crashes) crashes, \(hangs) hangs), \(when): \(Self.text(payload.jsonRepresentation()))")
        }
    }
}
