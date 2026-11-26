import SwiftUI
import UniformTypeIdentifiers

/// The plugins, a page of the panel: a card each, drawn from what the list says of them, in the
/// order they were put in. A card held by its name lifts and follows the finger or Pencil; let go
/// over another card, it takes that one's place.
struct PluginsPage: View {
    let model: CanvasModel
    /// As tall as its cards may be: past that they scroll.
    var most: CGFloat = 760
    let onBack: () -> Void

    @State private var plugins = PluginsModel.shared
    @State private var importing = false

    var body: some View {
        let shown = plugins.plugins
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 0) {
                PageHeader(title: "Plugins", onBack: onBack)
                Spacer(minLength: 0)
                Text(verbatim: "Import")
                    .font(.system(size: 17))
                    .foregroundStyle(Pc.accent)
                    .lineLimit(1)
                    .padding(.horizontal, 16)
                    .frame(height: 34)
                    .contentShape(Rectangle())
                    .onTapGesture { importing = true }
                    .padding(.top, 9)
            }
            ScrollView {
                VStack(spacing: 20) {
                    if shown.isEmpty {
                        Text(verbatim: "Nothing imported yet. Import a .procraft-plugin file to add one.")
                            .font(.system(size: 15))
                            .foregroundStyle(Pc.actionsRow)
                            .padding(16)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    ForEach(Array(shown.enumerated()), id: \.element.id) { i, plugin in
                        PluginCard(model: model, plugin: plugin)
                            .dropDestination(for: String.self) { ids, _ in
                                guard let id = ids.first, id != plugin.id, shown.contains(where: { $0.id == id }) else { return false }
                                if let said = plugins.move(id, to: i) {
                                    model.showNotice(said, warning: true)
                                }
                                return true
                            }
                    }
                }
                .padding(.horizontal, 12)
                .padding(.top, 8)
                .padding(.bottom, 13)
            }
            .scrollIndicators(.hidden)
            .frame(maxHeight: max(most, 88))
            .fixedSize(horizontal: false, vertical: true)
        }
        .fileImporter(isPresented: $importing, allowedContentTypes: [.item]) { result in
            if case let .success(url) = result {
                plugins.offer(url) { model.showNotice($0, warning: true) }
            }
        }
        .onAppear { plugins.read() }
    }
}

/// A plugin: its name and tag, what it does, what is the matter with it, its controls whatever
/// they are, and the rows that turn it on or off and remove it.
private struct PluginCard: View {
    let model: CanvasModel
    let plugin: PluginInfo

    private var line: some View {
        Rectangle().fill(Pc.actionsSeparator).frame(height: 1)
    }

    private var about: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(verbatim: plugin.name)
                .font(.system(size: 17, weight: .semibold))
                .foregroundStyle(Pc.panelTitle)
                .lineLimit(1)
            Text(verbatim: plugin.description)
                .font(.system(size: 14))
                .foregroundStyle(Pc.actionsRow)
                .padding(.top, 6)
            if let problem = plugin.problem {
                Text(verbatim: problem)
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(Pc.slideValue)
                    .padding(.top, 4)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        // held, the card is lifted by its name to another place
        .draggable(plugin.id)
    }

    private func control(_ control: PluginControl) -> some View {
        PluginControlView(model: model, control: control)
    }

    /// What it does when tapped, under its controls.
    private func action(_ action: PluginAction) -> some View {
        PluginActionRow(label: action.label) { model.runPluginAction(action.key) }
    }

