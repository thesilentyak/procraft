import SwiftUI
import UIKit

private let actionsWidth: CGFloat = 390

/// One row of the actions panel. Rows whose features are still to come on the iPad show dimmed.
enum ActionRow {
    /// A button row (dimmed without an action), with its value on the right if it has one.
    case act(String, value: String? = nil, action: (() -> Void)?)
    /// A switch row (dimmed without an action).
    case toggle(String, on: Bool, change: ((Bool) -> Void)?)
    /// A button row that a swipe to the left slides open, its private action's button coming in.
    case privately(String, label: String, action: (() -> Void)?, onPrivate: (() -> Void)?)
    /// A slider along the row's foot, its value at the right.
    case slide(String, value: String, share: Float, change: (Float) -> Void)
    /// A group's heading, above it.
    case heading(String)
}

private struct ActionsTab {
    let label: String
    let icon: PcIcon
}

private let tabs = [
    ActionsTab(label: "Add", icon: PcIcons.tabAdd),
    ActionsTab(label: "Canvas", icon: PcIcons.tabCanvas),
    ActionsTab(label: "Share", icon: PcIcons.tabShare),
    ActionsTab(label: "Video", icon: PcIcons.tabVideo),
    ActionsTab(label: "Prefs", icon: PcIcons.tabPrefs),
    ActionsTab(label: "Help", icon: PcIcons.tabHelp),
]

/// On a model, its own tab takes Video's place.
private let modelTabs = tabs.map { $0.label == "Video" ? ActionsTab(label: "3D", icon: PcIcons.tab3D) : $0 }

/// Each of the panel's pages' heights as laid out, for the panel to take a page's height as it
/// turns to it.
private struct PageHeights: PreferenceKey {
    static let defaultValue: [ActionsPage: CGFloat] = [:]
    static func reduce(value: inout [ActionsPage: CGFloat], nextValue: () -> [ActionsPage: CGFloat]) {
        value.merge(nextValue()) { $1 }
    }
}

/// What the panel keeps as it turns pages and changes tabs.
@Observable
private final class ActionsUi {
    /// Each page's height as laid out, and the height the panel has (none until laid out).
    var heights: [ActionsPage: CGFloat] = [:]
    var height: CGFloat?
    /// A page is sliding in: the panel's height follows it.
    var turning = false
    /// The tab whose rows show, none for the chosen one: changing tabs, the old rows fade out first.
    var shownTab: Int?
    var rowsShown = true
    /// The tab under a finger: it lights at once, the chosen one staying lit until let go.
    var pressedTab: Int?
}

/// The actions panel: six tabs of grouped rows (adding, the canvas, sharing, video, preferences,
/// help). A new tab's rows fade in as the old ones fade out; a page (the cursor's, pressure and
/// smoothing, plugins) slides in from the right as the main page slides off, and back.
struct ActionsPanel: View {
    let model: CanvasModel
    let arrowX: CGFloat
    /// As tall as it may be: past that its rows scroll.
    var maxHeight: CGFloat = .infinity

    #if DEBUG
    @State private var page = ScreenCapture.actionsPage
    #else
    @State private var page = ActionsPage.main
    #endif
    @State private var ui = ActionsUi()
    /// Brush scaling's menu is open.
    @State private var scalingMenu = false

    /// The title and the tabs over the main page's rows, and a page's title over its own.
    private let mainHead: CGFloat = 116
    private let pageHead: CGFloat = 48

    /// The tab whose rows show.
    private var tab: Int {
        ui.shownTab ?? model.actionsTab
    }

    /// Brush scaling's choices beside its row, the third of the group: its left edge 54 in from the
    /// row's right end.
    private var scalingChoices: some View {
        ZStack(alignment: .topLeading) {
            MenuDismissal { scalingMenu = false }
            GlassMenu(overPanel: true, width: 254) {
                ForEach(0 ..< 2, id: \.self) { i in
                    let chosen = AppSettings.shared.brushOnScreen == (i == 1)
                    HStack(spacing: 0) {
                        Text(verbatim: i == 0 ? "Relative to canvas" : "Relative to screen")
                            .font(.system(size: 17))
                            .foregroundStyle(Pc.text)
                            .lineLimit(1)
                        Spacer(minLength: 0)
                        if chosen {
                            PcIconView(PcIcons.menuTick, size: 24, color: Pc.accent)
                        }
                    }
                    .padding(.leading, 20)
                    .padding(.trailing, 19)
                    .frame(height: 40)
                    .contentShape(Rectangle())
                    .onTapGesture {
                        model.changeBrushScaling(onScreen: i == 1)
                        scalingMenu = false
                    }
                }
            }
            .offset(x: actionsWidth - 12 - 54, y: 218)
        }
    }

