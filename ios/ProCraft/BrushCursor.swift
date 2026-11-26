import QuartzCore
import UIKit

/// The brush's outline where the Pencil (or the pointer) hovers over the canvas, or where it
/// paints: the brush's own shape at the size it paints on the screen, turned as a stamp laid there
/// would be, as the core gives it, drawn as a thin dark line edged in white, or in the paint
/// colour, as the brush says. Hovering, the brush's stamp fills it in the colour if its Hover fill
/// says so, as the brush would paint at its estimated pressure; a brush whose own outline is none
/// shows the light and dark rings unless that fill shows, so the cursor never goes missing.
final class BrushCursorView: UIView {
    /// A hover fill drawn at least this strong (the brush's opacity) shows by itself.
    static let fillSeen: CGFloat = 0.1
    /// A hover fill whose stamp is nowhere stronger than this (of 255) is too faint to see.
    static let fillFaintest: UInt8 = 8
    /// The hover fill's stamp is made at most this many pixels across (larger, it is drawn bigger).
    static let fillMost = 256

    private let fill = CALayer()
    private let under = CAShapeLayer()
    private let over = CAShapeLayer()
    /// What was last read: the brush's radius on the screen (points), how the brush draws its
    /// outline (`cursor_outline`: none, rings, the colour), its loops, the hover fill's look, the
    /// opacity and colour painted with, and when.
    private var brushRadius: CGFloat = 0
    private var outline = 1
    private var loops: [[Float]] = []
    private var fillLook: HoverFill?
    private var opacity: CGFloat = 1
    private var colour: [Float] = []
    private var readAt: CFTimeInterval = 0
    /// The radius the loops are drawn at (points), and how they are drawn (0 not at all, 1 the
    /// rings, 2 the colour).
    private var radius: CGFloat = 0
    private var style = 1
    /// Whether the fill has a picture to show.
    private var filled = false
    /// The stamp the fill was made from, its side and colour, to make it again only when one
    /// changes.
    private var fillMade: (side: Int, colour: [Float], stamp: [UInt8])?

