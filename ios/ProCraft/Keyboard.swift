import AudioToolbox
import SwiftUI
import UIKit
import UniformTypeIdentifiers

private let companionWidth: CGFloat = 432
private let companionHeight: CGFloat = 89
private let companionFill = Color(argb: 0xE640_4040)
private let companionInk = Color(argb: 0xFFBE_BEBE)
private let companionGrab = Color(argb: 0xFF8A_8A8A)
/// Where the bar sits at first: its middle across the screen and this far down it.
private let companionStartY: CGFloat = 0.54
/// The latest colour is named at most this often while it keeps changing, seconds.
private let colourNameGap = 0.15
/// The sound a key makes.
private let clickSound: SystemSoundID = 1104

/// What the app keeps in the system's Settings (`ios/Settings.bundle`), read as it comes forward: the
/// app itself never changes them.
enum SystemSettings {
    private static func flag(_ key: String, _ starts: Bool) -> Bool {
        (UserDefaults.standard.object(forKey: key) as? NSNumber)?.boolValue ?? starts
    }

    static var orientationMemory: Bool { flag("orientationMemory", true) }
    static var fitInside: Bool { flag("fitInside", false) }
    static var noTimeLapse: Bool { flag("noTimeLapse", false) }
    static var simplifiedUndos: Bool { flag("simplifiedUndos", true) }
    static var singleTouchCompanion: Bool { flag("singleTouchCompanion", false) }
    static var colorNames: Bool { flag("colorNames", false) }
    static var feedbackSounds: Bool { flag("feedbackSounds", false) }
    static var tapAssistance: Bool { flag("tapAssistance", false) }

    static var speedDistance: Float {
        (UserDefaults.standard.object(forKey: "speedDistance") as? NSNumber)?.floatValue ?? 1
    }

    /// What an artwork dragged out of the gallery becomes, and a picture's format when one is wanted.
    static var dragFormat: String {
        UserDefaults.standard.string(forKey: "dragFormat") ?? "ProCraft"
    }

    static var dragImageFormat: String {
        UserDefaults.standard.string(forKey: "dragImageFormat") ?? "JPEG"
    }
}

/// The app's settings on the open artwork, a hardware keyboard, and what comes in by being dropped.
extension CanvasModel {
    /// The system's Settings as they stand now.
    func readAppSettings() {
        colorNames = SystemSettings.colorNames
        feedbackSounds = SystemSettings.feedbackSounds
        singleTouchCompanion = SystemSettings.singleTouchCompanion
        if !singleTouchCompanion {
            companionMode = nil
        }
        session.setOrientationMemory(on: SystemSettings.orientationMemory)
        session.setSimplifiedUndos(on: SystemSettings.simplifiedUndos)
        session.setSpeedDistance(part: SystemSettings.speedDistance)
        applyFitInsets()
    }

    /// What fitting the canvas keeps clear of: the top bar and the sidebar, with Fit Canvas Inside
    /// Interface.
    func applyFitInsets() {
        guard SystemSettings.fitInside else {
            session.setFitInsets(left: 0, top: 0, right: 0, bottom: 0)
            return
        }
        let right = AppSettings.shared.rightHanded
        // (moved off its edge by Edit layout, the side bar leaves the canvas the whole width)
        let side = showSideBar && barMoved(.sideBar) == nil ? Float((Pc.sideBarShown + Pc.popoverGap) * pixels) : 0
        session.setFitInsets(left: right ? 0 : side, top: Float(Pc.barShown * pixels), right: right ? side : 0, bottom: 0)
    }

    /// A new artwork just started: without time-lapse if it is turned off.
    func startedNew() {
        if SystemSettings.noTimeLapse {
            timeLapse.setRecording(false)
        }
    }

    /// A click for an action done, with Feedback Sounds on.
    func feedback() {
        if feedbackSounds {
            AudioServicesPlaySystemSound(clickSound)
        }
    }

    /// The single touch companion's Zoom or Move on, or off again.
    func toggleCompanionMode(_ mode: CompanionMode) {
        feedback()
        companionMode = companionMode == mode ? nil : mode
    }