    /// A page's height read as it is laid out.
    private func measured(_ which: ActionsPage) -> some View {
        GeometryReader { g in
            Color.clear.preference(key: PageHeights.self, value: [which: g.size.height])
        }
    }

    /// The pages as laid out: the panel takes the shown one's height, following it as it slides in.
    private func took(_ heights: [ActionsPage: CGFloat]) {
        ui.heights.merge(heights) { $1 }
        guard let h = heights[page], h != ui.height else { return }
        if ui.height != nil, ui.turning {
            withAnimation(Motion.panelPage) { ui.height = h }
        } else {
            ui.height = h
        }
    }

    /// Another page slides in from the right as the one shown slides off to the left; back, the
    /// page goes at once and the main page slides in from the left. The panel's height follows.
    private func turn(to next: ActionsPage) {
        scalingMenu = false
        ui.turning = true
        withAnimation(Motion.panelPage, completionCriteria: .logicallyComplete) {
            page = next
            if let h = ui.heights[next] {
                ui.height = h
            }
        } completion: {
            ui.turning = false
        }
    }

    private func back() {
        turn(to: .main)
    }

    /// A page coming in from the right, and going at once when left.
    private var pushed: AnyTransition {
        .asymmetric(insertion: .move(edge: .trailing), removal: .opacity.animation(.linear(duration: 0.01)))
    }

    var body: some View {
        let room = maxHeight - Pc.arrowHeight
        Popover(width: actionsWidth, arrowX: arrowX) {
            ZStack(alignment: .top) {
                switch page {
                case .main:
                    main(rowsMost: room - mainHead)
                        .fixedSize(horizontal: false, vertical: true)
                        .background(measured(.main))
                        .transition(.move(edge: .leading))
                case .cursor:
                    CursorPage { back() }
                        .fixedSize(horizontal: false, vertical: true)
                        .background(measured(.cursor))
                        .transition(pushed)
                case .pressure:
                    // its title sits 8 lower than the other pages'
                    PressurePage(model: model, most: room - pageHead - 8) { back() }
                        .fixedSize(horizontal: false, vertical: true)
                        .background(measured(.pressure))
                        .transition(pushed)
                case .plugins:
                    PluginsPage(model: model, most: room - pageHead) { back() }
                        .fixedSize(horizontal: false, vertical: true)
                        .background(measured(.plugins))
                        .transition(pushed)
                }
            }
            .frame(height: ui.height, alignment: .top)
            .onPreferenceChange(PageHeights.self) { took($0) }
        }
        .overlay(alignment: .topLeading) {
            if scalingMenu {
                scalingChoices
            }
        }
    }

    /// A tab chosen: it lights and the old one goes grey; the old rows fade out, the panel takes the
    /// new rows' size, and they fade in.
    private func choose(_ i: Int) {
        guard i != model.actionsTab else { return }
        scalingMenu = false
        ui.shownTab = tab
        model.actionsTab = i
        withAnimation(Motion.actionsTabOut, completionCriteria: .logicallyComplete) {
            ui.rowsShown = false
        } completion: {
            // the chosen tab's rows from now on, whoever changes it next
            ui.shownTab = nil
            withAnimation(Motion.actionsTabIn) { ui.rowsShown = true }
        }
    }

    /// A tab: lit as it is touched, chosen as it is let go over it.
    private func tabButton(_ i: Int, _ tab: ActionsTab) -> some View {
        let lit = i == model.actionsTab || i == ui.pressedTab
        let bounds = CGRect(x: 0, y: 0, width: 54, height: 60)
        return ZStack(alignment: .top) {
            // a 50 frame centred 24 down
            PcIconView(tab.icon, size: 50, color: lit ? Pc.accent : Pc.tabIdle)
                .offset(y: -1)
            Text(verbatim: tab.label)
                .font(.system(size: 12))
                .foregroundStyle(lit ? Pc.text : Pc.tabIdle)
                .padding(.top, 39.5)
        }
        .animation(lit ? Motion.highlightOn : Motion.highlightOff, value: lit)
        .frame(width: 54, height: 60, alignment: .top)
        .contentShape(Rectangle())
        .gesture(
            DragGesture(minimumDistance: 0)
                .onChanged { drag in
                    ui.pressedTab = bounds.contains(drag.location) ? i : nil
                }
                .onEnded { drag in
                    ui.pressedTab = nil
                    if bounds.contains(drag.location) {
                        choose(i)
                    }
                }
        )
    }

