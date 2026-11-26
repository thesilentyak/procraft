import SwiftUI

/// How far from the middle a ring's first row runs, and each next one further.
private let ringFirst: CGFloat = 62
private let ringStep: CGFloat = 58
/// A colour's disc on a ring, and a brush's.
private let ringColour: CGFloat = 34
private let ringBrush: CGFloat = 52
/// How far the ring's backdrop reaches past its outer row, and stays in from the window's edges.
private let ringMargin: CGFloat = 12

/// Row `k`'s distance from the middle of a ring.
private func ringRadius(_ k: Int) -> CGFloat {
    ringFirst + ringStep * CGFloat(k)
}

/// The size of each of `n` items on a ring `radius` out, `base` across at most: they never touch,
/// 6 apart at least.
private func ringItem(_ base: CGFloat, _ radius: CGFloat, _ n: Int) -> CGFloat {
    n <= 1 ? base : min(base, 2 * radius * sin(.pi / CGFloat(n)) - 6)
}

/// Where item `i` of `n` sits on a ring `radius` out, from its middle: the first at the top, the
/// rest clockwise.
private func ringPlace(_ i: Int, _ n: Int, _ radius: CGFloat) -> CGPoint {
    let angle = -CGFloat.pi / 2 + 2 * .pi * CGFloat(i) / CGFloat(max(n, 1))
    return CGPoint(x: radius * cos(angle), y: radius * sin(angle))
}

/// How far a ring of `rows` reaches from its middle, its backdrop with it.
private func ringReach(_ rows: [PanelRow]) -> CGFloat {
    let outer = max(rows.count - 1, 0)
    var item = ringColour
    if case .brushes = rows.last {
        item = ringBrush
    }
    return ringRadius(outer) + item / 2 + ringMargin
}

/// The middle of a ring reaching `reach` at `want`, moved in so it stays in a window `size` where
/// it fits.
private func ringMiddle(_ want: CGPoint, _ reach: CGFloat, _ size: CGSize) -> CGPoint {
    func inside(_ v: CGFloat, _ most: CGFloat) -> CGFloat {
        most < reach * 2 ? most / 2 : min(max(v, reach), most - reach)
    }
    return CGPoint(x: inside(want.x, size.width), y: inside(want.y, size.height))
}

/// One of a plugin's actions on its card or in a place it puts it: its words in blue, run when
/// tapped.
struct PluginActionRow: View {
    let label: String
    let action: () -> Void

    var body: some View {
        Text(verbatim: label)
            .font(.system(size: 17))
            .foregroundStyle(Pc.accent)
            .lineLimit(1)
            .padding(.horizontal, 16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .frame(height: 44)
            .contentShape(Rectangle())
            .onTapGesture(perform: action)
    }
}

/// A setting on a card named at every step: its label, the one chosen in blue; a tap opens a menu
/// of them.
struct PluginChoiceRow: View {
    let slider: PluginSlider
    let choices: [String]
    let onSet: (PluginSlider, Float) -> Void

    var body: some View {
        let chosen = Int(pluginSliderChosen(slider: slider, value: slider.value))
        Menu {
            ForEach(choices.indices, id: \.self) { i in
                Button {
                    onSet(slider, pluginSliderChoice(slider: slider, index: UInt32(i)))
                } label: {
                    if i == chosen {
                        Label(choices[i], systemImage: "checkmark")
                    } else {
                        Text(verbatim: choices[i])
                    }
                }
            }
        } label: {
            HStack(spacing: 0) {
                Text(verbatim: slider.label)
                    .font(.system(size: 17))
                    .foregroundStyle(Pc.actionsRow)
                    .lineLimit(1)
                Spacer(minLength: 0)
                Text(verbatim: pluginSliderShown(slider: slider, value: slider.value))
                    .font(.system(size: 17))
                    .foregroundStyle(Pc.accent)
                    .lineLimit(1)
            }
            .padding(.horizontal, 16)
            .frame(height: 44)
            .contentShape(Rectangle())
        }
    }
}

/// The controls and actions the plugins that are on put in a place of the interface (the Canvas
/// tab's, say): a group of their own, each row as on its plugin's card.
struct PluginPlaceRows: View {
    let model: CanvasModel
    let place: PluginPlace
    /// A row of the app's own after theirs, for what a plugin lets it do there (Edit Layout).
    var more: (label: String, action: () -> Void)?

