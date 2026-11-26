import SwiftUI

private let studioBar = Color(argb: 0xFF26_292A)
private let studioTitle = Color(argb: 0xFFD8_D8D8)
private let pillFill = Color(argb: 0xFF1D_2021)
private let pillEdge = Color(argb: 0xFF3B_3E3F)
private let pillLit = Color(argb: 0xFF47_4747)
private let pillInk = Color(argb: 0xFFC1_C1C1)
private let doneBlue = Color(argb: 0xFF01_90FF)
private let popoverFill = Color(argb: 0xFF28_2A2D)
private let groupFill = Color(argb: 0xFF23_2528)
private let rowLine = Color(argb: 0xFF2C_2E31)
private let labelInk = Color(argb: 0xFFB7_B7B7)
private let dimInk = Color(argb: 0xFF8A_8D8E)
private let settingsFill = Color(argb: 0xFF29_292B)
private let settingsGroup = Color(argb: 0xFF22_2427)
private let barHeight: CGFloat = 85
/// How bright the light can be taken: Min to Max on the exposure slider.
private let mostExposure: Float = 5
/// How far a touch goes before it is a drag, not a tap.
private let touchSlop: CGFloat = 8
/// Light Settings: its width, and its body's height (the arrow is more).
private let settingsWidth: CGFloat = 280
private let settingsHeight: CGFloat = 347

/// The hue bar's colours, evenly spaced along it (as the lights take them).
private let lightHueStops: [UInt32] = [0xFFFF_0000, 0xFFFF_6E00, 0xFFFF_DC00, 0xFF35_FF00, 0xFF00_FFEC, 0xFF00_34FF, 0xFFAA_00FF, 0xFFFF_0000]
private let saturationStart: UInt32 = 0xFF80_8080

/// A colour's red, green and blue, 0...1.
private func channels(_ argb: UInt32) -> [Float] {
    [16, 8, 0].map { Float((argb >> UInt32($0)) & 0xFF) / 255 }
}

private func mix(_ a: [Float], _ b: [Float], _ t: Float) -> [Float] {
    zip(a, b).map { $0 + ($1 - $0) * t }
}

private func color(_ c: [Float]) -> Color {
    Color(.sRGB, red: Double(c[0]), green: Double(c[1]), blue: Double(c[2]))
}

/// The hue bar's colour at `t` along it (0 to 1).
private func hueColor(_ t: Float) -> [Float] {
    let at = min(max(t, 0), 1) * Float(lightHueStops.count - 1)
    let i = min(Int(at), lightHueStops.count - 2)
    return mix(channels(lightHueStops[i]), channels(lightHueStops[i + 1]), at - Float(i))
}

/// A share as the light's sliders show it: None, a percentage, or Max.
private func level(_ v: Float) -> String {
    v <= 0 ? "None" : v >= 1 ? "Max" : "\(Int((v * 100).rounded()))%"
}

/// The Lighting Studio over a model: its bar (Cancel, Add Light, Environment and Done), the model
/// on its floor among its lights, seen through the studio's camera, which a finger takes round the
/// model and two fingers nearer; a light's cube is dragged across the view, and tapped, its
/// settings show. The Environment popover: whether the environment shows, the exposure, and which
/// environment lights the model.
struct LightingStudio: View {
    let model: CanvasModel
    let screen: CGSize

    @State private var environmentOpen = false
    /// The light whose settings are showing.
    @State private var lightOpen: Int?
    // the touch on the view: the light it grabbed, how far it has gone, and whether it only closes
    // what was open
    @State private var touching = false
    @State private var grabbed: Int?
    @State private var last = CGSize.zero
    @State private var travelled: CGFloat = 0
    @State private var closing = false
    @State private var pinch: CGFloat?

    private var scene: ModelScene { model.scene }

