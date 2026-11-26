import PhotosUI
import SwiftUI
import UIKit
import UniformTypeIdentifiers

private let panelWidth: CGFloat = 376
/// Screens shorter than this (landscape) get the short panel: no history, the rest tighter.
private let shortScreen: CGFloat = 900
/// Short, the classic square takes the height left over: the screen less the bar and arrow (56),
/// the header (64), the sliders (108), the palette (142), the tabs (70) and a 25 margin.
private let shortClassicRest: CGFloat = 465
private let ringOuter: CGFloat = 182
private let ringInner: CGFloat = 124
private let discRadius: CGFloat = 105
private let harmonyRadius: CGFloat = 149
/// A brightness slider runs from black to white whatever the colour.
private let brightnessTrack: [UInt32] = [0x000000, 0xFFFFFF]
/// The space the panel's rows and the menus they open are placed in.
private let colorsSpace = "colors"
private let compactWidth: CGFloat = 260
/// How far the grabber is dragged before the panel floats.
private let undockDrag: CGFloat = 30
/// Where the floating window's grabber is held from: its middle.
private let floatingGrab = CGPoint(x: 130, y: 7.5)
private let floatingGroup = Color.white.opacity(0.06)
private let compactFill = Color(argb: 0xFF1E_1E1E)
private let compactGroup = Color(argb: 0xFF29_2929)
private let compactHexField = Color(argb: 0xFF34_3434)
/// The colour model's button, and its menu's width.
private let modelButtonSize = CGSize(width: 70, height: 25)
private let modelMenuWidth: CGFloat = 160
private var modelButtonFill: Color { Look.shared.light ? Color(argb: 0xFFDC_DCDC) : Color(argb: 0xFF3A_3A3A) }

/// The colour panel's modes.
enum ColorMode: CaseIterable {
    case disc, classic, harmony, value, palettes

    var label: String {
        switch self {
        case .disc: "Disc"
        case .classic: "Classic"
        case .harmony: "Harmony"
        case .value: "Value"
        case .palettes: "Palettes"
        }
    }

    var icon: PcIcon {
        switch self {
        case .disc: PcIcons.modeDisc
        case .classic: PcIcons.modeClassic
        case .harmony: PcIcons.modeHarmony
        case .value: PcIcons.modeValue
        case .palettes: PcIcons.modePalettes
        }
    }
}

/// Companion colours in the harmony mode, as hue turns (degrees) from the chosen colour.
enum HarmonyKind: CaseIterable {
    case complementary, splitComplementary, analogous, triadic, tetradic

    var label: String {
        switch self {
        case .complementary: "Complementary"
        case .splitComplementary: "Split Complementary"
        case .analogous: "Analogous"
        case .triadic: "Triadic"
        case .tetradic: "Tetradic"
        }
    }

    var turns: [Float] {
        switch self {
        case .complementary: [180]
        case .splitComplementary: [150, 210]
        case .analogous: [30, -30]
        case .triadic: [120, 240]
        case .tetradic: [90, 180, 270]
        }
    }
}

/// What the panel's parts open over it, and the files it asks for.
@Observable
private final class ColorUi {
    var harmonyMenu = false
    var addMenu = false
    /// The colour models' menu is open, under the model button (where it is).
    var modelMenu = false
    var modelButton = CGRect.zero
    /// The palette whose menu is open, and where its button is.
    var paletteMenu: Int?
    var paletteButton = CGRect.zero
    var shortMenu = false
    var shortButton = CGRect.zero
    /// The digits typed on the hex keypad since it opened, none while it is shut; where its row is,
    /// and what a whole code sets.
    var hexTyped: String?
    var hexRow = CGRect.zero
    var hexSet: ((UInt32) -> Void)?
    /// The window in the panel's own space, where a panel that can be anywhere keeps the keypad.
    var hexRoom: CGRect?
    /// Where the harmony kinds list goes, from the panel's top left: to the left of it, level with the
    /// wheel, unless it is floating.
    var harmonyAt = CGPoint(x: -184, y: 86 + Pc.arrowHeight)
    var renaming: Int?
    var choosingPhoto = false
    var choosingFile = false
    var takingPhoto = false

    func closeAll() {
        withAnimation(Motion.menuClose) {
            harmonyMenu = false
            addMenu = false
            modelMenu = false
            paletteMenu = nil
            shortMenu = false
        }
        hexTyped = nil
    }
}

/// The colour as a canvas colour.
private func canvasColor(_ rgb: UInt32) -> Color {
    CanvasSpace.shared.color(rgb)
}

private func hsvColor(_ h: Float, _ s: Float, _ v: Float) -> Color {
    canvasColor(CanvasModel.rgb(hue: h, saturation: s, brightness: v))
}

/// The hue slider's colours: the core keeps the hues measured at evenly spaced points along it.
private let sliderHues: [UInt32] = hueStops(control: .slider).map {
    CanvasModel.rgb(hue: $0 / 360, saturation: 1, brightness: 1)
}

/// The colour at `t` (0..1) along evenly spaced `colours`.
private func colourAlong(_ colours: [UInt32], _ t: Float) -> UInt32 {
    guard colours.count > 1 else { return colours.first ?? 0 }
    let p = min(max(t, 0), 1) * Float(colours.count - 1)
    let i = min(Int(p), colours.count - 2)
    let part = p - Float(i)
    func mix(_ shift: UInt32) -> UInt32 {
        let a = Float((colours[i] >> shift) & 0xFF)
        let b = Float((colours[i + 1] >> shift) & 0xFF)
        return UInt32(wholeNumber(a + (b - a) * part)) << shift
    }
    return mix(16) | mix(8) | mix(0)
}

/// The colour panel: the current and second colours, a picker per mode, the history and the mode
/// tabs. With `background` it sets the canvas's background colour instead, titled so, with a tick
/// that closes it. On a screen less than 900 tall it leaves the history out and tightens the
/// pickers to fit.
struct ColorPanel: View {
    let model: CanvasModel
    let arrowX: CGFloat
    let screenHeight: CGFloat
    var background: HeldColor?

    @State private var ui = ColorUi()
    @State private var photo: PhotosPickerItem?
    /// Where the background's colour stands in the colour model.
    @State private var backgroundPlace = ColourPlace()
    /// The colour as the panel opened, for a plugin's swatch of the colour before.
    @State private var opened: UInt32?

    private var pick: any ColorPick {
        if let background {
            return background
        }
        return BrushColor(model: model)
    }

    private var header: some View {
        let short = screenHeight < shortScreen
        // short, the disc reaches up into the header's last 7
        let height: CGFloat = short && model.colorMode == .disc ? 57 : 64
        return ZStack(alignment: .topLeading) {
            if background != nil {
                PanelHeader(title: "Background", start: 20, end: 12, height: height, chevron: false) {
                    PcIconView(PcIcons.doneTick, size: 40, color: .white)
                        .frame(width: 44, height: 44)
                        .background(Circle().fill(Pc.accent))
                        .contentShape(Circle())
                        .onTapGesture { model.endBackgroundPick() }
                        .padding(.top, 19)
                }
            } else if model.colorMode == .palettes {
                PanelHeader(title: "Palettes", start: 20, end: 17, chevron: false) {
                    PcIconView(PcIcons.plusMedium, size: 41, color: Pc.panelGlyph)
                        .frame(width: 40, height: 41)
                        .contentShape(Rectangle())
                        .onTapGesture { withAnimation(Motion.menuOpen) { ui.addMenu = true } }
                        .padding(.top, 12)
                }
            } else {
                PanelHeader(title: "Colors", start: 20, end: 20, height: height, chevron: false) {
                    HStack(spacing: 8) {
                        // the colour model, for the pages that work in one
                        if model.colorMode != .harmony {
                            ColourModelButton(ui: ui)
                        }
                        HStack(spacing: 7) {
                            let swatch = RoundedRectangle(cornerRadius: 6, style: .circular)
                            if ColourLooks.shared.before, let opened {
                                // a plugin's: the colour as the panel opened (a tap goes back to it)
                                // beside the colour now
                                HStack(spacing: 0) {
                                    Rectangle()
                                        .fill(canvasColor(opened))
                                        .contentShape(Rectangle())
                                        .onTapGesture { BrushColor(model: model).setRgb(opened) }
                                    Rectangle().fill(canvasColor(model.colorRgb()))
                                }
                                .clipShape(swatch)
                                .overlay(swatch.strokeBorder(Pc.swatchEdge, lineWidth: 1))
                                .frame(width: 43, height: 25)
                            } else {
                                swatch.fill(canvasColor(model.colorRgb()))
                                    .overlay(swatch.strokeBorder(Pc.swatchEdge, lineWidth: 1))
                                    .frame(width: 43, height: 25)
                            }
                            swatch.fill(canvasColor(model.secondaryRgb))
                                .frame(width: 43, height: 25)
                                .contentShape(Rectangle())
                                .onTapGesture { model.swapColors() }
                        }
                    }
                    .padding(.top, 21.5)
                }
            }
            Capsule()
                .fill(Pc.grabber)
                .frame(width: 40, height: 4)
                .padding(.top, 4)
                .frame(maxWidth: .infinity)
            if background == nil {
                UndockHandle(model: model)
                    .frame(maxWidth: .infinity)
            }
            if model.colorMode == .harmony {
                // the harmony kind under the title; tapping it lists the kinds
                Text(verbatim: model.harmonyKind.label)
                    .font(.system(size: 17))
                    .foregroundStyle(Pc.panelTitle)
                    .frame(height: 28)
                    .contentShape(Rectangle())
                    .onTapGesture { withAnimation(Motion.menuOpen) { ui.harmonyMenu = true } }
                    .padding(.leading, 20)
                    .padding(.top, 50)
            }
        }
    }

    @ViewBuilder
    private func picker(_ c: any ColorPick, short: Bool) -> some View {
        // the disc, Classic and Value work in the colour model chosen (the background's colour with
        // a place of its own)
        let m = ModelColor(c: c, model: AppSettings.shared.colourModel, place: background != nil ? backgroundPlace : model.colourPlace)
        switch model.colorMode {
        case .disc:
            DiscPicker(c: c, short: short, m: m)
        case .classic:
            ClassicPicker(c: c, square: short ? min(max(screenHeight - shortClassicRest, 150), 344) : 344, m: m, session: model.session)
        case .harmony:
            HarmonyPicker(kind: model.harmonyKind, c: c, short: short)
        case .value:
            if let inks = model.inks {
                InkPicker(c: c, ui: ui, inks: inks, m: m)
            } else {
                ValuePicker(c: c, ui: ui, short: short, m: m)
            }
        case .palettes:
            PalettesList(model: model, c: c, ui: ui, short: short)
        }
    }

    var body: some View {
        let short = screenHeight < shortScreen
        let c = pick
        Popover(width: panelWidth, arrowX: arrowX) {
            header
            picker(c, short: short)
            if model.colorMode != .palettes {
                if short {
                    ShortBottom(model: model, c: c, ui: ui)
                } else {
                    HistoryBlock(model: model, c: c)
                    if model.palettes.indices.contains(model.defaultPalette) {
                        let p = model.palettes[model.defaultPalette]
                        Text(verbatim: p.name)
                            .font(.system(size: 14, weight: .semibold))
                            .foregroundStyle(Pc.panelTitle)
                            .lineLimit(1)
                            .padding(.leading, 21)
                            .padding(.top, 13)
                            .frame(maxWidth: .infinity, minHeight: 44, maxHeight: 44, alignment: .topLeading)
                        SwatchGrid(model: model, c: c, index: model.defaultPalette, palette: p)
                            .padding(.horizontal, 8)
                    }
                }
            }
            ModeTabs(model: model)
        }
        .coordinateSpace(.named(colorsSpace))
        // dragged out by its grabber, it isn't seen until let go, the window floating
        .opacity(background == nil && model.colorsUndocking ? 0 : 1)
        // a swatch dragged out and let go over the panel itself fills nothing
        .background {
            if background == nil {
                GeometryReader { place in
                    Color.clear.onChange(of: place.frame(in: .global), initial: true) { _, frame in
                        model.drop.avoid = Interface.shared.rect(frame)
                    }
                }
            }
        }
        .onDisappear {
            if background == nil {
                model.drop.avoid = nil
            }
        }
        .onAppear { opened = model.colorRgb() }
        .overlay(alignment: .topLeading) { ColorMenus(model: model, ui: ui) }
        .photosPicker(isPresented: photoAsked, selection: $photo, matching: .images)
        .onChange(of: photo) { _, item in
            guard let item else { return }
            photo = nil
            Task { @MainActor in
                let data = try? await item.loadTransferable(type: Data.self)
                model.paletteFromPicture(data.flatMap { UIImage(data: $0) })
            }
        }
        .fileImporter(isPresented: fileAsked, allowedContentTypes: [.item]) { result in
            if case let .success(url) = result {
                model.paletteFromFile(url)
            }
        }
        .fullScreenCover(isPresented: cameraAsked) {
            CameraPicker { picture in
                ui.takingPhoto = false
                if let picture {
                    model.paletteFromPicture(picture)
                }
            }
            .ignoresSafeArea()
        }
        #if DEBUG
        .onAppear {
            switch ScreenCapture.colorsShows {
            case "harmony-menu":
                ui.harmonyMenu = true
            case "add":
                ui.addMenu = true
            case "palette-menu":
                ui.paletteMenu = 0
                ui.paletteButton = CGRect(x: 330, y: 150, width: 30, height: 44)
            case "hex":
                ui.hexTyped = ""
                ui.hexRow = CGRect(x: 8, y: 470, width: 360, height: 44)
                ui.hexSet = { _ in }
            default:
                break
            }
        }
        #endif
    }

