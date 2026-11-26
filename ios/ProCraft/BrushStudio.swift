import SwiftUI

/// The space the studio's menus and the rows they hang from are placed in: the whole screen.
let studioSpace = "studio"

private let settingsWidth: CGFloat = 323
private let padHeight: CGFloat = 510
private let tabsWidth: CGFloat = 81

/// The studio's colours, dark or as the light interface has them.
enum StudioInk {
    private static func pick(_ dark: UInt32, _ light: UInt32) -> Color {
        Color(argb: Look.shared.light ? light : dark)
    }

    static var tabs: Color { pick(0xFF27_2727, 0xFFEB_EBEB) }
    static var back: Color { pick(0xFF1F_1F1F, 0xFFF3_F3F3) }
    static var line: Color { pick(0xFF30_3030, 0xFFCF_CFCF) }
    static var header: Color { pick(0xFF9C_9C9C, 0xFF81_8181) }
    static var label: Color { pick(0xFFED_EDED, 0xFF34_3434) }
    static var dim: Color { pick(0xFF8E_8E8E, 0xFF9A_9A9A) }
    static var off: Color { pick(0xFF50_5050, 0xFFD5_D5D5) }
    static var cardEdge: Color { pick(0xFF37_3737, 0xFFDA_DADA) }
    static var cardTitle: Color { pick(0xFFC1_C1C1, 0xFF4A_4A4A) }
    static var pageInk: Color { pick(0xFF79_7979, 0xFF3E_3E3E) }
    static var pillOff: Color { pick(0xFF22_2222, 0xFFED_EDED) }
    static var valueOff: Color { pick(0xFF64_6464, 0xFFA0_A0A0) }
    static var knobOff: Color { pick(0xFF35_3535, 0xFFD0_D0D0) }
    static var list: Color { pick(0xFF24_2424, 0xFFFF_FFFF) }
    static var listLine: Color { pick(0xFF20_2020, 0xFFE5_E5E5) }
    static var segments: Color { pick(0xFF3C_3C3C, 0xFFDC_DCDC) }
    static var material: Color { pick(0xFF23_2323, 0xFFEC_ECEC) }
    static var materialInk: Color { pick(0xFFB1_B1B1, 0xFF4A_4A4A) }
    static var pill: Color { pick(0xFF29_2929, 0xFFE5_E5E5) }
    static var track: Color { pick(0xFF33_3333, 0xFFE5_E5E5) }
    static var tabIcon: Color { pick(0xFFAA_AAAA, 0xFF3D_3D3D) }
    static var button: Color { pick(0xFF1C_1C1C, 0xFFFB_FBFB) }
    static var buttonEdge: Color { pick(0xFF38_3838, 0xFFE0_E0E0) }
    static var buttonInk: Color { pick(0xFFAA_AAAA, 0xFF58_5858) }
    static var card: Color { pick(0xFF31_3131, 0xFFE5_E5E5) }
    static var previewFill: Color { pick(0xFF2A_2A2A, 0xFFE8_E8E8) }
    static var previewEdge: Color { pick(0xFF4A_4A4A, 0xFFD0_D0D0) }
    static var previewName: Color { pick(0xFFC5_C5C5, 0xFF3A_3A3A) }
    static var aboutCard: Color { pick(0xFF25_2525, 0xFFEB_EBEB) }
    static var aboutFace: Color { pick(0xFF3A_3A3A, 0xFFD8_D8D8) }
    static var aboutName: Color { pick(0xFFE0_E0E0, 0xFF2A_2A2A) }
    static var aboutRowInk: Color { pick(0xFFBB_BBBB, 0xFF4A_4A4A) }
    static var aboutSignature: Color { pick(0xFF59_5959, 0xFFA8_A8A8) }
    static var padBehind: Color { back }
    static var partCard: Color { pick(0xFF21_2121, 0xFFE0_E0E0) }
    static let callout = Color(argb: 0xFFE9_E9E9)
    static let calloutInk = Color(argb: 0xFF1C_1C1C)
    static let switchFixed = Color(argb: 0xFF51_5153)
    static let switchFixedKnob = Color(argb: 0xFF9B_9B9B)
    static let done = Color(argb: 0xFF01_90FF)
}

