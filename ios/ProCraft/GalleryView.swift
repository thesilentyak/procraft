import PhotosUI
import SwiftUI
import UIKit
import UniformTypeIdentifiers

// as measured: a thumbnail's shadow dropped below it, and the rim round it
private let thumbShadowDrop: CGFloat = 5
private var thumbShadow: Color { Look.shared.light ? Color(argb: 0x1D00_0000) : Color(argb: 0x4500_0000) }
private var thumbRim: Color { Look.shared.light ? Color(argb: 0xFFBE_BEBE) : Color(argb: 0xFF42_4242) }

/// A swiped cell's three actions share its width, 3 apart (as measured, 81 x 55 on a 249 pitch),
/// never narrower than their names need.
private let swipePillGap: CGFloat = 3
private let swipePillLeast: CGFloat = 74
private let swipePillHeight: CGFloat = 55
private var swipePillFill: Color { Look.shared.light ? Color(argb: 0xFFF5_F5F5) : Color(argb: 0xDB1C_1C1C) }
private var swipePillInk: Color { Look.shared.light ? Color(argb: 0xFF57_5757) : Color(argb: 0xFFA9_A9A9) }
private let swipeDeleteFill = Color(argb: 0xFFFE_604D)
/// How far a swiped artwork fades back under its actions.
private let swipedFade = 0.64
private let doneBlue = Color(argb: 0xFF01_90FF)
private let renameBlur: CGFloat = 20
/// How far the gallery behind a rename fades into its background (a white picture shows at 89 over
/// the dark gallery's 33, as measured).
private let renameDim = 0.75
/// How long Gallery Recovery's pie takes to come most of the way, the card shown long enough to be
/// read however quick the search; and how long it stays once done.
private let recoverySearchSteps = 44
private let recoveryStep = Duration.milliseconds(16)
private let recoveryDone = Duration.milliseconds(900)
/// How long an artwork can take to open before the gallery says it is opening, and what dims it.
private let slowOpeningAfter = Duration.milliseconds(500)
private let openingVeil = Color(argb: 0x6600_0000)

/// A gallery cell's sizes: its pitch both ways (the screen's width over the columns, three upright
/// and four on its side), the content's width (40 less) and the thumbnail box's height (82 less).
private struct CellSize {
    let pitch: CGFloat
    let width: CGFloat
    let thumb: CGFloat

    init(screenWidth: CGFloat, columns: Int) {
        pitch = screenWidth / CGFloat(columns)
        width = pitch - 40
        thumb = pitch - 82
    }

    /// The square an artwork's picture is fitted in, in the middle of the box: as measured, 165
    /// across on a 248 pitch, as wide for a wide picture as for a square one.
    var side: CGFloat {
        thumb - 1
    }

    /// How wide each of a swiped cell's actions is.
    var pill: CGFloat {
        max((pitch - swipePillGap * 2) / 3, swipePillLeast)
    }

    /// Where a touch on the cell is on its name rather than its picture.
    var nameTop: CGFloat {
        39 + thumb + 8
    }
}

/// The title's font at its own size, for how wide and tall it is.
private let titleFont = UIFont.systemFont(ofSize: 36, weight: .bold)

/// How many columns the gallery has in a window `screen` big (the shared core's rule): three
/// upright and four on its side; in a narrow window (beside another app) by its width.
private func galleryColumnCount(_ screen: CGSize) -> Int {
    max(Int(galleryColumns(width: Float(screen.width), landscape: screen.width > screen.height)), 1)
}

/// A title row button's name, as More's list shows it.
private func galleryActionLabel(_ action: GalleryAction) -> String {
    switch action {
    case .select: "Select"
    case .`import`: "Import"
    case .photo: "Photo"
    case .stack: "Stack"
    case .preview: "Preview"
    case .share: "Share"
    case .duplicate: "Duplicate"
    case .delete: "Delete"
    case .more: "More"
    }
}

/// An artwork's or stack's name being changed.
struct Renaming: Identifiable {
    let id: String
    let name: String
    let shown: GalleryArtwork
    let stack: Bool
}

/// Artworks to share, and the swiped Share button's place on the screen (none from Select); several
/// as one PDF are named `group`.
struct GalleryShare {
    let artworks: [GalleryArtwork]
    let from: CGRect?
    var group = GalleryModel.groupName
}

/// The gallery: its title and buttons, then the artworks kept on the iPad (the core's store, in the
/// app's own Application Support). A tap opens an artwork (on its name, renames it), a swipe left
/// shows Share, Duplicate and Delete (a stack's act on every artwork in it), two fingers spread on
/// it open it in Preview, and held it can be dragged (`GalleryDragger`); Select picks several
/// artworks and stacks; + makes a new canvas.
struct GalleryView: View {
    let onOpen: (OpenArtwork) -> Void

    @Environment(\.displayScale) private var displayScale
    @State private var model = GalleryModel.shared
    @State private var failure: String?
    /// The artwork or stack (by folder) whose actions a swipe left is showing.
    @State private var swiped: String?
    @State private var selecting = false
    /// The artworks and stacks picked, by folder.
    @State private var selected: Set<String> = []
    @State private var moreOpen = false
    @State private var newCanvasOpen = false
    @State private var sharing: GalleryShare?
    /// Artworks waiting for their PDF's quality (several go into one, named as the share says).
    @State private var pdfFor: GalleryShare?
    @State private var renaming: Renaming?
    @State private var deleting: GalleryArtwork?
    /// What Select's Delete is asking about, and a stack swiped to.
    @State private var deletingSelected: GalleryPick?
    @State private var deletingStack: GalleryPick?
    @State private var importAsked = false
    @State private var photoAsked = false
    @State private var photo: PhotosPickerItem?
    @State private var previewing: [GalleryArtwork]?
    /// Which of them Preview starts on.
    @State private var previewFrom = 0
    /// An artwork two fingers are spreading open into Preview.
    @State private var opening: PinchOpening?
    @State private var drag = GalleryDragShown()
    @State private var presses = GalleryPresses()
    /// The gallery growing back into place as a stack is left (0 to 1), and the stack's artworks
    /// flying back into its cell over it.
    @State private var regrown: CGFloat = 1
    @State private var flown: CGFloat = 0
    @State private var about = false
    /// Gallery Recovery's card, while it shows.
    @State private var recovery: ProgressShown?
    /// The artwork being opened has taken long enough to say so.
    @State private var slowOpening = false