    private var photoAsked: Binding<Bool> {
        Binding(get: { ui.choosingPhoto }, set: { ui.choosingPhoto = $0 })
    }

    private var fileAsked: Binding<Bool> {
        Binding(get: { ui.choosingFile }, set: { ui.choosingFile = $0 })
    }

    private var cameraAsked: Binding<Bool> {
        Binding(get: { ui.takingPhoto }, set: { ui.takingPhoto = $0 })
    }
}

/// The system's camera, for a palette from a photo taken.
struct CameraPicker: UIViewControllerRepresentable {
    let onDone: (UIImage?) -> Void

    final class Coordinator: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
        let onDone: (UIImage?) -> Void

        init(onDone: @escaping (UIImage?) -> Void) {
            self.onDone = onDone
        }

        func imagePickerController(_ picker: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
            onDone(info[.originalImage] as? UIImage)
        }

        func imagePickerControllerDidCancel(_ picker: UIImagePickerController) {
            onDone(nil)
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(onDone: onDone)
    }

    func makeUIViewController(context: Context) -> UIImagePickerController {
        let picker = UIImagePickerController()
        picker.sourceType = .camera
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_ picker: UIImagePickerController, context: Context) {}
}

/// A colour knob: `fill` inside a thin white ring, over a soft shadow.
private struct Knob: View {
    let radius: CGFloat
    let fill: Color

    var body: some View {
        ZStack {
            Circle()
                .fill(Color(argb: 0x5900_0000))
                .blur(radius: 2)
                .offset(y: 2)
            Circle().fill(fill)
            Circle().strokeBorder(Color.white, lineWidth: 1.2)
        }
        .frame(width: radius * 2, height: radius * 2)
    }
}

/// How near either end of a slider a tap steps it, with a plugin's steps on.
private let stepEnd: CGFloat = 18

/// A 36-tall slider row: a 4 tall track through `colours` and a pill showing the colour under it.
/// Touching anywhere on the row moves the pill there. With a plugin's steps on, a tap within
/// `stepEnd` of either end moves it one `unit` (of 0..1) down or up instead; a drag from anywhere
/// still goes where the finger is.
private struct SliderRow: View {
    let value: Float
    let colours: [UInt32]
    var unit: Float = 0.01
    let onChange: (Float) -> Void

    /// The end a touch began at (-1, 1; 0 between), and whether it has turned into a drag.
    @State private var stepping: (side: Float, dragging: Bool)?

    var body: some View {
        GeometryReader { g in
            let w = g.size.width
            let x = min(max(CGFloat(value) * w, 8), max(w - 8, 8))
            let at: (CGFloat) -> Float = { Float(min(max($0 / max(w, 1), 0), 1)) }
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(LinearGradient(colors: colours.map(canvasColor), startPoint: .leading, endPoint: .trailing))
                    .frame(height: 4)
                Capsule()
                    .fill(canvasColor(colourAlong(colours, value)))
                    .frame(width: 16, height: 10)
                    .offset(x: x - 8)
            }
            .frame(width: w, height: 36)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { drag in
                        guard ColourLooks.shared.steps else {
                            onChange(at(drag.location.x))
                            return
                        }
                        var now: (side: Float, dragging: Bool)
                        if let held = stepping {
                            now = held
                        } else {
                            let start = drag.startLocation.x
                            let side: Float = start < stepEnd ? -1 : start > w - stepEnd ? 1 : 0
                            now = (side: side, dragging: side == 0)
                        }
                        if !now.dragging, hypot(drag.translation.width, drag.translation.height) > 10 {
                            now.dragging = true
                        }
                        stepping = now
                        if now.dragging {
                            onChange(at(drag.location.x))
                        }
                    }
                    .onEnded { _ in
                        if let ended = stepping, !ended.dragging {
                            onChange(min(max(value + ended.side * unit, 0), 1))
                        }
                        stepping = nil
                    }
            )
        }
        .frame(height: 36)
    }
}

/// The classic picker: saturation across and brightness up a square, then a slider for each;
/// compact, a 248 square with the hue alone under it.
private struct ClassicPicker: View {
    let c: any ColorPick
    let square: CGFloat
    var compact = false
    /// The colour in the colour model chosen; none or HSB, Classic is HSB's own.
    var m: ModelColor?
    /// The canvas's session, which draws a picker plugin's square (none: the triangle shows).
    var session: CanvasSession?

    var body: some View {
        if let m, m.model != .hsb {
            ModelClassicPicker(m: m, square: square, compact: compact)
        } else {
            hsbClassic
        }
    }

    @ViewBuilder private var hsbClassic: some View {
        let hue = c.hue
        let pure = CanvasModel.rgb(hue: hue, saturation: 1, brightness: 1)
        let across: CGFloat = compact ? 248 : 344
        let down: CGFloat = compact ? 248 : square
        let looks = ColourLooks.shared
        // a picker plugin's (drawn by the canvas's session) or the triangle in the square's place
        let plugin = looks.pickerOn && session != nil
        // a picker plugin's with the hue on a ring of its own takes the hue slider's place
        let ringed = plugin && PluginsModel.shared.store.pickerHuePoint(h: hue) != nil
        VStack(spacing: 0) {
            if looks.classicReplaced {
                TrianglePicker(c: c, side: compact ? 248 : min(344, down), down: down, plugin: plugin, session: session)
                    .frame(width: across, height: down)
                    .overlay(alignment: .bottomLeading) {
                        if !compact, !looks.classicChoices.isEmpty {
                            ClassicCornerMenu(choices: looks.classicChoices)
                        }
                    }
                    .padding(.horizontal, compact ? 6 : 16)
            } else {
                plainSquare(hue: hue, pure: pure, across: across, down: down)
                    .overlay(alignment: .bottomLeading) {
                        if !compact, !looks.classicChoices.isEmpty {
                            ClassicCornerMenu(choices: looks.classicChoices)
                        }
                    }
                    .padding(.horizontal, compact ? 6 : 16)
            }
            if compact {
                // the hue only, 240 wide under the 248 square
                if !ringed {
                    SliderRow(value: huePosition(control: .slider, hue: hue), colours: sliderHues, unit: 1 / 360) {
                        c.setHsv(hueAt(control: .slider, position: $0), c.saturation, c.brightness)
                    }
                    .padding(.horizontal, 10)
                    .frame(height: 40, alignment: .top)
                }
            } else {
                VStack(spacing: 0) {
                    if !ringed {
                        SliderRow(value: huePosition(control: .slider, hue: hue), colours: sliderHues, unit: 1 / 360) {
                            c.setHsv(hueAt(control: .slider, position: $0), c.saturation, c.brightness)
                        }
                    }
                    SliderRow(value: c.saturation, colours: [0x7F7F7F, pure]) { c.setHsv(hue, $0, c.brightness) }
                    SliderRow(value: c.brightness, colours: brightnessTrack) { c.setHsv(hue, c.saturation, $0) }
                }
                .padding(.horizontal, 20)
            }
        }
    }

    /// The square: saturation across from white to the pure hue, brightness up, with the knob.
    private func plainSquare(hue: Float, pure: UInt32, across: CGFloat, down: CGFloat) -> some View {
        ZStack(alignment: .topLeading) {
            ZStack {
                LinearGradient(colors: [.white, canvasColor(pure)], startPoint: .leading, endPoint: .trailing)
                LinearGradient(colors: [.clear, .black], startPoint: .top, endPoint: .bottom)
            }
            .frame(width: across, height: down)
            .clipShape(RoundedRectangle(cornerRadius: 10, style: .circular))
            .contentShape(Rectangle())
            .gesture(DragGesture(minimumDistance: 0).onChanged { drag in
                let x = Float(min(max(drag.location.x / across, 0), 1))
                let y = Float(min(max(drag.location.y / max(down, 1), 0), 1))
                c.setHsv(hue, x, 1 - y)
            })
            // the knob may hang over the square's edge
            Knob(radius: 10.75, fill: canvasColor(c.rgb()))
                .offset(x: CGFloat(c.saturation) * across - 10.75, y: CGFloat(1 - c.brightness) * down - 10.75)
                .allowsHitTesting(false)
        }
        .frame(width: across, height: down, alignment: .topLeading)
    }
}

/// What a plugin's square's picture was drawn for.
private struct TriangleKey: Equatable {
    let hue: Float
    let plugin: Bool
    let version: Int
    let side: Int
}

/// A plugin's picker in Classic's square, `side` across in the middle of the room `down` tall: a
/// picker plugin's picture, touches and knob (drawn by `session`), or else a triangle (saturation
/// and brightness between the pure hue at the right, white at the top left and black at the bottom
/// left) as the core draws it. A picker plugin's may pick the hue too, on a ring of its own: a
/// touch that starts where it says a hue moves the hue, and the hue's knob sits where it says.
private struct TrianglePicker: View {
    let c: any ColorPick
    let side: CGFloat
    let down: CGFloat
    /// A picker plugin's, not the triangle.
    let plugin: Bool
    let session: CanvasSession?
    @Environment(\.displayScale) private var displayScale
    @State private var picture: (key: TriangleKey, image: UIImage)?
    /// Whether the touch now held began on the plugin's hue ring.
    @State private var onRing: Bool?

    var body: some View {
        let store = PluginsModel.shared.store
        let hue = c.hue
        let key = TriangleKey(hue: hue, plugin: plugin, version: ColourLooks.shared.version, side: Int((side * displayScale).rounded()))
        let huePoint = plugin ? store.pickerHuePoint(h: hue) : nil
        let at = (plugin ? store.pickerPoint(s: c.saturation, v: c.brightness) : nil) ?? trianglePoint(s: c.saturation, v: c.brightness)
        ZStack(alignment: .topLeading) {
            if let image = picture?.image {
                Image(uiImage: image)
                    .resizable()
                    .frame(width: side, height: side)
            }
            if let p = huePoint, p.count >= 2 {
                Knob(radius: 10.75, fill: canvasColor(CanvasModel.rgb(hue: hue, saturation: 1, brightness: 1)))
                    .offset(x: CGFloat(p[0]) * side - 10.75, y: CGFloat(p[1]) * side - 10.75)
                    .allowsHitTesting(false)
            }
            if at.count >= 2 {
                Knob(radius: 10.75, fill: canvasColor(c.rgb()))
                    .offset(x: CGFloat(at[0]) * side - 10.75, y: CGFloat(at[1]) * side - 10.75)
                    .allowsHitTesting(false)
            }
        }
        .frame(width: side, height: side, alignment: .topLeading)
        .contentShape(Rectangle())
        .gesture(
            DragGesture(minimumDistance: 0)
                .onChanged { drag in
                    let x = Float(drag.location.x / max(side, 1))
                    let y = Float(drag.location.y / max(side, 1))
                    if onRing == nil {
                        let sx = Float(drag.startLocation.x / max(side, 1))
                        let sy = Float(drag.startLocation.y / max(side, 1))
                        onRing = plugin && store.pickerHue(x: sx, y: sy) != nil
                    }
                    if onRing == true {
                        if let h = store.pickerHue(x: x, y: y) {
                            c.setHsv(h, c.saturation, c.brightness)
                        }
                        return
                    }
                    let cx = min(max(x, 0), 1)
                    let cy = min(max(y, 0), 1)
                    let sv = (plugin ? store.pickerPick(x: cx, y: cy) : nil) ?? triangleSv(x: cx, y: cy)
                    if sv.count >= 2 {
                        c.setHsv(hue, sv[0], sv[1])
                    }
                }
                .onEnded { _ in onRing = nil }
        )
        .frame(height: down)
        .task(id: key) {
            if picture?.key == key {
                return
            }
            let drawer = plugin ? self.session : nil
            let made = await Task.detached(priority: .userInitiated) { () -> UIImage? in
                let n = UInt32(max(key.side, 1))
                let rgba: Data
                if let drawer {
                    // a plugin's that couldn't be drawn shows nothing (the plugin goes off)
                    rgba = drawer.pickerPicture(hue: key.hue, side: n) ?? Data(count: Int(n) * Int(n) * 4)
                } else {
                    rgba = trianglePicture(hue: key.hue, side: n)
                }
                return CanvasModel.image(ThumbnailImage(width: n, height: n, rgba: rgba))
            }.value
            if let made, !Task.isCancelled {
                picture = (key, made)
            }
        }
    }
}

/// The plugins' menus at Classic's corner: three dots on a dark disc in the square's lower left
/// corner (its touch 36 across), opening a row of their choices' icons above it, those that stand
/// now lit; picking one closes it.
private struct ClassicCornerMenu: View {
    let choices: [PluginChoice]
    @State private var open = false

    var body: some View {
        ZStack {
            Circle()
                .fill(Color.black.opacity(0.45))
                .frame(width: 26, height: 26)
            HStack(spacing: 5.5 - 3.2) {
                ForEach(0 ..< 3, id: \.self) { _ in
                    Circle().fill(Color.white).frame(width: 3.2, height: 3.2)
                }
            }
        }
        .frame(width: 36, height: 36)
        .contentShape(Rectangle())
        .onTapGesture { withAnimation(Motion.menuOpen) { open.toggle() } }
        .overlay(alignment: .bottomLeading) {
            if open {
                ZStack(alignment: .bottomLeading) {
                    MenuDismissal { withAnimation(Motion.menuClose) { open = false } }
                    choiceRow
                        .offset(y: -(36 + 6))
                        .menuTransition(.bottomLeading)
                }
            }
        }
    }