    var body: some View {
        let plugins = PluginsModel.shared
        // asked again as the plugins change
        let _ = plugins.changes
        let controls = plugins.store.controlsIn(place: place)
        let actions = plugins.store.actionsIn(place: place)
        if !controls.isEmpty || !actions.isEmpty || more != nil {
            let shape = RoundedRectangle(cornerRadius: 22, style: .circular)
            VStack(spacing: 0) {
                ForEach(controls.indices, id: \.self) { i in
                    if i > 0 {
                        Rectangle().fill(Pc.actionsSeparator).frame(height: 1)
                    }
                    PluginControlView(model: model, control: controls[i])
                }
                ForEach(actions.indices, id: \.self) { i in
                    if i > 0 || !controls.isEmpty {
                        Rectangle().fill(Pc.actionsSeparator).frame(height: 1)
                    }
                    PluginActionRow(label: actions[i].label) { model.runPluginAction(actions[i].key) }
                }
                if let more {
                    if !controls.isEmpty || !actions.isEmpty {
                        Rectangle().fill(Pc.actionsSeparator).frame(height: 1)
                    }
                    PluginActionRow(label: more.label, action: more.action)
                }
            }
            .background(shape.fill(Pc.actionsGroup))
            .clipShape(shape)
        }
    }
}

/// A plugin's control as its card shows it, whatever it is: a switch, a slider, a setting named at
/// every step, a file, a texture.
struct PluginControlView: View {
    let model: CanvasModel
    let control: PluginControl

    var body: some View {
        let plugins = PluginsModel.shared
        switch control {
        case let .switch(one):
            RowView(row: .toggle(one.label, on: one.on, change: { plugins.setSwitch(one.key, $0) }))
        case let .slider(one):
            let choices = pluginSliderChoices(slider: one)
            if choices.isEmpty {
                PluginSliderRow(slider: one) { plugins.setSlider($0, $1) }
            } else {
                PluginChoiceRow(slider: one, choices: choices) { plugins.setSlider($0, $1) }
            }
        case let .file(one):
            PluginFileRow(file: one) { model.showNotice($0, warning: true) }
        case let .texture(one):
            PluginTextureRow(texture: one) { model.showNotice($0, warning: true) }
        case let .folder(one):
            PluginFolderRow(folder: one) { model.showNotice($0, warning: true) }
        }
    }
}

/// A plugin's own panel, while its action keeps it open: floating under the bar at the right, its
/// title and Done, then its rows on a card as the Actions panel's are. A slider is told as it lets
/// go, anything else at once.
struct PluginPanelView: View {
    let model: CanvasModel
    let panel: PluginPanel

    private func line() -> some View {
        Rectangle().fill(Pc.actionsSeparator).frame(height: 1)
    }