/// What the studio's rows ask to show over everything, where a scrolling column can't cut it.
@Observable
final class StudioUi {
    var keypad: KeypadAsk?
    var blendList: BlendListAsk?
    var nodeMenu: NodeMenuAsk?
    /// A slider has the touch: the settings don't scroll under it meanwhile.
    var sliding = false
    /// Where a dual brush's parts start, for their cards to open over them.
    var partsAt = CGPoint.zero
    /// The Secondary card held: where, for the Uncombine callout under it.
    var callout: CGPoint?
}

/// The brush studio: tabs down the left, the drawing pad above, the tab's settings below; turned
/// sideways, the pages by name on a card, the settings beside them and the pad on the right. It
/// slides up from the bottom of the screen, and back down when it closes.
struct BrushStudio: View {
    let model: CanvasModel
    @State private var ui = StudioUi()

    private var studio: StudioModel {
        model.studio
    }

    var body: some View {
        GeometryReader { screen in
            let landscape = screen.size.width > screen.size.height
            ZStack(alignment: .topLeading) {
                StudioInk.back
                if landscape {
                    HStack(spacing: 0) {
                        StudioPagesCard(studio: studio, ui: ui)
                        StudioSettings(model: model, ui: ui, landscape: true)
                            .frame(width: settingsWidth)
                        StudioInk.line.frame(width: 1)
                        StudioPad(model: model)
                    }
                } else {
                    HStack(spacing: 0) {
                        StudioRail(studio: studio, ui: ui)
                        VStack(spacing: 0) {
                            StudioPad(model: model)
                                .frame(height: padHeight)
                            StudioInk.line.frame(height: 1)
                            StudioSettings(model: model, ui: ui, landscape: false)
                        }
                    }
                }
                if studio.padMenuOpen {
                    PadMenu(studio: studio, width: screen.size.width)
                }
                if studio.partsOpen {
                    PartCards(studio: studio, ui: ui, landscape: landscape)
                }
                if let at = ui.callout {
                    UncombineCallout(studio: studio, ui: ui, at: at)
                }
                if let ask = ui.blendList {
                    BlendModeList(ask: ask, height: screen.size.height) { picked in
                        ui.blendList = nil
                        if let picked {
                            ask.onPick(picked)
                        }
                    }
                }
                if let ask = ui.keypad {
                    SliderKeypad(ask: ask, ui: ui, width: screen.size.width, height: screen.size.height) { ui.keypad = nil }
                        .id(ask.id)
                }
                if let ask = ui.nodeMenu {
                    NodeMenu(ask: ask) { ui.nodeMenu = nil }
                }
                if let kind = studio.sourceEditing {
                    SourceEditor(studio: studio, kind: kind, landscape: landscape)
                        .id(kind)
                }
                // Uncombine, asking first
                AlertView(
                    content: studio.uncombineAsking ? AlertContent(
                        title: "Uncombine these brushes?",
                        message: "The pair becomes two brushes again, side by side in the set. It can't be undone.",
                        action: "Uncombine",
                        icon: PcIcons.alertQuestion,
                        destructive: false
                    ) : nil,
                    onCancel: { studio.uncombineAsking = false },
                    onAction: {
                        studio.uncombineAsking = false
                        studio.uncombine()
                    }
                )
            }
            .coordinateSpace(.named(studioSpace))
            #if DEBUG
            .onAppear {
                ScreenCapture.studioShows?(studio, ui)
                ScreenCapture.studioShows = nil
            }
            #endif
        }
    }
}

/// The pages a dual brush's second part has: the first alone has the rest, which go for the pair.
private let secondPages: Set<String> = [
    "Stroke path", "Stabilization", "Taper", "Shape", "Grain", "Rendering", "Dynamics", "Apple Pencil", "Properties",
]

/// The pages listed, by their place among all of them: fewer while a dual brush's second shows.
private func shownTabs(_ studio: StudioModel) -> [(index: Int, tab: StudioTab)] {
    studioTabs.enumerated()
        .filter { !studio.second || secondPages.contains($0.element.name) }
        .map { (index: $0.offset, tab: $0.element) }
}

/// Portrait: the pages as icons down the left, a dual brush's two parts above them.
private struct StudioRail: View {
    let studio: StudioModel
    let ui: StudioUi