    private var choiceRow: some View {
        let shape = RoundedRectangle(cornerRadius: 17, style: .circular)
        return HStack(spacing: 0) {
            ForEach(choices, id: \.key) { choice in
                // its path data on the icons' 24-unit grid, filled even-odd
                let icon = PcIcon(name: "plugin-choice:" + choice.icon, width: 24, height: 24, parts: [
                    PcIconPart(d: choice.icon, stroke: 0, fill: true, evenOdd: true),
                ])
                PcIconView(icon, size: 26, color: choice.chosen ? Pc.accent : Pc.menuInk)
                    .frame(width: 48, height: 44)
                    .background(RoundedRectangle(cornerRadius: 11, style: .circular).fill(choice.chosen ? Pc.accent.opacity(0.18) : Color.clear))
                    .contentShape(Rectangle())
                    .onTapGesture {
                        withAnimation(Motion.menuClose) { open = false }
                        PluginsModel.shared.choose(choice.key)
                    }
                    .accessibilityLabel(Text(verbatim: choice.label))
            }
        }
        .padding(6)
        .background(shape.fill(Pc.glassMenuOverPanel))
        .overlay(shape.strokeBorder(Pc.glassMenuEdge, lineWidth: 1))
        .fixedSize()
    }
}

/// Classic in another colour model: its square (saturation across, the model's third channel up, as
/// the core draws it), then sliders for its hue, saturation and third; compact, the hue only.
private struct ModelClassicPicker: View {
    let m: ModelColor
    let square: CGFloat
    var compact = false
    @Environment(\.displayScale) private var displayScale
    /// The square's picture, and the model and hue it was made for.
    @State private var picture: (key: ModelHueKey, image: UIImage)?

    var body: some View {
        let model = m.model
        let coords = m.coords()
        let tracks = colourModelTracks(model: model, h: coords[0], s: coords[1], t: coords[2])
        let across: CGFloat = compact ? 248 : 344
        let down: CGFloat = compact ? 248 : square
        let side = Int(colourModelSquareSide(side: UInt32(max((across * displayScale).rounded(), 1))))
        let key = ModelHueKey(model: model, hue: coords[0])
        VStack(spacing: 0) {
            ZStack(alignment: .topLeading) {
                ZStack {
                    if let image = picture?.image {
                        Image(uiImage: image)
                            .resizable()
                    }
                }
                .frame(width: across, height: down)
                .clipShape(RoundedRectangle(cornerRadius: 10, style: .circular))
                .contentShape(Rectangle())
                .gesture(DragGesture(minimumDistance: 0).onChanged { drag in
                    let x = Float(min(max(drag.location.x / across, 0), 1))
                    let y = Float(min(max(drag.location.y / max(down, 1), 0), 1))
                    m.set([m.coords()[0], x, 1 - y])
                })
                // the knob may hang over the square's edge
                Knob(radius: 10.75, fill: canvasColor(m.c.rgb()))
                    .offset(x: CGFloat(coords[1]) * across - 10.75, y: CGFloat(1 - coords[2]) * down - 10.75)
                    .allowsHitTesting(false)
            }
            .frame(width: across, height: down, alignment: .topLeading)
            .padding(.horizontal, compact ? 6 : 16)
            if compact {
                TrackSlider(m: m, coords: coords, tracks: tracks, channel: 0)
                    .padding(.horizontal, 10)
                    .frame(height: 40, alignment: .top)
            } else {
                VStack(spacing: 0) {
                    ForEach(0 ..< 3, id: \.self) { channel in
                        TrackSlider(m: m, coords: coords, tracks: tracks, channel: channel)
                    }
                }
                .padding(.horizontal, 20)
            }
        }
        .task(id: key) {
            // a soft gradient, drawn at the core's few pixels across and stretched
            if picture?.key == key {
                return
            }
            let made = await Task.detached(priority: .userInitiated) { () -> UIImage? in
                let rgba = colourModelSquarePicture(model: model, hue: key.hue, side: UInt32(side))
                return CanvasModel.image(ThumbnailImage(width: UInt32(side), height: UInt32(side), rgba: rgba))
            }.value
            if let made, !Task.isCancelled {
                picture = (key, made)
            }
        }
    }
}

/// A colour model and a hue, for what a picture was made for.
private struct ModelHueKey: Equatable {
    let model: ColourModel
    let hue: Float
}

/// The slider of channel `channel` (0 the hue) of the colour at `coords` in `m`'s model, its
/// colours `tracks`' (the core's).
private struct TrackSlider: View {
    let m: ModelColor
    let coords: [Float]
    let tracks: [ColourTrack]
    let channel: Int

    var body: some View {
        if tracks.indices.contains(channel) {
            let track = tracks[channel]
            SliderRow(value: track.place, colours: track.colours) { to in
                m.set(colourModelSlid(model: m.model, h: coords[0], s: coords[1], t: coords[2], channel: UInt32(channel), to: to))
            }
        }
    }
}

/// The harmony picker: a 298 wheel (hue around, saturation out, in Display P3; compact, 200) with a
/// knob for the colour and one for each companion of the harmony kind, and a brightness slider below.
private struct HarmonyPicker: View {
    let kind: HarmonyKind
    let c: any ColorPick
    let short: Bool
    var compact = false
    /// Its wheel's radius, where it isn't the usual one.
    var across: CGFloat?

    /// The hue, saturation and brightness dragged to, kept while the colour they make is the colour
    /// (not worked out again from it: a dark or greyish colour's would wander, and black's be lost).
    @State private var held: (rgb: UInt32, hsv: [Float])?
    @State private var wheel: (value: Float, image: UIImage)?
    @Environment(\.displayScale) private var displayScale

    private func set(_ h: Float, _ s: Float, _ v: Float) {
        let p3 = CanvasSpace.shared.p3
        c.setRgb(p3HsvToRgb(h: h, s: s, v: v, p3: p3))
        held = (c.rgb(), [h, s, v])
    }

    var body: some View {
        let p3 = CanvasSpace.shared.p3
        let rgb = c.rgb()
        let hsv = held.flatMap { $0.rgb == rgb ? $0.hsv : nil } ?? rgbToP3Hsv(rgb: rgb, p3: p3)
        let radius: CGFloat = across ?? (compact ? 100 : short ? harmonyRadius - 1 : harmonyRadius)
        let value = hsv[2]
        let at: (Float) -> CGPoint = { turn in
            let a = CGFloat(turn) * 2 * .pi
            let reach = CGFloat(hsv[1]) * radius
            return CGPoint(x: radius + cos(a) * reach, y: radius - sin(a) * reach)
        }
        VStack(spacing: 0) {
            ZStack(alignment: .topLeading) {
                if let picture = wheel?.image {
                    Image(uiImage: picture)
                        .resizable()
                        .frame(width: radius * 2, height: radius * 2)
                }
                ForEach(Array(kind.turns.enumerated()), id: \.offset) { _, t in
                    let h = ((hsv[0] + t / 360).truncatingRemainder(dividingBy: 1) + 1).truncatingRemainder(dividingBy: 1)
                    let p = at(h)
                    Knob(radius: 10.75, fill: canvasColor(p3HsvToRgb(h: h, s: hsv[1], v: value, p3: p3)))
                        .offset(x: p.x - 10.75, y: p.y - 10.75)
                }
                let p = at(hsv[0])
                Knob(radius: 14.5, fill: canvasColor(rgb))
                    .offset(x: p.x - 14.5, y: p.y - 14.5)
            }
            .frame(width: radius * 2, height: radius * 2, alignment: .topLeading)
            .contentShape(Circle())
            .gesture(DragGesture(minimumDistance: 0).onChanged { drag in
                let u = Float(min(max(drag.location.x / (radius * 2), 0), 1)) * 2 - 1
                let w = Float(min(max(drag.location.y / (radius * 2), 0), 1)) * 2 - 1
                let h = (atan2(-w, u) / (2 * .pi) + 1).truncatingRemainder(dividingBy: 1)
                set(h, min((u * u + w * w).squareRoot(), 1), value)
            })
            SliderRow(value: value, colours: brightnessTrack) { set(hsv[0], hsv[1], $0) }
                .padding(.horizontal, compact ? 10 : 20)
                // floating (its wheel's size given), the slider's middle 31 under the wheel
                .padding(.top, across != nil ? 13 : compact ? 8 : short ? 10 : 31)
        }
        .frame(maxWidth: .infinity)
        // the header is 78 tall here, with the kind under the title
        .padding(.top, short ? 4 : 10)
        .task(id: value) {
            // the harmony wheel at the colour's value, as the core draws it
            if wheel?.value == value {
                return
            }
            let side = Int((radius * displayScale / 2).rounded())
            let made = await Task.detached(priority: .userInitiated) { () -> UIImage? in
                let rgba = harmonyWheel(v: value, side: UInt32(side), p3: p3)
                return CanvasModel.image(ThumbnailImage(width: UInt32(side), height: UInt32(side), rgba: rgba))
            }.value
            if let made {
                wheel = (value, made)
            }
        }
    }
}

/// Red, green and blue (0..255) as 0xRRGGBB.
private func packed(_ r: UInt32, _ g: UInt32, _ b: UInt32) -> UInt32 {
    (r << 16) | (g << 8) | b
}

/// Number sliders: hue, saturation and brightness (or the colour model's three), then red, green
/// and blue, then the hex code.
private struct ValuePicker: View {
    let c: any ColorPick
    let ui: ColorUi
    let short: Bool
    var m: ModelColor?

    private func group<Rows: View>(@ViewBuilder _ rows: () -> Rows) -> some View {
        VStack(spacing: short ? 7 : 9) {
            rows()
        }
        .padding(.top, short ? 3 : 9)
        .frame(maxWidth: .infinity)
        .frame(height: short ? 151 : 167, alignment: .top)
        .background(RoundedRectangle(cornerRadius: 10, style: .circular).fill(Pc.valueGroup))
    }

    var body: some View {
        let rgb = c.rgb()
        let r = (rgb >> 16) & 0xFF
        let g = (rgb >> 8) & 0xFF
        let b = rgb & 0xFF
        let byte: (Float) -> UInt32 = { UInt32(wholeNumber($0 * 255)) }
        VStack(spacing: 0) {
            group {
                ModelValueRows(c: c, m: m)
            }
            group {
                ValueRow(letter: "R", value: Float(r) / 255, colours: [packed(0, g, b), packed(255, g, b)], shown: "\(r)", unit: 1 / 255) {
                    c.setRgb(packed(byte($0), g, b))
                }
                ValueRow(letter: "G", value: Float(g) / 255, colours: [packed(r, 0, b), packed(r, 255, b)], shown: "\(g)", unit: 1 / 255) {
                    c.setRgb(packed(r, byte($0), b))
                }
                ValueRow(letter: "B", value: Float(b) / 255, colours: [packed(r, g, 0), packed(r, g, 255)], shown: "\(b)", unit: 1 / 255) {
                    c.setRgb(packed(r, g, byte($0)))
                }
            }
            .padding(.top, 6)
            HexRow(rgb: rgb, ui: ui) { c.setRgb($0) }
            if short {
                Color.clear.frame(height: 2)
            }
        }
        .padding(.horizontal, 8)
    }
}

/// Value on a CMYK canvas: the hue, saturation and brightness (or the colour model's three), the
/// four inks, then the hex code.
private struct InkPicker: View {
    let c: any ColorPick
    let ui: ColorUi
    let inks: Inks
    var m: ModelColor?

    /// The inks dragged to, kept while the colour they make is the colour (not worked out again
    /// from it, which could move the other inks).
    @State private var held: (rgb: UInt32, inks: [Float])?

    private func group<Rows: View>(height: CGFloat, gap: CGFloat, @ViewBuilder _ rows: () -> Rows) -> some View {
        VStack(spacing: gap) {
            rows()
        }
        .padding(.top, 1)
        .frame(maxWidth: .infinity)
        .frame(height: height, alignment: .top)
        .background(RoundedRectangle(cornerRadius: 10, style: .circular).fill(Pc.valueGroup))
    }

    var body: some View {
        let rgb = c.rgb()
        let now = held.flatMap { $0.rgb == rgb ? $0.inks : nil } ?? inks.of(rgb)
        let with: (Int, Float) -> [Float] = { k, v in
            var next = now
            if next.indices.contains(k) {
                next[k] = min(max(v, 0), 1)
            }
            return next
        }
        VStack(spacing: 0) {
            group(height: 142, gap: 4) {
                ModelValueRows(c: c, m: m)
            }
            group(height: 187, gap: 3) {
                ForEach(Array(["C", "M", "Y", "K"].enumerated()), id: \.offset) { k, letter in
                    let v = now.indices.contains(k) ? now[k] : 0
                    ValueRow(letter: letter, value: v, colours: [inks.rgb(with(k, 0)), inks.rgb(with(k, 1))], shown: "\(wholeNumber(v * 100))%") {
                        let next = with(k, $0)
                        let made = inks.rgb(next)
                        held = (made, next)
                        c.setRgb(made)
                    }
                }
            }
            .padding(.top, 8)
            HexRow(rgb: rgb, ui: ui) { c.setRgb($0) }
                .padding(.top, 2)
        }
        .padding(.horizontal, 8)
    }
}

/// Value's first three sliders: hue, saturation and brightness, or the colour model's (OKLab's: its
/// own lightness, a and b, as the core works them out).
private struct ModelValueRows: View {
    let c: any ColorPick
    let m: ModelColor?