    /// A hardware keyboard's key as the shared shortcuts name it.
    static func name(of key: UIKey) -> String? {
        switch key.keyCode {
        case .keyboardSpacebar: return "space"
        case .keyboardDeleteOrBackspace: return "backspace"
        case .keyboardDeleteForward: return "delete"
        case .keyboardLeftArrow: return "left"
        case .keyboardRightArrow: return "right"
        case .keyboardUpArrow: return "up"
        case .keyboardDownArrow: return "down"
        default: break
        }
        let typed = key.charactersIgnoringModifiers.lowercased()
        return typed.count == 1 && "abcdefghijklmnopqrstuvwxyz0123456789[];".contains(typed) ? typed : nil
    }

    /// A press written out with its modifiers ("command+shift+z"), to tell one press from another.
    static func pressText(_ name: String, _ flags: UIKeyModifierFlags) -> String {
        let held: [(UIKeyModifierFlags, String)] = [(.command, "command+"), (.control, "control+"), (.shift, "shift+"), (.alternate, "alt+")]
        return held.filter { flags.contains($0.0) }.map(\.1).joined() + name
    }

    /// A key went down; whether it did anything.
    func keyPressed(_ key: UIKey) -> Bool {
        guard let name = Self.name(of: key) else { return false }
        let flags = key.modifierFlags
        return runShortcut(
            name, command: flags.contains(.command) || flags.contains(.control),
            shift: flags.contains(.shift), alt: flags.contains(.alternate)
        )
    }

    /// The key `name`'s shortcut, the shared shortcuts saying what it does (as a plugin rebinding
    /// keys takes the press), or a plugin's switch or action it is given to, or the key a plugin's
    /// panel asked for; whether it did anything.
    func runShortcut(_ name: String, command: Bool, shift: Bool, alt: Bool) -> Bool {
        let press = KeyCombo(key: name, command: command, shift: shift, alt: alt)
        let plugins = PluginsModel.shared
        // a plugin's panel asked for the next key: it is told it, and nothing else is done
        if plugins.panel?.keyPress == true {
            pluginPanelKey(press)
            return true
        }
        // the arrows move what is transformed, or the strokes the selection takes
        let nudges = transforming || !selection.takenBox.isEmpty
        guard let shortcut = plugins.store.keyShortcut(key: name, command: command, shift: shift, alt: alt, transforming: nudges) else {
            return pluginPress(press)
        }
        switch shortcut {
        case .undo: undo()
        case .redo: redo()
        case .copyLayer: copyLayer()
        case .cutLayer: cutLayer()
        case .paste: paste()
        case .paint: select(.paint)
        case .erase: select(.erase)
        case .eyedropper: pickNext.toggle()
        case .selection: toggleSelecting()
        case .transform: toggleTransform()
        case .hueSaturationBrightness: startAdjustment(.hsb)
        case .colours: toggle(.color)
        case .actions: toggle(.actions)
        case .layers: toggle(.layers)
        case .swapColours: swapColors()
        case .deselect: selection.clear()
        case .duplicateLayer: duplicateCurrent()
        case .interface: toggleBars()
        case .clearLayer: clearCurrentLayer()
        case .copyCanvas: runQuickAction(.copyCanvas, at: screenCentre)
        case .colourBalance: runQuickAction(.colorBalance, at: screenCentre)
        case .perspectiveGuide: runQuickAction(.guidePerspective, at: screenCentre)
        case .quickMenu: toggleQuickMenu(at: screenCentre)
        case .greyView: session.setGreyView(on: !session.greyView())
        case let .brushSize(by): changeSize(min(max(size + by, 0), 1))
        case let .nudge(dx, dy):
            if transforming {
                if let moved = session.transformArrow(dx: dx, dy: dy) {
                    transform.moved(moved)
                }
            } else if session.nudgePluginItems(dx: dx, dy: dy) {
                refresh()
            }
        // the view's own moves, which a plugin gives keys to
        case .zoomView, .turnView, .panView:
            _ = session.keyView(shortcut: shortcut)
            refreshView()
        case .fitView:
            _ = session.keyView(shortcut: shortcut)
            followView()
        }
        return true
    }

    /// A press none of the app's shortcuts take, as a plugin gives it one of its switches (flipped,
    /// kept as its card keeps it) or actions (run as its row runs it); false if no plugin's.
    private func pluginPress(_ press: KeyCombo) -> Bool {
        let plugins = PluginsModel.shared
        // as a plugin rebinding keys takes it (it may make it no press at all)
        guard let taken = plugins.store.keyPress(press: press), let pressed = plugins.store.pluginPress(press: taken) else { return false }
        switch pressed {
        case let .switched(control):
            if case let .switch(one) = control {
                plugins.setSwitch(one.key, one.on)
            }
        case let .action(key):
            runPluginAction(key)
        }
        return true
    }