    var body: some View {
        let settings = AppSettings.shared
        VStack(alignment: .leading, spacing: 3) {
            if studio.dual {
                StudioParts(studio: studio, ui: ui, width: partRailWidth, landscape: false)
                    .padding(.leading, 8)
            }
            ForEach(shownTabs(studio), id: \.index) { i, tab in
                let on = settings.studioTab == i
                ZStack {
                    Capsule().fill(on ? Pc.accent : Color.clear)
                    // drawn in a 50 frame, fitted to the tab's own size
                    PcIconView(tab.icon, size: tab.iconSize, color: on ? .white : StudioInk.tabIcon)
                }
                .frame(width: 65, height: 45)
                .contentShape(Rectangle())
                .onTapGesture { settings.studioTab = i }
                .padding(.leading, 8)
            }
        }
        .padding(.top, 46)
        .frame(width: tabsWidth, alignment: .leading)
        .frame(maxHeight: .infinity, alignment: .top)
        .background(StudioInk.tabs)
    }
}

/// Landscape: the pages by icon and name on a card headed "Brush Studio", the list scrolling under
/// the title, a dual brush's two parts heading it.
private struct StudioPagesCard: View {
    let studio: StudioModel
    let ui: StudioUi

    var body: some View {
        let settings = AppSettings.shared
        let shape = RoundedRectangle(cornerRadius: 20, style: .circular)
        VStack(alignment: .leading, spacing: 0) {
            Text(verbatim: "Brush Studio")
                .font(.system(size: 23, weight: .bold))
                .foregroundStyle(StudioInk.cardTitle)
                .lineLimit(1)
                .padding(.leading, 24)
                .padding(.top, 19)
            ScrollView {
                VStack(spacing: 4) {
                    if studio.dual {
                        StudioParts(studio: studio, ui: ui, width: partListWidth, landscape: true)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    ForEach(shownTabs(studio), id: \.index) { i, tab in
                        let on = settings.studioTab == i
                        HStack(spacing: 0) {
                            PcIconView(tab.icon, size: tab.iconSize, color: on ? .white : StudioInk.tabIcon)
                                .frame(width: 52)
                            Text(verbatim: tab.name)
                                .font(.system(size: 15.5))
                                .foregroundStyle(on ? Color.white : StudioInk.pageInk)
                                .lineLimit(1)
                            Spacer(minLength: 0)
                        }
                        .frame(height: 44)
                        .background(Capsule().fill(on ? Pc.accent : Color.clear))
                        .contentShape(Rectangle())
                        .onTapGesture { settings.studioTab = i }
                    }
                }
                .padding(.horizontal, 8)
                .padding(.top, 17)
                .padding(.bottom, 8)
            }
            .scrollIndicators(.hidden)
        }
        .frame(width: 320)
        .frame(maxHeight: .infinity, alignment: .top)
        .background(shape.fill(StudioInk.tabs))
        .overlay(shape.strokeBorder(StudioInk.cardEdge, lineWidth: 1))
        .clipShape(shape)
        .padding(.leading, 10)
        .padding(.vertical, 10)
    }
}

/// A dual brush's two parts as the pages' column heads them: each this wide (upright, or on the
/// card turned sideways), this tall and this far apart, and the room they take before the pages.
private let partRailWidth: CGFloat = 57
private let partListWidth: CGFloat = 288
private let partHeight: CGFloat = 61
private let partGap: CGFloat = 3
private let partsHeight: CGFloat = 146
/// Tapped, the part shown opens both as cards, Combine Mode between; its list opens this far in.
private let partCardWidth: CGFloat = 340
private let partCardHeight: CGFloat = 111
private let combineRowHeight: CGFloat = 51
private let combineListLeft: CGFloat = 368

/// A dual brush's two parts, the one shown barred: tapped, it opens their cards, and the other
/// shows. Upright they are narrow, a piece of each part's sample stroke in them.
private struct StudioParts: View {
    let studio: StudioModel
    let ui: StudioUi
    let width: CGFloat
    let landscape: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: partGap) {
            ForEach([false, true], id: \.self) { second in
                let on = second == studio.second
                ZStack(alignment: .leading) {
                    if let image = studio.parts[second ? 1 : 0] {
                        if landscape {
                            Image(uiImage: image)
                                .resizable()
                                .scaledToFit()
                                .frame(width: width, height: partHeight)
                        } else {
                            Image(uiImage: image)
                                .resizable()
                                .scaledToFill()
                                .frame(width: width, height: partHeight)
                                .clipped()
                        }
                    }
                    if on {
                        RoundedRectangle(cornerRadius: 2, style: .circular)
                            .fill(Pc.accent)
                            .frame(width: 5, height: partHeight - 1)
                    }
                }
                .frame(width: width, height: partHeight, alignment: .leading)
                .overlay {
                    GeometryReader { g in
                        Color.clear
                            .contentShape(Rectangle())
                            .onTapGesture {
                                if on {
                                    // the cards open where the parts start
                                    let at = g.frame(in: .named(studioSpace)).origin
                                    ui.partsAt = CGPoint(x: at.x, y: at.y - (second ? partHeight + partGap : 0))
                                    studio.partsOpen = true
                                } else {
                                    studio.showPart(second: second)
                                }
                            }
                    }
                }
            }
        }
        .frame(height: partsHeight, alignment: .top)
    }
}