    /// One of its features (interface 16): its name and words, and the switch that turns it on.
    private func featureHead(_ feature: PluginFeature) -> some View {
        let one = feature.control
        return HStack(spacing: 8) {
            VStack(alignment: .leading, spacing: 4) {
                Text(verbatim: one.label)
                    .font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(Pc.panelTitle)
                    .lineLimit(1)
                if !feature.description.isEmpty {
                    Text(verbatim: feature.description)
                        .font(.system(size: 14))
                        .foregroundStyle(Pc.actionsRow)
                }
            }
            Spacer(minLength: 0)
            PcSwitch(on: one.on)
        }
        .padding(.leading, 16)
        .padding(.trailing, 8)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .onTapGesture { PluginsModel.shared.setSwitch(one.key, !one.on) }
    }

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: 22, style: .circular)
        let plugins = PluginsModel.shared
        VStack(spacing: 0) {
            about
            // a filter's own controls are on its panel; what the card shows comes with the list
            if plugin.on, plugin.problem == nil, !plugin.features.isEmpty {
                // its own, under none of its features (a folder of the whole plugin's)
                ForEach(plugin.top.indices, id: \.self) { i in
                    line
                    control(plugin.top[i])
                }
                // made of features: each a heading with its switch, what comes under it while it is on
                ForEach(plugin.features.indices, id: \.self) { f in
                    let feature = plugin.features[f]
                    line
                    featureHead(feature)
                    if feature.control.on {
                        ForEach(feature.controls.indices, id: \.self) { i in
                            line
                            control(feature.controls[i])
                        }
                        ForEach(feature.actions.indices, id: \.self) { i in
                            line
                            action(feature.actions[i])
                        }
                    }
                }
            } else if plugin.on, plugin.problem == nil {
                ForEach(plugin.controls.indices, id: \.self) { i in
                    line
                    control(plugin.controls[i])
                }
                ForEach(plugin.actions.indices, id: \.self) { i in
                    line
                    action(plugin.actions[i])
                }
            }
            line
            RowView(row: .act(plugin.on ? "Turn off" : "Turn on", action: { plugins.setOn(plugin, !plugin.on) }))
            if plugin.source == .file {
                line
                // asked first, as on the other apps: the plugin and its settings go
                RowView(row: .act("Remove", action: { plugins.removing = plugin }))
            }
        }
        .background(shape.fill(Pc.actionsGroup))
        .clipShape(shape)
    }
}

/// A file control on a card (a sound or a picture): its label, then the name of what was picked
/// and Clear, or Import; a tap on the row picks another.
struct PluginFileRow: View {
    let file: PluginFile
    let told: (String) -> Void

    @State private var picking = false

    private var picture: Bool {
        file.accepts == .picture
    }

    var body: some View {
        HStack(spacing: 0) {
            Text(verbatim: file.label)
                .font(.system(size: 17))
                .foregroundStyle(Pc.actionsRow)
                .lineLimit(1)
            Spacer(minLength: 0)
            if let chosen = file.chosen {
                Text(verbatim: chosen)
                    .font(.system(size: 15))
                    .foregroundStyle(Pc.slideValue)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .padding(.leading, 12)
                Text(verbatim: "Clear")
                    .font(.system(size: 17))
                    .foregroundStyle(Pc.accent)
                    .lineLimit(1)
                    .padding(.leading, 12)
                    .contentShape(Rectangle())
                    .onTapGesture { PluginsModel.shared.clearFile(file.key) }
            } else {
                Text(verbatim: "Import")
                    .font(.system(size: 17))
                    .foregroundStyle(Pc.accent)
                    .lineLimit(1)
            }
        }
        .padding(.horizontal, 16)
        .frame(height: 44)
        .contentShape(Rectangle())
        .onTapGesture { picking = true }
        .fileImporter(isPresented: $picking, allowedContentTypes: [picture ? UTType.image : UTType.audio]) { result in
            guard case let .success(url) = result else { return }
            if picture {
                PluginsModel.shared.pickPicture(file.key, from: url, told: told)
            } else {
                PluginsModel.shared.pickSound(file.key, from: url, told: told)
            }
        }
    }
}

/// A folder control on a card: its label, then the folder's name and Clear, or Choose; a tap on the
/// row picks another with the document picker.
struct PluginFolderRow: View {
    let folder: PluginFolder
    let told: (String) -> Void

    @State private var picking = false