    private var view: some View {
        Color.clear
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { drag in
                        if !touching {
                            touching = true
                            last = .zero
                            travelled = 0
                            // a touch with a popover open only closes it
                            closing = environmentOpen || lightOpen != nil
                            environmentOpen = false
                            lightOpen = nil
                            grabbed = closing ? nil : scene.lightAt(drag.startLocation)
                        }
                        let moved = CGSize(width: drag.translation.width - last.width, height: drag.translation.height - last.height)
                        last = drag.translation
                        guard !closing, pinch == nil else { return }
                        // still for a tap until it goes past the slop
                        let before = travelled
                        travelled += abs(moved.width) + abs(moved.height)
                        if travelled > touchSlop, before.isFinite {
                            if let grabbed {
                                scene.moveLight(grabbed, by: moved)
                            } else {
                                scene.studioTurn(by: moved)
                            }
                        }
                    }
                    .onEnded { _ in
                        if !closing, let grabbed, travelled <= touchSlop {
                            lightOpen = grabbed
                        }
                        touching = false
                        grabbed = nil
                    }
            )
            .simultaneousGesture(
                // two fingers: nearer or farther, never a tap
                MagnifyGesture()
                    .onChanged { value in
                        if let pinch, pinch > 0 {
                            scene.studioZoom(value.magnification / pinch)
                        }
                        pinch = value.magnification
                        travelled = .infinity
                    }
                    .onEnded { _ in pinch = nil }
            )
    }

    private func pill(_ label: String, width: CGFloat, lit: Bool, action: @escaping () -> Void) -> some View {
        Text(verbatim: label)
            .font(.system(size: 17))
            .foregroundStyle(pillInk)
            .lineLimit(1)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Capsule().fill(lit ? pillLit : Color.clear))
            .padding(2)
            .frame(width: width, height: 43)
            .contentShape(Capsule())
            .onTapGesture(perform: action)
    }

    private var bar: some View {
        ZStack(alignment: .topLeading) {
            studioBar
                .contentShape(Rectangle())
                .onTapGesture { environmentOpen = false }
            Text(verbatim: "Lighting Studio")
                .font(.system(size: 20, weight: .bold))
                .foregroundStyle(studioTitle)
                .lineLimit(1)
                .offset(x: 32, y: 27)
            HStack(spacing: 13) {
                HStack(spacing: 0) {
                    pill("Cancel", width: 73, lit: false) { scene.closeStudio(keep: false) }
                    pill("Add Light", width: 92, lit: false) {
                        environmentOpen = false
                        lightOpen = nil
                        scene.addLight()
                    }
                    pill("Environment", width: 113, lit: environmentOpen) { environmentOpen.toggle() }
                }
                .frame(height: 43)
                .background(Capsule().fill(pillFill))
                .overlay(Capsule().strokeBorder(pillEdge, lineWidth: 1))
                .clipShape(Capsule())
                PcIconView(PcIcons.doneTick, size: 40, color: .white)
                    .frame(width: 44, height: 45)
                    .background(Capsule().fill(doneBlue))
                    .contentShape(Capsule())
                    .onTapGesture { scene.closeStudio(keep: true) }
            }
            .padding(.top, 21)
            .padding(.trailing, 20)
            .frame(width: screen.width, alignment: .trailing)
        }
        .frame(width: screen.width, height: barHeight)
    }

    var body: some View {
        ZStack(alignment: .topLeading) {
            view
            bar
            if environmentOpen {
                EnvironmentPopover(scene: scene)
                    .frame(maxHeight: max(screen.height - barHeight - 12, 0), alignment: .top)
                    .fixedSize(horizontal: true, vertical: false)
                    .offset(x: screen.width - 350 - 12, y: barHeight)
            }
            if let index = lightOpen, scene.lights.indices.contains(index), let at = scene.lightOnScreen(index) {
                lightSettings(index, at: at.at, reach: at.reach)
            }
        }
        .frame(width: screen.width, height: screen.height, alignment: .topLeading)
    }

    /// Light Settings, pointing at the light from above it (below it when there's no room).
    private func lightSettings(_ index: Int, at: CGPoint, reach: CGFloat) -> some View {
        let above = at.y - reach - Pc.arrowHeight - settingsHeight > barHeight + 8
        let left = min(max(at.x - settingsWidth / 2, 12), max(screen.width - settingsWidth - 12, 12))
        let top = above ? at.y - reach - Pc.arrowHeight - settingsHeight : at.y + reach
        return LightSettings(scene: scene, index: index, light: scene.lights[index]) { lightOpen = nil }
            .modifier(SettingsPopover(arrowX: min(max(at.x - left, 28), settingsWidth - 28), above: above))
            .offset(x: left, y: top)
    }
}

