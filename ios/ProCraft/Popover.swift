import SwiftUI

/// The side of a popover its arrow sticks out of, pointing at its button; or none.
enum ArrowSide {
    case top, bottom, left, right, none
}

/// A popover's outline: its rounded body and the arrow out of its `side`, the arrow's tip `arrowX`
/// from the body's left edge (from its top, for an arrow on the left or right).
struct PopoverShape: Shape {
    var side: ArrowSide
    var arrowX: CGFloat
    var corner: CGFloat

    func path(in rect: CGRect) -> Path {
        let arrowH = Pc.arrowHeight
        let half = arrowH
        let tip: CGFloat = 2
        let at = arrowX
        let w = rect.width
        let h = rect.height
        var path = Path()
        func body(_ r: CGRect) {
            path.addRoundedRect(in: r, cornerSize: CGSize(width: corner, height: corner), style: .circular)
        }
        switch side {
        case .top:
            body(CGRect(x: 0, y: arrowH, width: w, height: h - arrowH))
            path.move(to: CGPoint(x: at - half - tip, y: arrowH + 0.5))
            path.addLine(to: CGPoint(x: at - tip, y: tip))
            path.addQuadCurve(to: CGPoint(x: at + tip, y: tip), control: CGPoint(x: at, y: 0))
            path.addLine(to: CGPoint(x: at + half + tip, y: arrowH + 0.5))
            path.closeSubpath()
        case .bottom:
            body(CGRect(x: 0, y: 0, width: w, height: h - arrowH))
            path.move(to: CGPoint(x: at - half - tip, y: h - arrowH - 0.5))
            path.addLine(to: CGPoint(x: at - tip, y: h - tip))
            path.addQuadCurve(to: CGPoint(x: at + tip, y: h - tip), control: CGPoint(x: at, y: h))
            path.addLine(to: CGPoint(x: at + half + tip, y: h - arrowH - 0.5))
            path.closeSubpath()
        case .left:
            body(CGRect(x: arrowH, y: 0, width: w - arrowH, height: h))
            path.move(to: CGPoint(x: arrowH + 0.5, y: at - half - tip))
            path.addLine(to: CGPoint(x: tip, y: at - tip))
            path.addQuadCurve(to: CGPoint(x: tip, y: at + tip), control: CGPoint(x: 0, y: at))
            path.addLine(to: CGPoint(x: arrowH + 0.5, y: at + half + tip))
            path.closeSubpath()
        case .right:
            body(CGRect(x: 0, y: 0, width: w - arrowH, height: h))
            path.move(to: CGPoint(x: w - arrowH - 0.5, y: at - half - tip))
            path.addLine(to: CGPoint(x: w - tip, y: at - tip))
            path.addQuadCurve(to: CGPoint(x: w - tip, y: at + tip), control: CGPoint(x: w, y: at))
            path.addLine(to: CGPoint(x: w - arrowH - 0.5, y: at + half + tip))
            path.closeSubpath()
        case .none:
            body(rect)
        }
        return path
    }
}

/// Where the panel opening now hangs from a button Edit layout moved: the side its arrow is on, and
/// how far along that side its tip is. The panel's own popover takes it in place of an arrow up from
/// under the top bar; nothing that opens inside it does.
struct Hang {
    let side: ArrowSide
    let arrow: CGFloat
}

private struct HangKey: EnvironmentKey {
    static let defaultValue: Hang? = nil
}

extension EnvironmentValues {
    /// The hang of the panel opening now, while Edit layout moved its button.
    var hang: Hang? {
        get { self[HangKey.self] }
        set { self[HangKey.self] = newValue }
    }
}

/// A panel floating in a window of its own (every panel may, while a plugin lets it): what its
/// strip and corner do, and whether it has a corner. The panel's own popover draws as that window,
/// with no arrow and a strip over it; nothing that opens inside it does.
struct FloatWindow {
    let frame: FloatingFrame
    let sizable: Bool
}

/// The panels lie on solid ground, not frosted: while a plugin lays them out (see
/// `Pc.popoverSolid`).
private struct PanelsSolidKey: EnvironmentKey {
    static let defaultValue = false
}

extension EnvironmentValues {
    var panelsSolid: Bool {
        get { self[PanelsSolidKey.self] }
        set { self[PanelsSolidKey.self] = newValue }
    }
}

private struct FloatWindowKey: EnvironmentKey {
    static let defaultValue: FloatWindow? = nil
}

extension EnvironmentValues {
    /// The window the panel showing now floats in, if it does.
    var floatWindow: FloatWindow? {
        get { self[FloatWindowKey.self] }
        set { self[FloatWindowKey.self] = newValue }
    }
}

