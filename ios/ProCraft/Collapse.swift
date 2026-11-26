import SwiftUI

/// Collapse to edge: a bar or a window sliding into a side of the window and back out (0.2 seconds,
/// easing out either way), the slim tab left on the side, and the button that sends it in. The core
/// says where each goes and what is collapsed; this draws it and hears the taps.
let collapseSlide = Animation.easeOut(duration: 0.2)

/// How far `place` says something collapsing moves to be all the way into its side, while it is
/// collapsed (`collapsed`); nothing while it shows.
func collapseOffset(_ place: EdgeCollapse?, _ collapsed: Bool) -> CGSize {
    guard collapsed, let place else { return .zero }
    return CGSize(width: CGFloat(place.slideX), height: CGFloat(place.slideY))
}

/// A chevron in `rect`'s middle, `size` across, pointing into `edge`.
private func chevron(_ rect: CGRect, _ edge: WindowEdge, size: CGFloat) -> Path {
    let (along, out): (CGPoint, CGPoint) = switch edge {
    case .left: (CGPoint(x: 0, y: 1), CGPoint(x: -1, y: 0))
    case .right: (CGPoint(x: 0, y: 1), CGPoint(x: 1, y: 0))
    case .top: (CGPoint(x: 1, y: 0), CGPoint(x: 0, y: -1))
    case .bottom: (CGPoint(x: 1, y: 0), CGPoint(x: 0, y: 1))
    }
    let half = size / 2
    let c = CGPoint(x: rect.midX, y: rect.midY)
    let tip = CGPoint(x: c.x + out.x * half * 0.5, y: c.y + out.y * half * 0.5)
    var path = Path()
    path.move(to: CGPoint(x: tip.x - out.x * half + along.x * half, y: tip.y - out.y * half + along.y * half))
    path.addLine(to: tip)
    path.addLine(to: CGPoint(x: tip.x - out.x * half - along.x * half, y: tip.y - out.y * half - along.y * half))
    return path
}

/// A window strip's collapse button: a disc with a chevron pointing into `edge`, as Fold and Close
/// are drawn.
struct FloatingCollapse: View {
    let edge: WindowEdge
    let onCollapse: () -> Void
    var height: CGFloat = 40

    var body: some View {
        Canvas { context, size in
            let o = CGPoint(x: size.width / 2, y: size.height / 2)
            let r: CGFloat = 8.5
            context.fill(Path(ellipseIn: CGRect(x: o.x - r, y: o.y - r, width: r * 2, height: r * 2)), with: .color(.white.opacity(0.15)))
            let at = CGRect(x: o.x - 7, y: o.y - 7, width: 14, height: 14)
            context.stroke(chevron(at, edge, size: 7), with: .color(.black.opacity(0.3)), lineWidth: 1.6)
        }
        .frame(width: 40, height: height)
        .contentShape(Rectangle())
        .onTapGesture(perform: onCollapse)
        .accessibilityLabel("Collapse")
    }
}

/// Collapse to edge's buttons and tabs, over the bars and windows: a button by each bar against a
/// side (none while Edit Layout moves them), sliding it in; and the tab on the side of each bar and
/// window gone in, a tap or a drag out of the side bringing it back.
struct CollapseControls: View {
    let model: CanvasModel
    let screen: CGSize

    var body: some View {
        ZStack(alignment: .topLeading) {
            if model.layoutLook.collapse {
                let belt = model.beltShown() != nil
                ForEach([LayoutBar.topLeft, .topRight, .sideBar, .belt], id: \.self) { bar in
                    if (bar != .belt || belt), (bar != .sideBar || model.showSideBar) {
                        if let collapsed = model.barCollapsed(bar) {
                            CollapsedTab(collapse: collapsed, onTap: { model.collapseBar(bar, false) }) { by in
                                model.pullBarTab(bar, by: by)
                            }
                        } else if !model.editingLayout, let at = model.barCollapseButton(bar), let place = model.barCollapsePlace(bar) {
                            let r = CGRect(x: CGFloat(at.x), y: CGFloat(at.y), width: CGFloat(at.width), height: CGFloat(at.height))
                            ZStack {
                                Circle().fill(Pc.bar)
                                chevron(CGRect(origin: .zero, size: r.size), place.edge, size: r.width * 0.4)
                                    .stroke(Pc.barGlyph, lineWidth: 1.6)
                            }
                            .frame(width: r.width, height: r.height)
                            .contentShape(Circle())
                            .onTapGesture { model.collapseBar(bar, true) }
                            .accessibilityLabel("Collapse")
                            .offset(x: r.minX, y: r.minY)
                        }
                    }
                }
                ForEach([LayoutPiece.layers, .brushes, .actions, .adjustments], id: \.self) { piece in
                    if let collapsed = model.pieceCollapsed(piece) {
                        CollapsedTab(collapse: collapsed, onTap: { model.collapsePiece(piece, false) }) { by in
                            model.pullPieceTab(piece, by: by)
                        }
                    }
                }
            }
        }
        .frame(width: screen.width, height: screen.height, alignment: .topLeading)
    }
}

/// The tab of what went into a side (`collapse`): a slim pill with a grip along it, fading in as it
/// is left there.
private struct CollapsedTab: View {
    let collapse: EdgeCollapse
    let onTap: () -> Void
    let onPull: (CGSize) -> Void
    @State private var shown = false

    var body: some View {
        let r = collapse.tab
        let (w, h) = (CGFloat(r.width), CGFloat(r.height))
        Capsule()
            .fill(Pc.bar)
            .overlay {
                // the grip: a short line along it
                Capsule()
                    .fill(Pc.barGlyph)
                    .frame(width: h > w ? 2 : w * 0.5, height: h > w ? h * 0.5 : 2)
            }
            .frame(width: w, height: h)
            .contentShape(Rectangle())
            .opacity(shown ? 1 : 0)
            .onAppear { withAnimation(collapseSlide) { shown = true } }
            .onTapGesture(perform: onTap)
            .gesture(
                DragGesture(minimumDistance: 1).onChanged { drag in
                    // (in the interface's points, as the layout has them)
                    let k = Interface.shared.scale
                    onPull(CGSize(width: drag.translation.width / k, height: drag.translation.height / k))
                }
            )
            .accessibilityLabel("Collapsed")
            .offset(x: CGFloat(r.x), y: CGFloat(r.y))
    }
}