    var body: some View {
        HStack(spacing: 0) {
            Text(verbatim: folder.label)
                .font(.system(size: 17))
                .foregroundStyle(Pc.actionsRow)
                .lineLimit(1)
            Spacer(minLength: 0)
            if let chosen = folder.chosen {
                Text(verbatim: chosen)
                    .font(.system(size: 15))
                    .foregroundStyle(Pc.slideValue)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .padding(.leading, 12)
                Text(verbatim: "Clear")
                    .font(.system(size: 17))
                    .foregroundStyle(Pc.accent)
                    .lineLimit(1)
                    .padding(.leading, 12)
                    .contentShape(Rectangle())
                    .onTapGesture { PluginsModel.shared.clearFile(folder.key) }
            } else {
                Text(verbatim: "Choose")
                    .font(.system(size: 17))
                    .foregroundStyle(Pc.accent)
                    .lineLimit(1)
            }
        }
        .padding(.horizontal, 16)
        .frame(height: 44)
        .contentShape(Rectangle())
        .onTapGesture { picking = true }
        .fileImporter(isPresented: $picking, allowedContentTypes: [UTType.folder]) { result in
            guard case let .success(url) = result else { return }
            PluginsModel.shared.pickFolder(folder.key, from: url, told: told)
        }
    }
}

/// A slider on a card: the row follows the finger, and the value goes in as it lets go, as what it
/// sets may move the row itself, unless it is live.
struct PluginSliderRow: View {
    let slider: PluginSlider
    let onSet: (PluginSlider, Float) -> Void

    @State private var moved: Float?

    var body: some View {
        let value = moved ?? slider.value
        ActionSlideRow(
            label: slider.label,
            value: pluginSliderShown(slider: slider, value: value),
            share: pluginSliderAlong(slider: slider, value: value),
            change: { along in
                let to = pluginSliderAt(slider: slider, along: along)
                if slider.live {
                    onSet(slider, to)
                } else {
                    moved = to
                }
            },
            onDone: {
                if let moved {
                    onSet(slider, moved)
                }
                moved = nil
            }
        )
    }
}

/// The one screen that asks whether to import a plugin file: what it is, and what it can do. Also
/// where a setting that has moved to a plugin is said, once, and where a texture control's sheet
/// opens over the whole window.
struct PluginAlerts: View {
    @State private var plugins = PluginsModel.shared
    /// What went wrong keeping a file.
    @State private var failed: String?
    /// A plugin asked about that couldn't be removed.
    @State private var notRemoved = false

    var body: some View {
        ZStack {
            if let ask = plugins.texturing {
                // laid out at the interface's size, from the window's corner
                GeometryReader { place in
                    TextureLibrary(ask: ask)
                        .sized(Interface.shared.screen(place.size))
                }
                .ignoresSafeArea()
            }
            AlertView(
                content: plugins.movedNotice.map {
                    AlertContent(title: "Now a plugin", message: $0, action: "OK", destructive: false, alone: true)
                },
                onCancel: { plugins.movedNoticeRead() },
                onAction: { plugins.movedNoticeRead() }
            )
            AlertView(
                content: plugins.offer.map {
                    AlertContent(title: $0.name, message: $0.message, action: "Import", destructive: false)
                },
                onCancel: { plugins.declineOffer() },
                onAction: { failed = plugins.acceptOffer() }
            )
            AlertView(
                content: failed.map {
                    AlertContent(title: "Can't import this file", message: $0, action: "OK", destructive: false, alone: true)
                },
                onCancel: { failed = nil },
                onAction: { failed = nil }
            )
            AlertView(
                content: plugins.removing.map {
                    AlertContent(
                        title: "Remove this plugin?",
                        message: "“\($0.name)” goes, and its settings with it.",
                        action: "Remove",
                        icon: PcIcons.trash
                    )
                },
                onCancel: { plugins.removing = nil },
                onAction: {
                    if let plugin = plugins.removing, !plugins.remove(plugin) {
                        notRemoved = true
                    }
                    plugins.removing = nil
                }
            )
            AlertView(
                content: notRemoved
                    ? AlertContent(title: "This plugin couldn't be removed.", action: "OK", destructive: false, alone: true)
                    : nil,
                onCancel: { notRemoved = false },
                onAction: { notRemoved = false }
            )
        }
    }
}

