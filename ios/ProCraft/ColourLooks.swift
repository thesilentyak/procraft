import SwiftUI

/// What the plugins on change in the colour panel: Classic's square drawn by a picker plugin, or a
/// triangle in its place, with their menus at its corner; sliders stepping one unit at a tap on
/// either end; the colour as the panel opened beside the colour now; the floating window as
/// Classic's picker alone. Read again whenever the plugins change.
@Observable
final class ColourLooks {
    static let shared = ColourLooks()

    /// A plugin has Classic show a triangle (the pure hue, white and black) in place of its square.
    private(set) var triangle = false
    /// A plugin has the colour panel's sliders step one unit at a tap on either end.
    private(set) var steps = false
    /// A plugin has the colour swatch show the colour as the panel opened beside the colour now.
    private(set) var before = false
    /// A picker plugin draws Classic's square its own way: its picture, where touches land.
    private(set) var pickerOn = false
    /// A plugin has the floating colour window show Classic's picker alone.
    private(set) var floatingPickerOnly = false
    /// The plugins' choices at Classic's corner, while any has.
    private(set) var classicChoices: [PluginChoice] = []
    /// Goes up with each change, for a picker plugin's picture to be drawn again.
    private(set) var version = 0

    private init() {}

    /// Classic's square is a plugin's: a picker plugin's or the triangle.
    var classicReplaced: Bool {
        triangle || pickerOn
    }

    func follow(_ store: Plugins) {
        triangle = store.colourTriangle()
        steps = store.colourSteps()
        before = store.colourBefore()
        pickerOn = store.pickerOn()
        floatingPickerOnly = store.floatingPickerOnly()
        classicChoices = store.menuChoices(place: .classic)
        version += 1
    }
}