    /// Files dropped on the canvas: pictures on layers of their own, a picture alone ready to
    /// transform, a video's frames on the timeline; other files' brushes join the library. The
    /// copies in `folder` go after them.
    func dropOnCanvas(_ urls: [URL], from folder: URL) {
        let inserted = urls.filter { $0.isMovie || UTType(filenameExtension: $0.pathExtension)?.conforms(to: .image) == true }
        let others = urls.filter { !inserted.contains($0) }
        guard !urls.isEmpty else { return }
        let gone = DispatchGroup()
        for file in others {
            gone.enter()
            importBrushes(from: file) { gone.leave() }
        }
        if !inserted.isEmpty {
            if transforming {
                endTransform(commit: true)
            }
            closePanels()
            gone.enter()
            insertDropped(inserted[...], transform: inserted.count == 1) { gone.leave() }
        }
        gone.notify(queue: .global(qos: .utility)) {
            try? FileManager.default.removeItem(at: folder)
        }
    }

    /// Dropped pictures and videos in, one after another (a video's frames read before the next),
    /// until one can't come in; then `done`.
    private func insertDropped(_ files: ArraySlice<URL>, transform: Bool, done: @escaping () -> Void) {
        guard let file = files.first else {
            done()
            return
        }
        let rest = files.dropFirst()
        guard file.isMovie else {
            if insertPicture(try? Data(contentsOf: file), name: file.lastPathComponent, isPrivate: false, transform: transform) {
                insertDropped(rest, transform: transform, done: done)
            } else {
                done()
            }
            return
        }
        insertVideo(file, isPrivate: false) { [weak self] came in
            if came, let self {
                self.insertDropped(rest, transform: transform, done: done)
            } else {
                done()
            }
        }
    }
}

/// The canvas's keyboard shortcuts as the system lists them while ⌘ is held: the shared shortcuts
/// with an Apple keyboard's keys (as a plugin rebinding keys has them, and the presses plugins give
/// their switches and actions), each press a command that sends `action` (the key's name in its
/// property list), under the name of what it does (a brush size's with its group's). The modify
/// button's key (⌥ held by itself) and ⌦ aren't listed: they come as presses, as before.
func shortcutKeyCommands(action: Selector) -> [UIKeyCommand] {
    PluginsModel.shared.store.keyShortcutList(desktop: false, mac: true).flatMap { group in
        group.rows.flatMap { row in
            listedPresses(row.combos).compactMap { combo -> UIKeyCommand? in
                guard let input = keyCommandInput(combo.key) else { return nil }
                var flags: UIKeyModifierFlags = []
                if combo.command {
                    flags.insert(.command)
                }
                if combo.shift {
                    flags.insert(.shift)
                }
                if combo.alt {
                    flags.insert(.alternate)
                }
                let title = group.heading == "Brush size" ? "Brush size: \(row.does)" : row.does
                let command = UIKeyCommand(
                    title: title, action: action, input: input, modifierFlags: flags, propertyList: combo.key, discoverabilityTitle: title
                )
                // the canvas's own keys before what the system would do with them
                command.wantsPriorityOverSystemBehavior = true
                return command
            }
        }
    }
}

/// The presses the system lists for a row of shortcuts: a key alone that does with ⌘ what it does
/// by itself is listed with ⌘, as the measured command table has it (⌘0, ⌘K, ⌘J, ⌘D, ⌘U); alone
/// it still works, as a press.
private func listedPresses(_ combos: [KeyCombo]) -> [KeyCombo] {
    var listed: [KeyCombo] = []
    for combo in combos {
        var shown = combo
        if !combo.command, !combo.shift, !combo.alt,
           let does = keyShortcut(key: combo.key, command: false, shift: false, alt: false, transforming: false),
           keyShortcut(key: combo.key, command: true, shift: false, alt: false, transforming: false) == does
        {
            shown = KeyCombo(key: combo.key, command: true, shift: false, alt: false)
        }
        if !listed.contains(shown) {
            listed.append(shown)
        }
    }
    return listed
}

/// The input a key command takes for a key the shared shortcuts name; none for one it can't.
private func keyCommandInput(_ name: String) -> String? {
    switch name {
    case "space": return " "
    case "backspace": return "\u{8}"
    case "left": return UIKeyCommand.inputLeftArrow
    case "right": return UIKeyCommand.inputRightArrow
    case "up": return UIKeyCommand.inputUpArrow
    case "down": return UIKeyCommand.inputDownArrow
    default: return name.count == 1 ? name : nil
    }
}