    var body: some View {
        if m?.model == .oklab {
            // a and b run ±0.4 across their sliders
            let made = oklabOfHsv(h: c.hue, s: c.saturation, v: c.brightness)
            let lab = made.count >= 3 ? made : [0, 0, 0]
            let tracks = oklabValueTracks(l: lab[0], a: lab[1], b: lab[2])
            ForEach(Array(["L", "a", "b"].enumerated()), id: \.offset) { i, letter in
                if tracks.indices.contains(i) {
                    ValueRow(
                        letter: letter, value: tracks[i].place, colours: tracks[i].colours,
                        shown: i == 0 ? "\(wholeNumber(lab[0] * 100))" : signedShown(lab[i])
                    ) {
                        let hsv = oklabValueSlid(l: lab[0], a: lab[1], b: lab[2], channel: UInt32(i), to: $0, hue: c.hue)
                        if hsv.count >= 3 {
                            c.setHsv(hsv[0], hsv[1], hsv[2])
                        }
                    }
                }
            }
        } else if let m, m.model != .hsb {
            let coords = m.coords()
            let tracks = colourModelTracks(model: m.model, h: coords[0], s: coords[1], t: coords[2])
            let letters = colourModelLetters(model: m.model)
            ForEach(0 ..< 3, id: \.self) { i in
                if tracks.indices.contains(i) {
                    ValueRow(
                        letter: letters.indices.contains(i) ? letters[i] : "", value: tracks[i].place, colours: tracks[i].colours,
                        shown: i == 0 ? degreesShown(coords[0]) : percentShown(coords[i]), unit: i == 0 ? 1 / 360 : 0.01
                    ) {
                        m.set(colourModelSlid(model: m.model, h: coords[0], s: coords[1], t: coords[2], channel: UInt32(i), to: $0))
                    }
                }
            }
        } else {
            let hue = c.hue
            ValueRow(letter: "H", value: huePosition(control: .slider, hue: hue), colours: sliderHues, shown: degreesShown(hue), unit: 1 / 360) {
                c.setHsv(hueAt(control: .slider, position: $0), c.saturation, c.brightness)
            }
            ValueRow(letter: "S", value: c.saturation, colours: [0x7F7F7F, CanvasModel.rgb(hue: hue, saturation: 1, brightness: 1)], shown: percentShown(c.saturation)) {
                c.setHsv(hue, $0, c.brightness)
            }
            ValueRow(letter: "B", value: c.brightness, colours: brightnessTrack, shown: percentShown(c.brightness)) {
                c.setHsv(hue, c.saturation, $0)
            }
        }
    }
}

/// One number slider: its letter, a track from 32 to 296, and the value centred at 326.5.
private struct ValueRow: View {
    let letter: String
    let value: Float
    let colours: [UInt32]
    let shown: String
    /// One step of a plugin's steps (of 0..1).
    var unit: Float = 0.01
    let onChange: (Float) -> Void

    var body: some View {
        ZStack(alignment: .leading) {
            Text(verbatim: letter)
                .font(.system(size: 15))
                .foregroundStyle(Pc.panelTitle)
                .padding(.leading, 13)
            SliderRow(value: value, colours: colours, unit: unit, onChange: onChange)
                .frame(width: 264)
                .padding(.leading, 32)
            Text(verbatim: shown)
                .font(.system(size: 15))
                .foregroundStyle(Pc.panelTitle)
                .lineLimit(1)
                .frame(width: 45)
                .padding(.leading, 304)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .frame(height: 44)
    }
}

/// The colour as a hex code: a tap opens the hex keypad beside the panel, and the code typed on it
/// (six digits) sets the colour.
private struct HexRow: View {
    let rgb: UInt32
    let ui: ColorUi
    var rowFill = Pc.hexRow
    var fieldFill = Pc.hexField
    let onSet: (UInt32) -> Void

    var body: some View {
        let typed = ui.hexTyped
        let field = RoundedRectangle(cornerRadius: 7, style: .circular)
        let shown = typed.flatMap { $0.isEmpty ? nil : $0 } ?? String(format: "%06X", rgb)
        HStack(spacing: 0) {
            Text(verbatim: "Hexadecimal")
                .font(.system(size: 16))
                .foregroundStyle(Pc.panelTitle)
                .padding(.leading, 13)
            Spacer(minLength: 0)
            Text(verbatim: "#" + shown)
                .font(.system(size: 14))
                .foregroundStyle(Pc.panelTitle)
                .lineLimit(1)
                .frame(width: 84.5, height: 28)
                .background(field.fill(fieldFill))
                .overlay {
                    if typed != nil {
                        field.strokeBorder(Pc.accent, lineWidth: 1)
                    }
                }
                .padding(.trailing, 8)
        }
        .frame(height: 44)
        .background(RoundedRectangle(cornerRadius: 10, style: .circular).fill(rowFill))
        .overlay {
            GeometryReader { g in
                Color.clear
                    .contentShape(Rectangle())
                    .onTapGesture {
                        ui.hexRow = g.frame(in: .named(colorsSpace))
                        ui.hexSet = onSet
                        ui.hexTyped = ""
                    }
            }
        }
        .padding(.top, 6)
    }
}

/// The hex keypad while it is open, for a ZStack over a panel: beside the panel, its middle level
/// with the row, and on the window where the panel knows where that is.
private struct HexOverlay: View {
    let ui: ColorUi

    private var at: CGPoint {
        var x = ui.hexRow.minX - 12 - 259
        var y = ui.hexRow.midY - 170
        if let room = ui.hexRoom {
            x = max(x, room.minX)
            y = min(max(y, room.minY), max(room.maxY - 340, room.minY))
        }
        return CGPoint(x: x, y: y)
    }

    var body: some View {
        if let typed = ui.hexTyped {
            MenuDismissal { ui.closeAll() }
            HexPad(
                onKey: { key in
                    // a seventh digit starts a new code
                    let next = (typed.count >= 6 ? "" : typed) + String(key)
                    ui.hexTyped = next
                    if next.count == 6, let rgb = UInt32(next, radix: 16) {
                        ui.hexSet?(rgb)
                    }
                },
                onBack: { ui.hexTyped = String(typed.dropLast()) }
            )
            .offset(x: at.x, y: at.y)
        }
    }
}

/// The hex keypad: D E F, A B C, 7 8 9, 4 5 6, 1 2 3, then a wide 0 and backspace; 259 x 340.
private struct HexPad: View {
    let onKey: (Character) -> Void
    let onBack: () -> Void

    private let keyInk = Color(argb: 0xFFEE_EEEE)
    /// Frosted, as measured: a grey tint at 77% over the blur (what is behind shows through), the
    /// keys white at 7% over it.
    private let padTint = Pc.onDarkFrost(0xC53C_3C3C)
    private let keyFill = Color.white.opacity(0.07)

    private func key<Label: View>(width: CGFloat, action: @escaping () -> Void, @ViewBuilder label: () -> Label) -> some View {
        ZStack {
            RoundedRectangle(cornerRadius: 21, style: .circular).fill(keyFill)
            label()
        }
        .frame(width: width, height: 50)
        .contentShape(Rectangle())
        .onTapGesture(perform: action)
    }

    private func digit(_ c: Character, width: CGFloat) -> some View {
        key(width: width, action: { onKey(c) }) {
            Text(verbatim: String(c))
                .font(.system(size: 21))
                .foregroundStyle(keyInk)
        }
    }

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: 30, style: .circular)
        VStack(spacing: 4) {
            ForEach(["DEF", "ABC", "789", "456", "123"], id: \.self) { row in
                HStack(spacing: 4) {
                    ForEach(Array(row), id: \.self) { c in
                        digit(c, width: 77)
                    }
                }
            }
            HStack(spacing: 4) {
                digit("0", width: 158)
                key(width: 77, action: onBack) {
                    PcIconView(PcIcons.hexBackspace, size: 24, color: Color(argb: 0xFFBA_BABA))
                }
            }
        }
        .padding(10)
        .frame(width: 259, height: 340, alignment: .top)
        .background {
            ZStack {
                shape.fill(.ultraThinMaterial)
                    .environment(\.colorScheme, .dark)
                shape.fill(padTint)
            }
        }
        .overlay(shape.strokeBorder(Color(argb: 0xFF26_2626), lineWidth: 1))
    }
}

/// The last colours painted with, under their title and Clear.
private struct HistoryBlock: View {
    let model: CanvasModel
    let c: any ColorPick

    var body: some View {
        ZStack(alignment: .topLeading) {
            Text(verbatim: "History")
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(Pc.panelTitle)
                .padding(.leading, 21)
                .padding(.top, 13)
            Text(verbatim: "Clear")
                .font(.system(size: 13))
                .foregroundStyle(Pc.clearText)
                .frame(width: 86, height: 22.5)
                .background(RoundedRectangle(cornerRadius: 7, style: .circular).fill(Pc.setCell))
                .contentShape(Rectangle())
                .onTapGesture { model.clearHistory() }
                .padding(.top, 11)
                .padding(.trailing, 16)
                .frame(maxWidth: .infinity, alignment: .trailing)
        }
        .frame(height: 44, alignment: .top)
        HistoryStrip(model: model, c: c)
    }
}

/// The last colours painted with, ten 36 swatches; a tap takes one.
private struct HistoryStrip: View {
    let model: CanvasModel
    let c: any ColorPick

    var body: some View {
        HStack(spacing: 0) {
            ForEach(0 ..< 10, id: \.self) { i in
                let rgb = model.history.indices.contains(i) ? model.history[i] : nil
                Rectangle()
                    .fill(rgb.map(canvasColor) ?? Pc.emptySwatch)
                    .contentShape(Rectangle())
                    .onTapGesture {
                        if let rgb {
                            c.setRgb(rgb)
                        }
                    }
            }
        }
        .frame(height: 36)
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .circular))
        .padding(.horizontal, 8)
    }
}

/// The short panel's last block: the colour history (a 44 header and the strip) or the active
/// palette (a 34 header and its grid), whichever its dots' menu picked.
private struct ShortBottom: View {
    let model: CanvasModel
    let c: any ColorPick
    let ui: ColorUi

    var body: some View {
        let palette = model.palettes.indices.contains(model.defaultPalette) ? model.palettes[model.defaultPalette] : nil
        let history = AppSettings.shared.colorHistoryShown || palette == nil
        ZStack(alignment: .topLeading) {
            Text(verbatim: history ? "History" : palette?.name ?? "")
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(Pc.panelTitle)
                .lineLimit(1)
                .padding(.leading, 21)
                .padding(.top, history ? 13 : 8)
            HStack(spacing: 4.9) {
                ForEach(0 ..< 3, id: \.self) { _ in
                    Circle().fill(Pc.panelTitle).frame(width: 3.6, height: 3.6)
                }
            }
            .frame(width: 44, height: 34)
            .overlay {
                GeometryReader { g in
                    Color.clear
                        .contentShape(Rectangle())
                        .onTapGesture {
                            ui.shortButton = g.frame(in: .named(colorsSpace))
                            withAnimation(Motion.menuOpen) { ui.shortMenu = true }
                        }
                }
            }
            .padding(.trailing, 9)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .trailing)
        }
        .frame(height: history ? 44 : 34)
        if history {
            HistoryStrip(model: model, c: c)
        } else if let palette {
            SwatchGrid(model: model, c: c, index: model.defaultPalette, palette: palette)
                .padding(.horizontal, 8)
        }
    }
}

/// A palette's swatches, ten by three, 36 each. Tap a colour to paint with it, tap an empty slot to
/// keep the current colour there, hold a colour to take it out.
private struct SwatchGrid: View {
    let model: CanvasModel
    let c: any ColorPick
    let index: Int
    let palette: PaletteInfo
    /// A colour dragged out onto the canvas fills there.
    var droppable = true

    var body: some View {
        VStack(spacing: 0) {
            ForEach(0 ..< 3, id: \.self) { row in
                HStack(spacing: 0) {
                    ForEach(0 ..< 10, id: \.self) { col in
                        let slot = row * 10 + col
                        let rgb = palette.colors.indices.contains(slot) ? palette.colors[slot] : nil
                        if let rgb, droppable, c is BrushColor {
                            DropSwatch(model: model, rgb: rgb, onHold: { model.setSwatch(index, slot, nil) }) { c.setRgb(rgb) }
                        } else {
                            PressCell(onHold: rgb == nil ? nil : { model.setSwatch(index, slot, nil) }) {
                                if let rgb {
                                    c.setRgb(rgb)
                                } else {
                                    model.setSwatch(index, slot, c.rgb())
                                }
                            } content: {
                                Rectangle().fill(rgb.map(canvasColor) ?? Pc.emptySlot)
                            }
                        }
                    }
                }
                .frame(height: 36)
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .circular))
    }
}

/// A palette's colours as named tiles, three to a row, 120 square: a tap takes the colour, a hold
/// takes it out of the palette.
private struct PaletteTiles: View {
    let model: CanvasModel
    let c: any ColorPick
    let index: Int
    let palette: PaletteInfo
    var across = 3