/// A dual brush's parts as cards over the parts they opened from, Combine Mode between: a card
/// tapped shows that part, the cards staying; Secondary held offers Uncombine; a tap beside them
/// only closes them. Upright they sit on a card of their own.
private struct PartCards: View {
    let studio: StudioModel
    let ui: StudioUi
    let landscape: Bool

    var body: some View {
        let edge: CGFloat = landscape ? 0 : 8
        let shape = RoundedRectangle(cornerRadius: 12 + edge, style: .circular)
        let name = studio.combineModes.first { Int($0.id) == studio.combineMode }?.name ?? ""
        ZStack(alignment: .topLeading) {
            Color.clear
                .contentShape(Rectangle())
                .onTapGesture { studio.partsOpen = false }
            VStack(spacing: partGap) {
                PartCard(studio: studio, second: false)
                HStack(spacing: 0) {
                    Text(verbatim: "Combine Mode")
                        .font(.system(size: 17))
                        .foregroundStyle(StudioInk.dim)
                        .lineLimit(1)
                    Spacer(minLength: 0)
                    Text(verbatim: name)
                        .font(.system(size: 15))
                        .foregroundStyle(Pc.accent)
                        .lineLimit(1)
                    Color.clear.frame(width: 6)
                    PcIconView(PcIcons.chevronRight, size: 24, color: StudioInk.dim)
                }
                .padding(.leading, 11)
                .padding(.trailing, 12)
                .frame(width: partCardWidth, height: combineRowHeight)
                .overlay {
                    GeometryReader { g in
                        Color.clear
                            .contentShape(Rectangle())
                            .onTapGesture {
                                ui.blendList = BlendListAsk(
                                    modes: studio.combineModes,
                                    current: studio.combineMode,
                                    centreY: g.frame(in: .named(studioSpace)).midY,
                                    left: combineListLeft
                                ) { studio.setCombineMode($0) }
                            }
                    }
                }
                PartCard(studio: studio, second: true) { ui.callout = $0 }
            }
            .padding(edge)
            .background(shape.fill(StudioInk.tabs))
            .clipShape(shape)
            .contentShape(shape)
            .offset(x: max(ui.partsAt.x - edge, 0), y: max(ui.partsAt.y - edge, 0))
        }
    }
}

/// A part as a card: blue when it shows; tapped, it shows; held a second (Secondary only),
/// `onHold` hears where, in the studio's space.
private struct PartCard: View {
    let studio: StudioModel
    let second: Bool
    var onHold: ((CGPoint) -> Void)?
    @State private var holding: Task<Void, Never>?
    @State private var held = false

    var body: some View {
        let on = second == studio.second
        let shape = RoundedRectangle(cornerRadius: 12, style: .circular)
        let card = ZStack(alignment: .topLeading) {
            shape.fill(on ? Pc.accent : StudioInk.partCard)
            if let image = studio.parts[second ? 1 : 0] {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFit()
                    .padding(.horizontal, 30)
                    .padding(.vertical, 20)
                    .frame(width: partCardWidth, height: partCardHeight)
            }
            Text(verbatim: second ? "Secondary" : "Primary")
                .font(.system(size: 12.5, weight: .semibold))
                .foregroundStyle(on ? Color.white : StudioInk.pageInk)
                .lineLimit(1)
                .padding(.leading, 11)
                .padding(.top, 7)
        }
        .frame(width: partCardWidth, height: partCardHeight)
        .clipShape(shape)
        .contentShape(shape)
        if let onHold {
            // one touch tells the tap from the hold: let go before a second, it is a tap
            card.gesture(
                DragGesture(minimumDistance: 0, coordinateSpace: .named(studioSpace))
                    .onChanged { drag in
                        if holding == nil {
                            let at = drag.startLocation
                            holding = Task { @MainActor in
                                try? await Task.sleep(for: .seconds(1))
                                guard !Task.isCancelled else { return }
                                held = true
                                onHold(at)
                            }
                        } else if hypot(drag.translation.width, drag.translation.height) > 10 {
                            holding?.cancel()
                        }
                    }
                    .onEnded { drag in
                        holding?.cancel()
                        holding = nil
                        if !held, hypot(drag.translation.width, drag.translation.height) <= 10 {
                            studio.showPart(second: second)
                        }
                        held = false
                    }
            )
        } else {
            card.onTapGesture { studio.showPart(second: second) }
        }
    }
}