    init(onOpen: @escaping (OpenArtwork) -> Void) {
        self.onOpen = onOpen
    }

    /// What Select's actions act on now (the shared core's rules): a stack picked whole.
    private func picked() -> GalleryPick {
        model.pick(selected)
    }

    /// Several shared as one PDF are named after the one stack picked alone, else the stack open.
    private func shareName(_ pick: GalleryPick) -> String {
        if pick.dirs.count == 1, case let .stack(_, name, _)? = model.entries.first(where: { $0.dir == pick.dirs.first }) {
            return name
        }
        return model.openStack?.name ?? GalleryModel.groupName
    }

    /// Selecting, a tap picks the artwork or stack in `dir`, or lets it go.
    private func toggle(_ dir: String) {
        if selected.contains(dir) {
            selected.remove(dir)
        } else {
            selected.insert(dir)
        }
    }

    /// Select ends: its marks fade out and the title row changes back (0.15 s, as measured).
    private func endSelecting() {
        withAnimation(Motion.selectMode) {
            selecting = false
            selected = []
        }
        moreOpen = false
    }

    /// A stack's artworks flying back into its cell, over the gallery growing back.
    @ViewBuilder private func flyingBack(_ screen: CGSize) -> some View {
        if let back = model.flyingBack {
            let columns = galleryColumnCount(screen)
            let cell = CellSize(screenWidth: screen.width, columns: columns)
            GeometryReader { place in
                let origin = Interface.shared.rect(place.frame(in: .global)).origin
                ZStack(alignment: .topLeading) {
                    ForEach(back.cells, id: \.artwork.dir) { flying in
                        let at = flying.at
                        ArtworkCell(
                            model: model, artwork: flying.artwork, cell: cell, version: model.version, chosen: nil, wash: false
                        )
                        .offset(
                            x: at.minX - origin.x + (back.to.minX - at.minX) * flown,
                            y: at.minY - origin.y + (back.to.minY - at.minY) * flown
                        )
                    }
                }
            }
            .allowsHitTesting(false)
        }
    }

    /// Nothing is open over the gallery, so a touch on a cell can be taken for a drag or a spread; nor
    /// is it zooming into an artwork or out of one.
    private func quiet() -> Bool {
        guard GalleryZoomer.shared.zoom == nil, !selecting, !newCanvasOpen, !model.customOpen, sharing == nil, renaming == nil, deleting == nil,
              deletingStack == nil, model.endedOpening == nil, pdfFor == nil, previewing == nil, opening == nil, !about
        else { return false }
        return true
    }

    /// Two fingers began at `point` on the window: the artwork whose picture is there starts to open,
    /// if nothing is open over the gallery.
    private func startOpening(at point: CGPoint) -> Bool {
        guard quiet(), let a = model.artworks.first(where: { model.thumbAt[$0.dir]?.contains(point) == true }),
              let from = model.thumbAt[a.dir]
        else { return false }
        swiped = nil
        opening = PinchOpening(artwork: a, from: from, at: point)
        return true
    }

    private func tapped(_ a: GalleryArtwork, at y: CGFloat, cell: CellSize) {
        if swiped != nil {
            withAnimation(Motion.swipeSettle) { swiped = nil }
        } else if selecting {
            toggle(a.dir)
        } else if y > cell.nameTop {
            renaming = Renaming(id: a.dir, name: a.name, shown: a, stack: false)
        } else {
            model.open(a)
        }
    }

    /// How far a swiped cell's actions shift to stay on the screen: they are wider than a narrow
    /// cell, so the first and last columns' would run off it.
    private func pillShift(_ dir: String, cell: CellSize, columns: Int) -> CGFloat {
        let over = max((cell.pill * 3 + swipePillGap * 2 - cell.pitch) / 2, 0)
        let column = (model.entries.firstIndex { $0.dir == dir } ?? 0) % columns
        return column == 0 ? over : column == columns - 1 ? -over : 0
    }

    /// Moved left across the cell of `dir`, its actions show; moved right, they go.
    private func swipe(_ dir: String) -> some Gesture {
        DragGesture(minimumDistance: 14).onChanged { drag in
            guard !selecting, opening == nil, self.drag.dragging == nil, abs(drag.translation.width) > abs(drag.translation.height) else { return }
            let next = drag.translation.width < 0 ? dir : nil
            if next != swiped {
                withAnimation(Motion.swipeSettle) { swiped = next }
            }
        }
    }

    private func artworkCell(_ a: GalleryArtwork, cell: CellSize, columns: Int) -> some View {
        let open = swiped == a.dir
        let chosen = selected.contains(a.dir)
        return ArtworkCell(
            model: model, artwork: a, cell: cell, version: model.version,
            chosen: selecting ? chosen : nil, wash: drag.onto == a.dir,
            lift: model.lifted == a.dir ? Motion.artworkLifted : model.pressed == a.dir ? Motion.artworkPressed : 1
        )
        .modifier(CellPlace(model: model, dir: a.dir))
        .modifier(FlyIn(from: model.flyOutOf))
        // dragged, it stays behind faint
        .opacity(drag.dragging == a.dir ? draggedAlpha : 1)
        .opacity(open ? 1 - swipedFade : 1)
        // out of its place while two fingers spread it
        .opacity(opening?.artwork.dir == a.dir ? 0 : 1)
        .overlay(alignment: .top) {
            if open {
                SwipeActions(
                    width: cell.pill,
                    onShare: { at in sharing = GalleryShare(artworks: [a], from: at) },
                    onDuplicate: {
                        swiped = nil
                        model.duplicate(a)
                    },
                    onDelete: {
                        swiped = nil
                        deleting = a
                    }
                )
                // centred on the thumbnail's frame
                .padding(.top, 39 + cell.thumb / 2 - swipePillHeight / 2)
                .offset(x: pillShift(a.dir, cell: cell, columns: columns))
                .transition(.scale(scale: 0.2, anchor: .trailing).combined(with: .opacity))
            }
        }
        .zIndex(open ? 1 : 0)
        .contentShape(Rectangle())
        .onTapGesture { location in tapped(a, at: location.y, cell: cell) }
        .simultaneousGesture(swipe(a.dir))
    }