    var body: some View {
        let filled: [(slot: Int, rgb: UInt32)] = palette.colors.enumerated().compactMap { slot, rgb in
            rgb.map { (slot: slot, rgb: $0) }
        }
        LazyVGrid(columns: Array(repeating: GridItem(.fixed(120), spacing: 0), count: across), spacing: 0) {
            ForEach(filled, id: \.slot) { tile in
                let bright = max((tile.rgb >> 16) & 0xFF, (tile.rgb >> 8) & 0xFF, tile.rgb & 0xFF) >= 217
                PressCell(onHold: { model.setSwatch(index, tile.slot, nil) }) {
                    c.setRgb(tile.rgb)
                } content: {
                    ZStack(alignment: .bottomLeading) {
                        canvasColor(tile.rgb)
                        // dark lettering on bright colours
                        Text(verbatim: colourName(rgb: tile.rgb & 0xFFFFFF))
                            .font(.system(size: 15))
                            .foregroundStyle(bright ? Color.black : Color.white)
                            .padding(8)
                    }
                    .frame(width: 120, height: 120)
                }
            }
        }
    }
}

/// All palettes as compact cards: the default one ticked, each with its menu and its swatches.
private struct PalettesList: View {
    let model: CanvasModel
    let c: any ColorPick
    let ui: ColorUi
    let short: Bool

    private func option(_ on: Bool, _ icon: PcIcon, _ label: String, cards: Bool) -> some View {
        HStack(spacing: 6) {
            // drawn in a 21 x 32 frame, shown as it is
            PcIconView(icon, color: on ? .white : Pc.panelTitle)
            Text(verbatim: label)
                .font(.system(size: 15, weight: on ? .semibold : .regular))
                .foregroundStyle(on ? Color.white : Pc.panelTitle)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(RoundedRectangle(cornerRadius: 7, style: .circular).fill(on ? Pc.accent : Color.clear))
        .contentShape(Rectangle())
        .onTapGesture { AppSettings.shared.paletteCards = cards }
    }

    private func card(_ i: Int, _ p: PaletteInfo, cards: Bool) -> some View {
        let isDefault = i == model.defaultPalette
        return VStack(spacing: 0) {
            HStack(spacing: 0) {
                if isDefault {
                    PcIconView(PcIcons.tick, size: 14, color: .white)
                        .frame(width: 20, height: 20)
                        .background(Circle().fill(Pc.accent))
                        .padding(.leading, 11.5)
                }
                // a tap on the name renames it in place
                Group {
                    if ui.renaming == i {
                        InlineName(name: p.name, color: Pc.panelTitle, fontSize: 14) {
                            model.renamePalette(i, $0)
                            ui.renaming = nil
                        }
                    } else {
                        Text(verbatim: p.name)
                            .font(.system(size: 14, weight: .semibold))
                            .foregroundStyle(Pc.panelTitle)
                            .lineLimit(1)
                            .contentShape(Rectangle())
                            .onTapGesture { ui.renaming = i }
                    }
                }
                .padding(.leading, isDefault ? 4 : 12)
                Spacer(minLength: 0)
                PcIconView(PcIcons.more, size: 20, color: Pc.panelTitle)
                    .frame(width: 30, height: 44)
                    .overlay {
                        GeometryReader { g in
                            Color.clear
                                .contentShape(Rectangle())
                                .onTapGesture {
                                    ui.paletteButton = g.frame(in: .named(colorsSpace))
                                    withAnimation(Motion.menuOpen) { ui.paletteMenu = i }
                                }
                        }
                    }
                    .padding(.trailing, 8.5)
            }
            .frame(height: 44)
            if cards {
                PaletteTiles(model: model, c: c, index: i, palette: p)
            } else {
                SwatchGrid(model: model, c: c, index: i, palette: p, droppable: false)
            }
        }
        .background(Pc.setCell)
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .circular))
    }

    var body: some View {
        let cards = AppSettings.shared.paletteCards
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                option(!cards, PcIcons.cardCompact, "Compact", cards: false)
                option(cards, PcIcons.cardStack, "Cards", cards: true)
            }
            .padding(2)
            .frame(height: 36)
            .background(RoundedRectangle(cornerRadius: 9, style: .circular).fill(Pc.setCell))
            .padding(.horizontal, 8.5)
            ScrollView {
                VStack(spacing: 8) {
                    ForEach(Array(model.palettes.enumerated()), id: \.offset) { i, p in
                        card(i, p, cards: cards)
                    }
                }
                .padding(.horizontal, cards ? 8 : 8.5)
                .padding(.bottom, 8)
            }
            .scrollIndicators(.hidden)
            .frame(height: short ? 458 : 577)
            .padding(.top, 8)
        }
    }
}

/// A round choice mark, 18 across: a grey ring, or filled blue with a white tick when chosen.
private struct Radio: View {
    let chosen: Bool

    var body: some View {
        ZStack {
            if chosen {
                Circle().fill(Pc.accent)
                PcIconView(PcIcons.tick, size: 12, color: .white)
            } else {
                Circle().strokeBorder(Pc.radioRing, lineWidth: 1.5)
            }
        }
        .frame(width: 18, height: 18)
    }
}

/// What the panel opens over itself.
private struct ColorMenus: View {
    let model: CanvasModel
    let ui: ColorUi

    private func close() {
        ui.closeAll()
    }

    var body: some View {
        ZStack(alignment: .topLeading) {
            if ui.harmonyMenu {
                MenuDismissal { close() }
                harmonyMenu
            }
            if ui.addMenu {
                MenuDismissal { close() }
                addMenu
            }
            if ui.modelMenu {
                MenuDismissal { close() }
                modelMenu
            }
            if let i = ui.paletteMenu {
                MenuDismissal { close() }
                paletteMenu(i)
            }
            if ui.shortMenu {
                MenuDismissal { close() }
                shortMenu
            }
            HexOverlay(ui: ui)
        }
    }

    /// The kinds, to the left of the panel, its top level with the wheel.
    private var harmonyMenu: some View {
        VStack(spacing: 0) {
            ForEach(HarmonyKind.allCases, id: \.self) { kind in
                HStack(spacing: 0) {
                    // the longest name runs a point into the row's margin, whole
                    Text(verbatim: kind.label)
                        .font(.system(size: 17))
                        .foregroundStyle(Pc.menuInk)
                        .lineLimit(1)
                        .fixedSize()
                    Spacer(minLength: 0)
                    if kind == model.harmonyKind {
                        PcIconView(PcIcons.tick, size: 18, color: Pc.menuInk)
                    }
                }
                .padding(.leading, 20)
                .padding(.trailing, 18)
                .frame(height: 38)
                .contentShape(Rectangle())
                .onTapGesture {
                    model.harmonyKind = kind
                    close()
                }
            }
        }
        .padding(.vertical, 9)
        .frame(width: 195)
        .background(RoundedRectangle(cornerRadius: 24, style: .circular).fill(Pc.menuFill))
        .offset(x: ui.harmonyAt.x, y: ui.harmonyAt.y)
        .menuTransition(.topTrailing)
    }

    /// The colour models under their button, its right edge level with the button's, 6 below it; the
    /// one in use ticked.
    private var modelMenu: some View {
        let settings = AppSettings.shared
        return VStack(spacing: 0) {
            ForEach(colourModels(), id: \.self) { m in
                HStack(spacing: 0) {
                    Text(verbatim: colourModelLabel(model: m))
                        .font(.system(size: 16))
                        .foregroundStyle(Pc.menuInk)
                        .lineLimit(1)
                    Spacer(minLength: 0)
                    if m == settings.colourModel {
                        PcIconView(PcIcons.tick, size: 18, color: Pc.menuInk)
                    }
                }
                .padding(.leading, 18)
                .padding(.trailing, 16)
                .frame(height: 38)
                .contentShape(Rectangle())
                .onTapGesture {
                    settings.colourModel = m
                    close()
                }
            }
        }
        .padding(.vertical, 9)
        .frame(width: modelMenuWidth)
        .background(RoundedRectangle(cornerRadius: 18, style: .circular).fill(Pc.menuFill))
        .offset(x: ui.modelButton.maxX - modelMenuWidth, y: ui.modelButton.maxY + 6)
        // it grows from under the button's middle
        .menuTransition(UnitPoint(x: 1 - modelButtonSize.width / 2 / modelMenuWidth, y: 0))
    }

    /// The palettes' "+": a new palette, empty or from a photo taken, a file or a picture, and our
    /// example palettes back.
    private var addMenu: some View {
        GlassMenu(overPanel: true) {
            MenuRow(icon: PcIcons.menuGrid, label: "Create new palette") {
                close()
                model.addPalette()
            }
            MenuRow(icon: PcIcons.menuCamera, label: "New from camera") {
                close()
                if UIImagePickerController.isSourceTypeAvailable(.camera) {
                    ui.takingPhoto = true
                } else {
                    model.showNotice("No camera is available.", warning: true)
                }
            }
            MenuRow(icon: PcIcons.menuFolder, label: "New from file") {
                close()
                ui.choosingFile = true
            }
            MenuRow(icon: PcIcons.menuPhoto, label: "New from photos") {
                close()
                ui.choosingPhoto = true
            }
            MenuLine()
            MenuRow(icon: PcIcons.menuRestore, label: "Restore example palettes") {
                close()
                model.restoreExamplePalettes()
            }
        }
        .offset(x: panelWidth - 17 - 40 - glassMenuWidth, y: 53 + Pc.arrowHeight)
        .menuTransition(.topTrailing)
    }

    /// A palette's "..." menu: Set as active (unless it is), Duplicate, Share; Delete. Its right edge
    /// 5 left of the button, its top 25 below the button's.
    private func paletteMenu(_ i: Int) -> some View {
        GlassMenu(overPanel: true) {
            if model.defaultPalette != i {
                MenuRow(icon: PcIcons.menuCheckCircle, label: "Set as active") {
                    close()
                    model.chooseDefaultPalette(i)
                }
            }
            MenuRow(icon: PcIcons.menuDuplicate, label: "Duplicate") {
                close()
                model.duplicatePalette(i)
            }
            MenuRow(icon: PcIcons.menuShare, label: "Share") {
                close()
                model.sharePalette(i)
            }
            if model.palettes.count > 1 {
                MenuLine()
                MenuRow(icon: PcIcons.menuTrash, label: "Delete", ink: Pc.menuDanger) {
                    close()
                    model.deletePalette(i)
                }
            }
        }
        .offset(x: ui.paletteButton.minX - 5 - glassMenuWidth, y: ui.paletteButton.minY + 25)
        .menuTransition(.topTrailing)
    }

    /// The short panel's menu: Color history or Active palette (a radio each), and with the history
    /// shown, Clear history. Its right edge 16 left of the dots, its top 15 below them.
    private var shortMenu: some View {
        let settings = AppSettings.shared
        let history = settings.colorHistoryShown
        let shape = RoundedRectangle(cornerRadius: 17, style: .circular)
        return VStack(spacing: 0) {
            ForEach([true, false], id: \.self) { on in
                HStack(spacing: 0) {
                    Text(verbatim: on ? "Color history" : "Active palette")
                        .font(.system(size: 17))
                        .foregroundStyle(Pc.menuInk)
                        .lineLimit(1)
                    Spacer(minLength: 0)
                    Radio(chosen: on == history)
                }
                .padding(.leading, 20)
                .padding(.trailing, 19)
                .frame(height: 40)
                .contentShape(Rectangle())
                .onTapGesture {
                    settings.colorHistoryShown = on
                    close()
                }
            }
            if history {
                MenuLine(gap: 10.5)
                MenuRow(icon: PcIcons.sweep, label: "Clear history", ink: Pc.menuDanger, iconSize: 41) {
                    model.clearHistory()
                    close()
                }
            }
        }
        .padding(.top, 6)
        .padding(.bottom, history ? 9.5 : 7)
        .frame(width: 252)
        .background(shape.fill(Pc.glassMenuOverPanel))
        .overlay(shape.strokeBorder(Pc.glassMenuEdge, lineWidth: 1))
        .offset(x: ui.shortButton.minX - 246, y: ui.shortButton.minY + 32)
        .menuTransition(.topTrailing)
    }
}

/// The colour model's button left of the swatches: the model's name in a small box with a chevron;
/// a tap lists the models (HSB, HLS, HSY′, OKLab) under it.
private struct ColourModelButton: View {
    let ui: ColorUi

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: 6, style: .circular)
        ZStack(alignment: .leading) {
            shape.fill(modelButtonFill)
            Text(verbatim: colourModelLabel(model: AppSettings.shared.colourModel))
                .font(.system(size: 13))
                .foregroundStyle(Pc.panelTitle)
                .lineLimit(1)
                .padding(.leading, 8)
            // a small downward chevron, 9 in from the right
            Path { p in
                let c = CGPoint(x: modelButtonSize.width - 9, y: modelButtonSize.height / 2)
                p.move(to: CGPoint(x: c.x - 3.5, y: c.y - 1.8))
                p.addLine(to: CGPoint(x: c.x, y: c.y + 1.8))
                p.addLine(to: CGPoint(x: c.x + 3.5, y: c.y - 1.8))
            }
            .stroke(Pc.panelTitle, lineWidth: 1.4)
        }
        .frame(width: modelButtonSize.width, height: modelButtonSize.height)
        .overlay {
            GeometryReader { g in
                Color.clear
                    .contentShape(Rectangle())
                    .onTapGesture {
                        ui.modelButton = g.frame(in: .named(colorsSpace))
                        withAnimation(Motion.menuOpen) { ui.modelMenu = true }
                    }
            }
        }
    }
}