/// The light Uncombine callout under where the Secondary card was held: tapped, the cards close
/// and Uncombine is asked; a tap beside it closes it.
private struct UncombineCallout: View {
    let studio: StudioModel
    let ui: StudioUi
    let at: CGPoint

    var body: some View {
        ZStack(alignment: .topLeading) {
            Color.clear
                .contentShape(Rectangle())
                .onTapGesture { ui.callout = nil }
            Text(verbatim: "Uncombine")
                .font(.system(size: 15))
                .foregroundStyle(StudioInk.calloutInk)
                .lineLimit(1)
                .frame(width: 104, height: 36)
                .background(Capsule().fill(StudioInk.callout))
                .contentShape(Capsule())
                .onTapGesture {
                    ui.callout = nil
                    studio.partsOpen = false
                    studio.uncombineAsking = true
                }
                .offset(x: at.x - 52, y: at.y + 10)
        }
    }
}

/// The drawing pad with its buttons: Drawing Pad (its menu), Cancel and Done.
private struct StudioPad: View {
    let model: CanvasModel

    var body: some View {
        let studio = model.studio
        let settings = AppSettings.shared
        // a sphere showing the brush's materials on their page, or on every page with the 3D
        // preview on
        let tab = settings.studioTab
        let sphere = (studioTabs.indices.contains(tab) && studioTabs[tab].name == "Materials") || settings.pad3d
        ZStack(alignment: .topLeading) {
            // the pad's own colour behind it, so the canvas doesn't show before its first frame
            StudioInk.padBehind
            PadHost(studio: studio)
            if !studio.padShown {
                StudioInk.padBehind
                    .allowsHitTesting(false)
            }
            HStack(spacing: 6) {
                PcIconView(PcIcons.studioPad, size: 26, color: StudioInk.buttonInk)
                Text(verbatim: "Drawing Pad")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(StudioInk.buttonInk)
                    .lineLimit(1)
            }
            .padding(.leading, 9)
            .padding(.trailing, 13)
            .frame(height: 43)
            .background(Capsule().fill(StudioInk.button))
            .overlay(Capsule().strokeBorder(StudioInk.buttonEdge, lineWidth: 1))
            .overlay {
                GeometryReader { g in
                    Color.clear
                        .contentShape(Capsule())
                        .onTapGesture {
                            studio.padButtonCentre = g.frame(in: .named(studioSpace)).midX
                            studio.padMenuOpen.toggle()
                        }
                }
            }
            .offset(x: 21, y: 21)
            HStack(spacing: 13) {
                // a plugin's: the brush set at random
                if studio.randomises {
                    Text(verbatim: "Randomise")
                        .font(.system(size: 17))
                        .foregroundStyle(StudioInk.buttonInk)
                        .lineLimit(1)
                        .padding(.horizontal, 16)
                        .frame(height: 43)
                        .background(Capsule().fill(StudioInk.button))
                        .overlay(Capsule().strokeBorder(StudioInk.buttonEdge, lineWidth: 1))
                        .contentShape(Capsule())
                        .onTapGesture { studio.randomise() }
                }
                Text(verbatim: "Cancel")
                    .font(.system(size: 17))
                    .foregroundStyle(StudioInk.buttonInk)
                    .lineLimit(1)
                    .frame(width: 73, height: 43)
                    .background(Capsule().fill(StudioInk.button))
                    .overlay(Capsule().strokeBorder(StudioInk.buttonEdge, lineWidth: 1))
                    .contentShape(Capsule())
                    .onTapGesture { studio.close(keep: false) }
                PcIconView(PcIcons.doneTick, size: 40, color: .white)
                    .frame(width: 44, height: 44)
                    .background(Circle().fill(StudioInk.done))
                    .contentShape(Circle())
                    .onTapGesture { studio.close(keep: true) }
            }
            .padding(.top, 20)
            .padding(.trailing, 20)
            .frame(maxWidth: .infinity, alignment: .trailing)
        }
        .clipped()
        .onChange(of: sphere, initial: true) { _, on in
            studio.showPadSphere(on)
        }
    }
}