    private func stackCell(dir: String, name: String, artworks: [GalleryArtwork], cell: CellSize, columns: Int) -> some View {
        let open = swiped == dir
        return StackCell(
            name: name, artworks: artworks, cell: cell, version: model.version,
            chosen: selecting ? selected.contains(dir) : nil, wash: drag.onto == dir
        )
        .modifier(CellPlace(model: model, dir: dir))
        .opacity(drag.dragging == dir ? draggedAlpha : 1)
        // swiped open, it fades back and its actions grow in over it, as an artwork's
        .opacity(open ? 1 - swipedFade : 1)
        .overlay(alignment: .top) {
            if open {
                // a stack's act on every artwork in it
                SwipeActions(
                    width: cell.pill,
                    onShare: { at in sharing = GalleryShare(artworks: artworks, from: at, group: name) },
                    onDuplicate: {
                        swiped = nil
                        model.duplicate(dirs: [dir])
                    },
                    onDelete: {
                        swiped = nil
                        deletingStack = model.pick([dir])
                    }
                )
                .padding(.top, 39 + cell.thumb / 2 - swipePillHeight / 2)
                .offset(x: pillShift(dir, cell: cell, columns: columns))
                .transition(.scale(scale: 0.2, anchor: .trailing).combined(with: .opacity))
            }
        }
        .zIndex(open ? 1 : 0)
        .contentShape(Rectangle())
        .onTapGesture { location in
            // a tap puts away the actions showing; selecting, it picks the stack (or lets it go)
            if swiped != nil {
                withAnimation(Motion.swipeSettle) { swiped = nil }
            } else if selecting {
                toggle(dir)
            } else if location.y > cell.nameTop, let top = artworks.first {
                renaming = Renaming(id: dir, name: name, shown: top, stack: true)
            } else {
                model.showStack(dir, name: name)
            }
        }
        .simultaneousGesture(swipe(dir))
    }

    private func grid(_ screen: CGSize) -> some View {
        let columns = galleryColumnCount(screen)
        let cell = CellSize(screenWidth: screen.width, columns: columns)
        return ScrollView {
            LazyVGrid(columns: Array(repeating: GridItem(.fixed(cell.pitch), spacing: 0), count: columns), spacing: 0) {
                ForEach(model.entries, id: \.dir) { entry in
                    switch entry {
                    case let .artwork(artwork):
                        artworkCell(artwork, cell: cell, columns: columns)
                    case let .stack(dir, name, artworks):
                        stackCell(dir: dir, name: name, artworks: artworks, cell: cell, columns: columns)
                    }
                }
            }
            .scrollTargetLayout()
            .padding(.bottom, 24)
        }
        .scrollIndicators(.hidden)
        .scrollDisabled(opening != nil)
        // back from an artwork, the gallery is where it was
        .scrollPosition(id: Binding(get: { model.scrolledTo }, set: { model.scrolledTo = $0 }), anchor: .top)
        // a stack left: the gallery grows back into place, fading in
        .scaleEffect(Motion.stackPopScale + (1 - Motion.stackPopScale) * regrown)
        .opacity(Double(regrown))
        .background {
            GeometryReader { place in
                Color.clear.onChange(of: place.frame(in: .global), initial: true) { _, frame in
                    model.gridAt = Interface.shared.rect(frame)
                }
            }
        }
    }

    /// The title row laid out for the window (the shared core's rules): the buttons that fit, the
    /// rest under More, and the title on one line, shrunk to fit beside them.
    private func headerFit(_ screen: CGSize) -> GalleryHeaderFit {
        let title = model.openStack?.name ?? "ProCraft"
        let width = (title as NSString).size(withAttributes: [.font: titleFont]).width
        // in a stack the name follows the way back; selecting, it stands alone
        let titleX: Float = model.openStack != nil && !selecting ? 60 : 32
        return galleryHeaderFit(
            width: Float(screen.width), titleX: titleX, titleWidth: Float(width.rounded(.up)),
            selecting: selecting, landscape: screen.width > screen.height, inStack: model.openStack != nil
        )
    }

    /// The middle of `action`'s button in the title row, if it shows there.
    private func buttonCentre(_ action: GalleryAction, in fit: GalleryHeaderFit) -> CGFloat? {
        var x = CGFloat(fit.capsuleLeft)
        for button in fit.buttons {
            if button.action == action {
                return x + CGFloat(button.width) / 2
            }
            x += CGFloat(button.width)
        }
        return nil
    }

    /// Whether `action` can act now: Stack on two or more picked, the rest of Select's on any; the
    /// others always.
    private func canAct(_ action: GalleryAction, _ pick: GalleryPick?) -> Bool {
        switch action {
        case .stack: pick?.canStack == true
        case .preview, .share, .duplicate, .delete: pick?.dirs.isEmpty == false
        default: true
        }
    }

    /// What a button in the title row, or a row of More's list, does.
    private func act(_ action: GalleryAction) {
        switch action {
        case .select:
            swiped = nil
            // the marks fade in and the title row changes over 0.15 s, as measured
            withAnimation(Motion.selectMode) { selecting = true }
        case .`import`:
            importAsked = true
        case .photo:
            photoAsked = true
        case .stack:
            model.stackPicked(picked().dirs)
            endSelecting()
        case .preview:
            showPicked()
        case .share:
            let pick = picked()
            sharing = GalleryShare(artworks: pick.artworks, from: nil, group: shareName(pick))
        case .duplicate:
            model.duplicate(dirs: picked().dirs)
            endSelecting()
        case .delete:
            deletingSelected = picked()
        case .more:
            moreOpen.toggle()
        }
    }