/// The disc picker, 364 square: a hue ring (radii 124 to 182, the hue scale running anticlockwise
/// from the right) around the saturation/brightness disc (radius 105).
private struct DiscPicker: View {
    let c: any ColorPick
    let short: Bool
    var compact = false
    /// The colour in the colour model chosen; none or HSB, the disc is HSB's own.
    var m: ModelColor?
    @Environment(\.displayScale) private var displayScale
    /// The disc's picture for the hue it was made for.
    @State private var disc: (hue: Float, image: UIImage)?
    /// Whether the touch now held began on the ring.
    @State private var onRing: Bool?

    private static let ring: AngularGradient = {
        let hues = hueStops(control: .ring)
        let last = Float(max(hues.count - 1, 1))
        // anticlockwise from the right: a hue at t round the ring sits at 1 - t of the clockwise sweep
        let stops = hues.enumerated().map { i, h in
            Gradient.Stop(color: CanvasSpace.shared.color(CanvasModel.rgb(hue: h / 360, saturation: 1, brightness: 1)), location: CGFloat(1 - Float(i) / last))
        }
        return AngularGradient(stops: stops.reversed(), center: .center, startAngle: .zero, endAngle: .degrees(360))
    }()

    var body: some View {
        if let m, m.model != .hsb {
            ModelDiscPicker(m: m, short: short, compact: compact)
        } else {
            hsbDisc
        }
    }

    @ViewBuilder private var hsbDisc: some View {
        let hue = c.hue
        // compact, the whole picker is 240 across instead of 364
        let k: CGFloat = compact ? 240 / 364 : 1
        let across = 364 * k
        let radius = discRadius * k
        let inner = ringInner * k
        let outer = ringOuter * k
        let side = Int((radius * 2 * displayScale).rounded())
        let left: CGFloat = compact ? 10 : 6
        let top: CGFloat = compact ? 4 : short ? 0 : 8
        let bottom: CGFloat = compact ? 6 : short ? 3 : 17
        Canvas { context, size in
            let o = CGPoint(x: size.width / 2, y: size.height / 2)
            let mid = (inner + outer) / 2
            context.stroke(
                Path(ellipseIn: CGRect(x: o.x - mid, y: o.y - mid, width: mid * 2, height: mid * 2)),
                with: .style(Self.ring),
                lineWidth: outer - inner
            )
            if let picture = disc?.image {
                context.draw(Image(uiImage: picture), in: CGRect(x: o.x - radius, y: o.y - radius, width: radius * 2, height: radius * 2))
            }
            // the ring's knob sits on the middle of the ring, where the hue is
            let turn = CGFloat(huePosition(control: .ring, hue: hue)) * 2 * .pi
            drawKnob(&context, at: CGPoint(x: o.x + cos(turn) * mid, y: o.y - sin(turn) * mid), radius: 26.2 * k,
                     fill: CanvasSpace.shared.color(CanvasModel.rgb(hue: hue, saturation: 1, brightness: 1)))
            let p = squareToDiscPoint(x: c.saturation * 2 - 1, y: 1 - c.brightness * 2)
            if p.count >= 2 {
                drawKnob(&context, at: CGPoint(x: o.x + CGFloat(p[0]) * radius, y: o.y + CGFloat(p[1]) * radius), radius: 10.75,
                         fill: CanvasSpace.shared.color(c.rgb()))
            }
        }
        .frame(width: across, height: across)
        .padding(.leading, left)
        .padding(.top, top)
        .padding(.bottom, bottom)
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .gesture(
            DragGesture(minimumDistance: 0)
                .onChanged { drag in
                    // in the picker's own square (past its padding)
                    let centre = CGPoint(x: left + across / 2, y: top + across / 2)
                    let d = CGPoint(x: drag.location.x - centre.x, y: drag.location.y - centre.y)
                    if onRing == nil {
                        let start = CGPoint(x: drag.startLocation.x - centre.x, y: drag.startLocation.y - centre.y)
                        onRing = hypot(start.x, start.y) > (radius + inner) / 2
                    }
                    if onRing == true {
                        var turn = Float(atan2(-d.y, d.x) / (2 * .pi))
                        turn = (turn + 1).truncatingRemainder(dividingBy: 1)
                        c.setHsv(hueAt(control: .ring, position: turn), c.saturation, c.brightness)
                    } else {
                        var u = d.x / radius
                        var v = d.y / radius
                        let length = hypot(u, v)
                        if length > 1 {
                            u /= length
                            v /= length
                        }
                        let sq = discToSquarePoint(u: Float(u), v: Float(v))
                        if sq.count >= 2 {
                            c.setHsv(c.hue, (sq[0] + 1) / 2, (1 - sq[1]) / 2)
                        }
                    }
                }
                .onEnded { _ in onRing = nil }
        )
        .task(id: hue) {
            // the saturation/brightness disc for the hue, with a soft edge, as the core draws it
            if disc?.hue == hue {
                return
            }
            let made = await Task.detached(priority: .userInitiated) { () -> UIImage? in
                let rgba = discPicture(hue: hue, side: UInt32(side))
                return CanvasModel.image(ThumbnailImage(width: UInt32(side), height: UInt32(side), rgba: rgba))
            }.value
            if let made {
                disc = (hue, made)
            }
        }
    }
}

/// A colour knob drawn into `context`: `fill` inside a thin white ring, over a soft shadow.
private func drawKnob(_ context: inout GraphicsContext, at c: CGPoint, radius r: CGFloat, fill: Color) {
    var shadowed = context
    shadowed.addFilter(.blur(radius: 2))
    shadowed.fill(Path(ellipseIn: CGRect(x: c.x - r, y: c.y - r + 2, width: r * 2, height: r * 2)), with: .color(Color(argb: 0x5900_0000)))
    context.fill(Path(ellipseIn: CGRect(x: c.x - r, y: c.y - r, width: r * 2, height: r * 2)), with: .color(fill))
    let ring = r - 0.6
    context.stroke(Path(ellipseIn: CGRect(x: c.x - ring, y: c.y - ring, width: ring * 2, height: ring * 2)), with: .color(.white), lineWidth: 1.2)
}

/// A picker's picture in a colour model, and what it was made for.
private struct ModelPictureKey: Equatable {
    let model: ColourModel
    let key: UInt32
}

/// The disc picker in another colour model, as the core works it out: the ring's hues as the model
/// shows them at their purest (evenly spaced in OKLab, else as HSB's ring), round its disc
/// (saturation across and the third up, or HSY′'s wheel at the colour's linear luma). In the panel,
/// HSY′'s saturation and luma follow on sliders under it.
private struct ModelDiscPicker: View {
    let m: ModelColor
    let short: Bool
    var compact = false
    @Environment(\.displayScale) private var displayScale
    /// The disc's picture, and what it was made for.
    @State private var disc: (key: ModelPictureKey, image: UIImage)?
    /// Whether the touch now held began on the ring.
    @State private var onRing: Bool?

    /// Each model's ring, made once.
    private static var rings: [ColourModel: AngularGradient] = [:]

    private static func ring(_ model: ColourModel) -> AngularGradient {
        if let made = rings[model] {
            return made
        }
        let colours = colourModelRing(model: model, n: UInt32(max(hueStops(control: .ring).count, 2)))
        let last = CGFloat(max(colours.count - 1, 1))
        // anticlockwise from the right: a colour at t round the ring sits at 1 - t of the clockwise sweep
        let stops = colours.enumerated().map { i, rgb in
            Gradient.Stop(color: CanvasSpace.shared.color(rgb), location: 1 - CGFloat(i) / last)
        }
        let made = AngularGradient(stops: stops.reversed(), center: .center, startAngle: .zero, endAngle: .degrees(360))
        rings[model] = made
        return made
    }

    var body: some View {
        let model = m.model
        let coords = m.coords()
        let h = coords[0]
        let s = coords[1]
        let t = coords[2]
        // compact, the whole picker is 240 across instead of 364
        let k: CGFloat = compact ? 240 / 364 : 1
        let across = 364 * k
        let radius = discRadius * k
        let inner = ringInner * k
        let outer = ringOuter * k
        let side = Int((radius * 2 * displayScale).rounded())
        let key = ModelPictureKey(model: model, key: colourModelDiscKey(model: model, h: h, s: s, t: t))
        let turn = CGFloat(colourModelRingTurn(model: model, hue: h)) * 2 * .pi
        let pure = colourModelPure(model: model, hue: h)
        let at = colourModelDiscPoint(model: model, h: h, s: s, t: t)
        let current = m.c.rgb()
        let luma = model == .hsy && !compact
        let left: CGFloat = compact ? 10 : 6
        let top: CGFloat = compact ? 4 : short ? 0 : 8
        // HSY′'s sliders start 4 under the picker
        let bottom: CGFloat = compact ? 6 : luma ? 4 : short ? 3 : 17
        let ring = Self.ring(model)
        VStack(spacing: 0) {
            Canvas { context, size in
                let o = CGPoint(x: size.width / 2, y: size.height / 2)
                let mid = (inner + outer) / 2
                context.stroke(
                    Path(ellipseIn: CGRect(x: o.x - mid, y: o.y - mid, width: mid * 2, height: mid * 2)),
                    with: .style(ring),
                    lineWidth: outer - inner
                )
                if let picture = disc?.image {
                    context.draw(Image(uiImage: picture), in: CGRect(x: o.x - radius, y: o.y - radius, width: radius * 2, height: radius * 2))
                }
                // the ring's knob sits on the middle of the ring, where the hue is
                drawKnob(&context, at: CGPoint(x: o.x + cos(turn) * mid, y: o.y - sin(turn) * mid), radius: 26.2 * k,
                         fill: CanvasSpace.shared.color(pure))
                if at.count >= 2 {
                    drawKnob(&context, at: CGPoint(x: o.x + CGFloat(at[0]) * radius, y: o.y + CGFloat(at[1]) * radius), radius: 10.75,
                             fill: CanvasSpace.shared.color(current))
                }
            }
            .frame(width: across, height: across)
            .padding(.leading, left)
            .padding(.top, top)
            .padding(.bottom, bottom)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { drag in
                        // in the picker's own square (past its padding)
                        let centre = CGPoint(x: left + across / 2, y: top + across / 2)
                        let d = CGPoint(x: drag.location.x - centre.x, y: drag.location.y - centre.y)
                        if onRing == nil {
                            let start = CGPoint(x: drag.startLocation.x - centre.x, y: drag.startLocation.y - centre.y)
                            onRing = hypot(start.x, start.y) > (radius + inner) / 2
                        }
                        let was = m.coords()
                        if onRing == true {
                            var turnAt = Float(atan2(-d.y, d.x) / (2 * .pi))
                            turnAt = (turnAt + 1).truncatingRemainder(dividingBy: 1)
                            m.set([colourModelRingHue(model: model, turn: turnAt), was[1], was[2]])
                        } else {
                            m.set(colourModelDiscPick(
                                model: model, h: was[0], s: was[1], t: was[2], u: Float(d.x / radius), v: Float(d.y / radius)
                            ))
                        }
                    }
                    .onEnded { _ in onRing = nil }
            )
            if luma {
                LumaSliders(m: m, coords: coords)
                Color.clear.frame(height: short ? 3 : 17)
            }
        }
        .task(id: key) {
            // the disc for where the colour stands, as the core draws it
            if disc?.key == key {
                return
            }
            let made = await Task.detached(priority: .userInitiated) { () -> UIImage? in
                let rgba = colourModelDiscPicture(model: model, h: h, s: s, t: t, side: UInt32(side))
                return CanvasModel.image(ThumbnailImage(width: UInt32(side), height: UInt32(side), rgba: rgba))
            }.value
            if let made, !Task.isCancelled {
                disc = (key, made)
            }
        }
    }
}

/// HSY′'s saturation and luma on sliders under its wheel, each row 36 tall: its letter 13 in, the
/// track from 40 to 56 short of the right, the percentage 10 short of it.
private struct LumaSliders: View {
    let m: ModelColor
    let coords: [Float]

    var body: some View {
        let letters = colourModelLetters(model: m.model)
        VStack(spacing: 0) {
            ForEach([1, 2], id: \.self) { channel in
                let v = coords[channel]
                let colours = colourModelAlong(model: m.model, h: coords[0], s: coords[1], t: coords[2], channel: UInt32(channel), n: 12)
                ZStack(alignment: .leading) {
                    Text(verbatim: letters.indices.contains(channel) ? letters[channel] : "")
                        .font(.system(size: 15))
                        .foregroundStyle(Pc.panelTitle)
                        .padding(.leading, 13)
                    SliderRow(value: v, colours: colours) { to in
                        var next = coords
                        next[channel] = to
                        m.set(next)
                    }
                    .padding(.leading, 40)
                    .padding(.trailing, 56)
                    Text(verbatim: percentShown(v))
                        .font(.system(size: 15))
                        .foregroundStyle(Pc.panelTitle)
                        .lineLimit(1)
                        .padding(.trailing, 10)
                        .frame(maxWidth: .infinity, alignment: .trailing)
                }
                .frame(height: 36)
            }
        }
        .padding(.horizontal, 8)
    }
}

/// A hue as Value shows it, in whole degrees.
private func degreesShown(_ hue: Float) -> String {
    "\(wholeNumber(hue * 360))°"
}

/// A share as Value shows it, in whole percent.
private func percentShown(_ v: Float) -> String {
    "\(wholeNumber(v * 100))%"
}

/// OKLab's a or b as Value shows it: signed, to two places, with a point whatever the language.
private func signedShown(_ v: Float) -> String {
    String(format: "%+.2f", locale: Locale(identifier: "en_US_POSIX"), Double(v))
}

/// A swatch that can be dragged out onto the canvas to fill there, shrinking while pressed.
private struct DropSwatch: View {
    let model: CanvasModel
    let rgb: UInt32
    let onHold: () -> Void
    let onTap: () -> Void