    private func main(rowsMost: CGFloat) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(verbatim: "Actions")
                .font(.system(size: 24, weight: .bold))
                .foregroundStyle(Pc.panelTitle)
                .frame(height: 34)
                .padding(.leading, 24)
                .padding(.top, 22)
            HStack(spacing: 10) {
                ForEach(Array((model.scene.open ? modelTabs : tabs).enumerated()), id: \.offset) { i, tab in
                    tabButton(i, tab)
                }
            }
            .padding(.leading, 10)
            .frame(height: 60, alignment: .top)
            ScrollView {
                VStack(spacing: 20) {
                    ForEach(Array(groups().enumerated()), id: \.offset) { _, rows in
                        ActionsGroup(rows: rows)
                    }
                    // the plugins' own rows in the Canvas tab, after its own
                    if tab == 1, !model.scene.open {
                        // and Edit Layout, while a plugin lets the bars move
                        PluginPlaceRows(
                            model: model, place: .canvas,
                            more: model.layoutLook.edit ? ("Edit Layout", { model.startEditLayout() }) : nil
                        )
                    }
                }
                .padding(.horizontal, 12)
                .padding(.top, 20)
                .padding(.bottom, 11)
            }
            .scrollIndicators(.hidden)
            .frame(maxHeight: max(rowsMost, 88))
            .fixedSize(horizontal: false, vertical: true)
            .opacity(ui.rowsShown ? 1 : 0)
        }
    }

    /// Reference turned on: the panel closes at once as the window fades in.
    private func showReference(_ on: Bool) {
        if on {
            model.closePanels()
        }
        model.reference.setOn(on)
    }

    private func ask(_ how: InsertAsk, _ isPrivate: Bool) {
        model.insertPrivate = isPrivate
        model.insertAsk = how
    }

    /// The log as a text file, to the share sheet; made off the main thread (the panel may close
    /// meanwhile).
    private func shareLog() {
        let model = model
        DispatchQueue.global(qos: .userInitiated).async {
            let log = AppLog.toShare()
            DispatchQueue.main.async {
                if let log {
                    Sharing.share([log])
                } else {
                    model.saying = ("Share log", "The log couldn't be made ready to share.")
                }
            }
        }
    }

    /// The shown tab's groups of rows.
    private func groups() -> [[ActionRow]] {
        let settings = AppSettings.shared
        let undoRange = AppSettings.rapidUndoRange
        if model.scene.open, tab <= 3 {
            return modelGroups(tab)
        }
        switch tab {
        case 0:
            return [
                // swiped to the left, the first three and Paste put what they bring on a private layer
                [
                    .privately("Insert a file", label: "Insert a private file", action: { ask(.file, false) }, onPrivate: { ask(.file, true) }),
                    .privately("Insert a photo", label: "Insert a private photo", action: { ask(.photo, false) }, onPrivate: { ask(.photo, true) }),
                    .privately("Take a photo", label: "Take a private photo", action: { ask(.camera, false) }, onPrivate: { ask(.camera, true) }),
                    .act("Add text", action: { model.text.add() }),
                ],
                // Paste is grey until something has been cut or copied
                [
                    .act("Cut", action: { model.cutLayer() }),
                    .act("Copy", action: { model.copyLayer() }),
                    .act("Copy canvas", action: { model.copyCanvas() }),
                    .privately(
                        "Paste", label: "Paste Private",
                        action: model.canPaste ? { model.paste() } : nil,
                        onPrivate: model.canPaste ? { model.paste(isPrivate: true) } : nil
                    ),
                ],
            ]
        case 1:
            return [
                [.act("Crop & Resize", action: { model.crop.begin() })],
                [
                    .toggle("Animation Assist", on: model.animationAssist.settings?.pages == false, change: {
                        model.animationAssist.setAssist($0, pages: false)
                    }),
                    .toggle("Page Assist", on: model.animationAssist.settings?.pages == true, change: {
                        model.animationAssist.setAssist($0, pages: true)
                    }),
                    .toggle("Drawing Guide", on: model.guideOn, change: { model.setGuideOn($0) }),
                    // only with the guide on
                    .act("Edit Drawing Guide", action: model.guideOn ? { model.guide.open() } : nil),
                ],
                [.toggle("Reference", on: model.reference.on, change: { showReference($0) })],
                [
                    .act("Flip horizontal", action: { model.flipCanvas(horizontal: true) }),
                    .act("Flip vertical", action: { model.flipCanvas(horizontal: false) }),
                ],
                [.act("Canvas information", action: {
                    model.closePanels()
                    withAnimation(Motion.studioIn) { model.canvasInfoOpen = true }
                })],
            ]
        case 2:
            // files plugins make of the artwork's layers of kinds they add, under their own names
            let made: [ActionRow] = model.session.pluginExports().map { e in
                ActionRow.act(e.label, action: { model.sharePlugin(e) })
            }
            var share: [[ActionRow]] = [
                [
                    .heading("Share Image"),
                    // ours first, then the same package named for the other app
                    .act("ProCraft", action: { model.sharePackage(suffix: ".procraft") }),
                    .act("Procreate", action: { model.sharePackage(suffix: ".procreate") }),
                    .act("PSD", action: { model.share(.psd) }),
                    .act("PDF", action: { model.pdfAsking = .pdf }),
                    .act("JPEG", action: { model.share(.jpeg) }),
                    .act("PNG", action: { model.share(.png) }),
                    .act("TIFF", action: { model.share(.tiff) }),
                ],
                [
                    .heading("Share Layers"),
                    .act("PDF", action: { model.pdfAsking = .layersPdf }),
                    .act("PNG Files", action: { model.share(.layerPngs) }),
                    .act("Animated GIF", action: { model.openAnimatedExport(.gif) }),
                    .act("Animated PNG", action: { model.openAnimatedExport(.png) }),
                    .act("Animated MP4", action: { model.openAnimatedExport(.mp4) }),
                    .act("Animated HEVC", action: { model.openAnimatedExport(.hevc) }),
                ],
            ]
            // a plugin's artwork, its shapes kept, after TIFF; then what plugins make of its layers
            if model.session.exportsSvg() {
                share[0].append(.act("SVG", action: { model.share(.svg) }))
            }
            share[0] += made
            return share
        case 3:
            return [
                [
                    // Replay and Export wait for something recorded
                    .act("Time-lapse Replay", action: model.timeLapse.recorded ? {
                        // the panel fades away as the replay takes the screen
                        model.fadePanels()
                        withAnimation(Motion.replayFade) { model.timeLapse.replaying = true }
                    } : nil),
                    .toggle("Time-lapse Recording", on: model.timeLapse.recording, change: { model.timeLapse.setRecording($0) }),
                ],
                [.act("Export time-lapse video", action: model.timeLapse.recorded ? { model.timeLapse.lengthAsking = true } : nil)],
            ]
        case 4:
            return [
                [
                    .toggle("Light interface", on: AppSettings.shared.lightInterface, change: { model.changeLightInterface($0) }),
                    .toggle("Right-hand interface", on: AppSettings.shared.rightHanded, change: {
                        AppSettings.shared.rightHanded = $0
                        model.applyFitInsets()
                    }),
                    .act("Brush scaling", value: AppSettings.shared.brushOnScreen ? "Screen" : "Canvas", action: { scalingMenu = true }),
                    .toggle("Project canvas", on: AppSettings.shared.projectCanvas, change: { model.changeProjectCanvas($0) }),
                    .toggle("Brush cursor", on: AppSettings.shared.brushCursorOn, change: { AppSettings.shared.brushCursorOn = $0 }),
                    .act("Advanced cursor settings", action: { turn(to: .cursor) }),
                ],
                [
                    .act("Pressure and Smoothing", action: { turn(to: .pressure) }),
                    .act("Gesture controls", action: {
                        withAnimation(Motion.studioIn) { model.gestureControlsOpen = true }
                    }),
                    .act("Plugins", action: { turn(to: .plugins) }),
                ],
                [
                    .slide(
                        "Rapid undo delay", value: String(format: "%.2fs", settings.rapidUndoDelay),
                        share: (settings.rapidUndoDelay - undoRange.lowerBound) / (undoRange.upperBound - undoRange.lowerBound),
                        change: { model.changeRapidUndoDelay($0) }
                    ),
                    .slide(
                        "Selection mask visibility", value: String(format: "%.1f%%", settings.selectionVisibility * 100),
                        share: settings.selectionVisibility, change: { model.changeSelectionVisibility($0) }
                    ),
                ],
                [.toggle("Size & Opacity sidebar", on: model.showSideBar, change: { model.showSideBar = $0 })],
            ]
        default:
            return [
                [
                    // the app's own settings, in the system's Settings app
                    .act("Advanced settings", action: {
                        if let url = URL(string: UIApplication.openSettingsURLString) {
                            UIApplication.shared.open(url)
                        }
                    }),
                    .act("About ProCraft", action: {
                        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? ""
                        model.saying = ("ProCraft", "Version \(version)")
                    }),
                    .act("Licences", action: {
                        model.closePanels()
                        withAnimation(Motion.studioIn) { model.licencesOpen = true }
                    }),
                ],
                [.act("Share log", action: { shareLog() })],
            ]
        }
    }
}

