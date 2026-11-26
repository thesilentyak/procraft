import PhotosUI
import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// The canvas filling the screen with the interface over it: the bar across the top and the side
/// bar. The artwork is kept saved as it changes, as the app goes into the background, and as it
/// is left.
struct CanvasScreen: View {
    let model: CanvasModel

    @State private var photo: PhotosPickerItem?

    private func asked(_ ask: InsertAsk) -> Binding<Bool> {
        Binding(get: { model.insertAsk == ask }, set: { if !$0 { model.insertAsk = nil } })
    }

    private func sharePdf(_ quality: UInt8) {
        if let kind = model.pdfAsking {
            model.share(kind, quality: quality)
        }
        model.pdfAsking = nil
    }

    var body: some View {
        GeometryReader { place in
            let screen = Interface.shared.screen(place.size)
            let right = AppSettings.shared.rightHanded
            let hidden = model.barsHidden
            // the side bar at its kept place down its edge; its modify button dragged, it follows
            // the finger up or down stretching a little toward it, and let go settles back
            let sideFit = model.sideBarPlace(height: screen.size.height)
            let sideMoving = model.sideBarMoving
            let sideStretch = CGFloat(sideMoving?.stretch ?? 0)
            ZStack(alignment: .topLeading) {
                // (a touch on the canvas while a panel is open waits: a tap only closes the panel, a
                // drag fades it and paints from where it came down, as the core's touches say)
                CanvasHost(model: model)
                if let pick = model.picking {
                    Loupe(pick: pick, current: CanvasSpace.shared.color(model.colorRgb()), pixels: model.pixels)
                    if !hidden {
                        PickingBar(fromReference: pick.fromReference)
                            .offset(y: Pc.barShown)
                    }
                }
                // with the interface hidden, a button in the top left brings it back
                if hidden {
                    ShowInterfaceButton(model: model)
                        .transition(.opacity)
                }
                GuideOverlay(model: model)
                if model.transforming {
                    TransformOverlay(model: model)
                }
                if model.selecting {
                    SelectionOverlay(model: model)
                }
                ColorDropBand(drop: model.drop, width: screen.size.width)
                // the top bar slides up off the screen and the side bar out to its side as they fade
                // (each half on glass of its own while moved, or collapsed into its side: Collapse to edge)
                if model.barMoved(.topLeft) == nil, model.barMoved(.topRight) == nil,
                   model.barCollapsed(.topLeft) == nil, model.barCollapsed(.topRight) == nil {
                    TopBar(model: model)
                        .offset(y: hidden ? -Pc.barHeight : 0)
                        .opacity(hidden ? 0 : 1)
                } else {
                    // moved by Edit layout: each half on glass of its own where it was left
                    ForEach([LayoutBar.topLeft, .topRight], id: \.self) { bar in
                        if let at = model.barMoved(bar) ?? model.barLayout.shown(bar: bar) {
                            let into = collapseOffset(model.barCollapsePlace(bar), model.barCollapsed(bar) != nil)
                            TopBarHalf(
                                model: model, left: bar == .topLeft, upright: at.vertical, screenWidth: screen.size.width,
                                edges: model.barTouching(bar)
                            )
                                .offset(x: CGFloat(at.x), y: CGFloat(at.y))
                                // collapsed: slid into its side
                                .offset(into)
                                .animation(collapseSlide, value: into)
                                .opacity(hidden ? 0 : 1)
                        }
                    }
                    .frame(width: screen.size.width, height: screen.size.height, alignment: .topLeading)
                }
                NoticeBand(model: model)
                if let moved = model.barMoved(.sideBar) {
                    // moved by Edit layout: where it was left, lying down turned a quarter about its
                    // top-left
                    // (square only on its sides against the window's edges, its own way up)
                    let edges = model.barTouching(.sideBar).map { moved.vertical ? $0 : turnedSides($0) }
                    let into = collapseOffset(model.barCollapsePlace(.sideBar), model.barCollapsed(.sideBar) != nil)
                    // (standing against a side: mirrored to that side)
                    let standsRight = moved.vertical ? (model.barTouching(.sideBar)?.right ?? right) : right
                    SideBar(model: model, right: standsRight, windowHeight: screen.size.height, edges: edges)
                        .rotationEffect(.degrees(moved.vertical ? 0 : -90), anchor: .topLeading)
                        .offset(x: CGFloat(moved.x), y: CGFloat(moved.y) + (moved.vertical ? 0 : Pc.sideBarShown))
                        // collapsed: slid into its side
                        .offset(into)
                        .animation(collapseSlide, value: into)
                        .frame(width: screen.size.width, height: screen.size.height, alignment: .topLeading)
                        .opacity(hidden ? 0 : 1)
                } else {
                    let into = collapseOffset(model.barCollapsePlace(.sideBar), model.barCollapsed(.sideBar) != nil)
                    SideBar(model: model, right: right, windowHeight: screen.size.height, stretch: sideStretch)
                        .offset(into)
                        .animation(collapseSlide, value: into)
                        .frame(maxWidth: .infinity, alignment: right ? .trailing : .leading)
                        // stretched, what is on it moves out with its far side
                        .offset(x: sideBarShift(right) + (right ? -sideStretch : sideStretch), y: CGFloat(sideMoving?.top ?? sideFit.top))
                        .opacity(hidden ? 0 : 1)
                }
                // over the canvas, under the panels; not over a page that takes the screen
                if !hidden, !model.canvasInfoOpen, !model.gestureControlsOpen, model.crop.shown == nil, !model.timeLapse.replaying,
                   model.animatedExport == nil, model.scene.animatedExport == nil, !model.scene.studioOpen {
                    AnimationBar(model: model, screen: screen.size)
                        .frame(width: screen.size.width, height: screen.size.height, alignment: .bottom)
                }
                // the panels a plugin lets float, in windows of their own, under those hanging
                if !hidden {
                    FloatingPanelWindows(model: model, screen: screen.size)
                    // Collapse to edge's buttons by the bars and the tabs of what went into a side
                    CollapseControls(model: model, screen: screen.size)
                }
                if model.panel != .none || model.panelFading {
                    // the one view all the way, so a list fading keeps its place
                    PanelHost(model: model, screen: screen.size)
                        .opacity(model.panelFading ? 0 : 1)
                        .allowsHitTesting(!model.panelFading)
                }
                if !hidden {
                    FloatingColors(model: model, screen: screen.size)
                }
                if model.editingLayout, !hidden {
                    EditLayoutHandles(model: model, screen: screen.size)
                }
                // where the bars always are, told to Edit layout as it changes
                Color.clear
                    .frame(width: 0, height: 0)
                    .onChange(
                        of: BarsKey(
                            size: screen.size, rightHanded: right, centre: model.sideBarCentre, topBar: Look.shared.scale(.topBar),
                            sideBar: Look.shared.scale(.sideBar), sideBarShown: Pc.sideBarShown
                        ),
                        initial: true
                    ) { _, bars in model.tellBars(bars) }
                // the room the floating panels go in, told to their layout as it changes
                Color.clear
                    .frame(width: 0, height: 0)
                    .onChange(
                        of: LayoutRoomKey(
                            size: screen.size,
                            sideBar: model.showSideBar && model.barMoved(.sideBar) == nil && model.barCollapsed(.sideBar) == nil,
                            rightHanded: AppSettings.shared.rightHanded,
                            sideBarShown: Pc.sideBarShown, barShown: Pc.barShown, floating: Look.shared.scale(.floating)
                        ),
                        initial: true
                    ) { _, room in model.layoutRoom(room) }
                QuickShapeHandles(model: model)
                if !hidden {
                    QuickShapeBar(model: model)
                        .offset(y: Pc.barShown)
                }
                if model.recoloring {
                    RecolorReticle(model: model)
                }
                if model.opacityLayer != nil, !hidden {
                    LayerOpacityBar(value: model.opacityValue)
                        .offset(y: Pc.barShown)
                }
                if model.adjustment == .clone, !hidden {
                    ClonePuck(model: model)
                }
                if model.adjustment == .perspectiveBlur, !hidden {
                    PerspectivePuck(model: model)
                }
                if model.adjusting, !hidden {
                    AdjustmentBar(model: model)
                        .offset(y: Pc.barShown)
                    AdjustModes(model: model)
                }
                // its brush's reach where it is on the canvas, under its panel
                if model.adjustment == .liquify, let ring = model.liquifyRing {
                    LiquifyRingView(ring: ring)
                }
                if model.adjustment == .liquify, !hidden {
                    LiquifyPanel(model: model, screen: screen.size)
                        .modifier(FloatsAtBottom(model: model, piece: .liquify, visible: true))
                        .transition(.move(edge: .bottom))
                }
                if model.adjusting, !hidden {
                    AdjustmentMenu(model: model, screen: screen.size)
                }
                // the bottom toolbars rise from below the screen, and drop away
                // (each may float, while a plugin lets it: the layer it is in moves, its menus with it)
                ZStack {
                    if model.recoloring, !hidden {
                        RecolorPanel(tool: model.recolor)
                            .reportsPiece(.adjusting, lift: 25)
                            .transition(.move(edge: .bottom))
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
                .animation(model.recoloring ? Motion.toolbarIn : Motion.toolbarOut, value: model.recoloring)
                .modifier(FloatsAtBottom(model: model, piece: .adjusting, visible: model.recoloring && !hidden))
                ZStack {
                    if model.adjusting, model.adjustment != .liquify, !hidden {
                        AdjustmentPanel(model: model, screen: screen.size)
                            .reportsPiece(.adjusting, lift: 25)
                            .transition(.move(edge: .bottom))
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
                .animation(model.adjusting ? Motion.toolbarIn : Motion.toolbarOut, value: model.adjusting)
                .modifier(FloatsAtBottom(model: model, piece: .adjusting, visible: model.adjusting && model.adjustment != .liquify && !hidden))
                ZStack {
                    if model.transforming, !hidden {
                        TransformToolbar(model: model, screenWidth: screen.size.width)
                            .reportsPiece(.transform, lift: 25)
                            .transition(.move(edge: .bottom))
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
                .animation(model.transforming ? Motion.toolbarIn : Motion.toolbarOut, value: model.transforming)
                .overlay {
                    if model.transforming, !hidden {
                        TransformPopover(model: model, screen: screen.size)
                    }
                }
                .modifier(FloatsAtBottom(model: model, piece: .transform, visible: model.transforming && !hidden))
                if model.transforming, !hidden {
                    TransformNumeric(model: model, screen: screen.size)
                }
                ZStack {
                    if model.selecting, !hidden {
                        SelectionToolbar(model: model, screenWidth: screen.size.width)
                            .reportsPiece(.selection, lift: 25)
                            .transition(.move(edge: .bottom))
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
                .animation(model.selecting ? Motion.toolbarIn : Motion.toolbarOut, value: model.selecting)
                .overlay {
                    if model.selecting, !hidden {
                        SelectionPopover(model: model, screen: screen.size)
                    }
                }
                .modifier(FloatsAtBottom(model: model, piece: .selection, visible: model.selecting && !hidden))
                // a picture being placed on a model: its box, and the bar that keeps it
                if model.scene.placing {
                    ModelPictureOverlay(model: model)
                }
                if model.adjustment == .gradientMap, let pick = model.gradientMap.stopColour, !hidden {
                    // over the stop, its arrow at the stop's top, kept inside the window
                    let left = min(max(pick.at.x - compactColorsWidth / 2, 0), max(screen.size.width - compactColorsWidth, 0))
                    Color.clear
                        .contentShape(Rectangle())
                        .onTapGesture { model.gradientMap.endColour() }
                    CompactColors(c: pick.held, arrowX: pick.at.x - left, screen: screen.size, side: .bottom, harmonyKind: model.harmonyKind)
                        .frame(width: screen.size.width, height: pick.at.y, alignment: .bottomLeading)
                        .offset(x: left)
                }
                TextEditing(model: model, screen: screen.size)
                // Crop and Resize puts them away while it is open, and they come back after
                if model.crop.shown == nil {
                    ForEach(model.reference.shown) { window in
                        ReferenceWindow(model: model, window: window, screen: screen.size)
                            .transition(.opacity)
                    }
                }
                if model.singleTouchCompanion {
                    TouchCompanion(model: model, screen: screen.size)
                }
                if let format = model.animatedExport {
                    AnimatedExport(model: model, format: format)
                        .transition(.move(edge: .bottom))
                }
                if let format = model.scene.animatedExport {
                    ModelAnimatedExport(model: model, format: format)
                        .transition(.move(edge: .bottom))
                }
                if model.scene.studioOpen {
                    LightingStudio(model: model, screen: screen.size)
                }
                if model.timeLapse.replaying {
                    TimeLapseReplay(model: model)
                        .transition(.opacity)
                }
                CropOverlay(model: model)
                if model.guide.editing {
                    GuideEditor(model: model, screen: screen.size)
                }
                if model.copyPasteOpen {
                    CopyPastePanel(model: model, screen: screen.size)
                }
                QuickMenuView(model: model, screen: screen.size)
                if let choice = model.layerChoice {
                    LayerChoiceList(model: model, choice: choice, screen: screen.size)
                }
                if model.studio.isOpen {
                    BrushStudio(model: model)
                    #if DEBUG
                        .sidewaysInCaptures(screen.size)
                    #endif
                        .transition(.move(edge: .bottom))
                    NoticeBand(model: model)
                }
                if model.licencesOpen {
                    LicencesPage { withAnimation(Motion.studioOut) { model.licencesOpen = false } }
                        .transition(.move(edge: .bottom))
                }
                if model.canvasInfoOpen {
                    CanvasInfoPage(model: model, screen: screen.size)
                        .transition(.move(edge: .bottom))
                }
                if model.gestureControlsOpen {
                    GestureControls(model: model)
                        .transition(.move(edge: .bottom))
                }
                // a plugin's own panel, while its action keeps it open: a ring where the Pencil was,
                // or a card under the bar at the right
                if let panel = PluginsModel.shared.panel {
                    if panel.ring {
                        PluginRingView(model: model, panel: panel, screen: screen.size)
                    } else {
                        PluginPanelView(model: model, panel: panel)
                            .padding(.top, Pc.barShown + 12)
                            .padding(.trailing, 12)
                            .frame(width: screen.size.width, height: screen.size.height, alignment: .topTrailing)
                    }
                }
                ColorDropDot(drop: model.drop)
                ProgressCard(shown: model.exportShown, title: "Exporting", glyph: PcIcons.tabShare) {
                    model.exportCancelled = true
                }
                // its Cancel: the card goes at once, and the brushes read are left out
                ProgressCard(shown: model.importShown, title: "Importing", glyph: PcIcons.tabImport) {
                    model.importCancelled = true
                    model.importShown = nil
                }
                AlertView(
                    content: model.pdfAsking != nil ? AlertContent(
                        title: "PDF quality", action: "Best", icon: PcIcons.page, destructive: false,
                        cancel: "Good", extra: "Better"
                    ) : nil,
                    onCancel: { model.pdfAsking = nil },
                    onAction: { sharePdf(PdfQuality.best) },
                    onExtra: { sharePdf(PdfQuality.better) },
                    onFirst: { sharePdf(PdfQuality.good) }
                )
                AlertView(
                    content: model.saying.map {
                        AlertContent(title: $0.title, message: $0.message, action: "OK", destructive: false, alone: true)
                    },
                    onCancel: { model.saying = nil },
                    onAction: { model.saying = nil }
                )
                // the artwork couldn't be saved to leave it: Stay, the default (a tap beside it too), or
                // Leave Without Saving, which says what it loses
                AlertView(
                    content: model.leaveUnsaved.map {
                        AlertContent(title: $0.title, message: $0.message, action: "Leave Without Saving", cancel: "Stay")
                    },
                    onCancel: {
                        model.leaveUnsaved = nil
                        model.afterLeaving = nil
                    },
                    onAction: { model.leaveWithoutSaving() }
                )
                // what a model newly opened may not paint well with, said once
                AlertView(
                    content: model.scene.warnings.isEmpty ? nil : AlertContent(
                        title: "Unsupported model features",
                        message: "Painting on this model may not go as expected because of:\n" + model.scene.warnings.joined(separator: "\n"),
                        action: "OK", destructive: false, alone: true
                    ),
                    onCancel: { model.scene.warnings = [] },
                    onAction: { model.scene.warnings = [] }
                )
                // the time-lapse in full or cut to 30 seconds (a tap outside: neither)
                AlertView(
                    content: model.timeLapse.lengthAsking ? AlertContent(
                        title: "Video length", action: "30 Seconds", icon: PcIcons.clock, destructive: false, cancel: "Full length"
                    ) : nil,
                    onCancel: { model.timeLapse.lengthAsking = false },
                    onAction: {
                        model.timeLapse.lengthAsking = false
                        model.shareTimeLapse(seconds: 30)
                    },
                    onFirst: {
                        model.timeLapse.lengthAsking = false
                        model.shareTimeLapse(seconds: nil)
                    }
                )
                // recording turned off with something recorded: deleted, or kept (a tap beside it keeps it)
                AlertView(
                    content: model.timeLapse.purgeAsking ? AlertContent(
                        title: "Purge Video?", message: "The video recorded so far will be deleted.\nIt can't be brought back.",
                        action: "Purge", icon: PcIcons.alertQuestion, cancel: "Don't Purge"
                    ) : nil,
                    onCancel: { model.timeLapse.answerPurge(false) },
                    onAction: { model.timeLapse.answerPurge(true) }
                )
                BlockedDialog(model: model)
                AlertView(
                    content: model.question.map {
                        AlertContent(title: $0.title, message: $0.message, action: $0.action, icon: $0.icon)
                    },
                    onCancel: { model.question = nil },
                    onAction: {
                        let perform = model.question?.perform
                        model.question = nil
                        perform?()
                    }
                )
                AlertView(
                    content: model.lockedDeleteTold ? AlertContent(
                        title: "Locked layer selection",
                        message: "A locked layer can’t be deleted. Unlock it first.",
                        action: "OK",
                        icon: PcIcons.pageLocked,
                        destructive: false,
                        alone: true
                    ) : nil,
                    onCancel: { model.lockedDeleteTold = false },
                    onAction: { model.lockedDeleteTold = false }
                )
                AlertView(
                    content: model.askRasterize ? AlertContent(
                        title: "Rasterize this text layer?",
                        message: "Its text can't be edited afterwards; you can paint on it.",
                        action: "Rasterize",
                        destructive: false
                    ) : nil,
                    onCancel: { model.askRasterize = false },
                    onAction: { model.rasterizeCurrent() }
                )
            }
            // while a plugin lays the panels out, a panel may lie over a window, or a window over
            // another or a bar: they hide what is under them whole (only the canvas blurs behind
            // the frost)
            .environment(\.panelsSolid, model.layoutLook.on)
            .animation(Motion.bars, value: hidden)
            .animation(model.sideBarShown ? Motion.sideBarIn : Motion.sideBarOut, value: model.sideBarShown)
            .sized(screen)
        }
        .ignoresSafeArea()
        // a video's frames come in too, but not on a model, where a picture is placed on the surface
        .fileImporter(isPresented: asked(.file), allowedContentTypes: model.scene.open ? [UTType.image] : [UTType.image, UTType.movie]) { result in
            if case let .success(url) = result {
                model.insertFile(url, isPrivate: model.insertPrivate)
            }
        }
        .photosPicker(isPresented: asked(.photo), selection: $photo, matching: model.scene.open ? PHPickerFilter.images : PHPickerFilter.any(of: [.images, .videos]))
        .onChange(of: photo) { _, item in
            guard let item else { return }
            photo = nil
            let isPrivate = model.insertPrivate
            Task { @MainActor in
                if item.supportedContentTypes.contains(where: { $0.conforms(to: .movie) }) {
                    guard let movie = try? await item.loadTransferable(type: PickedMovie.self) else {
                        model.showNotice("This video can't be opened.", warning: true)
                        return
                    }
                    model.insertVideo(movie.url, isPrivate: isPrivate) { _ in
                        try? FileManager.default.removeItem(at: movie.url)
                    }
                    return
                }
                let data = try? await item.loadTransferable(type: Data.self)
                model.insertPicture(data, name: nil, isPrivate: isPrivate)
            }
        }
        .fullScreenCover(isPresented: asked(.camera)) {
            CameraPicker { picture in
                model.insertAsk = nil
                if let picture {
                    model.insertPhoto(picture, isPrivate: model.insertPrivate)
                }
            }
            .ignoresSafeArea()
        }
    }

    /// How far the side bar has moved off its edge: all the way with the interface hidden or a
    /// mode taking the screen.
    private func sideBarShift(_ right: Bool) -> CGFloat {
        guard model.barsHidden || !model.sideBarShown else { return 0 }
        return right ? Pc.sideBarShown : -Pc.sideBarShown
    }
}

/// The current layer turned a change away (locked, hidden or left out of the picture, or an empty
/// group): Cancel, unlock or show it, or open Layers. What was tried is not done afterwards.
private struct BlockedDialog: View {
    let model: CanvasModel

    var body: some View {
        AlertView(
            content: model.blocked.map(content),
            onCancel: { model.answerBlocked(fix: false, openLayers: false) },
            onAction: { model.answerBlocked(fix: false, openLayers: true) },
            onExtra: { model.answerBlocked(fix: true, openLayers: false) }
        )
    }

    private func content(_ b: BlockedLayer) -> AlertContent {
        let name = b.layer.name
        switch b.why {
        case .locked:
            return AlertContent(
                title: "Layer is locked", message: "\u{201C}\(name)\u{201D} can't be changed while it is locked.",
                action: "Open Layers", icon: PcIcons.pageLocked, destructive: false, extra: "Unlock"
            )
        case .hidden, .groupHidden:
            return AlertContent(
                title: b.why == .hidden ? "Layer is hidden" : "Group is hidden", message: Self.hiddenMessage(name, b.by),
                action: "Open Layers", icon: PcIcons.eyeStruck, destructive: false, extra: "Show"
            )
        case .group:
            return AlertContent(
                title: "Group is selected", message: "\u{201C}\(name)\u{201D} is a group: choose one of its layers to paint on.",
                action: "Open Layers", icon: PcIcons.folder, destructive: false
            )
        case .emptyGroup:
            return AlertContent(
                title: "Group is empty", message: "\u{201C}\(name)\u{201D} holds no layers to change.",
                action: "Open Layers", icon: PcIcons.folder, destructive: false
            )
        // an adjustment or a copy, which take one layer
        case .several:
            return AlertContent(
                title: "Several layers are chosen", message: "This works on one layer at a time: choose just one.",
                action: "Open Layers", icon: PcIcons.folder, destructive: false
            )
        // only on a model
        case .meshHidden:
            return AlertContent(
                title: "Mesh is hidden", message: "\u{201C}\(model.session.meantMeshName() ?? "")\u{201D} can't be painted while it is hidden.",
                action: "Open Layers", icon: PcIcons.eyeStruck, destructive: false, extra: "Show"
            )
        case .meshLocked:
            return AlertContent(
                title: "Mesh is locked", message: "\u{201C}\(model.session.meantMeshName() ?? "")\u{201D} can't be painted while it is locked.",
                action: "Open Layers", icon: PcIcons.pageLocked, destructive: false, extra: "Unlock"
            )
        // Liquify on a layer whose alpha is locked
        case .alphaLocked:
            return AlertContent(
                title: "Alpha Lock is on", message: "\u{201C}\(name)\u{201D} can't be liquified while its alpha is locked.",
                action: "Open Layers", icon: PcIcons.pageLocked, destructive: false, extra: "Turn Off"
            )
        }
    }

    /// Why layer or group `name`, which would hide a change, takes none, by what hides it (`by`).
    private static func hiddenMessage(_ name: String, _ by: HiddenBy?) -> String {
        let why: String = switch by {
        case .maskLayer: "its layer is hidden."
        case .group: "its group is hidden."
        case .inside: "a layer in it is hidden."
        case .solo: "another layer is shown alone."
        case .itself, nil: "it is hidden."
        }
        return "\u{201C}\(name)\u{201D} can't be changed while " + why
    }
}

/// How much farther right than the others' (`Pc.arrowShift` past the button's centre) the Layers
/// panel's arrow tip sits, and the Colors panel's, as measured (+3 and +1).
private let layersArrowNudge: CGFloat = 1
private let colorsArrowNudge: CGFloat = -1

/// The open panel, hanging from its button under the bar, clear of the side bar. A plugin may size
/// the panels and the top bar: each panel is laid out at its own points and shown as big as the
/// panels are, from where it hangs.
private struct PanelHost: View {
    let model: CanvasModel
    let screen: CGSize

    /// How big the panels show next to the rest of the interface, and the top bar.
    private var k: CGFloat { Look.shared.scale(.panels) }
    private var bar: CGFloat { Look.shared.scale(.topBar) }

    /// Where a panel `width` wide (its own points) goes under its button, `buttonX` across the
    /// window: clear of the side bar, on either side. Its arrow's tip is in its own points.
    private func place(_ width: CGFloat, _ buttonX: CGFloat) -> PopoverPlace {
        // (moved off its edge by Edit layout, clear of no side bar)
        let clear = model.barMoved(.sideBar) == nil ? Pc.sideBarShown + Pc.popoverGap : Pc.popoverGap
        let at = AppSettings.shared.rightHanded
            ? popoverPlace(width: width * k, buttonCentreX: buttonX, screenWidth: screen.width, minRight: clear)
            : popoverPlace(width: width * k, buttonCentreX: buttonX, screenWidth: screen.width, minLeft: clear)
        return PopoverPlace(left: at.left, arrowX: at.arrowX / k)
    }

    /// The room under the bar, `less` above the screen's edge, in a panel's own points.
    private func room(less: CGFloat) -> CGFloat {
        (screen.height - Pc.barShown - less) / k
    }

    /// Tool button `index`'s centre across the window (0 the colour button, counting leftwards),
    /// and action button `index`'s (0 Actions, counting rightwards), as big as the bar shows.
    private func toolX(_ index: Int) -> CGFloat {
        screen.width - Pc.toolCentreFromRight(index, barWidth: screen.width / bar) * bar
    }

    private func actionX(_ index: Int) -> CGFloat {
        Pc.actionCentre(index, barWidth: screen.width / bar) * bar
    }

    /// From button `index` of the top bar's left half (`left`) or its right while Edit layout moved
    /// that half, where a panel `width` wide (its own points) hangs: under the button, over it or
    /// beside it, as the core works it out; its arrow `nudge` from the button's middle, as measured
    /// where it always is. None from a half where it always is.
    private func moved(_ left: Bool, _ index: Int, _ width: CGFloat, nudge: CGFloat = 0) -> BarHang? {
        let half: LayoutBar = left ? .topLeft : .topRight
        guard model.barMoved(half) != nil else { return nil }
        let gaps = Pc.barGaps(screen.width / bar)
        let along = left
            ? topBarHalfEnd + Pc.galleryWidth + gaps.action + Pc.barButtonWidth / 2 + (Pc.barButtonWidth + gaps.action) * CGFloat(index)
            : topBarHalfLength(left: false, gaps: gaps) - topBarHalfEnd - Pc.barButtonWidth / 2
                - (Pc.barButtonWidth + gaps.tool) * CGFloat(index)
        return model.barLayout.hang(
            bar: half, along: Float(along * bar + nudge), width: Float(width * k), shift: Float(Pc.arrowShift),
            gap: Float(Pc.popoverGap), arrowHeight: Float(Pc.arrowHeight * k)
        )
    }

    /// Over a panel hanging under its button, its grabber, while a plugin lets it float.
    @ViewBuilder private func grabber(_ piece: LayoutPiece, _ hang: BarHang?) -> some View {
        if model.mayFloat(piece), hang == nil || hang?.side == .below {
            PieceGrabber(model: model, piece: piece)
                .padding(.top, Pc.arrowHeight)
        }
    }

    /// The arrow's tip in a panel's own points: hanging from a moved button, the core's; else `place`'s.
    private func arrow(_ hang: BarHang?, _ place: PopoverPlace) -> CGFloat {
        hang.map { CGFloat($0.arrow) / k } ?? place.arrowX
    }

    /// The room a panel has, `less` short of the screen's edge, in its own points: hanging from a
    /// moved button, what the core leaves it; else under the bar.
    private func room(_ hang: BarHang?, less: CGFloat) -> CGFloat {
        hang.map { (CGFloat($0.room) - less) / k } ?? room(less: less)
    }

    /// Where a panel's top left shows (the window's points): hanging over a moved bar, as tall as
    /// `height` (its own points) above it.
    private func origin(_ hang: BarHang?, _ place: PopoverPlace, height: CGFloat) -> CGPoint {
        guard let hang else { return CGPoint(x: place.left, y: Pc.barShown) }
        let y = hang.side == .above ? CGFloat(hang.y) - height * k : CGFloat(hang.y)
        return CGPoint(x: CGFloat(hang.x), y: y)
    }

    /// `panel` hung under the bar at `place`, as big as the panels show; or, from a button Edit
    /// layout moved (`hang`), under it, over it (placed by its bottom) or beside it, its arrow on
    /// that side.
    @ViewBuilder private func hung(_ panel: some View, _ place: PopoverPlace, _ hang: BarHang? = nil) -> some View {
        if let hang {
            let side: ArrowSide = switch hang.side {
            case .below: .top
            case .above: .bottom
            case .right: .left
            case .left: .right
            }
            let arrowed = panel.environment(\.hang, Hang(side: side, arrow: CGFloat(hang.arrow) / k))
            if hang.side == .above {
                arrowed
                    .scaleEffect(k, anchor: .bottomLeading)
                    .frame(width: screen.width, height: CGFloat(hang.y), alignment: .bottomLeading)
                    .offset(x: CGFloat(hang.x))
            } else {
                arrowed
                    .scaleEffect(k, anchor: .topLeading)
                    .offset(x: CGFloat(hang.x), y: CGFloat(hang.y))
            }
        } else {
            panel
                .scaleEffect(k, anchor: .topLeading)
                .offset(x: place.left, y: Pc.barShown)
        }
    }

    var body: some View {
        switch model.panel == .none ? model.lastPanel : model.panel {
        case .brushes:
            let place = place(brushLibraryWidth, toolX(libraryButton(model.tool)))
            let hang = moved(false, libraryButton(model.tool), brushLibraryWidth)
            let height = room(hang, less: 25)
            let at = origin(hang, place, height: height)
            // ending 25 above the screen's edge, as measured
            hung(
                BrushLibrary(
                    model: model, arrowX: arrow(hang, place), origin: CGPoint(x: at.x / k, y: at.y / k),
                    screen: CGSize(width: screen.width / k, height: screen.height / k)
                )
                .frame(height: height)
                // torn off by its grabber: unseen until let go, its window following the finger
                .opacity(model.floating.undocking == .brushes ? 0 : 1),
                place, hang
            )
        case .layers:
            if let held = model.backgroundPick {
                let place = place(colorPanelWidth, toolX(1) + layersArrowNudge)
                let hang = moved(false, 1, colorPanelWidth, nudge: layersArrowNudge)
                hung(
                    ColorPanel(model: model, arrowX: arrow(hang, place), screenHeight: room(hang, less: 0) + Pc.barHeight, background: held),
                    place, hang
                )
                // closed any way at all, the colour is kept
                .onDisappear { model.endBackgroundPick() }
            } else {
                let place = place(layersPanelWidth, toolX(1) + layersArrowNudge)
                let hang = moved(false, 1, layersPanelWidth, nudge: layersArrowNudge)
                hung(
                    LayersPanel(model: model, arrowX: arrow(hang, place), maxHeight: room(hang, less: Pc.popoverGap))
                        .opacity(model.floating.undocking == .layers ? 0 : 1),
                    place, hang
                )
            }
        case .actions:
            let place = place(actionsPanelWidth, actionX(0))
            let hang = moved(true, 0, actionsPanelWidth)
            let most = room(hang, less: 25)
            // ending 25 above the screen's edge, its rows scrolling, as measured
            hung(
                ActionsPanel(model: model, arrowX: arrow(hang, place), maxHeight: most).frame(maxHeight: most, alignment: .top)
                    .overlay(alignment: .top) { grabber(.actions, hang) }
                    .opacity(model.floating.undockingPiece == .actions ? 0 : 1),
                place, hang
            )
        case .adjustments:
            let place = place(adjustmentsPanelWidth, actionX(1))
            let hang = moved(true, 1, adjustmentsPanelWidth)
            hung(
                AdjustmentsPanel(model: model, arrowX: arrow(hang, place)).frame(maxHeight: room(hang, less: 25), alignment: .top)
                    .overlay(alignment: .top) { grabber(.adjustments, hang) }
                    .opacity(model.floating.undockingPiece == .adjustments ? 0 : 1),
                place, hang
            )
        case .color:
            let place = place(colorPanelWidth, toolX(0) + colorsArrowNudge)
            let hang = moved(false, 0, colorPanelWidth, nudge: colorsArrowNudge)
            hung(ColorPanel(model: model, arrowX: arrow(hang, place), screenHeight: room(hang, less: 0) + Pc.barHeight), place, hang)
        default:
            EmptyView()
        }
    }
}

/// The canvas's view and its Pencil interaction, kept by a controller of their own.
private struct CanvasHost: UIViewControllerRepresentable {
    let model: CanvasModel

    func makeUIViewController(context: Context) -> CanvasViewController {
        CanvasViewController(model: model)
    }

    func updateUIViewController(_ controller: CanvasViewController, context: Context) {}
}

final class CanvasViewController: UIViewController, UIPencilInteractionDelegate {
    private let model: CanvasModel
    private let canvas: CanvasView
    private lazy var dropper = CanvasDropper(model: model)
    private var observers: [NSObjectProtocol] = []
    /// Back to the gallery, once the artwork is kept.
    private let leave: () -> Void
    /// The artwork is being saved to leave it.
    private var leaving = false

    init(model: CanvasModel) {
        self.model = model
        leave = model.onClose
        canvas = CanvasView(session: model.session)
        super.init(nibName: nil, bundle: nil)
        CanvasLife.made(.screen)
        // the top bar's Gallery keeps the artwork first
        model.onClose = { [weak self] in self?.close() }
        model.leaveWithoutSaving = { [weak self] in self?.leaveUnsaved() }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("the canvas is made in code")
    }

    deinit {
        CanvasLife.gone(.screen)
    }

    override func loadView() {
        view = canvas
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        let session = model.session
        let dir = model.dir
        // a new artwork exists in the gallery from the start (an opened one is there already)
        if !FileManager.default.fileExists(atPath: (dir as NSString).appendingPathComponent("artwork.json")) {
            DispatchQueue.global(qos: .utility).async { try? session.save(dir: dir) }
            model.startedNew()
        }
        canvas.addInteraction(UIDropInteraction(delegate: dropper))
        // on a second display too, with Project canvas on
        Projection.shared.artwork(session)
        // kept saved by the core a few seconds after it changes; a failure told
        session.startAutosave(dir: dir)
        canvas.onCommands = { [weak self] in self?.obey($0) }
        canvas.takesTouches = { [weak self] in !(self?.model.editingLayout ?? false) }
        canvas.beforeTouches = { [weak self] in
            guard let self else { return }
            self.model.prepareTouches()
            // with a panel open, the touch waits to be a tap (it closes the panel) or a drag
            self.model.session.setTouchPanelOpen(open: self.model.panel != Panel.none)
            // a field that had the keys let them go: they come back here
            if !self.isFirstResponder, self.model.text.info == nil {
                self.becomeFirstResponder()
            }
        }
        canvas.afterTouches = { [weak self] ended in self?.model.layerOpacityTouched(ended: ended) }
        canvas.ringAt = { [weak self] in self?.model.showLiquifyRing($0) }
        canvas.tapAssistance = SystemSettings.tapAssistance
        let pencil = UIPencilInteraction()
        pencil.delegate = self
        canvas.addInteraction(pencil)
        // Reduce Motion: a dropped colour's fill shows at once rather than spreading in
        session.setReduceMotion(reduce: UIAccessibility.isReduceMotionEnabled)
        observers.append(NotificationCenter.default.addObserver(
            forName: UIAccessibility.reduceMotionStatusDidChangeNotification, object: nil, queue: .main
        ) { [weak self] _ in self?.model.session.setReduceMotion(reduce: UIAccessibility.isReduceMotionEnabled) })
        // the app may be ended without a word once in the background: saved as it goes there; away,
        // the canvas draws nothing and the autosave rests
        observers.append(NotificationCenter.default.addObserver(
            forName: UIApplication.didEnterBackgroundNotification, object: nil, queue: .main
        ) { [weak self] _ in
            self?.model.session.setShown(shown: false)
            self?.saveInBackground()
        })
        observers.append(NotificationCenter.default.addObserver(
            forName: UIApplication.willEnterForegroundNotification, object: nil, queue: .main
        ) { [weak self] _ in
            self?.model.session.setShown(shown: true)
            self?.model.followStrokeSound()
            self?.model.readAppSettings()
            self?.canvas.tapAssistance = SystemSettings.tapAssistance
        })
        // time on the artwork counts only while the app is active: not while the iPad is locked,
        // the app is in the background or under the system's own screens (the app switcher,
        // Control Centre)
        observers.append(NotificationCenter.default.addObserver(
            forName: UIApplication.willResignActiveNotification, object: nil, queue: .main
        ) { [weak self] _ in
            self?.model.noteTime()
        })
        observers.append(NotificationCenter.default.addObserver(
            forName: UIApplication.didBecomeActiveNotification, object: nil, queue: .main
        ) { [weak self] _ in
            guard let self, !self.leaving else { return }
            self.model.resumeTracking()
        })
        // Paste offered once anything holds a picture: what another app copied meanwhile, or this
        // app's clipboard changing
        model.lookAtSystemClipboard()
        for name in [UIApplication.didBecomeActiveNotification, UIPasteboard.changedNotification] {
            observers.append(NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                self?.model.lookAtSystemClipboard()
            })
        }
    }

    override var canBecomeFirstResponder: Bool { true }

    /// The system's own three-finger editing gestures (undo, redo, copy, paste, the editing menu)
    /// stay off the canvas, whose finger taps undo and redo: they would take them first.
    override var editingInteractionConfiguration: UIEditingInteractionConfiguration { .none }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        becomeFirstResponder()
    }

    private static func isAlt(_ key: UIKey) -> Bool {
        key.keyCode == .keyboardLeftAlt || key.keyCode == .keyboardRightAlt
    }

    /// The shortcuts as key commands, which the system lists while ⌘ is held, made again when the
    /// plugins change (one may rebind keys or give its own).
    private var shortcuts: (made: Int, commands: [UIKeyCommand])?
    /// The press a shortcut last ran for, and when: one press may come both as a key command and
    /// as a press, and is done once.
    private var lastShortcut: (press: String, at: CFTimeInterval)?
    /// Within this long (seconds) the same press is that one press come again.
    private static let samePress: CFTimeInterval = 0.05

    override var keyCommands: [UIKeyCommand]? {
        let now = PluginsModel.shared.changes
        if let made = shortcuts, made.made == now {
            return made.commands
        }
        let commands = shortcutKeyCommands(action: #selector(CanvasViewController.shortcutCommand(_:)))
        shortcuts = (now, commands)
        return commands
    }

    /// Whether a shortcut just ran for `press`.
    private func justRan(_ press: String) -> Bool {
        guard let last = lastShortcut else { return false }
        return last.press == press && CACurrentMediaTime() - last.at < Self.samePress
    }

    /// A shortcut's key command: what its press does.
    @objc private func shortcutCommand(_ command: UIKeyCommand) {
        guard let name = command.propertyList as? String else { return }
        let flags = command.modifierFlags
        let press = CanvasModel.pressText(name, flags)
        guard !justRan(press) else { return }
        lastShortcut = (press, CACurrentMediaTime())
        _ = model.runShortcut(name, command: flags.contains(.command), shift: flags.contains(.shift), alt: flags.contains(.alternate))
    }

    /// A hardware keyboard's shortcuts on the canvas (keys no text field took); Alt held is □ held.
    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        var taken = false
        for key in presses.compactMap(\.key) {
            if Self.isAlt(key) {
                model.modifyPressed()
                taken = true
                continue
            }
            guard let name = CanvasModel.name(of: key) else { continue }
            let press = CanvasModel.pressText(name, key.modifierFlags.intersection([.command, .control, .shift, .alternate]))
            // done already as its key command
            if justRan(press) {
                taken = true
            } else if model.keyPressed(key) {
                lastShortcut = (press, CACurrentMediaTime())
                taken = true
            }
        }
        if !taken {
            super.pressesBegan(presses, with: event)
        }
    }

    private func altUp(_ presses: Set<UIPress>) -> Bool {
        guard presses.compactMap(\.key).contains(where: Self.isAlt), model.modifyHeld else { return false }
        model.modifyReleased(cancelled: true)
        return true
    }

    override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        if !altUp(presses) {
            super.pressesEnded(presses, with: event)
        }
    }

    override func pressesCancelled(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        if !altUp(presses) {
            super.pressesCancelled(presses, with: event)
        }
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        let was = model.pixels
        model.pixels = (view.window?.screen.scale ?? view.traitCollection.displayScale) * Interface.shared.scale
        if model.pixels != was {
            model.applyFitInsets()
        }
        model.screenCentre = CGPoint(x: view.bounds.midX * model.pixels, y: view.bounds.midY * model.pixels)
    }

    override var prefersStatusBarHidden: Bool { true }
    override var prefersHomeIndicatorAutoHidden: Bool { true }
    // the screen's edges paint first; a second swipe is the system's
    override var preferredScreenEdgesDeferringSystemGestures: UIRectEdge { .all }

    /// What the touches asked.
    private func obey(_ commands: [TouchCommand]) {
        for command in commands {
            switch command {
            case .undo:
                model.undo()
            case .redo:
                model.redo()
            case let .run(action, x, y):
                // a Pencil gesture set to a QuickMenu action: done as its button would
                model.runQuickAction(action, at: CGPoint(x: CGFloat(x) / model.pixels, y: CGFloat(y) / model.pixels))
            case let .runPlugin(key, x, y):
                model.runPluginAction(key, x: x, y: y)
            case .clearLayer:
                model.clearCurrentLayer()
            case .toggleBars:
                model.toggleBars()
            case .fit:
                model.settleToFit()
            case let .layerSelect(x, y):
                model.layerSelect(x: x, y: y)
            case .quickShapeLast:
                model.quickShapeLast()
            case let .pick(x, y):
                model.pick(x: x, y: y)
            case .pickDone:
                model.finishPick()
            case .pickCancel:
                model.cancelPick()
            case .shapeSnapped:
                model.quickShapeSnapped()
                canvas.shapeSnapped()
            case .shapeChanged:
                model.quickShapeChanged()
            case .shapeEnded:
                model.quickShapeEnded()
                model.refresh()
            case .strokeEnded:
                model.strokeEnded()
            case .viewMoved:
                model.refreshView()
            // let go near a right angle, the view turns onto it by itself: what is over it follows
            case .viewSettles:
                model.followView()
            case let .cursor(x, y):
                let pixels = model.pixels
                canvas.showCursor(at: CGPoint(x: CGFloat(x) / pixels, y: CGFloat(y) / pixels), hovering: false)
            case .cursorGone:
                canvas.showCursor(at: nil, hovering: false)
            // a pen touching at no pressure before it presses: shown as it hovering there
            case let .penHovers(x, y):
                let pixels = model.pixels
                canvas.showCursor(at: CGPoint(x: CGFloat(x) / pixels, y: CGFloat(y) / pixels), hovering: true)
            // a touch on the canvas with a panel open: dragged, it fades the panel and goes on; tapped,
            // it only closes it
            case .panelFades:
                model.swipedBrush = nil
                model.fadePanels()
            case .panelTapped:
                model.tapOutsidePanel()
            case .textLayer:
                model.askRasterize = true
            case let .recolorMoved(x, y):
                model.recolor.move(to: CGPoint(x: CGFloat(x), y: CGFloat(y)))
            case let .recolorTapped(x, y):
                model.recolor.tap(CGPoint(x: CGFloat(x), y: CGFloat(y)))
            case .copyPaste:
                model.copyPasteOpen = true
            case .transformTouched:
                model.transform.popup = .none
            case let .transformMoved(transform):
                // the box snapping onto a guide is felt (Apple Pencil Pro)
                let wasSnapped = model.transform.box.map { !$0.snapLines.isEmpty } ?? false
                model.transform.moved(transform)
                if let transform, !transform.snapLines.isEmpty, !wasSnapped {
                    canvas.linedUp()
                }
            case let .transformHeld(handle, x, y):
                model.transform.hold(handle, x: x, y: y)
            case let .transformLetGo(handle, x, y):
                model.transform.release(handle, x: x, y: y)
            case let .transformNumeric(angle, x, y, right):
                let at = CGPoint(x: CGFloat(x) / model.pixels, y: CGFloat(y) / model.pixels)
                model.transform.numeric = TransformNumericAsk(angle: angle, at: at, right: right)
            case let .selectionDrag(x0, y0, x1, y1):
                model.selection.dragged(CGPoint(x: CGFloat(x0), y: CGFloat(y0)), CGPoint(x: CGFloat(x1), y: CGFloat(y1)))
            case let .selectionThreshold(x, y, threshold, done):
                model.selection.colourSlid(CGPoint(x: CGFloat(x), y: CGFloat(y)), threshold: threshold, done: done)
            case .freehandChanged:
                model.selection.freehandChanged()
            case .selectionChanged:
                model.selection.touched()
            case let .pourAt(x, y, fill):
                model.drop.tapFill(x: x, y: y, fill: fill)
            case let .fillThreshold(threshold):
                model.drop.pourThreshold(threshold)
            case let .pourEnd(keep):
                model.drop.tapFillEnd(keep: keep)
            case .filled:
                model.drop.filled()
            case let .quickMenu(x, y):
                model.toggleQuickMenu(at: CGPoint(x: CGFloat(x) / model.pixels, y: CGFloat(y) / model.pixels))
            case .adjustChanged:
                model.adjustModesOpen = false
                model.readAdjust()
            case let .adjustMenu(x, y):
                if !model.closedAdjustMenu {
                    model.adjustMenuAt = CGPoint(x: CGFloat(x) / model.pixels, y: CGFloat(y) / model.pixels)
                }
            case .brushChanged:
                model.brushTouched()
            case .showColours:
                model.toggle(.color)
            case .showBrushes:
                model.toggle(.brushes)
            case .guideTouched:
                model.guide.touched(true)
            case .guideChanged:
                model.guide.touched(false)
            case let .guideNodeTapped(point, x, y):
                model.guide.nodeTapped(point: point, x: x, y: y)
            default:
                break
            }
        }
    }

    /// Back to the gallery, the artwork kept. An artwork is never left with work only in memory: one
    /// that can't be saved (the storage full, say) stays open, kept saved again, and asks whether to
    /// stay or leave without saving (the core's rule, as the other apps'); what was to follow
    /// leaving (a file brought in) waits for that.
    private func close() {
        guard !leaving else { return }
        let model = model
        let session = model.session
        let dir = model.dir
        model.noteTime()
        model.keepChoicesNow()
        leaving = true
        // off the main thread: a large artwork takes a while
        DispatchQueue.global(qos: .userInitiated).async {
            let left = session.saveToLeave(dir: dir)
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                self.leaving = false
                switch left {
                case .left:
                    model.saveSucceeded()
                    self.leaveCanvas(saved: true)
                case let .stays(title, message):
                    // still open: its time counts again, and the person chooses
                    AppLog.error("save", "the artwork couldn't be saved to leave it: \(message)")
                    model.resumeTracking()
                    model.leaveUnsaved = (title, message)
                }
            }
        }
    }

    /// Leaves without saving, as asked: what wasn't saved is lost, its last save stays as it was.
    private func leaveUnsaved() {
        model.leaveUnsaved = nil
        model.noteTime()
        leaveCanvas(saved: false)
    }

    /// Back to the gallery, the artwork saved (`saved`) or left without saving; then what was to
    /// follow leaving. Its memory goes, and the frames it recorded into one more video (only once
    /// saved: the storage full, they stay pictures).
    private func leaveCanvas(saved: Bool) {
        let session = model.session
        let dir = model.dir
        let lapse = model.timeLapse.lapse
        let then = model.afterLeaving
        model.afterLeaving = nil
        model.stopWatching()
        StrokeSound.shared.follow(nil, on: false)
        // the gallery zooms out of the artwork's cell, a picture of the canvas fading away over it
        GalleryZoomer.shared.close(session, dir: dir, pixels: canvas.window?.screen.scale ?? canvas.traitCollection.displayScale)
        canvas.detach()
        observers.forEach { NotificationCenter.default.removeObserver($0) }
        observers.removeAll()
        // the second display let go before the canvas is
        Projection.shared.artwork(nil)
        // from now on its model should go: the log says when it does
        CanvasLife.left(model)
        leave()
        Gallery.closingSaves += 1
        let canvases = CanvasLife.canvases
        weak var left = model
        DispatchQueue.global(qos: .userInitiated).async {
            // (saved: its autosave ended already; left without saving: it ends now)
            session.stopAutosave()
            // the artwork file keeps a time-lapse as video
            if saved {
                _ = lapse.forPackage()
            }
            lapse.close()
            // its memory free at once, not when the object goes
            session.releaseCanvas()
            // and the rest of it once nothing holds the session: the log says if anything does
            CanvasLife.lookAfterLeaving(session, model: left, canvases: canvases)
            DispatchQueue.main.async { Gallery.artworkKept(failure: nil) }
        }
        then?()
    }

    /// Saves the artwork off the main thread, the system letting the app finish first (a background
    /// task); the app may be ended without a word once it is in the background.
    private func saveInBackground() {
        model.noteTime()
        // away, the strokes' sound stops
        StrokeSound.shared.follow(nil, on: false)
        let model = model
        let session = model.session
        let dir = model.dir
        let app = UIApplication.shared
        var task = UIBackgroundTaskIdentifier.invalid
        task = app.beginBackgroundTask(withName: "Keep the artwork") {
            if task != .invalid {
                app.endBackgroundTask(task)
                task = .invalid
            }
        }
        DispatchQueue.global(qos: .userInitiated).async {
            var failure: String?
            do {
                try session.save(dir: dir)
            } catch {
                failure = "\(error)"
                AppLog.error("save", "keeping the artwork: \(error)")
            }
            DispatchQueue.main.async {
                if let failure {
                    model.saveFailed(failure)
                } else {
                    model.saveSucceeded()
                    // its new picture on the home screen's widget
                    RecentArtwork.refresh()
                }
                if task != .invalid {
                    app.endBackgroundTask(task)
                    task = .invalid
                }
            }
        }
    }

    // MARK: UIPencilInteractionDelegate

    /// The double-tap before iPadOS 17.5 (from then on it comes with where the Pencil hovers, below).
    func pencilInteractionDidTap(_ interaction: UIPencilInteraction) {
        if #available(iOS 17.5, *) {
            return
        }
        canvas.pencilDoubleTapped(at: nil, action: UIPencilInteraction.preferredTapAction)
    }

    @available(iOS 17.5, *)
    func pencilInteraction(_ interaction: UIPencilInteraction, didReceiveTap tap: UIPencilInteraction.Tap) {
        canvas.pencilDoubleTapped(at: tap.hoverPose?.location, action: UIPencilInteraction.preferredTapAction)
    }

    @available(iOS 17.5, *)
    func pencilInteraction(_ interaction: UIPencilInteraction, didReceiveSqueeze squeeze: UIPencilInteraction.Squeeze) {
        let action = UIPencilInteraction.preferredSqueezeAction
        // what a squeeze opens over the canvas clears the way, as does the system's own palette
        let purpose = Gestures.shared.purposeOf(.penSqueeze)
        let opens = purpose == .eyedropper || purpose == .quickmenu || purpose == .layerSelect
        if squeeze.phase == .began, opens || action == .showContextualPalette {
            model.closePopovers()
        }
        canvas.pencilSqueezed(squeeze, action: action)
    }
}

/// The canvas's objects alive, counted as they are made and as they go, for the log's look once a
/// canvas is left: each holds the artwork's session, and the session's memory (its brushes, its
/// document, all but the canvas, let go as it is left) goes only once nothing holds it.
enum CanvasLife {
    enum Kind: Int {
        case model, screen, view
    }

    private static let lock = NSLock()
    private static var alive = [0, 0, 0]
    /// Canvases made so far (on the main thread): one made since a canvas was left is alive rightly.
    private(set) static var canvases = 0
    /// Each model made, by its artwork's folder, while it is alive (on the main thread).
    private static var models: [WeakModel] = []
    /// Touches begun on the window so far, as the gallery's watcher sees them (under the lock).
    private static var touches = 0
    /// The models of canvases left, until they go: which, when they were left and the touches
    /// counted by then; the last few (under the lock: a model may go on any thread).
    private static var leftModels: [(id: ObjectIdentifier, at: CFAbsoluteTime, touches: Int)] = []

    private final class WeakModel {
        weak var model: CanvasModel?
        let dir: String

        init(_ model: CanvasModel) {
            self.model = model
            dir = model.dir
        }
    }

    /// `model` was made (on the main thread): followed while it is alive, for the look after leaving.
    static func follow(_ model: CanvasModel) {
        models.removeAll { $0.model == nil }
        models.append(WeakModel(model))
    }

    static func made(_ kind: Kind) {
        lock.lock()
        alive[kind.rawValue] += 1
        lock.unlock()
        if kind == .model {
            canvases += 1
        }
    }

    /// (as an object goes, on whichever thread lets it go)
    static func gone(_ kind: Kind) {
        lock.lock()
        alive[kind.rawValue] -= 1
        lock.unlock()
    }

    /// A touch began on the window.
    static func touched() {
        lock.lock()
        touches += 1
        lock.unlock()
    }

    /// The canvas of `model` is being left (on the main thread): once its model goes, the log says
    /// how long after, and how many touches came between.
    static func left(_ model: CanvasModel) {
        lock.lock()
        leftModels.append((id: ObjectIdentifier(model), at: CFAbsoluteTimeGetCurrent(), touches: touches))
        if leftModels.count > 8 {
            leftModels.removeFirst()
        }
        lock.unlock()
    }

    /// The model `id` went (on whichever thread let it go): one whose canvas was left says how long
    /// after, and how many touches the window had meanwhile. What holds a model past its canvas is
    /// told by what lets it go: the next touch, a time, or neither (a look at the screen's
    /// accessibility elements, say).
    static func modelGone(_ id: ObjectIdentifier) {
        lock.lock()
        let found = leftModels.firstIndex { $0.id == id }
        let left = found.map { leftModels[$0] }
        if let found {
            leftModels.remove(at: found)
        }
        let now = touches
        lock.unlock()
        guard let left else { return }
        let seconds = String(format: "%.1f", CFAbsoluteTimeGetCurrent() - left.at)
        AppLog.info("memory", "the model of a canvas left \(seconds) s ago let go, after \(now - left.touches) touches on the window")
    }

    /// What is left of a canvas left, looked at a while after: held weakly, so looking keeps none of
    /// it; the touches counted on the window as it was left.
    private final class LeftCanvas {
        weak var session: CanvasSession?
        weak var model: CanvasModel?
        let dir: String?
        /// Canvases made by then.
        let since: Int
        let touches: Int

        init(session: CanvasSession, model: CanvasModel?, since: Int, touches: Int) {
            self.session = session
            self.model = model
            dir = model?.dir
            self.since = since
            self.touches = touches
        }

        var held: Bool {
            session != nil || model != nil
        }
    }

    /// The canvas of `session` and `model` was left (`since` canvases made by then) and its canvas
    /// let go: a few seconds on, once its zoom out is over, the log says what is still alive of it
    /// (whether the app still holds the session itself, or only the core its state; what of the
    /// app's own holds its model, the artworks whose models are alive, and how many touches the
    /// window has had since); if anything is, and no canvas was made since, as a problem. What is
    /// still held is looked at again 15 and 35 seconds on (the model says too when it goes).
    static func lookAfterLeaving(_ session: CanvasSession, model: CanvasModel?, canvases since: Int) {
        lock.lock()
        let left = LeftCanvas(session: session, model: model, since: since, touches: touches)
        lock.unlock()
        DispatchQueue.main.asyncAfter(deadline: .now() + 5) {
            lookFirst(left)
        }
    }

    /// The touches begun on the window since `left` was left.
    private static func touchesSince(_ left: LeftCanvas) -> Int {
        lock.lock()
        defer { lock.unlock() }
        return touches - left.touches
    }

    /// The look 5 seconds after leaving `left` (on the main thread).
    private static func lookFirst(_ left: LeftCanvas) {
        lock.lock()
        let counts = alive
        lock.unlock()
        let sessions = liveSessions()
        var line = "\(sessions) sessions, \(counts[0]) canvas models, \(counts[1]) canvas screens, "
            + "\(counts[2]) canvas views alive; the one left \(left.session == nil ? "let go" : "still held by the app")"
        if let leftModel = left.model {
            // the root view's open canvas (which the gallery's open handler reaches, and nothing
            // else of it), or the gallery's zoom still under way on it
            var by: [String] = []
            if OpenCanvas.shared.model === leftModel {
                by.append("the open canvas")
            }
            if let dir = left.dir, GalleryZoomer.shared.zoom?.dir == dir {
                by.append("the gallery's zoom")
            }
            let holders = by.isEmpty ? "none of the open canvas, the gallery or its zoom" : by.joined(separator: " and ")
            line += "; its model held by " + holders
        }
        models.removeAll { $0.model == nil }
        if !models.isEmpty {
            line += "; models alive for " + models.map { ($0.dir as NSString).lastPathComponent }.joined(separator: ", ")
        }
        line += "; \(touchesSince(left)) touches on the window since it was left"
        if canvases == left.since, left.held || sessions > 0 || counts.contains(where: { $0 > 0 }) {
            AppLog.error("memory", "a canvas left 5 s ago is still held: \(line)")
        } else {
            AppLog.info("memory", "5 s after leaving a canvas: \(line)")
        }
        if left.held {
            DispatchQueue.main.asyncAfter(deadline: .now() + 10) {
                lookAgain(left, at: 15, then: 35)
            }
        }
    }

    /// The look `seconds` after leaving `left` (on the main thread), when it was still held at the
    /// one before: said only if it still is; and looked at once more at `then`.
    private static func lookAgain(_ left: LeftCanvas, at seconds: Int, then: Int?) {
        guard left.held else { return }
        let what = [left.model == nil ? nil : "its model", left.session == nil ? nil : "its session"].compactMap { $0 }
        AppLog.error(
            "memory",
            "a canvas left \(seconds) s ago is still held: \(what.joined(separator: " and ")) alive; "
                + "\(touchesSince(left)) touches on the window since it was left"
        )
        if let then {
            DispatchQueue.main.asyncAfter(deadline: .now() + .seconds(then - seconds)) {
                lookAgain(left, at: then, then: nil)
            }
        }
    }
}