    @State private var pressed = false

    var body: some View {
        Rectangle()
            .fill(canvasColor(rgb))
            .scaleEffect(pressed ? Motion.swatchPressed : 1)
            .overlay {
                DropSource(model: model, swatch: rgb, onHold: onHold, onPress: { down in
                    withAnimation(down ? Motion.swatchPress : Motion.swatchRelease) { pressed = down }
                }, onTap: onTap)
            }
    }
}

/// A swatch or tile: tapped `onTap`, held `onHold` (and then not tapped); it shrinks while pressed
/// and springs back when let go.
private struct PressCell<Content: View>: View {
    var onHold: (() -> Void)?
    let onTap: () -> Void
    @ViewBuilder let content: () -> Content

    @State private var pressed = false

    var body: some View {
        content()
            .scaleEffect(pressed ? Motion.swatchPressed : 1)
            .contentShape(Rectangle())
            .onTapGesture(perform: onTap)
            .onLongPressGesture(minimumDuration: 0.5, maximumDistance: 10) {
                onHold?()
            } onPressingChanged: { down in
                withAnimation(down ? Motion.swatchPress : Motion.swatchRelease) { pressed = down }
            }
    }
}

/// A mode tab: its icon over its name, the chosen one lit as it is let go; held, its icon shrinks.
private struct ModeTabStyle: ButtonStyle {
    let mode: ColorMode
    let selected: Bool

    func makeBody(configuration: Configuration) -> some View {
        ZStack(alignment: .top) {
            // a 36 frame centred 27 down
            PcIconView(mode.icon, size: 36, color: selected ? Pc.accent : Pc.panelTitle)
                .scaleEffect(configuration.isPressed ? Motion.tabPressed : 1)
                .animation(configuration.isPressed ? Motion.highlightOn : Motion.highlightOff, value: configuration.isPressed)
                .padding(.top, 9)
            Text(verbatim: mode.label)
                .font(.system(size: 13, weight: .medium))
                // the chosen one white, or black with the light interface
                .foregroundStyle(!selected ? Pc.panelTitle : (Look.shared.light ? Color.black : Color.white))
                .lineLimit(1)
                .padding(.top, 41)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .contentShape(Rectangle())
    }
}

/// The mode tabs along the panel's foot: each its icon over its name, the chosen one lit.
private struct ModeTabs: View {
    let model: CanvasModel

    var body: some View {
        HStack(spacing: 0) {
            ForEach(ColorMode.allCases, id: \.self) { mode in
                Button { model.colorMode = mode } label: { EmptyView() }
                    .buttonStyle(ModeTabStyle(mode: mode, selected: model.colorMode == mode))
            }
        }
        .frame(height: 70)
    }
}

/// The colour panel's width.
let colorPanelWidth = panelWidth
/// How wide the compact colour picker is, for placing it.
let compactColorsWidth = compactWidth

/// The compact colour picker other screens open (a gradient stop's colour, Custom Canvas's
/// background): "Colors", a picker per mode and the modes as icons, 260 wide; no swatches, history
/// or palette. It needs no canvas: the gallery opens it too.
struct CompactColors: View {
    let c: any ColorPick
    let arrowX: CGFloat
    /// The window, to keep the hex keypad on it.
    let screen: CGSize
    var side = ArrowSide.top
    var harmonyKind = HarmonyKind.complementary

    @State private var mode = ColorMode.disc
    @State private var ui = ColorUi()
    @State private var palettes: [PaletteInfo] = []

    @ViewBuilder private var picker: some View {
        switch mode {
        case .disc:
            DiscPicker(c: c, short: false, compact: true)
        case .classic:
            ClassicPicker(c: c, square: 248, compact: true)
        case .harmony:
            HarmonyPicker(kind: harmonyKind, c: c, short: false, compact: true)
        case .value:
            CompactValue(c: c, ui: ui)
        case .palettes:
            CompactPalettes(c: c, palettes: palettes)
        }
    }

    var body: some View {
        Popover(width: compactWidth, arrowX: arrowX, fill: compactFill, side: side) {
            // a 16.5 cap, as measured over a gradient stop
            Text(verbatim: "Colors")
                .font(.system(size: 23, weight: .bold))
                .foregroundStyle(Pc.panelTitle)
                .padding(.leading, 18)
                .padding(.top, 9)
                .padding(.bottom, 5)
                .frame(maxWidth: .infinity, alignment: .leading)
            picker
            CompactTabs(mode: mode) { mode = $0 }
        }
        .coordinateSpace(.named(colorsSpace))
        .background {
            GeometryReader { place in
                Color.clear.onChange(of: place.frame(in: .global), initial: true) { _, frame in
                    let at = Interface.shared.rect(frame)
                    ui.hexRoom = CGRect(x: -at.minX, y: -at.minY, width: screen.width, height: screen.height)
                }
            }
        }
        .overlay(alignment: .topLeading) {
            ZStack(alignment: .topLeading) {
                HexOverlay(ui: ui)
            }
        }
        .onAppear {
            palettes = CanvasModel.paletteStore.list()
            #if DEBUG
            mode = ScreenCapture.compactMode ?? mode
            #endif
        }
    }
}

/// The compact picker's body and its mode tabs, 260 wide, for a page of its own.
struct CompactPicker: View {
    let c: any ColorPick
    /// How wide it shows: its body laid out at 260 and scaled to this, the tabs this wide.
    var shown: CGFloat = 260
    /// The body's height as it shows.
    var bodyHeight: CGFloat = 340

    @State private var mode = ColorMode.disc
    @State private var ui = ColorUi()
    @State private var palettes: [PaletteInfo] = []

    var body: some View {
        VStack(spacing: 0) {
            Group {
                switch mode {
                case .disc: DiscPicker(c: c, short: false, compact: true)
                case .classic: ClassicPicker(c: c, square: 248, compact: true)
                case .harmony: HarmonyPicker(kind: .complementary, c: c, short: false, compact: true)
                case .value: CompactValue(c: c, ui: ui)
                case .palettes: CompactPalettes(c: c, palettes: palettes)
                }
            }
            .frame(width: compactWidth, height: bodyHeight * compactWidth / shown, alignment: .top)
            .scaleEffect(shown / compactWidth, anchor: .topLeading)
            .frame(width: shown, height: bodyHeight, alignment: .topLeading)
            CompactTabs(mode: mode) { mode = $0 }
                .frame(width: shown)
        }
        .coordinateSpace(.named(colorsSpace))
        .overlay(alignment: .topLeading) {
            ZStack(alignment: .topLeading) {
                HexOverlay(ui: ui)
            }
        }
        .onAppear { palettes = CanvasModel.paletteStore.list() }
    }
}

/// The compact pickers' modes as a row of icons, the chosen one blue.
private struct CompactTabs: View {
    let mode: ColorMode
    let onMode: (ColorMode) -> Void

    var body: some View {
        HStack(spacing: 0) {
            ForEach(ColorMode.allCases, id: \.self) { m in
                // the tabs' 36-unit glyphs, a little smaller here
                PcIconView(m.icon, size: 33, color: m == mode ? Pc.accent : Pc.panelTitle)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .contentShape(Rectangle())
                    .onTapGesture { onMode(m) }
            }
        }
        .frame(height: 52)
    }
}

/// The panel's grabber: dragged out past `undockDrag`, the panel floats and the window follows.
private struct UndockHandle: View {
    let model: CanvasModel

    var body: some View {
        Color.clear
            .frame(width: 150, height: 30)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 1, coordinateSpace: .global)
                    .onChanged { drag in
                        guard model.colorsUndocking || hypot(drag.translation.width, drag.translation.height) > undockDrag else { return }
                        let at = Interface.shared.point(drag.location)
                        model.colorsUndocking = true
                        model.undockColours(at: at, grab: floatingGrab)
                    }
                    .onEnded { _ in
                        if model.colorsUndocking {
                            model.colorsUndocking = false
                            model.letGoColours()
                            model.closePanels()
                        }
                    }
            )
    }
}

/// Close: a disc with a cross cut out of it.
private struct FloatingClose: View {
    let onClose: () -> Void

    var body: some View {
        Canvas { context, size in
            let o = CGPoint(x: size.width / 2, y: size.height / 2)
            let r: CGFloat = 8.5
            context.fill(Path(ellipseIn: CGRect(x: o.x - r, y: o.y - r, width: r * 2, height: r * 2)), with: .color(.white.opacity(0.15)))
            let d: CGFloat = 3.2
            var cross = Path()
            cross.move(to: CGPoint(x: o.x - d, y: o.y - d))
            cross.addLine(to: CGPoint(x: o.x + d, y: o.y + d))
            cross.move(to: CGPoint(x: o.x + d, y: o.y - d))
            cross.addLine(to: CGPoint(x: o.x - d, y: o.y + d))
            context.stroke(cross, with: .color(.black.opacity(0.3)), lineWidth: 1.3)
        }
        .frame(width: 40, height: 40)
        .contentShape(Rectangle())
        .onTapGesture(perform: onClose)
    }
}

/// Palettes floating: Compact or Cards, then each palette as a card of swatches six to a row, or as
/// named tiles two to a row.
private struct FloatingPalettes: View {
    let model: CanvasModel
    let c: any ColorPick
    let ui: ColorUi

    private func option(_ label: String, cards: Bool) -> some View {
        let on = AppSettings.shared.paletteCards == cards
        return Text(verbatim: label)
            .font(.system(size: 15, weight: on ? .semibold : .regular))
            .foregroundStyle(on ? Color.white : Pc.panelTitle)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(RoundedRectangle(cornerRadius: 9, style: .circular).fill(on ? Pc.accent : Color.clear))
            .contentShape(Rectangle())
            .onTapGesture { AppSettings.shared.paletteCards = cards }
    }

    private func swatches(_ palette: PaletteInfo) -> some View {
        VStack(spacing: 0) {
            ForEach(0 ..< (palette.colors.count + 5) / 6, id: \.self) { row in
                HStack(spacing: 0) {
                    ForEach(0 ..< 6, id: \.self) { col in
                        let slot = row * 6 + col
                        let rgb = palette.colors.indices.contains(slot) ? palette.colors[slot] : nil
                        Rectangle()
                            .fill(rgb.map(canvasColor) ?? Pc.emptySlot)
                            .contentShape(Rectangle())
                            .onTapGesture {
                                if let rgb {
                                    c.setRgb(rgb)
                                }
                            }
                    }
                }
                .frame(height: 40)
            }
        }
    }

    private func card(_ i: Int, _ palette: PaletteInfo) -> some View {
        let isDefault = i == model.defaultPalette
        let shape = RoundedRectangle(cornerRadius: 10, style: .circular)
        return VStack(spacing: 0) {
            HStack(spacing: 0) {
                if isDefault {
                    PcIconView(PcIcons.tick, size: 13, color: .white)
                        .frame(width: 21, height: 21)
                        .background(Circle().fill(Pc.accent))
                        .padding(.leading, 8.5)
                }
                Text(verbatim: palette.name)
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(Pc.panelTitle)
                    .lineLimit(1)
                    .padding(.leading, isDefault ? 6 : 12)
                Spacer(minLength: 0)
                PcIconView(PcIcons.more, size: 20, color: Pc.panelTitle)
                    .frame(width: 30, height: 44)
                    .overlay {
                        GeometryReader { g in
                            Color.clear
                                .contentShape(Rectangle())
                                .onTapGesture {
                                    ui.paletteButton = g.frame(in: .named(colorsSpace))
                                    withAnimation(Motion.menuOpen) { ui.paletteMenu = i }
                                }
                        }
                    }
                    .padding(.trailing, 8.5)
            }
            .frame(height: 44)
            if AppSettings.shared.paletteCards {
                // as measured: the tiles' block with its top corners rounded
                PaletteTiles(model: model, c: c, index: i, palette: palette, across: 2)
                    .clipShape(UnevenRoundedRectangle(topLeadingRadius: 10, topTrailingRadius: 10, style: .circular))
            } else {
                swatches(palette)
            }
        }
        .background(shape.fill(floatingGroup))
        .clipShape(shape)
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                option("Compact", cards: false)
                option("Cards", cards: true)
            }
            .frame(height: 36)
            .background(RoundedRectangle(cornerRadius: 9, style: .circular).fill(floatingGroup))
            .padding(.horizontal, 10)
            .padding(.top, 4)
            ScrollView {
                VStack(spacing: 10) {
                    ForEach(Array(model.palettes.enumerated()), id: \.offset) { i, palette in
                        card(i, palette)
                    }
                }
                .padding(.horizontal, 10)
            }
            .scrollIndicators(.hidden)
            .frame(height: 246)
            .padding(.top, 6)
        }
    }
}

/// The colour panel floating: a window of its own, 260 wide, moved by its grabber and closed by its
/// Close (the colour button then opens the panel again). The same colour and mode as the panel; no
/// swatches, history or palette under the pickers, the modes as icons under them.
struct FloatingColors: View {
    let model: CanvasModel
    let screen: CGSize

    @State private var ui = ColorUi()
    /// Where the window was as the drag on its grabber began.
    @State private var dragFrom: CGPoint?

    private var title: some View {
        Text(verbatim: model.colorMode == .palettes ? "Palettes" : "Colors")
            .font(.system(size: 24, weight: .bold))
            .foregroundStyle(Pc.panelTitle)
            .frame(height: 40)
            .padding(.leading, 18)
            .padding(.top, 4)
    }