/// A panel hanging from the top bar, with an arrow pointing up at its button: `arrowX` is the
/// tip's distance from the panel's left edge (from its top, for an arrow on the `side`).
/// `popoverPlace` finds where it goes. Without a `fill` of its own it is frosted over the canvas:
/// translucent, with what is behind it blurred (solid while the panels float: `panelsSolid`).
/// `width` is the body's: a side arrow adds to it.
/// Hanging from a button Edit layout moved (`hang`), an arrow that would point up points at it.
struct Popover<Content: View>: View {
    let width: CGFloat
    let arrowX: CGFloat
    var fill: Color?
    var corner: CGFloat = Pc.popoverCorner
    var side: ArrowSide = .top
    @ViewBuilder let content: () -> Content
    @Environment(\.hang) private var hang
    @Environment(\.floatWindow) private var floatWindow
    @Environment(\.panelsSolid) private var solid

    var body: some View {
        if side == .top, let window = floatWindow {
            floating(window)
        } else {
            hanging
        }
    }

    /// Floating: as big as its window, its strip over it and the room for its corner under it.
    private func floating(_ window: FloatWindow) -> some View {
        let shape = RoundedRectangle(cornerRadius: corner, style: .circular)
        return VStack(spacing: 0) {
            FloatingTopStrip(frame: window.frame)
            VStack(spacing: 0) {
                content()
            }
            .frame(maxHeight: .infinity, alignment: .top)
            if window.sizable {
                FloatingBottomStrip()
            }
        }
        .environment(\.floatWindow, nil)
        .environment(\.hang, nil)
        .frame(width: window.frame.width, height: window.frame.height)
        .clipShape(shape)
        .background {
            ZStack {
                shape.fill(Color.black)
                    .shadow(color: Pc.popoverShadow, radius: Pc.popoverShadowRadius / 2)
                if let fill {
                    shape.fill(fill)
                } else if solid {
                    // over another window or a bar, which would show through sharp
                    shape.fill(Pc.popoverSolid)
                } else {
                    shape.fill(.ultraThinMaterial)
                        .environment(\.colorScheme, Look.shared.light ? .light : .dark)
                    shape.fill(Pc.popoverFrost)
                }
            }
        }
    }

    @ViewBuilder private var hanging: some View {
        let moved = side == .top ? hang : nil
        let side = moved?.side ?? self.side
        let arrowX = moved?.arrow ?? self.arrowX
        let shape = PopoverShape(side: side, arrowX: arrowX, corner: corner)
        let sideways = side == .left || side == .right
        let shadow = Pc.popoverShadow
        VStack(spacing: 0) {
            content()
        }
        .environment(\.hang, nil)
        .clipShape(RoundedRectangle(cornerRadius: corner, style: .circular))
        .padding(.top, side == .top ? Pc.arrowHeight : 0)
        .padding(.bottom, side == .bottom ? Pc.arrowHeight : 0)
        .padding(.leading, side == .left ? Pc.arrowHeight : 0)
        .padding(.trailing, side == .right ? Pc.arrowHeight : 0)
        .frame(width: sideways ? width + Pc.arrowHeight : width)
        .background {
            ZStack {
                // the shadow only around the panel: none of it shows through
                shape.fill(Color.black)
                    .shadow(color: shadow, radius: Pc.popoverShadowRadius / 2)
                    .mask {
                        Rectangle()
                            .padding(-Pc.popoverShadowRadius * 2)
                            .overlay(shape.blendMode(.destinationOut))
                            .compositingGroup()
                    }
                if let fill {
                    shape.fill(fill)
                } else if solid {
                    // a floating window may lie under it, which would show through sharp
                    shape.fill(Pc.popoverSolid)
                } else {
                    // the dark frost whatever the system's appearance: a light one shows through
                    // the tint lighter than measured
                    shape.fill(.ultraThinMaterial)
                        .environment(\.colorScheme, Look.shared.light ? .light : .dark)
                    shape.fill(Pc.popoverFrost)
                }
            }
        }
    }
}

/// Where a popover goes: its left edge and its arrow's tip from that edge.
struct PopoverPlace {
    let left: CGFloat
    let arrowX: CGFloat
}

/// A popover `width` wide is centred on its button (the tip sits `Pc.arrowShift` right of the
/// button's centre) and kept `Pc.popoverGap` inside a screen `screenWidth` wide, and at least
/// `minLeft` from its left edge (clear of the side bar).
func popoverPlace(
    width: CGFloat, buttonCentreX: CGFloat, screenWidth: CGFloat,
    minLeft: CGFloat = Pc.popoverGap, minRight: CGFloat = Pc.popoverGap
) -> PopoverPlace {
    let tip = buttonCentreX + Pc.arrowShift
    let left = min(max(tip - width / 2, minLeft), max(screenWidth - minRight - width, minLeft))
    return PopoverPlace(left: left, arrowX: tip - left)
}