    @ViewBuilder private func row(_ i: Int, _ row: PanelRow) -> some View {
        switch row {
        case let .text(text):
            Text(verbatim: text)
                .font(.system(size: 15))
                .foregroundStyle(Pc.actionsRow)
                .padding(.horizontal, 16)
                .padding(.vertical, 11)
                .frame(maxWidth: .infinity, alignment: .leading)
        case let .slider(label, least, most, step, value, unit):
            PanelSliderRow(label: label, least: least, most: most, step: step, value: value, unit: unit) {
                model.touchPluginPanel(i, $0)
            }
        case let .switch(label, on):
            RowView(row: .toggle(label, on: on, change: { model.touchPluginPanel(i, $0 ? 1 : 0) }))
        case let .choice(label, options, chosen):
            // a tap picks the next
            HStack(spacing: 0) {
                Text(verbatim: label)
                    .font(.system(size: 17))
                    .foregroundStyle(Pc.actionsRow)
                    .lineLimit(1)
                Spacer(minLength: 0)
                Text(verbatim: options.indices.contains(Int(chosen)) ? options[Int(chosen)] : "")
                    .font(.system(size: 17))
                    .foregroundStyle(Pc.accent)
                    .lineLimit(1)
            }
            .padding(.horizontal, 16)
            .frame(height: 44)
            .contentShape(Rectangle())
            .onTapGesture { model.touchPluginPanel(i, Float((Int(chosen) + 1) % max(options.count, 1))) }
        case let .button(label):
            PluginActionRow(label: label) { model.touchPluginPanel(i, 1) }
        case let .colours(label, colours, chosen):
            HStack(spacing: 0) {
                Text(verbatim: label)
                    .font(.system(size: 17))
                    .foregroundStyle(Pc.actionsRow)
                    .lineLimit(1)
                    .padding(.trailing, 8)
                Spacer(minLength: 0)
                ForEach(colours.indices, id: \.self) { at in
                    let picked = chosen.map { Int($0) == at } ?? false
                    ZStack {
                        if picked {
                            Circle().strokeBorder(Pc.actionsRow, lineWidth: 2).frame(width: 24, height: 24)
                        }
                        Circle().fill(Color(argb: 0xFF00_0000 | colours[at])).frame(width: picked ? 16 : 24, height: picked ? 16 : 24)
                    }
                    .frame(width: 30, height: 30)
                    .contentShape(Circle())
                    .onTapGesture { model.touchPluginPanel(i, Float(at)) }
                }
            }
            .padding(.leading, 16)
            .padding(.trailing, 10)
            .frame(height: 52)
        case let .keys(label, keys):
            HStack(spacing: 12) {
                Text(verbatim: label)
                    .font(.system(size: 17))
                    .foregroundStyle(Pc.actionsRow)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 0)
                Text(verbatim: keys)
                    .font(.system(size: 15))
                    .foregroundStyle(Pc.slideValue)
                    .lineLimit(1)
                    .fixedSize()
            }
            .padding(.horizontal, 16)
            .frame(height: 44)
            .contentShape(Rectangle())
            .onTapGesture { model.touchPluginPanel(i, 1) }
        case let .brushes(label, brushes, chosen):
            VStack(alignment: .leading, spacing: 8) {
                Text(verbatim: label)
                    .font(.system(size: 17))
                    .foregroundStyle(Pc.actionsRow)
                    .lineLimit(1)
                HStack(spacing: 4) {
                    ForEach(brushes.indices, id: \.self) { at in
                        RingBrush(model: model, brush: brushes[at], chosen: chosen.map { Int($0) == at } ?? false, side: 34) {
                            model.touchPluginPanel(i, Float(at))
                        }
                    }
                }
            }
            .padding(.leading, 16)
            .padding(.trailing, 10)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
        case let .control(key):
            // one of its texture controls, as its card shows it (others show nothing)
            let textures = PluginsModel.shared.plugins.flatMap(\.controls).compactMap { control -> PluginTexture? in
                if case let .texture(one) = control, one.key == key {
                    return one
                }
                return nil
            }
            if let one = textures.first {
                PluginTextureRow(texture: one) { model.showNotice($0, warning: true) }
            }
        }
    }

    var body: some View {
        Popover(width: 340, arrowX: 0, side: .none) {
            HStack(spacing: 0) {
                Text(verbatim: panel.title)
                    .font(.system(size: 22, weight: .bold))
                    .foregroundStyle(Pc.panelTitle)
                    .lineLimit(1)
                Spacer(minLength: 0)
                Text(verbatim: "Done")
                    .font(.system(size: 17))
                    .foregroundStyle(Pc.accent)
                    .lineLimit(1)
                    .contentShape(Rectangle())
                    .onTapGesture { model.closePluginPanel() }
            }
            .padding(.horizontal, 24)
            .padding(.top, 12)
            .frame(height: 52)
            ScrollView {
                VStack(spacing: 0) {
                    ForEach(panel.rows.indices, id: \.self) { i in
                        if i > 0 {
                            line()
                        }
                        row(i, panel.rows[i])
                    }
                }
                .background(RoundedRectangle(cornerRadius: 13, style: .circular).fill(Pc.actionsGroup))
                .clipShape(RoundedRectangle(cornerRadius: 13, style: .circular))
                .padding(.horizontal, 10)
                .padding(.top, 8)
                .padding(.bottom, 10)
            }
            .scrollIndicators(.hidden)
            .frame(maxHeight: 600)
            .fixedSize(horizontal: false, vertical: true)
        }
        // what its rows don't take stays on the panel, never reaching the canvas under it
        .contentShape(Rectangle())
        .onTapGesture {}
    }
}

/// A slider on a plugin's panel: its value whole with whole steps, else to two places, then its
/// unit; told as it lets go.
private struct PanelSliderRow: View {
    let label: String
    let least: Float
    let most: Float
    let step: Float
    let value: Float
    let unit: String
    let onSet: (Float) -> Void

    @State private var moved: Float?

    var body: some View {
        let now = moved ?? value
        let whole = step >= 1 && step.truncatingRemainder(dividingBy: 1) == 0
        let shown = (whole ? "\(Int(now.rounded()))" : String(format: "%.2f", now)) + unit
        let span = most - least
        ActionSlideRow(
            label: label, value: shown, share: span > 0 ? (now - least) / span : 0,
            change: { along in
                let to = least + along * span
                let stepped = step > 0 ? least + ((to - least) / step).rounded() * step : to
                moved = min(max(stepped, least), most)
            },
            onDone: {
                if let moved {
                    onSet(moved)
                }
                moved = nil
            }
        )
    }
}