    @ViewBuilder private func header(_ screen: CGSize) -> some View {
        let fit = headerFit(screen)
        if selecting {
            let pick = picked()
            SelectHeader(
                title: model.openStack?.name ?? "ProCraft",
                fit: fit,
                canAct: { canAct($0, pick) },
                onAction: act,
                onDone: endSelecting
            )
        } else {
            GalleryHeader(
                stack: model.openStack?.name,
                fit: fit,
                backLit: drag.backLit,
                onBackPlaced: { model.backAt = $0 },
                onBack: { model.showStack(nil) },
                onAbout: { withAnimation(Motion.easeInOut(0.3)) { about = true } },
                onAction: act,
                onNew: { withAnimation(Motion.menuOpen) { newCanvasOpen.toggle() } }
            )
        }
    }

    private func startRecovery() {
        guard recovery == nil else { return }
        let said = "Looking for lost or corrupted artworks"
        Task { @MainActor in
            let search = Task { @MainActor in await model.recoverLost() }
            for step in 0 ..< recoverySearchSteps {
                recovery = ProgressShown(progress: 0.95 * Float(step) / Float(recoverySearchSteps), detail: said)
                try? await Task.sleep(for: recoveryStep)
            }
            await search.value
            recovery = ProgressShown(progress: 1, done: true, detail: said)
            try? await Task.sleep(for: recoveryDone)
            recovery = nil
            withAnimation(Motion.easeInOut(0.15)) { about = false }
        }
    }

    private func showPicked() {
        previewFrom = 0
        previewing = picked().artworks
    }

    /// An artwork spreading open, and Preview, over the whole screen.
    @ViewBuilder private var previews: some View {
        if let spreading = opening {
            PinchOpeningOverlay(opening: spreading) { opened in
                if opened {
                    let all = model.artworks
                    previewFrom = all.firstIndex { $0.dir == spreading.artwork.dir } ?? 0
                    previewing = all
                }
                opening = nil
            }
        }
        if let list = previewing {
            ArtworkPreview(list: list, version: model.version, start: previewFrom) { previewing = nil }
        }
        if about {
            AboutApp(onRecover: startRecovery) {
                withAnimation(Motion.easeInOut(0.15)) { about = false }
            }
            .transition(.opacity)
        }
    }

    /// The menus and panels over the gallery.
    @ViewBuilder private func overlays(_ screen: CGSize) -> some View {
        if newCanvasOpen {
            Color.clear
                .contentShape(Rectangle())
                .onTapGesture { withAnimation(Motion.menuClose) { newCanvasOpen = false } }
            NewCanvasPanel(
                model: model, screen: screen, scale: displayScale,
                onDone: { withAnimation(Motion.menuClose) { newCanvasOpen = false } }
            )
            .menuTransition(.topTrailing)
        }
        let fit = headerFit(screen)
        if moreOpen, !fit.more.isEmpty {
            Color.clear
                .contentShape(Rectangle())
                .onTapGesture { moreOpen = false }
            // what the title row has no room for, greyed until it can act on what is picked
            let pick = selecting ? picked() : nil
            MoreOptions(
                screen: screen, centre: buttonCentre(.more, in: fit) ?? screen.width / 2,
                rows: fit.more.map { action in
                    MoreRow(label: galleryActionLabel(action), enabled: canAct(action, pick)) {
                        moreOpen = false
                        act(action)
                    }
                }
            )
        }
        if let share = sharing {
            Color.clear
                .contentShape(Rectangle())
                .onTapGesture { sharing = nil }
            // under Select's Share, or under More when Share is in its list
            let centre = buttonCentre(.share, in: fit) ?? buttonCentre(.more, in: fit) ?? screen.width / 2
            ImageFormats(share: share, screen: screen, centre: centre) { format in
                sharing = nil
                swiped = nil
                if let animated = GalleryModel.animatedFormats.first(where: { $0.title == format }) {
                    // the first artwork opens into its animated export
                    if let first = share.artworks.first {
                        model.open(first, animated: animated)
                    }
                } else if format == "PDF" {
                    pdfFor = share
                } else {
                    model.share(share.artworks, format: format, from: share.from)
                }
                if selecting {
                    endSelecting()
                }
            }
        }
    }

    /// Everything that asks something over the gallery.
    @ViewBuilder private var alerts: some View {
        AlertView(
            content: deleting.map {
                AlertContent(title: "Delete “\($0.name)”?", message: "This can't be undone.", action: "Delete", icon: PcIcons.trash)
            },
            onCancel: { deleting = nil },
            onAction: {
                if let deleting {
                    model.delete(deleting)
                }
                deleting = nil
            }
        )
        // what Select picked (a stack with its artworks), or a stack swiped to: the core's words
        AlertView(
            content: deletingSelected.map {
                AlertContent(title: $0.deleteTitle, message: $0.deleteMessage, action: "Delete", icon: PcIcons.trash)
            },
            onCancel: { deletingSelected = nil },
            onAction: {
                if let pick = deletingSelected {
                    model.delete(dirs: pick.dirs)
                }
                deletingSelected = nil
                endSelecting()
            }
        )
        AlertView(
            content: deletingStack.map {
                AlertContent(title: $0.deleteTitle, message: $0.deleteMessage, action: "Delete", icon: PcIcons.trash)
            },
            onCancel: { deletingStack = nil },
            onAction: {
                if let pick = deletingStack {
                    model.delete(dirs: pick.dirs)
                }
                deletingStack = nil
            }
        )
        // an artwork whose opening closed the app last time: opened again only when asked to (it can
        // be shared, duplicated or deleted from the gallery meanwhile); how that run ended isn't
        // known here, so the log is asked for, with Share Log beside
        AlertView(
            content: model.endedOpening != nil ? AlertContent(
                title: openingEndedAppTitle(),
                message: openingEndedAppSaid(lowMemory: false),
                action: "Open Anyway", destructive: false, extra: "Share Log"
            ) : nil,
            onCancel: model.keepClosed,
            onAction: model.openAnyway,
            onExtra: shareLog
        )
        AlertView(
            content: pdfFor != nil ? AlertContent(
                title: "PDF quality", action: "Best", icon: PcIcons.page, destructive: false,
                cancel: "Good", extra: "Better"
            ) : nil,
            onCancel: { pdfFor = nil },
            onAction: { sharePdf(PdfQuality.best) },
            onExtra: { sharePdf(PdfQuality.better) },
            onFirst: { sharePdf(PdfQuality.good) }
        )
    }