    override init(frame: CGRect) {
        super.init(frame: frame)
        isUserInteractionEnabled = false
        isOpaque = false
        backgroundColor = .clear
        isHidden = true
        layer.addSublayer(fill)
        for line in [under, over] {
            line.fillColor = nil
            line.lineJoin = .round
            layer.addSublayer(line)
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("the cursor is made in code")
    }

    /// Shows the outline at `point` (points in this view), `alpha` strong, shaped for the current
    /// brush as `session` has it (`pixels` window pixels a point), drawn as `drawn` says, turned
    /// `turn` radians clockwise past the brush's own rotation (the core's `cursorHovers`, or
    /// `cursorTurn` while painting). Hovering, `scale` of the brush's size, its stamp filling it
    /// if the brush says so; painting (`painted` set), that radius (points), or the brush's size
    /// before the stroke has a stamp.
    func show(
        at point: CGPoint, alpha: CGFloat, scale: CGFloat = 1, painting: Bool = false, painted: CGFloat? = nil,
        session: CanvasSession, pixels: CGFloat, drawn: CursorStyle = .brush, turn: CGFloat = 0
    ) {
        let now = CACurrentMediaTime()
        // read as it appears, and again now and then: the brush or its size may change meanwhile
        let reread = isHidden || now - readAt >= 0.1
        if reread {
            readAt = now
            let info = session.info()
            brushRadius = CGFloat(info.brushDiameter * info.zoom / 2) / pixels
            opacity = CGFloat(info.opacity)
            outline = Int(session.currentBrushSetting(key: "cursor_outline").rounded())
            colour = session.color()
            loops = session.brushOutline()
            // the fill only while hovering
            fillLook = painting ? nil : session.hoverFill()
            filled = fillLook != nil && refill(session, radius: painted ?? brushRadius * scale)
        }
        let radiusNow = painted ?? brushRadius * scale
        let look = painting ? nil : fillLook
        let strength = min(max(opacity, 0), 1) * CGFloat(min(max(look?.opacity ?? 0, 0), 1))
        let fillR = radiusNow * CGFloat(min(max(look?.size ?? 0, 0), 1))
        let fillShown = look != nil && filled && fillR >= 1
        let lines = Self.lines(drawn, brushOutline: outline, filled: fillShown && strength >= Self.fillSeen)
        if lines != style || reread {
            style = lines
            restyle(pixels: pixels)
        }
        if abs(radiusNow - radius) >= 0.25 || reread {
            radius = radiusNow
            reshape()
        }
        // too small to see, or nothing to draw
        guard radius >= 2, style != 0 || fillShown, alpha > 0 else {
            isHidden = true
            return
        }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        let turned = CGAffineTransform(rotationAngle: turn)
        for line in [under, over] {
            line.isHidden = style == 0
            line.position = point
            line.setAffineTransform(turned)
        }
        fill.isHidden = !fillShown
        if fillShown {
            fill.setAffineTransform(.identity)
            fill.bounds = CGRect(x: 0, y: 0, width: 2 * fillR, height: 2 * fillR)
            fill.position = point
            fill.setAffineTransform(turned)
            fill.opacity = Float(strength)
        }
        self.alpha = min(alpha, 1)
        isHidden = false
        CATransaction.commit()
    }

    func hide() {
        isHidden = true
    }

    /// How the outline's lines are drawn: as chosen, or as the brush says (0 none, 1 the light and
    /// dark rings, 2 the colour); a brush that says none shows the rings unless its fill does.
    static func lines(_ drawn: CursorStyle, brushOutline: Int, filled: Bool) -> Int {
        switch drawn {
        case .contrast:
            return 1
        case .color:
            return 2
        case .brush:
            switch brushOutline {
            case 0:
                return filled ? 0 : 1
            case 2:
                return 2
            default:
                return 1
            }
        }
    }

    /// The brush's stamp in the colour, as the fill's picture, at the cursor's size on the screen
    /// (`radius` points); made again only when it changes. False for a stamp too faint to see.
    private func refill(_ session: CanvasSession, radius: CGFloat) -> Bool {
        let side = min(max(Int((radius * 2 * contentScaleFactor).rounded()), 2), Self.fillMost)
        let stamp = [UInt8](session.brushStamp(side: UInt32(side)))
        guard stamp.count >= side * side, stamp.contains(where: { $0 >= Self.fillFaintest }) else {
            fill.contents = nil
            fillMade = nil
            return false
        }
        if let made = fillMade, made.side == side, made.colour == colour, made.stamp == stamp, fill.contents != nil {
            return true
        }
        fillMade = (side, colour, stamp)
        let colour = self.colour
        let channel = { (i: Int) -> Float in colour.count > i ? min(max(colour[i], 0), 1) : 0 }
        let (red, green, blue) = (channel(0), channel(1), channel(2))
        var rgba = [UInt8](repeating: 0, count: side * side * 4)
        for i in 0 ..< side * side {
            let a = Float(stamp[i]) / 255
            rgba[i * 4] = UInt8((red * a * 255).rounded())
            rgba[i * 4 + 1] = UInt8((green * a * 255).rounded())
            rgba[i * 4 + 2] = UInt8((blue * a * 255).rounded())
            rgba[i * 4 + 3] = stamp[i]
        }
        guard let provider = CGDataProvider(data: Data(rgba) as CFData),
              let image = CGImage(
                  width: side, height: side, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: side * 4,
                  space: CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB(),
                  bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                  provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent
              )
        else {
            fill.contents = nil
            return false
        }
        fill.contents = image
        return true
    }

    /// Contrasting rings (white under black), or the colour over a light edge.
    private func restyle(pixels: CGFloat) {
        under.contentsScale = pixels
        over.contentsScale = pixels
        under.strokeColor = UIColor(white: 1, alpha: style == 2 ? 0.6 : 1).cgColor
        under.lineWidth = 3
        if style == 2, colour.count >= 3 {
            over.strokeColor = UIColor(red: CGFloat(colour[0]), green: CGFloat(colour[1]), blue: CGFloat(colour[2]), alpha: 1).cgColor
            over.lineWidth = 1.5
        } else {
            over.strokeColor = UIColor.black.cgColor
            over.lineWidth = 1
        }
    }

    /// The brush's own loops (x, y pairs in brush diameters about its middle), or a circle for the
    /// round tip, about the layers' position.
    private func reshape() {
        let path = CGMutablePath()
        let across = 2 * radius
        let drawn = loops.filter { $0.count >= 6 }
        if drawn.isEmpty {
            path.addEllipse(in: CGRect(x: -radius, y: -radius, width: across, height: across))
        }
        for loop in drawn {
            path.move(to: CGPoint(x: CGFloat(loop[0]) * across, y: CGFloat(loop[1]) * across))
            for i in stride(from: 2, to: loop.count - 1, by: 2) {
                path.addLine(to: CGPoint(x: CGFloat(loop[i]) * across, y: CGFloat(loop[i + 1]) * across))
            }
            path.closeSubpath()
        }
        under.path = path
        over.path = path
    }
}