/// A brush on a ring or a panel's row: its sample stroke in a disc `side` across, ringed in the
/// accent while `chosen`.
private struct RingBrush: View {
    let model: CanvasModel
    let brush: PanelBrush
    let chosen: Bool
    let side: CGFloat
    let action: () -> Void

    var body: some View {
        let info = BrushInfo(set: brush.set, index: brush.index, name: brush.name)
        let preview = model.previews[CanvasModel.brushKey(info)]
        ZStack {
            Circle().fill(chosen ? Pc.rowSelected : Pc.actionDisc)
            if let preview {
                Image(uiImage: preview)
                    .resizable()
                    // white strokes: dark on the light interface's discs
                    .renderingMode(Look.shared.light && !chosen ? .template : .original)
                    .foregroundStyle(Color.black)
                    .scaledToFill()
                    .frame(width: side, height: side)
            }
        }
        .frame(width: side, height: side)
        .clipShape(Circle())
        .overlay(Circle().strokeBorder(chosen ? Pc.accent : Pc.swatchEdge, lineWidth: chosen ? 2 : 1))
        .contentShape(Circle())
        .onTapGesture(perform: action)
        .accessibilityLabel(Text(verbatim: brush.name))
        .onAppear { model.loadPreview(info) }
    }
}

/// A plugin's panel shown as a ring: round where its action came from (the Pencil whose gesture
/// ran it, or the window's middle), its rows of colours and brushes each a ring about it, the first
/// innermost, on a round backdrop. A colour or a brush tapped is told to the plugin as its place; a
/// tap anywhere else closes it, as Done would.
struct PluginRingView: View {
    let model: CanvasModel
    let panel: PluginPanel
    let screen: CGSize

    var body: some View {
        let reach = ringReach(panel.rows)
        let want = panel.at.map { CGPoint(x: CGFloat($0.x) / model.pixels, y: CGFloat($0.y) / model.pixels) }
            ?? CGPoint(x: screen.width / 2, y: screen.height / 2)
        let middle = ringMiddle(want, reach, screen)
        ZStack(alignment: .topLeading) {
            Color.clear
                .contentShape(Rectangle())
                .onTapGesture { model.closePluginPanel() }
            // the backdrop, which takes its own taps (between the items) without closing
            Circle()
                .fill(Pc.popoverTint)
                .frame(width: reach * 2, height: reach * 2)
                .contentShape(Circle())
                .onTapGesture {}
                .accessibilityLabel(Text(verbatim: panel.title))
                .offset(x: middle.x - reach, y: middle.y - reach)
            // where the Pencil was
            Circle()
                .strokeBorder(Pc.panelGlyph, lineWidth: 1.5)
                .frame(width: 8, height: 8)
                .offset(x: middle.x - 4, y: middle.y - 4)
                .allowsHitTesting(false)
            ForEach(panel.rows.indices, id: \.self) { k in
                ringRow(k, panel.rows[k], middle: middle)
            }
        }
        .frame(width: screen.width, height: screen.height, alignment: .topLeading)
    }

    @ViewBuilder private func ringRow(_ k: Int, _ row: PanelRow, middle: CGPoint) -> some View {
        let radius = ringRadius(k)
        switch row {
        case let .colours(_, colours, chosen):
            ForEach(colours.indices, id: \.self) { i in
                let d = ringItem(ringColour, radius, colours.count)
                let at = ringPlace(i, colours.count, radius)
                let picked = chosen.map { Int($0) == i } ?? false
                ZStack {
                    if picked {
                        Circle().strokeBorder(Pc.text, lineWidth: 2)
                    }
                    Circle().fill(Color(argb: 0xFF00_0000 | colours[i])).padding(picked ? 4 : 0)
                }
                .frame(width: d, height: d)
                .contentShape(Circle())
                .onTapGesture { model.touchPluginPanel(k, Float(i)) }
                .offset(x: middle.x + at.x - d / 2, y: middle.y + at.y - d / 2)
            }
        case let .brushes(_, brushes, chosen):
            ForEach(brushes.indices, id: \.self) { i in
                let d = ringItem(ringBrush, radius, brushes.count)
                let at = ringPlace(i, brushes.count, radius)
                RingBrush(model: model, brush: brushes[i], chosen: chosen.map { Int($0) == i } ?? false, side: d) {
                    model.touchPluginPanel(k, Float(i))
                }
                .offset(x: middle.x + at.x - d / 2, y: middle.y + at.y - d / 2)
            }
        // a ring holds no other kind of row
        default:
            EmptyView()
        }
    }
}