    /// The log as a text file, to the share sheet (made off the main thread): what the question about
    /// an artwork that closed the app asks for.
    private func shareLog() {
        DispatchQueue.global(qos: .userInitiated).async {
            let log = AppLog.toShare()
            DispatchQueue.main.async {
                if let log {
                    Sharing.share([log])
                }
            }
        }
    }

    private func sharePdf(_ quality: UInt8) {
        guard let share = pdfFor else { return }
        pdfFor = nil
        model.share(share.artworks, format: "PDF", quality: quality, group: share.group, from: nil)
    }

    var body: some View {
        // from the screen's top to its bottom: the grid runs under where the home indicator was
        cards(catchers(GeometryReader { place in screenContent(place) }.ignoresSafeArea(.container, edges: .vertical)))
            .onAppear {
                model.onOpen = onOpen
                model.reload()
                model.bringIn()
                #if DEBUG
                    // the capture says what it wants once the window is up
                    DispatchQueue.main.asyncAfter(deadline: .now() + 1) { showForCapture() }
                #endif
            }
            // an artwork just left is saved while the gallery shows: its new thumbnail read then
            .onReceive(NotificationCenter.default.publisher(for: Gallery.keptNotification)) { note in
                model.artworkSaved()
                if let reason = note.object as? String {
                    failure = saveFailureSaid(reason: reason)
                }
            }
            .alert("Can't open the artwork", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(verbatim: failure ?? "")
            }
            .fileImporter(isPresented: $importAsked, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
                if case let .success(urls) = result {
                    model.importFiles(urls)
                }
            }
            .photosPicker(isPresented: $photoAsked, selection: $photo, matching: .images)
            .onChange(of: photo) { _, item in
                guard let item else { return }
                photo = nil
                Task { @MainActor in
                    model.newFromPicture(try? await item.loadTransferable(type: Data.self))
                }
            }
    }

    /// What reads the gallery's touches on the window, and the gallery growing back from a stack.
    private func catchers<Content: View>(_ content: Content) -> some View {
        content
            .background(Pc.galleryBackground.ignoresSafeArea())
            // as measured: no status bar and no home indicator over the gallery (its title sits 11
            // from the screen's top), Preview's close button where the battery would be
            .statusBarHidden()
            .persistentSystemOverlays(.hidden)
            .background {
                PinchCatcher(
                    onBegin: startOpening(at:),
                    onChange: { scale, middle in opening?.follow(scale: scale, at: middle) },
                    onEnd: { opening?.end() }
                )
            }
            .background { GalleryDragCatcher(model: model, shown: drag, canStart: quiet) }
            .background { PressCatcher(model: model, presses: presses) { quiet() || selecting } }
            .onChange(of: model.leftStack) {
                regrown = 0
                flown = 0
                withAnimation(Motion.stackPop) {
                    regrown = 1
                    flown = 1
                }
            }
    }

    /// What shows over the whole gallery: Preview, the alerts, the cards.
    private func cards<Content: View>(_ content: Content) -> some View {
        content
            .overlay {
                ZStack {
                    previews
                }
            }
            .overlay {
                // only a slow artwork (a big one) says it is being opened; a quick one just opens
                if model.loadingArtwork, slowOpening {
                    ZStack {
                        openingVeil
                        Text(verbatim: "Opening…")
                            .font(.system(size: 17))
                            .foregroundStyle(Pc.text)
                    }
                    .ignoresSafeArea()
                    .allowsHitTesting(false)
                }
            }
            .task(id: model.loadingArtwork) {
                slowOpening = false
                guard model.loadingArtwork else { return }
                try? await Task.sleep(for: slowOpeningAfter)
                if !Task.isCancelled {
                    slowOpening = true
                }
            }
            .overlay { alerts }
            .overlay {
                ProgressCard(shown: model.exportShown, title: "Exporting", glyph: PcIcons.tabShare, onCancel: model.cancelExport)
            }
            .overlay {
                ProgressCard(shown: model.importShown, title: "Importing", glyph: PcIcons.tabImport, onCancel: model.cancelImport)
            }
            .overlay {
                ProgressCard(shown: recovery, title: "Gallery Recovery", glyph: PcIcons.tabImport, onCancel: nil)
            }
    }

    private func screenContent(_ place: GeometryProxy) -> some View {
            let screen = Interface.shared.screen(place.size)
            let renamed = renaming != nil || about
            return ZStack(alignment: .topLeading) {
                VStack(spacing: 0) {
                    header(screen.size)
                    if model.entries.isEmpty {
                        Text(verbatim: "Tap + to start a new canvas")
                            .font(.system(size: 17))
                            .foregroundStyle(Pc.galleryText)
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                    } else {
                        grid(screen.size)
                    }
                }
                .overlay { flyingBack(screen.size) }
                .blur(radius: renamed ? renameBlur : 0)
                .overlay {
                    Pc.galleryBackground
                        .opacity(renaming != nil ? renameDim : 0)
                        .allowsHitTesting(false)
                }
                .animation(.easeInOut(duration: renamed ? 0.3 : 0.15), value: renamed)
                overlays(screen.size)
                if let what = renaming {
                    RenameArtwork(renaming: what, from: model.thumbAt[what.shown.dir], screen: screen.size) { name in
                        if name != what.name {
                            if what.stack {
                                model.renameStack(what.id, name)
                            } else {
                                model.rename(what.shown, name)
                            }
                        }
                        renaming = nil
                    }
                    .id(what.id)
                }
                if let text = model.message {
                    Text(verbatim: text)
                        .foregroundStyle(Pc.text)
                        .padding(.horizontal, 18)
                        .padding(.vertical, 12)
                        .background(RoundedRectangle(cornerRadius: 10, style: .circular).fill(Pc.panel))
                        .padding(32)
                        .frame(width: screen.size.width, height: screen.size.height, alignment: .bottom)
                        .allowsHitTesting(false)
                        .task(id: text) {
                            try? await Task.sleep(for: .seconds(3))
                            if !Task.isCancelled {
                                model.message = nil
                            }
                        }
                }
                if model.customOpen {
                    CustomCanvas(model: model) { newCanvasOpen = false }
                        .transition(.move(edge: .bottom))
                }
            }
            .animation(Motion.studioIn, value: model.customOpen)
            .sized(screen)
    }