/// Files dragged in from another app, onto the canvas.
final class CanvasDropper: NSObject, UIDropInteractionDelegate {
    private unowned let model: CanvasModel

    init(model: CanvasModel) {
        self.model = model
    }

    func dropInteraction(_: UIDropInteraction, canHandle session: any UIDropSession) -> Bool {
        session.localDragSession == nil && session.hasItemsConforming(toTypeIdentifiers: [UTType.item.identifier])
    }

    func dropInteraction(_: UIDropInteraction, sessionDidUpdate _: any UIDropSession) -> UIDropProposal {
        UIDropProposal(operation: .copy)
    }

    func dropInteraction(_: UIDropInteraction, performDrop session: any UIDropSession) {
        loadDropped(session) { [model] urls, folder in
            model.dropOnCanvas(urls, from: folder)
        }
    }
}

/// The single touch gestures companion: a floating bar standing in for gestures that need more than
/// one finger. Undo, redo, Zoom and Move (while one is on, a finger dragged on the canvas zooms or
/// moves it), and Fit Canvas. Dragged by its top edge.
struct TouchCompanion: View {
    let model: CanvasModel
    let screen: CGSize

    /// Where it is, none until it is moved; and where a drag began.
    @State private var at: CGPoint?
    @State private var from: CGPoint?

    private func button(_ icon: PcIcon, _ label: String, on: Bool = false, size: CGFloat, action: @escaping () -> Void) -> some View {
        VStack(spacing: 0) {
            PcIconView(icon, size: size, color: on ? Pc.accent : companionInk)
                .frame(height: 30)
            Text(verbatim: label)
                .font(.system(size: 13))
                .foregroundStyle(companionInk)
                .lineLimit(1)
        }
        .frame(width: 80, height: 50)
        .contentShape(Rectangle())
        .onTapGesture(perform: action)
    }

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: 30, style: .circular)
        let first = CGPoint(x: (screen.width - companionWidth) / 2, y: screen.height * companionStartY - companionHeight / 2)
        let place = at ?? first
        VStack(spacing: 0) {
            // the grab line, and the strip it sits in, drags the bar about, kept on the screen
            Capsule()
                .fill(companionGrab)
                .frame(width: 40, height: 5)
                .frame(maxWidth: .infinity)
                .frame(height: 24)
                .contentShape(Rectangle())
                .gesture(
                    DragGesture(minimumDistance: 1, coordinateSpace: .global)
                        .onChanged { drag in
                            let start = from ?? place
                            from = start
                            let k = Interface.shared.scale
                            at = CGPoint(
                                x: min(max(start.x + drag.translation.width / k, 0), screen.width - companionWidth),
                                y: min(max(start.y + drag.translation.height / k, 0), screen.height - companionHeight)
                            )
                        }
                        .onEnded { _ in from = nil }
                )
            HStack(spacing: 4) {
                button(PcIcons.undo, "Undo", size: 30) { model.undo() }
                button(PcIcons.redo, "Redo", size: 30) { model.redo() }
                button(PcIcons.zoom, "Zoom", on: model.companionMode == .zoom, size: 22) { model.toggleCompanionMode(.zoom) }
                button(PcIcons.moveCanvas, "Move", on: model.companionMode == .move, size: 22) { model.toggleCompanionMode(.move) }
                button(PcIcons.fitCanvas, "Fit Canvas", size: 22) { model.settleToFit() }
            }
            .padding(.leading, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
            Spacer(minLength: 0)
        }
        .frame(width: companionWidth, height: companionHeight)
        .background(shape.fill(companionFill).shadow(color: .black.opacity(0.3), radius: 8))
        .clipShape(shape)
        .contentShape(shape)
        .onTapGesture {}
        .offset(x: place.x, y: place.y)
    }
}

extension CanvasModel {
    /// Names the colour as it changes (Color Description Notifications): at once, or, while it keeps
    /// changing, the latest colour at most every `colourNameGap`.
    func nameColourSoon() {
        naming?.cancel()
        let wait = max(namedAt.timeIntervalSinceNow + colourNameGap, 0)
        naming = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .seconds(wait))
            guard let self, !Task.isCancelled else { return }
            self.namedAt = Date()
            self.showNotice(colourName(rgb: self.colorRgb() & 0xFFFFFF))
        }
    }
}