/// Light Settings' popover: its arrow at `arrowX`, under it when `above` the light.
private struct SettingsPopover: ViewModifier {
    let arrowX: CGFloat
    let above: Bool

    func body(content: Content) -> some View {
        Popover(width: settingsWidth, arrowX: arrowX, fill: settingsFill, side: above ? .bottom : .top) {
            content
        }
        .contentShape(Rectangle())
        .onTapGesture {}
    }
}

/// A light's hue, how much of that colour it has, how strong it is; Delete and Duplicate.
private struct LightSettings: View {
    let scene: ModelScene
    let index: Int
    let light: ModelLight
    let onDone: () -> Void

    private func with(hue: Float? = nil, saturation: Float? = nil, intensity: Float? = nil) -> ModelLight {
        ModelLight(
            x: light.x, y: light.y, z: light.z, hue: hue ?? light.hue,
            saturation: saturation ?? light.saturation, intensity: intensity ?? light.intensity
        )
    }

    private func row(_ label: String, action: @escaping () -> Void) -> some View {
        Text(verbatim: label)
            .font(.system(size: 17))
            .foregroundStyle(labelInk)
            .lineLimit(1)
            .padding(.leading, 16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .frame(height: 44)
            .contentShape(Rectangle())
            .onTapGesture {
                action()
                onDone()
            }
    }

    var body: some View {
        let hue = hueColor(light.hue)
        let h = CanvasModel.hsv(hue[0], hue[1], hue[2]).0
        let degrees = light.hue >= 1 ? 360 : Int((h * 360).rounded())
        let grey = channels(saturationStart)
        VStack(alignment: .leading, spacing: 0) {
            Text(verbatim: "Light Settings")
                .font(.system(size: 24, weight: .bold))
                .foregroundStyle(Pc.panelTitle)
                .lineLimit(1)
                .frame(height: 40)
                .padding(.leading, 16)
                .padding(.top, 5)
            VStack(spacing: 0) {
                VStack(spacing: 0) {
                    LightSlider(
                        label: "Hue", value: light.saturation <= 0 ? "None" : "\(degrees)°", share: light.hue,
                        track: LinearGradient(colors: lightHueStops.map { Color(argb: $0) }, startPoint: .leading, endPoint: .trailing),
                        knob: color(hue)
                    ) { scene.changeLight(index, with(hue: $0)) }
                    LightSlider(
                        label: "Saturation", value: level(light.saturation), share: light.saturation,
                        track: LinearGradient(colors: [color(grey), color(hue)], startPoint: .leading, endPoint: .trailing),
                        knob: color(mix(grey, hue, light.saturation))
                    ) { scene.changeLight(index, with(saturation: $0)) }
                    LightSlider(
                        label: "Intensity", value: level(light.intensity), share: light.intensity,
                        track: LinearGradient(colors: [.black, .white], startPoint: .leading, endPoint: .trailing),
                        knob: color([light.intensity, light.intensity, light.intensity])
                    ) { scene.changeLight(index, with(intensity: $0)) }
                }
                .padding(.vertical, 5)
                .background(RoundedRectangle(cornerRadius: 14, style: .circular).fill(settingsGroup))
                VStack(spacing: 0) {
                    row("Delete") { scene.deleteLight(index) }
                    rowLine.frame(height: 1)
                    row("Duplicate") { scene.duplicateLight(index) }
                }
                .background(RoundedRectangle(cornerRadius: 14, style: .circular).fill(settingsGroup))
                .clipShape(RoundedRectangle(cornerRadius: 14, style: .circular))
                .padding(.top, 10)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 10)
        }
        .frame(width: settingsWidth, alignment: .leading)
    }
}

/// One of a light's settings, 56 tall: its name and value over a thin track, whose capsule knob
/// takes the track's colour where it is. Dragged by as far as the finger goes.
private struct LightSlider: View {
    let label: String
    let value: String
    let share: Float
    let track: LinearGradient
    let knob: Color
    let onChange: (Float) -> Void

    @State private var from: Float?