    #if DEBUG
        /// What a capture of the gallery opens.
        private func showForCapture() {
            guard let shows = ScreenCapture.galleryShows else { return }
            ScreenCapture.seedGallery(model)
            let first = model.artworks.first
            switch shows {
            case "gallery-select":
                selecting = true
                selected = Set(model.artworks.prefix(2).map(\.dir))
            case "gallery-swipe":
                swiped = first?.dir
            case "gallery-new":
                newCanvasOpen = true
            case "gallery-formats":
                selecting = true
                sharing = GalleryShare(artworks: model.artworks, from: nil)
            case "gallery-rename":
                renaming = first.map { Renaming(id: $0.dir, name: $0.name, shown: $0, stack: false) }
            case "gallery-custom", "gallery-custom-colour":
                model.openCustom()
            case "gallery-more":
                selecting = true
                selected = Set(model.artworks.prefix(2).map(\.dir))
                moreOpen = true
            case "gallery-about":
                about = true
            case "gallery-recovery":
                about = true
                recovery = ProgressShown(progress: 0.6, detail: "Looking for lost or corrupted artworks")
            case "gallery-press":
                model.pressed = first?.dir
            case "gallery-lift":
                model.lifted = first?.dir
            case "gallery-stack":
                model.stack(Array(model.artworks.prefix(2)))
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) {
                    if case let .stack(dir, name, _)? = model.entries.first(where: { if case .stack = $0 { return true } else { return false } }) {
                        model.showStack(dir, name: name)
                    }
                }
            case "gallery-drag":
                drag.dragging = model.artworks.dropFirst().first?.dir
                drag.onto = first?.dir
            case "gallery-preview", "gallery-preview-controls":
                previewFrom = 0
                previewing = model.artworks
            case "gallery-pinch":
                // once the seeded cells have been laid out
                DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
                    guard let a = model.artworks.first, let from = model.thumbAt[a.dir] else { return }
                    let middle = CGPoint(x: from.midX, y: from.midY)
                    let spreading = PinchOpening(artwork: a, from: from, at: middle)
                    spreading.follow(scale: 1.6, at: CGPoint(x: middle.x + 60, y: middle.y + 40))
                    opening = spreading
                }
            default:
                break
            }
        }
    #endif
}

/// The title's name on one line at its size shrunk as `fit` says, cut short if even then too long.
private struct GalleryTitle: View {
    let text: String
    let ink: Color
    let fit: GalleryHeaderFit

    var body: some View {
        Text(verbatim: text)
            .font(.system(size: 36 * CGFloat(fit.titleScale), weight: .bold))
            .foregroundStyle(ink)
            .lineLimit(1)
            .truncationMode(.tail)
            .frame(maxWidth: CGFloat(fit.titleRoom), alignment: .leading)
    }
}

/// A capsule button's face: its name, or, More alone in the capsule, its glyph.
private struct CapsuleLabel: View {
    let button: GalleryButton
    let alone: Bool
    let ink: Color

    var body: some View {
        if button.action == .more, alone {
            PcIconView(PcIcons.more, size: 24, color: ink)
        } else {
            // as measured: medium 15 (16 regular reads taller and lighter than the iPad's)
            Text(verbatim: button.label)
                .font(.system(size: 15, weight: .medium))
                .foregroundStyle(ink)
        }
    }
}

/// The capsule of the title row's buttons as `fit` lays them out, each dimmed while it can't act.
private struct HeaderCapsule: View {
    let fit: GalleryHeaderFit
    let ink: Color
    var canAct: (GalleryAction) -> Bool = { _ in true }
    let onAction: (GalleryAction) -> Void

    var body: some View {
        HStack(spacing: 0) {
            ForEach(fit.buttons.indices, id: \.self) { i in
                let button = fit.buttons[i]
                let enabled = canAct(button.action)
                CapsuleLabel(button: button, alone: fit.buttons.count == 1, ink: enabled ? ink : Pc.gallerySize)
                    .frame(width: CGFloat(button.width), height: 45)
                    .contentShape(Rectangle())
                    .onTapGesture {
                        if enabled {
                            onAction(button.action)
                        }
                    }
            }
        }
        // its edge outside the buttons: 208 round Select, Import and Photo, as measured
        .padding(.horizontal, 1)
        .background(Capsule().fill(Pc.capsule))
        .overlay(Capsule().strokeBorder(Pc.capsuleEdge, lineWidth: 1))
    }
}

/// The title row: the app's name (in a stack, the way back and its name), the capsule of Select,
/// Import and Photo and the round + that makes a new canvas, laid out by `fit`: in a narrow window
/// some or all of the buttons go under More and the title shrinks to fit, never wrapping.
private struct GalleryHeader: View {
    let stack: String?
    let fit: GalleryHeaderFit
    /// An artwork dragged rests on the way back, to go back.
    let backLit: Bool
    let onBackPlaced: (CGRect) -> Void
    let onBack: () -> Void
    let onAbout: () -> Void
    let onAction: (GalleryAction) -> Void
    let onNew: () -> Void