/// What the space above something follows or what comes after it, besides a control.
enum StudioNeighbour {
    case top
    case title
    case control(StudioControl)
}

private func isLine(_ neighbour: StudioNeighbour) -> Bool {
    guard case let .control(control) = neighbour else { return false }
    switch control {
    case .toggle, .picker:
        return true
    default:
        return false
    }
}

/// The space between `above` (a control, a section's title or the column's top) and `below` (a
/// control or a title), as measured. Titles here are 23 tall, slider rows 55, switch and picker
/// rows 47, list rows 44 with a line between.
func studioGap(_ above: StudioNeighbour, _ below: StudioNeighbour) -> CGFloat {
    if case .top = above {
        return 8
    }
    if case .title = above {
        if isLine(below) {
            return 6
        }
        guard case let .control(control) = below else { return 21.5 }
        switch control {
        case .pressureCurve, .about:
            return 27.5
        case .tiltAngle:
            return 26.5
        case .disclosure:
            return 39.5
        default:
            return 21.5
        }
    }
    guard case let .control(control) = above else { return 15.5 }
    if case .title = below {
        if isLine(above) {
            return 49.5
        }
        switch control {
        case .slider:
            return 45.5
        case .previewCard:
            return 47
        default:
            return 46
        }
    }
    if isLine(above) {
        return isLine(below) ? 4 : 19.5
    }
    switch control {
    case .slider, .choice:
        return isLine(below) ? 0 : 15.5
    case .kinds:
        return 16
    case .taperBar:
        return 23.5
    case .pressureCurve:
        return 24.5
    case .tiltAngle:
        return 21.5
    case .roundness:
        return 16.5
    case .disclosure:
        return 29
    default:
        return 15.5
    }
}

/// A line of a page's column.
private enum StudioRow {
    case space(CGFloat)
    case title(String)
    /// The About page's heading: the brush's name.
    case name
    case control(StudioControl)
}

/// The rows of `tab`, spaced as measured; the About page is headed by the brush's name. The rows
/// plugins add (`plugins`) follow, each under its own label.
private func studioRows(_ tab: StudioTab, plugins: [PluginBrushControl]) -> [StudioRow] {
    var rows: [StudioRow] = []
    var above = StudioNeighbour.top
    for (k, section) in tab.sections.enumerated() {
        var title = section.title.map { StudioRow.title($0) }
        if title == nil, let first = section.controls.first, case .about = first {
            title = .name
        }
        if let title {
            rows.append(.space(k == 0 ? 25 : studioGap(above, .title)))
            rows.append(title)
            above = .title
        }
        for control in section.controls {
            rows.append(.space(studioGap(above, .control(control))))
            rows.append(.control(control))
            above = .control(control)
        }
    }
    for row in plugins {
        let control = StudioControl.choice(key: row.key, options: row.choices)
        rows.append(.space(studioGap(above, .title)))
        rows.append(.title(row.label))
        rows.append(.space(studioGap(.title, .control(control))))
        rows.append(.control(control))
        above = .control(control)
    }
    return rows
}

/// The open page's settings, in sections, spaced as measured; each page opens at its top.
private struct StudioSettings: View {
    let model: CanvasModel
    let ui: StudioUi
    let landscape: Bool