extension ActionsPanel {
    /// The first four tabs' rows on a model: adding without text or private layers, the canvas's
    /// reference and information, sharing the model, pictures of it and its textures, and the 3D
    /// tab (the 2D texture, painting through, lighting, the model in the room). Prefs and Help are
    /// as on a canvas.
    fileprivate func modelGroups(_ tab: Int) -> [[ActionRow]] {
        let scene = model.scene
        switch tab {
        case 0:
            return [
                [
                    .act("Insert a file", action: { insertOnModel(.file) }),
                    .act("Insert a photo", action: { insertOnModel(.photo) }),
                    .act("Take a photo", action: { insertOnModel(.camera) }),
                ],
                [
                    .act("Cut", action: { model.cutLayer() }),
                    .act("Copy", action: { model.copyLayer() }),
                    .act("Copy canvas", action: { model.copyCanvas() }),
                    .act("Paste", action: model.canPaste ? { model.paste() } : nil),
                ],
            ]
        case 1:
            return [
                [.toggle("Reference", on: model.reference.on, change: { showReference($0) })],
                [.act("Canvas information", action: {
                    model.closePanels()
                    withAnimation(Motion.studioIn) { model.canvasInfoOpen = true }
                })],
            ]
        case 2:
            return [
                [
                    .heading("Share Model"),
                    // ours comes back whole in another copy of the app; the other package holds no model
                    .act("ProCraft", action: { model.shareModel(.artwork) }),
                    .act("Procreate", action: nil),
                    .act("USDZ", action: { model.shareModel(.usdz) }),
                    .act("OBJ", action: { model.shareModel(.obj) }),
                ],
                [
                    .heading("Share Image"),
                    .act("JPEG", action: { model.shareModel(.jpeg) }),
                    .act("PNG", action: { model.shareModel(.png) }),
                    .act("TIFF", action: { model.shareModel(.tiff) }),
                    .act("Animated GIF", action: { scene.openAnimatedExport(.gif) }),
                    .act("Animated PNG", action: { scene.openAnimatedExport(.png) }),
                    .act("Animated MP4", action: { scene.openAnimatedExport(.mp4) }),
                    .act("Animated HEVC", action: { scene.openAnimatedExport(.hevc) }),
                ],
                [.heading("Share Textures"), .act("PNG", action: { model.shareModel(.textures) })],
            ]
        default:
            // showing the 2D texture, the rest is for the model
            let shown = !scene.flat
            return [
                [
                    .toggle("Show 2D texture", on: scene.flat, change: { scene.setFlat($0) }),
                    .toggle("Paint through mesh", on: scene.paintThrough, change: shown ? { scene.setPaintThrough($0) } : nil),
                    .act("Edit lighting & environment", action: shown ? { scene.openStudio() } : nil),
                ],
                [.act("View in AR", action: { model.viewModelInAR() })],
            ]
        }
    }