    var body: some View {
        ZStack(alignment: .topLeading) {
            if let stack {
                // in a stack: back to the gallery, and the stack's name
                let ink = backLit ? Pc.accent : Pc.galleryTitle
                HStack(spacing: 0) {
                    PcIconView(PcIcons.chevronLeft, size: 28, color: ink)
                        .frame(width: 40)
                    GalleryTitle(text: stack, ink: ink, fit: fit)
                }
                .frame(height: 41)
                .contentShape(Rectangle())
                .onTapGesture(perform: onBack)
                .background {
                    GeometryReader { place in
                        Color.clear.onChange(of: place.frame(in: .global), initial: true) { _, frame in
                            onBackPlaced(Interface.shared.rect(frame))
                        }
                    }
                }
                .padding(.leading, 20)
                .padding(.top, 12)
            } else {
                // as wide as there is room beside the capsule, centred on the line it has at its size
                GalleryTitle(text: "ProCraft", ink: Pc.galleryTitle, fit: fit)
                    .contentShape(Rectangle())
                    .onTapGesture(perform: onAbout)
                    .frame(height: titleFont.lineHeight)
                    .padding(.leading, 32)
                    .padding(.top, 11)
            }
            HStack(spacing: 0) {
                Spacer()
                HeaderCapsule(fit: fit, ink: Pc.galleryText, onAction: onAction)
                // as measured: a circle 44 across
                ZStack {
                    Circle().fill(Pc.capsule).frame(width: 44, height: 44)
                    Circle().strokeBorder(Pc.capsuleEdge, lineWidth: 1).frame(width: 44, height: 44)
                    PcIconView(PcIcons.add, size: 40, color: Pc.galleryText)
                }
                .frame(width: 44, height: 45)
                .contentShape(Rectangle())
                .onTapGesture(perform: onNew)
                .padding(.leading, 12)
            }
            .padding(.top, 10)
            .padding(.trailing, 20)
        }
        .frame(height: 55, alignment: .top)
    }
}

/// The title row while artworks and stacks are picked: on a wide screen on its side Stack (none in
/// a stack), Preview, Share, Duplicate and Delete, else Share, Delete and More, in a narrow window
/// More alone, as `fit` lays it out; each dimmed until it can act on what is picked (More always
/// opens). Then the blue tick that ends it.
private struct SelectHeader: View {
    let title: String
    let fit: GalleryHeaderFit
    let canAct: (GalleryAction) -> Bool
    let onAction: (GalleryAction) -> Void
    let onDone: () -> Void

    var body: some View {
        ZStack(alignment: .topLeading) {
            GalleryTitle(text: title, ink: Pc.galleryTitle, fit: fit)
                .frame(height: titleFont.lineHeight)
                .padding(.leading, 32)
                .padding(.top, 11)
            HStack(spacing: 0) {
                Spacer()
                HeaderCapsule(fit: fit, ink: Pc.galleryTitle, canAct: canAct, onAction: onAction)
                PcIconView(PcIcons.doneTick, size: 40, color: .white)
                    .frame(width: 44, height: 45)
                    .background(Capsule().fill(doneBlue))
                    .contentShape(Capsule())
                    .onTapGesture(perform: onDone)
                    .padding(.leading, 13)
            }
            .padding(.top, 10)
            .padding(.trailing, 20)
        }
        .frame(height: 55, alignment: .top)
    }
}

/// A swiped artwork's actions, over its thumbnail.
private struct SwipeActions: View {
    /// Each action's width.
    let width: CGFloat
    let onShare: (CGRect) -> Void
    let onDuplicate: () -> Void
    let onDelete: () -> Void

    @State private var shareAt = CGRect.zero

    private func pill(_ label: String, delete: Bool = false, action: @escaping () -> Void) -> some View {
        Text(verbatim: label)
            .font(.system(size: 13, weight: .bold))
            .foregroundStyle(delete ? Color.white : swipePillInk)
            .lineLimit(1)
            .frame(width: width, height: swipePillHeight)
            .background(RoundedRectangle(cornerRadius: 12, style: .circular).fill(delete ? swipeDeleteFill : swipePillFill))
            .contentShape(Rectangle())
            .onTapGesture(perform: action)
    }

    var body: some View {
        HStack(spacing: swipePillGap) {
            pill("Share") { onShare(shareAt) }
                .background {
                    GeometryReader { place in
                        Color.clear.onChange(of: place.frame(in: .global), initial: true) { _, frame in
                            shareAt = frame
                        }
                    }
                }
            pill("Duplicate", action: onDuplicate)
            pill("Delete", delete: true, action: onDelete)
        }
        .frame(width: width * 3 + swipePillGap * 2)
    }
}

/// Flies what it is on in from where `from` (the window, in the interface's points) is, as its own
/// top left there; hidden until it is placed. Nothing moves without `from`.
private struct FlyIn: ViewModifier {
    let from: CGRect?

    @State private var placed = false
    @State private var shift = CGSize.zero

    func body(content: Content) -> some View {
        content
            .background {
                GeometryReader { place in
                    Color.clear.onChange(of: place.frame(in: .global), initial: true) { _, frame in
                        guard let from, !placed else { return }
                        placed = true
                        let at = Interface.shared.rect(frame).origin
                        shift = CGSize(width: from.minX - at.x, height: from.minY - at.y)
                        DispatchQueue.main.async {
                            withAnimation(Motion.stackPushMove) { shift = .zero }
                        }
                    }
                }
            }
            .offset(shift)
            .opacity(from == nil || placed ? 1 : 0)
            .zIndex(from == nil ? 0 : 2)
    }
}

/// Tells the gallery where a cell is on the window while it shows, for where a drag lands.
private struct CellPlace: ViewModifier {
    let model: GalleryModel
    let dir: String

    func body(content: Content) -> some View {
        content.background {
            GeometryReader { place in
                Color.clear
                    .onChange(of: place.frame(in: .global), initial: true) { _, frame in
                        model.cellAt[dir] = Interface.shared.rect(frame)
                    }
                    .onDisappear { model.cellAt[dir] = nil }
            }
        }
    }
}

/// An artwork's picture in the middle of its thumbnail box, as large as fits its square, with a
/// soft shadow under it.
private struct Thumbnail: View {
    let artwork: GalleryArtwork
    let cell: CellSize
    let version: Int
    var chosen: Bool?
    /// Blue, as what a dragged artwork would be stacked with.
    var wash = false
    @State private var picture: UIImage?