    var body: some View {
        let index = AppSettings.shared.studioTab
        let tab = studioTabs[min(max(index, 0), studioTabs.count - 1)]
        // the rows plugins that are on add go under Properties
        let rows = studioRows(tab, plugins: tab.name == "Properties" ? model.studio.pluginControls : [])
        ScrollViewReader { reader in
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(rows.enumerated()), id: \.offset) { i, row in
                        Group {
                            switch row {
                            case let .space(height):
                                Color.clear.frame(height: height)
                            case let .title(title):
                                Text(verbatim: title)
                                    .font(.system(size: 21, weight: .bold))
                                    .foregroundStyle(StudioInk.header)
                                    .lineLimit(1)
                                    .padding(.leading, 8)
                            case .name:
                                StudioNameHeading(studio: model.studio)
                            case let .control(control):
                                StudioControlView(model: model, ui: ui, control: control, landscape: landscape)
                            }
                        }
                        .id(i)
                    }
                }
                .padding(.horizontal, 20)
                .padding(.bottom, 77)
            }
            .scrollDisabled(ui.sliding)
            #if DEBUG
            .onAppear {
                if let row = captureRow(rows) {
                    reader.scrollTo(row, anchor: .top)
                }
            }
            #endif
        }
        .scrollIndicators(.hidden)
        .id(index)
    }

    #if DEBUG
    /// The row a capture wants at the top.
    private func captureRow(_ rows: [StudioRow]) -> Int? {
        rows.firstIndex { row in
            guard case let .control(control) = row else { return false }
            switch (ScreenCapture.studioRow, control) {
            case ("roundness", .roundness), ("tilt", .tiltAngle):
                return true
            default:
                return false
            }
        }
    }
    #endif
}

/// The About page's heading: the brush's name, tapped to change it there (named as it is typed, so
/// Done with the keyboard up keeps it).
private struct StudioNameHeading: View {
    let studio: StudioModel
    @State private var renaming = false

    var body: some View {
        if renaming {
            InlineName(
                name: studio.name, color: StudioInk.header, fontSize: 21, weight: .bold,
                onDone: {
                    studio.rename($0)
                    renaming = false
                },
                onChange: { studio.rename($0) }
            )
            .padding(.leading, 8)
        } else {
            Text(verbatim: studio.name)
                .font(.system(size: 21, weight: .bold))
                .foregroundStyle(StudioInk.header)
                .lineLimit(1)
                .padding(.leading, 8)
                .contentShape(Rectangle())
                .onTapGesture { renaming = true }
        }
    }
}

/// The drawing pad's settings: preview size, the pad's colour, and clearing or resetting. It opens
/// under the Drawing Pad button, centred on it but kept 14 in from the screen's sides (`width`),
/// its arrow pointing up at the button. The colour chosen tapped again closes it.
private struct PadMenu: View {
    let studio: StudioModel
    let width: CGFloat

    private let fill = Color(argb: 0xFFC1_C1C1)
    private let group = Color(argb: 0xFFCD_CDCD)
    private let ink = Color(argb: 0xFF39_3939)
    /// The line between a group's rows: 2 thick, as measured.
    private let rule = Color(argb: 0xFFC0_C0C0)
    private let ruleThick: CGFloat = 2
    /// A switch's track when off, on the light menu.
    private let switchOff = Color(argb: 0xFFC1_C0C4)
    /// The body's top, below the screen's, and its corners.
    private let top: CGFloat = 77
    private let corner: CGFloat = 26
    /// The ring round the colour chosen: wider than a colour's place, as measured.
    private let chosenRing: CGFloat = 44