    /// A picture brought in from the Add tab on a model: placed on its surface, never private.
    private func insertOnModel(_ how: InsertAsk) {
        model.insertPrivate = false
        model.insertAsk = how
    }
}

/// A group of rows on the panel's dark wash, a faint line between rows; one led by a heading shows
/// it above the group, not as a row of it.
private struct ActionsGroup: View {
    let rows: [ActionRow]

    var body: some View {
        let heading: String? = if case let .heading(label) = rows.first { label } else { nil }
        let listed = heading == nil ? rows : Array(rows.dropFirst())
        VStack(alignment: .leading, spacing: 0) {
            if let heading {
                Text(verbatim: heading)
                    .font(.system(size: 14, weight: .bold))
                    .foregroundStyle(Pc.panelTitle)
                    .padding(.leading, 13)
                    .padding(.top, 7)
                    .frame(height: 45, alignment: .topLeading)
            }
            VStack(spacing: 0) {
                ForEach(Array(listed.enumerated()), id: \.offset) { i, row in
                    if i > 0 {
                        Rectangle().fill(Pc.actionsSeparator).frame(height: 1)
                    }
                    RowView(row: row)
                }
            }
            .background(RoundedRectangle(cornerRadius: 22, style: .circular).fill(Pc.actionsGroup))
            .clipShape(RoundedRectangle(cornerRadius: 22, style: .circular))
        }
    }
}