    var body: some View {
        ZStack(alignment: .topLeading) {
            HStack(spacing: 0) {
                Text(verbatim: label).foregroundStyle(dimInk)
                Spacer(minLength: 0)
                Text(verbatim: value).foregroundStyle(Color.white)
            }
            .font(.system(size: 17))
            .lineLimit(1)
            .padding(.horizontal, 16)
            .padding(.top, 6)
            GeometryReader { place in
                let w = place.size.width
                let x = 18.5 + (w - 37) * CGFloat(min(max(share, 0), 1))
                ZStack(alignment: .leading) {
                    Capsule().fill(track).frame(width: max(w - 28, 0), height: 3).offset(x: 14)
                    Capsule().fill(knob).frame(width: 16, height: 10).offset(x: x - 8)
                }
                .frame(width: w, height: 24)
                .contentShape(Rectangle())
                .gesture(
                    DragGesture(minimumDistance: 0)
                        .onChanged { drag in
                            let start = from ?? share
                            from = start
                            onChange(min(max(start + Float(drag.translation.width / max(w - 37, 1)), 0), 1))
                        }
                        .onEnded { _ in from = nil }
                )
            }
            .frame(height: 24)
            .offset(y: 32)
        }
        .frame(height: 56, alignment: .topLeading)
    }
}

/// The Environment popover: Show Environment, Exposure, and the environments, the one lighting the
/// model blue.
private struct EnvironmentPopover: View {
    let scene: ModelScene

    var body: some View {
        Popover(width: 350, arrowX: 0, fill: popoverFill, side: .none) {
            VStack(alignment: .leading, spacing: 0) {
                Text(verbatim: "Environment")
                    .font(.system(size: 24, weight: .bold))
                    .foregroundStyle(Pc.panelTitle)
                    .lineLimit(1)
                    .frame(height: 40)
                    .padding(.leading, 26)
                    .padding(.top, 8)
                VStack(spacing: 0) {
                    HStack(spacing: 0) {
                        Text(verbatim: "Show Environment")
                            .font(.system(size: 17))
                            .foregroundStyle(labelInk)
                            .lineLimit(1)
                        Spacer(minLength: 0)
                        PcSwitch(on: scene.environmentShown)
                    }
                    .padding(.leading, 16)
                    .padding(.trailing, 8)
                    .frame(height: 44)
                    .background(RoundedRectangle(cornerRadius: 14, style: .circular).fill(groupFill))
                    .contentShape(Rectangle())
                    .onTapGesture { scene.setEnvironmentShown(!scene.environmentShown) }
                    ExposureSlider(value: scene.exposure) { scene.setExposure($0) }
                        .padding(.top, 10)
                    ScrollView {
                        VStack(spacing: 0) {
                            ForEach(Array(modelEnvironments.enumerated()), id: \.offset) { i, env in
                                if i > 0 {
                                    rowLine.frame(height: 1)
                                }
                                let chosen = env.asset == scene.environment
                                HStack(spacing: 0) {
                                    Text(verbatim: env.label)
                                        .font(.system(size: 16))
                                        .foregroundStyle(chosen ? Color.white : labelInk)
                                        .lineLimit(1)
                                    Spacer(minLength: 0)
                                    PcIconView(env.icon, size: 20, color: .white)
                                }
                                .padding(.leading, 11)
                                .padding(.trailing, 16)
                                .frame(height: 44)
                                .background(chosen ? Pc.accent : Color.clear)
                                .contentShape(Rectangle())
                                .onTapGesture {
                                    if !chosen {
                                        scene.chooseEnvironment(env.asset)
                                    }
                                }
                            }
                        }
                    }
                    .scrollIndicators(.hidden)
                    // as tall as the list, and less where the window has less room
                    .frame(maxHeight: CGFloat(modelEnvironments.count) * 45 - 1)
                    .background(groupFill)
                    .clipShape(RoundedRectangle(cornerRadius: 14, style: .circular))
                    .padding(.top, 12)
                }
                .padding(.horizontal, 10)
                .padding(.top, 13)
                .padding(.bottom, 12)
            }
        }
        .contentShape(Rectangle())
        .onTapGesture {}
    }
}

/// Exposure, from Min (none) to Max (5 times): its value on the right and a black-to-white track
/// whose knob is filled with the grey the exposure makes (white from 100% up). Dragging moves it
/// by as far as the finger goes.
private struct ExposureSlider: View {
    let value: Float
    let onChange: (Float) -> Void