    /// The pencil the pad's strokes stand for: the newest this iPad's system knows.
    private var pencil: String {
        if #available(iOS 17.5, *) {
            return "Apple Pencil Pro"
        }
        return "Apple Pencil"
    }

    private func row(_ label: String, height: CGFloat, action: @escaping () -> Void) -> some View {
        Text(verbatim: label)
            .font(.system(size: 17))
            .foregroundStyle(ink)
            .lineLimit(1)
            .padding(.leading, 16)
            .frame(maxWidth: .infinity, minHeight: height, maxHeight: height, alignment: .leading)
            .contentShape(Rectangle())
            .onTapGesture(perform: action)
    }

    var body: some View {
        let settings = AppSettings.shared
        let left = min(max(studio.padButtonCentre - 160, 14), max(width - 334, 14))
        // the arrow's tip a little right of the button's middle, as the panels' are
        let arrowX = min(max(studio.padButtonCentre + Pc.arrowShift - left, corner + Pc.arrowHeight), 320 - corner - Pc.arrowHeight)
        let size = studio.value("preview_size")
        VStack(alignment: .leading, spacing: 0) {
            Text(verbatim: "Settings")
                .font(.system(size: 22, weight: .bold))
                .foregroundStyle(ink)
                .lineLimit(1)
                .padding(.leading, 12)
            Color.clear.frame(height: 28.5)
            HStack(spacing: 0) {
                Text(verbatim: "Preview size")
                Spacer(minLength: 0)
                Text(verbatim: "\(wholeNumber(size * 100))%")
            }
            .font(.system(size: 16))
            .foregroundStyle(ink)
            .lineLimit(1)
            .padding(.horizontal, 10)
            PadSizeSlider(value: size) { studio.set("preview_size", $0) }
            Color.clear.frame(height: 15.5)
            HStack(spacing: 0) {
                ForEach(Array(padColors.enumerated()), id: \.offset) { i, c in
                    if i > 0 {
                        Spacer(minLength: 0)
                    }
                    ZStack {
                        // the ring reaches past the colour's place, into the room between them
                        Circle().fill(studio.padColor == i ? Color(argb: 0xFFEB_EBEB) : Color.clear)
                            .frame(width: chosenRing, height: chosenRing)
                        Circle().fill(CanvasSpace.shared.color(c)).frame(width: 19, height: 19)
                    }
                    .frame(width: 37, height: 37)
                    .contentShape(Circle())
                    .onTapGesture {
                        if studio.padColor == i {
                            studio.padMenuOpen = false
                        } else {
                            studio.choosePadColor(i)
                        }
                    }
                }
            }
            Color.clear.frame(height: 20)
            VStack(spacing: 0) {
                Text(verbatim: pencil)
                    .font(.system(size: 17))
                    .foregroundStyle(ink)
                    .lineLimit(1)
                    .padding(.leading, 16)
                    .frame(maxWidth: .infinity, minHeight: 44, maxHeight: 44, alignment: .leading)
                rule.frame(height: ruleThick)
                HStack(spacing: 0) {
                    Text(verbatim: "3D preview")
                        .font(.system(size: 17))
                        .foregroundStyle(ink)
                        .lineLimit(1)
                    Spacer(minLength: 0)
                    PcSwitch(on: settings.pad3d, offTrack: switchOff)
                }
                .padding(.leading, 16)
                .padding(.trailing, 8)
                .frame(height: 44)
                .contentShape(Rectangle())
                .onTapGesture { settings.pad3d.toggle() }
            }
            .background(RoundedRectangle(cornerRadius: 16, style: .circular).fill(group))
            Color.clear.frame(height: 10)
            VStack(spacing: 0) {
                row("Clear drawing pad", height: 40) { studio.clearPad() }
                rule.frame(height: ruleThick)
                row("Reset all brush settings", height: 40) { studio.reset() }
            }
            .background(RoundedRectangle(cornerRadius: 16, style: .circular).fill(group))
        }
        .padding(.horizontal, 10)
        .padding(.top, 22)
        .padding(.bottom, 11.5)
        .frame(width: 320)
        .padding(.top, Pc.arrowHeight)
        .background(PopoverShape(side: .top, arrowX: arrowX, corner: corner).fill(fill))
        .offset(x: left, y: top - Pc.arrowHeight)
    }
}

/// The pad menu's thin slider: blue up to a white knob, jumping to the touch.
private struct PadSizeSlider: View {
    let value: Float
    let onChange: (Float) -> Void

    var body: some View {
        GeometryReader { g in
            let w = g.size.width
            let y = g.size.height / 2
            let x = w * CGFloat(min(max(value, 0), 1))
            let kx = min(max(x, 8), max(w - 8, 8))
            ZStack(alignment: .topLeading) {
                Capsule().fill(Color(argb: 0xFFA9_A9A9))
                    .frame(width: w, height: 3)
                    .offset(y: y - 1.5)
                Capsule().fill(Pc.accent)
                    .frame(width: x, height: 3)
                    .offset(y: y - 1.5)
                Capsule().fill(Color(argb: 0x3300_0000))
                    .frame(width: 16, height: 10)
                    .offset(x: kx - 8, y: y - 4)
                Capsule().fill(Color.white)
                    .frame(width: 16, height: 10)
                    .offset(x: kx - 8, y: y - 5)
            }
            .frame(width: w, height: g.size.height, alignment: .topLeading)
            .contentShape(Rectangle())
            .gesture(DragGesture(minimumDistance: 0).onChanged { drag in
                onChange(Float(min(max(drag.location.x / max(w, 1), 0), 1)))
            })
        }
        // its line 27.5 under the words' capitals above it, as measured
        .frame(height: 23)
        .padding(.horizontal, 10)
    }
}