/// A row's value at its right (Brush scaling's): a translucent light grey, 116 over the dark rows
/// and 129 over the panel's lighter glass, as measured.
private var rowValue: Color {
    Look.shared.light ? Pc.tabIdle : Color(argb: 0x7ADA_DADA)
}

/// A row: its label, and a value or a switch at its right; a button row turns blue while held,
/// lighting in a fifteenth of a second and going over 0.15 s when let go.
struct RowView: View {
    let row: ActionRow
    @State private var held = false

    var body: some View {
        switch row {
        case let .act(label, value, action):
            let enabled = action != nil
            HStack(spacing: 0) {
                Text(verbatim: label)
                    .font(.system(size: 17))
                    .foregroundStyle(held ? Color.white : enabled ? Pc.actionsRow : Pc.actionsDisabled)
                    .lineLimit(1)
                Spacer(minLength: 0)
                if let value {
                    Text(verbatim: value)
                        .font(.system(size: 15))
                        .foregroundStyle(held ? Color.white : rowValue)
                        .padding(.trailing, 6)
                }
            }
            .padding(.leading, 16)
            .padding(.trailing, 8)
            .frame(height: 44)
            .background(held ? Pc.accent : Color.clear)
            .animation(held ? Motion.highlightOn : Motion.highlightOff, value: held)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { _ in if enabled { held = true } }
                    .onEnded { drag in
                        held = false
                        if abs(drag.translation.height) < 10, abs(drag.translation.width) < 10 {
                            action?()
                        }
                    }
            )
        case let .toggle(label, on, change):
            HStack(spacing: 0) {
                Text(verbatim: label)
                    .font(.system(size: 17))
                    .foregroundStyle(change != nil ? Pc.actionsRow : Pc.actionsDisabled)
                    .lineLimit(1)
                Spacer(minLength: 0)
                PcSwitch(on: on, enabled: change != nil)
            }
            .padding(.leading, 16)
            .padding(.trailing, 8)
            .frame(height: 44)
            .contentShape(Rectangle())
            .onTapGesture { change?(!on) }
        case let .privately(label, privateLabel, action, onPrivate):
            PrivateRow(label: label, privateLabel: privateLabel, action: action, onPrivate: onPrivate)
        case let .slide(label, value, share, change):
            ActionSlideRow(label: label, value: value, share: share, change: change)
        case let .heading(label):
            Text(verbatim: label)
                .font(.system(size: 17, weight: .semibold))
                .foregroundStyle(Pc.panelTitle)
                .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
                .padding(.leading, 16)
        }
    }
}

/// A row that a swipe to the left slides open, its private action's button coming in from the
/// right; let go past halfway it stays open. A tap on the row closes it again.
private struct PrivateRow: View {
    let label: String
    let privateLabel: String
    let action: (() -> Void)?
    let onPrivate: (() -> Void)?

    private let full: CGFloat = 248

    @State private var slide: CGFloat = 0
    @State private var from: CGFloat?
    /// A finger down on it, not (yet) swiping: it fills blue, as a button row held does.
    @GestureState private var pressed = false

