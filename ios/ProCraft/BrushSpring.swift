import Foundation
#if canImport(CoreGraphics)
import CoreGraphics
#endif

/// How long a dragged brush rests on another set's row before that set opens, to be let go among
/// its brushes, seconds (measured on the original: 1.76 s, from frames 0.1 s apart; the Android
/// and desktop apps wait as long).
let brushSpringSeconds: TimeInterval = 1.8

/// The set a dragged brush at `at` would open, resting there: the set whose row of `sets` (by
/// place, in the same space) it is over, unless that set shows already (`shown`) or is the Recent
/// set (its place below 0), which holds no brushes of its own.
func brushSpringSet(at: CGPoint, sets: [Int: CGRect], shown: Int) -> Int? {
    guard let set = sets.first(where: { $0.value.contains(at) })?.key, set >= 0, set != shown else {
        return nil
    }
    return set
}

/// A dragged brush resting on a set's row: held over it `seconds` (still or moving along it), that
/// set opens, the drag going on; moved off it first, or let go, nothing opens, and back on it the
/// wait starts again.
final class BrushSpring {
    let seconds: TimeInterval
    /// The set it rests on, waiting to open.
    private(set) var restingOn: Int?
    private var rest: DispatchWorkItem?

    init(seconds: TimeInterval = brushSpringSeconds) {
        self.seconds = seconds
    }

    /// The dragged brush is over `set` now (none: over no set that would open). On the set it rests
    /// on already it waits on; on another it waits there from now, and `open` hears that set when
    /// the wait is over.
    func over(_ set: Int?, open: @escaping (Int) -> Void) {
        if set == restingOn {
            return
        }
        stop()
        guard let set else { return }
        restingOn = set
        let work = DispatchWorkItem { [weak self] in
            guard let self, self.restingOn == set else { return }
            self.stop()
            open(set)
        }
        rest = work
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds, execute: work)
    }

    /// Nothing opens: the drag is over, or off the sets.
    func stop() {
        rest?.cancel()
        rest = nil
        restingOn = nil
    }
}