    private var grabber: some View {
        ZStack(alignment: .top) {
            Capsule()
                .fill(Pc.grabber)
                .frame(width: 40, height: 4)
                .padding(.top, 5.5)
            Color.clear
                .frame(width: 150, height: 30)
                .contentShape(Rectangle())
                .gesture(
                    DragGesture(minimumDistance: 1, coordinateSpace: .global)
                        .onChanged { drag in
                            let from = dragFrom ?? model.floatingColorsAt ?? .zero
                            dragFrom = from
                            let k = Interface.shared.scale
                            model.moveColours(by: CGSize(width: drag.translation.width / k, height: drag.translation.height / k), from: from)
                        }
                        .onEnded { _ in
                            dragFrom = nil
                            model.letGoColours()
                        }
                )
        }
        .frame(maxWidth: .infinity)
    }

    /// The colour in the panel's colour model (Value stays HSB, as it is here).
    private func modelColor(_ c: any ColorPick) -> ModelColor {
        ModelColor(c: c, model: AppSettings.shared.colourModel, place: model.colourPlace)
    }

    @ViewBuilder private func picker(_ c: any ColorPick) -> some View {
        switch model.colorMode {
        case .disc:
            DiscPicker(c: c, short: false, compact: true, m: modelColor(c))
                .padding(.top, 5)
        case .classic:
            ClassicPicker(c: c, square: 248, compact: true, m: modelColor(c), session: model.session)
                .padding(.top, 6)
        case .harmony:
            ZStack(alignment: .topLeading) {
                HarmonyPicker(kind: model.harmonyKind, c: c, short: false, compact: true, across: 106)
                    .padding(.top, 21)
                // tapping the kind lists them
                Text(verbatim: model.harmonyKind.label)
                    .font(.system(size: 17))
                    .foregroundStyle(Pc.panelTitle)
                    .frame(height: 28)
                    .contentShape(Rectangle())
                    .onTapGesture { withAnimation(Motion.menuOpen) { ui.harmonyMenu = true } }
                    .padding(.leading, 18)
            }
        case .value:
            CompactValue(c: c, ui: ui, group: floatingGroup, fieldFill: floatingGroup)
                .padding(.top, 4)
        case .palettes:
            FloatingPalettes(model: model, c: c, ui: ui)
        }
    }

    var body: some View {
        let height: CGFloat = model.colorMode == .disc ? 351 : 390
        // while a plugin lays the panels out, where the layout has it (folded, its tab)
        let laid = model.layoutLook.on ? model.shownColours(width: compactWidth, height: height) : nil
        // hidden from its menu: unseen until its button or Show Hidden brings it back
        let away = model.layoutLook.on && model.hiddenPieces().contains(.colours)
        if away {
            EmptyView()
        } else if model.floatingColorsAt != nil, let tab = laid, tab.folded {
            FloatingTab(
                shown: tab,
                title: model.colorMode == .palettes ? "Palettes" : "Colors",
                onUnfold: { model.foldColours(false) },
                onDrag: { _, by in model.moveColours(by: by, from: .zero) },
                onLetGo: { model.letGoColours() }
            )
            .frame(width: screen.width, height: screen.height, alignment: .topLeading)
        } else if let kept = model.floatingColorsAt {
            let at = laid.map { CGPoint(x: CGFloat($0.x), y: CGFloat($0.y)) } ?? kept
            // kept on the screen
            // at the floating windows' size, as the other floating windows are
            let k = Look.shared.scale(.floating)
            let x = min(max(at.x, 0), max(screen.width - compactWidth * k, 0))
            let y = min(max(at.y, 0), max(screen.height - height * k, 0))
            Popover(width: compactWidth, arrowX: 0, side: .none) {
                ZStack(alignment: .topLeading) {
                    VStack(spacing: 0) {
                        Color.clear.frame(height: 44)
                        picker(BrushColor(model: model))
                    }
                    title
                    grabber
                    FloatingClose { model.closeColours() }
                        .padding(.top, 4)
                        .padding(.trailing, 4)
                        .frame(maxWidth: .infinity, alignment: .trailing)
                    if model.layoutLook.on, model.layoutLook.fold {
                        FloatingFold { model.foldColours(true) }
                            .padding(.top, 4)
                            .padding(.trailing, 44)
                            .frame(maxWidth: .infinity, alignment: .trailing)
                    }
                    if model.layoutLook.on {
                        FloatingMenuButton(menu: windowMenu(model, .colours))
                            .padding(.top, 4)
                            .padding(.trailing, 84)
                            .frame(maxWidth: .infinity, alignment: .trailing)
                    }
                }
                .frame(height: height - 52, alignment: .top)
                CompactTabs(mode: model.colorMode) { model.colorMode = $0 }
            }
            .frame(height: height, alignment: .top)
            .coordinateSpace(.named(colorsSpace))
            // the canvas under it takes no touch
            .contentShape(Rectangle())
            .onTapGesture {}
            .background {
                GeometryReader { place in
                    Color.clear.onChange(of: place.frame(in: .global), initial: true) { _, frame in
                        let on = Interface.shared.rect(frame)
                        ui.hexRoom = CGRect(x: -on.minX, y: -on.minY, width: screen.width, height: screen.height)
                    }
                }
            }
            .overlay(alignment: .topLeading) { ColorMenus(model: model, ui: ui) }
            .scaleEffect(k, anchor: .topLeading)
            .offset(x: x, y: y)
            .onAppear { ui.harmonyAt = CGPoint(x: 0, y: 72) }
            // a plugin showing Classic's picker alone: this window gives way to that one
            .opacity(ColourLooks.shared.floatingPickerOnly ? 0 : 1)
            .allowsHitTesting(!ColourLooks.shared.floatingPickerOnly)
        }
        if model.floatingColorsAt != nil, ColourLooks.shared.floatingPickerOnly, !away {
            FloatingPicker(model: model, screen: screen)
        }
    }
}

/// How far down the floating picker's own picker starts: under its grabber.
private let floatingPickerTop: CGFloat = 30

/// The floating colour window as a plugin has it: Classic's picker alone, under the grabber that
/// moves it, with its Close; no title, mode icons or other pickers.
private struct FloatingPicker: View {
    let model: CanvasModel
    let screen: CGSize

    /// Where the window was as the drag on its grabber began.
    @State private var dragFrom: CGPoint?

    var body: some View {
        let c = BrushColor(model: model)
        let m = ModelColor(c: c, model: AppSettings.shared.colourModel, place: model.colourPlace)
        // a picker plugin's with the hue on a ring of its own has no hue slider under it
        let ringed = m.model == .hsb && ColourLooks.shared.pickerOn && PluginsModel.shared.store.pickerHuePoint(h: c.hue) != nil
        let height = floatingPickerTop + 248 + (ringed ? 0 : 40) + 6
        let at = model.floatingColorsAt ?? .zero
        // kept on the screen
        // at the floating windows' size, as the other floating windows are
        let k = Look.shared.scale(.floating)
        let x = min(max(at.x, 0), max(screen.width - compactWidth * k, 0))
        let y = min(max(at.y, 0), max(screen.height - height * k, 0))
        Popover(width: compactWidth, arrowX: 0, side: .none) {
            ZStack(alignment: .top) {
                Capsule()
                    .fill(Pc.grabber)
                    .frame(width: 40, height: 4)
                    .padding(.top, 5.5)
                Color.clear
                    .frame(width: 150, height: floatingPickerTop)
                    .contentShape(Rectangle())
                    .gesture(
                        DragGesture(minimumDistance: 1, coordinateSpace: .global)
                            .onChanged { drag in
                                let from = dragFrom ?? model.floatingColorsAt ?? .zero
                                dragFrom = from
                                let k = Interface.shared.scale
                                model.floatingColorsAt = CGPoint(x: from.x + drag.translation.width / k, y: from.y + drag.translation.height / k)
                            }
                            .onEnded { _ in dragFrom = nil }
                    )
                ClassicPicker(c: c, square: 248, compact: true, m: m, session: model.session)
                    .padding(.top, floatingPickerTop)
                FloatingClose { model.floatingColorsAt = nil }
                    .frame(maxWidth: .infinity, alignment: .trailing)
            }
            .frame(height: height, alignment: .top)
        }
        .frame(height: height, alignment: .top)
        // the canvas under it takes no touch
        .contentShape(Rectangle())
        .onTapGesture {}
        .scaleEffect(k, anchor: .topLeading)
        .offset(x: x, y: y)
    }
}

/// Value, compact: HSB or RGB (a switch picks), then the hex code; 240 wide.
private struct CompactValue: View {
    let c: any ColorPick
    let ui: ColorUi
    var group = compactGroup
    var fieldFill = compactHexField

    @State private var rgbShown = false

    private func choice(_ label: String, rgb: Bool) -> some View {
        let on = rgbShown == rgb
        return Text(verbatim: label)
            .font(.system(size: 15, weight: on ? .semibold : .regular))
            .foregroundStyle(on ? Color.white : Pc.panelTitle)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(RoundedRectangle(cornerRadius: 9, style: .circular).fill(on ? Pc.accent : Color.clear))
            .contentShape(Rectangle())
            .onTapGesture { rgbShown = rgb }
    }

    var body: some View {
        let rgb = c.rgb()
        let r = (rgb >> 16) & 0xFF
        let g = (rgb >> 8) & 0xFF
        let b = rgb & 0xFF
        let hue = c.hue
        let byte: (Float) -> UInt32 = { UInt32(wholeNumber($0 * 255)) }
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                choice("HSB", rgb: false)
                choice("RGB", rgb: true)
            }
            .frame(height: 36)
            .background(RoundedRectangle(cornerRadius: 9, style: .circular).fill(group))
            VStack(spacing: 18) {
                if rgbShown {
                    CompactValueRow(letter: "R", value: Float(r) / 255, colours: [packed(0, g, b), packed(255, g, b)], shown: "\(r)") {
                        c.setRgb(packed(byte($0), g, b))
                    }
                    CompactValueRow(letter: "G", value: Float(g) / 255, colours: [packed(r, 0, b), packed(r, 255, b)], shown: "\(g)") {
                        c.setRgb(packed(r, byte($0), b))
                    }
                    CompactValueRow(letter: "B", value: Float(b) / 255, colours: [packed(r, g, 0), packed(r, g, 255)], shown: "\(b)") {
                        c.setRgb(packed(r, g, byte($0)))
                    }
                } else {
                    CompactValueRow(letter: "H", value: huePosition(control: .slider, hue: hue), colours: sliderHues, shown: "\(wholeNumber(hue * 360))°") {
                        c.setHsv(hueAt(control: .slider, position: $0), c.saturation, c.brightness)
                    }
                    CompactValueRow(letter: "S", value: c.saturation, colours: [0x7F7F7F, CanvasModel.rgb(hue: hue, saturation: 1, brightness: 1)], shown: "\(wholeNumber(c.saturation * 100))%") {
                        c.setHsv(hue, $0, c.brightness)
                    }
                    CompactValueRow(letter: "B", value: c.brightness, colours: brightnessTrack, shown: "\(wholeNumber(c.brightness * 100))%") {
                        c.setHsv(hue, c.saturation, $0)
                    }
                }
            }
            .padding(.horizontal, 12)
            .padding(.top, 10)
            .frame(maxWidth: .infinity)
            .frame(height: 188, alignment: .top)
            .background(RoundedRectangle(cornerRadius: 10, style: .circular).fill(group))
            .padding(.top, 6)
            HexRow(rgb: rgb, ui: ui, rowFill: group, fieldFill: fieldFill) { c.setRgb($0) }
            Color.clear.frame(height: 10)
        }
        .padding(.horizontal, 10)
    }
}

/// One compact number slider, 216 wide: its letter, the track and the value.
private struct CompactValueRow: View {
    let letter: String
    let value: Float
    let colours: [UInt32]
    let shown: String
    let onChange: (Float) -> Void

    var body: some View {
        HStack(spacing: 0) {
            Text(verbatim: letter)
                .font(.system(size: 15))
                .foregroundStyle(Pc.panelTitle)
                .frame(width: 20, alignment: .leading)
            SliderRow(value: value, colours: colours, onChange: onChange)
            Text(verbatim: shown)
                .font(.system(size: 15))
                .foregroundStyle(Pc.panelTitle)
                .lineLimit(1)
                .frame(width: 48, alignment: .trailing)
        }
        .frame(height: 44)
    }
}

/// The palettes' swatches, one grid after another, a tap taking a colour.
private struct CompactPalettes: View {
    let c: any ColorPick
    let palettes: [PaletteInfo]

    private func grid(_ palette: PaletteInfo) -> some View {
        VStack(spacing: 0) {
            ForEach(0 ..< 3, id: \.self) { row in
                HStack(spacing: 0) {
                    ForEach(0 ..< 10, id: \.self) { col in
                        let slot = row * 10 + col
                        let rgb = palette.colors.indices.contains(slot) ? palette.colors[slot] : nil
                        Rectangle()
                            .fill(rgb.map(canvasColor) ?? Pc.emptySlot)
                            .contentShape(Rectangle())
                            .onTapGesture {
                                if let rgb {
                                    c.setRgb(rgb)
                                }
                            }
                    }
                }
                .frame(height: 24)
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .circular))
    }

    var body: some View {
        ScrollView(.vertical, showsIndicators: false) {
            LazyVStack(spacing: 10) {
                ForEach(palettes.indices, id: \.self) { i in
                    grid(palettes[i])
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 4)
        }
        .frame(height: 240)
    }
}