    @State private var from: Float?

    var body: some View {
        let shown = value <= 0 ? "Min" : value >= mostExposure ? "Max" : "\(Int((value * 100).rounded()))%"
        let grey = Double(min(max(value, 0), 1))
        ZStack(alignment: .topLeading) {
            HStack(spacing: 0) {
                Text(verbatim: "Exposure").foregroundStyle(dimInk)
                Spacer(minLength: 0)
                Text(verbatim: shown).foregroundStyle(Color.white)
            }
            .font(.system(size: 17))
            .lineLimit(1)
            .padding(.horizontal, 16)
            .padding(.top, 19)
            GeometryReader { place in
                let w = place.size.width
                let x = 18.5 + (w - 37) * CGFloat(min(max(value / mostExposure, 0), 1))
                ZStack(alignment: .leading) {
                    Capsule()
                        .fill(LinearGradient(colors: [.black, .white], startPoint: .leading, endPoint: .trailing))
                        .frame(width: max(w - 32, 0), height: 2)
                        .offset(x: 16)
                    Capsule()
                        .fill(Color(.sRGB, red: grey, green: grey, blue: grey))
                        .frame(width: 16, height: 9)
                        .offset(x: x - 8)
                }
                .frame(width: w, height: 30)
                .contentShape(Rectangle())
                .gesture(
                    DragGesture(minimumDistance: 0)
                        .onChanged { drag in
                            let start = from ?? value
                            from = start
                            let moved = Float(drag.translation.width / max(w - 37, 1)) * mostExposure
                            onChange(min(max(start + moved, 0), mostExposure))
                        }
                        .onEnded { _ in from = nil }
                )
            }
            .frame(height: 30)
            .offset(y: 84 - 13 - 30)
        }
        .frame(height: 84, alignment: .topLeading)
        .background(RoundedRectangle(cornerRadius: 14, style: .circular).fill(groupFill))
    }
}

/// While a picture is being placed on a model: the box round it, kept up with its place on the
/// window, and a bar to flip it, turn it, put it back, and keep it or take it away. A touch on the
/// picture moves it and two fingers on it scale and turn it; off it, a finger turns the model.
struct ModelPictureOverlay: View {
    let model: CanvasModel

    var body: some View {
        let scene = model.scene
        ZStack(alignment: .bottom) {
            TimelineView(.animation) { _ in
                let corners = scene.pictureCorners()
                Canvas { context, _ in
                    guard corners.count > 1 else { return }
                    var path = Path()
                    path.addLines(corners)
                    path.closeSubpath()
                    context.stroke(path, with: .color(Color.black.opacity(0.5)), lineWidth: 4)
                    context.stroke(path, with: .color(.white), lineWidth: 2)
                    for c in corners {
                        context.fill(Path(ellipseIn: CGRect(x: c.x - 6, y: c.y - 6, width: 12, height: 12)), with: .color(.white))
                    }
                }
            }
            .allowsHitTesting(false)
            HStack(spacing: 2) {
                ToolbarButton(icon: PcIcons.flipHorizontal, label: "Flip Horizontal", width: 88, iconSize: 50) {
                    scene.flipPicture(horizontal: true)
                }
                ToolbarButton(icon: PcIcons.flipVertical, label: "Flip Vertical", width: 88, iconSize: 50) {
                    scene.flipPicture(horizontal: false)
                }
                ToolbarButton(icon: PcIcons.rotate45, label: "Rotate 45°", width: 88, iconSize: 44) { scene.turnPicture() }
                ToolbarButton(icon: PcIcons.transformReset, label: "Reset", width: 88, iconSize: 44) { scene.resetPicture() }
                ToolbarButton(icon: PcIcons.cross, label: "Cancel", width: 88, iconSize: 28) { scene.endPicture(keep: false) }
                ToolbarButton(icon: PcIcons.tick, label: "Done", width: 88, iconSize: 28) { scene.endPicture(keep: true) }
            }
            .padding(.horizontal, 8)
            .background(RoundedRectangle(cornerRadius: 12, style: .circular).fill(Pc.panel))
            .padding(.bottom, 24)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