extension CanvasModel {
    /// The session goes by the plugins that reach a canvas.
    func applyPlugins() {
        let plugins = PluginsModel.shared.store
        session.usePlugins(plugins: plugins)
        session.setInterfaceSize(size: Float(Interface.shared.scale))
        // what they show changes with them: the layers' rows (a label a plugin gives), and the
        // library, once a plugin turned on gave it the brushes it comes with
        readLayers()
        if session.takeBrushesChanged() {
            libraryChanged()
        }
        followStrokeSound()
        // how they lay the floating panels out
        followLayout()
    }

    /// Plays the artwork's strokes while a plugin makes their sound (and the app is in front).
    func followStrokeSound() {
        StrokeSound.shared.follow(session, on: PluginsModel.shared.store.makesStrokeSound())
    }

    /// A plugin's action run on the artwork (a row tapped), off the main thread (it may take
    /// seconds): the panel closes so what it does shows, and what it says, or its stopping, is told.
    func runPluginAction(_ key: String) {
        runPlugin(key, at: nil)
    }

    /// A Pencil gesture set to a plugin's action: it runs where the Pencil is (window pixels), where
    /// a ring it shows opens.
    func runPluginAction(_ key: String, x: Float, y: Float) {
        runPlugin(key, at: CGPoint(x: CGFloat(x), y: CGFloat(y)))
    }

    private func runPlugin(_ key: String, at: CGPoint?) {
        closePanels()
        let plugins = PluginsModel.shared
        let store = plugins.store
        let session = session
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            if let at {
                _ = store.runActionAt(key: key, session: session, x: Float(at.x), y: Float(at.y))
            } else {
                _ = store.runAction(key: key, session: session)
            }
            DispatchQueue.main.async {
                self?.afterPlugin(session)
                // an action may turn the view, settling into place
                self?.followView()
            }
        }
    }

    /// Row `row` of the plugin's panel touched with `value`: told to the plugin off the main
    /// thread, and the panel shown as it answers (or closed).
    func touchPluginPanel(_ row: Int, _ value: Float) {
        let store = PluginsModel.shared.store
        let session = session
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            _ = store.panelTouched(row: UInt32(max(row, 0)), value: value, session: session)
            DispatchQueue.main.async { self?.afterPlugin(session) }
        }
    }

    /// The key `press` the plugin's panel asked for: told to the plugin off the main thread (as a
    /// touch on the panel is), and the panel shown as it answers.
    func pluginPanelKey(_ press: KeyCombo) {
        let store = PluginsModel.shared.store
        let session = session
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            _ = store.panelKey(press: press, session: session)
            DispatchQueue.main.async { self?.afterPlugin(session) }
        }
    }

    func closePluginPanel() {
        PluginsModel.shared.closePanel()
    }

    /// What a plugin's action or panel changed, read again: its panel, the controls it set, the
    /// colour and brush, what it said, and the library if it changed a brush.
    private func afterPlugin(_ session: CanvasSession) {
        guard session === self.session else { return }
        PluginsModel.shared.followRun()
        followPluginBrush()
        if let failure = session.takeFailure() {
            showNotice(failure, warning: true)
        }
        if session.takeBrushesChanged() {
            libraryChanged()
        }
        refresh()
    }

    func setPluginFilterSlider(_ key: String, _ value: Float) {
        _ = session.pluginFilterSlider(key: key, value: value, done: false)
        readAdjust()
    }

    /// The finger left the slider `key`: a filter too slow to follow it redraws now.
    func letGoPluginFilter(_ key: String) {
        for control in adjustShown?.plugin?.controls ?? [] {
            if case let .slider(slider) = control, slider.key == key {
                _ = session.pluginFilterSlider(key: key, value: slider.value, done: true)
            }
        }
    }

    func setPluginFilterSwitch(_ key: String, _ on: Bool) {
        session.pluginFilterSwitch(key: key, on: on)
        readAdjust()
    }

    /// A filter that stopped was closed by the core, which says why.
    func checkPluginFilter() {
        guard adjustment == .plugin, session.adjustmentShown().adjustment == nil else { return }
        if let failure = session.takeFailure() {
            showNotice(failure, warning: true)
        }
        adjustMenuAt = nil
        readAdjust()
        refresh()
    }
}