    var body: some View {
        let ratio = artwork.shownRatio
        let side = cell.side
        let size = ratio <= 1 ? CGSize(width: side * ratio, height: side) : CGSize(width: side, height: side / ratio)
        let corner = RoundedRectangle(cornerRadius: 8, style: .circular)
        ZStack(alignment: .topTrailing) {
            // a picture with its background hidden shows the gallery through it (white while it
            // loads)
            corner.fill(picture == nil ? Color.white : Pc.galleryBackground)
            if let picture {
                Image(uiImage: picture)
                    .resizable()
                    .interpolation(thumbnailInterpolation(picture, shown: size.width))
            }
            if wash {
                dropWash
            }
            if let chosen {
                // unpicked, a thin light ring
                ZStack {
                    if chosen {
                        Circle().fill(Color(argb: 0xFF0F_7EFD))
                        PcIconView(PcIcons.menuTick, size: 10, color: .white)
                    } else {
                        Circle().strokeBorder(Color(argb: 0xFFCC_CCCC), lineWidth: 1)
                    }
                }
                // as measured: 10 in from the thumbnail's top right
                .frame(width: 14, height: 14)
                .padding(10)
            }
        }
        .frame(width: size.width, height: size.height)
        .clipShape(corner)
        .overlay(corner.strokeBorder(thumbRim, lineWidth: 1))
        .background(
            corner.fill(thumbShadow)
                .blur(radius: 2)
                .offset(y: thumbShadowDrop)
        )
        .task(id: "\(artwork.modifiedMs)-\(version)") {
            let path = (artwork.dir as NSString).appendingPathComponent("thumbnail.png")
            picture = await Task.detached(priority: .userInitiated) { UIImage(contentsOfFile: path) }.value
        }
    }
}

/// An artwork's cell: its thumbnail, then its name and size.
private struct ArtworkCell: View {
    let model: GalleryModel
    let artwork: GalleryArtwork
    let cell: CellSize
    let version: Int
    let chosen: Bool?
    let wash: Bool
    /// How big its thumbnail shows: shrunk while pressed, lifted while held.
    var lift: CGFloat = 1

    var body: some View {
        VStack(spacing: 0) {
            Thumbnail(artwork: artwork, cell: cell, version: version, chosen: chosen, wash: wash)
                .background {
                    GeometryReader { place in
                        Color.clear
                            .onChange(of: place.frame(in: .global), initial: true) { _, frame in
                                model.thumbAt[artwork.dir] = Interface.shared.rect(frame)
                            }
                            // scrolled away it is nowhere, whatever it last said
                            .onDisappear { model.thumbAt[artwork.dir] = nil }
                    }
                }
                .frame(width: cell.width, height: cell.thumb)
                .scaleEffect(lift)
                .padding(.top, 39)
            // as measured: bold 12 (13 semibold reads wider and lighter than the iPad's)
            Text(verbatim: artwork.name)
                .font(.system(size: 12, weight: .bold))
                .foregroundStyle(chosen == true ? Color.white : Pc.galleryText)
                .lineLimit(1)
                .truncationMode(.tail)
                .padding(.top, 12)
            Text(verbatim: "\(artwork.width) × \(artwork.height)px")
                .font(.system(size: 10))
                .foregroundStyle(Pc.gallerySize)
        }
        .frame(width: cell.pitch, height: cell.pitch, alignment: .top)
    }
}

/// A stack: its newest artwork on two more cards, the name, and how many it holds; selecting, the
/// top card shows whether it is picked.
private struct StackCell: View {
    let name: String
    let artworks: [GalleryArtwork]
    let cell: CellSize
    let version: Int
    let chosen: Bool?
    let wash: Bool

    var body: some View {
        VStack(spacing: 0) {
            if let top = artworks.first {
                ZStack {
                    // the top artwork drawn again on two cards under it, a little turned (none for one)
                    if artworks.count > 1 {
                        ForEach([5.0, -4.0], id: \.self) { turn in
                            Thumbnail(artwork: top, cell: cell, version: version)
                                .rotationEffect(.degrees(turn))
                        }
                    }
                    Thumbnail(artwork: top, cell: cell, version: version, chosen: chosen, wash: wash)
                }
                .frame(width: cell.width, height: cell.thumb)
                .padding(.top, 39)
            }
            Text(verbatim: name)
                .font(.system(size: 12, weight: .bold))
                .foregroundStyle(chosen == true ? Color.white : Pc.galleryText)
                .lineLimit(1)
                .truncationMode(.tail)
                .padding(.top, 12)
            Text(verbatim: "\(artworks.count) artworks")
                .font(.system(size: 10))
                .foregroundStyle(Pc.gallerySize)
        }
        .frame(width: cell.pitch, height: cell.pitch, alignment: .top)
    }
}

/// Where the gallery's artworks are kept.
enum Gallery {
    /// An artwork left has been saved; its object is why it couldn't be, if it couldn't.
    static let keptNotification = Notification.Name("ProCraft.artworkKept")
    /// Artworks left and still being saved, on the main thread.
    static var closingSaves = 0

    static func artworkKept(failure: String?) {
        closingSaves -= 1
        NotificationCenter.default.post(name: keptNotification, object: failure)
    }

    private static var support: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? URL(fileURLWithPath: NSTemporaryDirectory())
    }

    static let store: ArtworkStore = ArtworkStore.open(root: support.appendingPathComponent("gallery").path)

    /// Where the artworks being opened are noted until each shows, outside the gallery: one whose
    /// opening ended the app is asked about when next picked.
    static var openMarks: URL {
        try? FileManager.default.createDirectory(at: support, withIntermediateDirectories: true)
        return support.appendingPathComponent("opening.txt")
    }

    /// Brush files brought in with no artwork open, for the next one to add to the library.
    static var pendingBrushes: URL {
        let dir = support.appendingPathComponent("pending-brushes")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }
}