    var body: some View {
        let held = pressed && action != nil && slide == 0 && from == nil
        ZStack(alignment: .trailing) {
            Text(verbatim: label)
                .font(.system(size: 17))
                .foregroundStyle(held ? Color.white : action != nil ? Pc.actionsRow : Pc.actionsDisabled)
                .lineLimit(1)
                .padding(.leading, 16)
                .frame(maxWidth: .infinity, alignment: .leading)
                .frame(maxHeight: .infinity)
                .background(held ? Pc.accent : Color.clear)
                .animation(held ? Motion.highlightOn : Motion.highlightOff, value: held)
                .offset(x: -slide)
            if slide > 0 {
                Text(verbatim: privateLabel)
                    .font(.system(size: 17))
                    .foregroundStyle(onPrivate != nil ? Color(argb: 0xFFA8_A8A8) : Pc.actionsDisabled)
                    .lineLimit(1)
                    .frame(width: 245)
                    .frame(maxHeight: .infinity)
                    .background(RoundedRectangle(cornerRadius: 13, style: .circular).fill(Color(argb: 0xFF45_4545)))
                    .contentShape(Rectangle())
                    .onTapGesture {
                        slide = 0
                        onPrivate?()
                    }
                    .offset(x: full - slide)
            }
        }
        .frame(height: 44)
        .clipped()
        .contentShape(Rectangle())
        .onTapGesture {
            if slide > 0 {
                withAnimation(Motion.swipeSettle) { slide = 0 }
            } else {
                action?()
            }
        }
        .simultaneousGesture(
            DragGesture(minimumDistance: 10)
                .onChanged { drag in
                    guard from != nil || abs(drag.translation.width) > abs(drag.translation.height) else { return }
                    let start = from ?? slide
                    from = start
                    slide = min(max(start - drag.translation.width, 0), full)
                }
                .onEnded { _ in
                    guard from != nil else { return }
                    from = nil
                    withAnimation(Motion.swipeSettle) { slide = slide > full / 2 ? full : 0 }
                }
        )
        .simultaneousGesture(
            DragGesture(minimumDistance: 0)
                .updating($pressed) { drag, pressed, _ in
                    pressed = abs(drag.translation.width) < 10 && abs(drag.translation.height) < 10
                }
        )
    }
}

/// A row with a slider along its foot, 16 in from each side; a touch anywhere on it moves it.
struct ActionSlideRow: View {
    let label: String
    let value: String
    let share: Float
    let change: (Float) -> Void
    var onDone: (() -> Void)?

    var body: some View {
        GeometryReader { g in
            let w = g.size.width
            let span = max(w - 32, 1)
            let x = 16 + CGFloat(min(max(share, 0), 1)) * span
            ZStack(alignment: .topLeading) {
                Text(verbatim: label)
                    .font(.system(size: 17))
                    .foregroundStyle(Pc.slideLabel)
                    .lineLimit(1)
                    .padding(.leading, 16)
                    .padding(.top, 9)
                Text(verbatim: value)
                    .font(.system(size: 17))
                    .foregroundStyle(Pc.slideValue)
                    .lineLimit(1)
                    .padding(.trailing, 16)
                    .padding(.top, 9)
                    .frame(maxWidth: .infinity, alignment: .trailing)
                Rectangle().fill(Pc.slideTrack)
                    .frame(width: span, height: 3)
                    .offset(x: 16, y: 63 - 17.5 - 1.5)
                Rectangle().fill(Pc.accent)
                    .frame(width: max(x - 16, 0), height: 3)
                    .offset(x: 16, y: 63 - 17.5 - 1.5)
                Capsule().fill(Color.white)
                    .frame(width: 16, height: 10)
                    .offset(x: x - 8, y: 63 - 17.5 - 5)
            }
            .frame(width: w, height: 63, alignment: .topLeading)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { drag in
                        change(Float(min(max((drag.location.x - 16) / span, 0), 1)))
                    }
                    .onEnded { _ in onDone?() }
            )
        }
        .frame(height: 63)
    }
}

/// A 63 x 28 switch: a blue track when on, grey when off, with a white knob that slides across.
struct PcSwitch: View {
    let on: Bool
    var enabled = true
    /// Its colours where they aren't the usual ones: fixed off, and its track when off.
    var fixedTrack: Color?
    var fixedKnob: Color?
    var offTrack: Color?

    var body: some View {
        let track = !enabled ? fixedTrack ?? Pc.switchFixed : on ? Pc.accent : offTrack ?? Pc.switchOff
        ZStack(alignment: .leading) {
            Capsule().fill(track)
            Capsule()
                .fill(enabled ? Color.white : fixedKnob ?? Pc.switchFixedKnob)
                .frame(width: 37, height: 24)
                .offset(x: 2 + (on ? 22 : 0))
        }
        .frame(width: 63, height: 28)
        .animation(.easeInOut(duration: 0.2), value: on)
    }
}

/// The actions panel's width.
let actionsPanelWidth = actionsWidth
