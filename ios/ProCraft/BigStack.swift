import Foundation

/// Runs `body` on a thread of its own with an 8 MB stack and returns what it made, the caller
/// waiting for it. Making, opening or bringing in an artwork goes deep into the core, deeper than
/// the 512 KB a dispatch queue's or a task's thread has; called from a queue, the queue's order
/// (one artwork opened at a time, imports one after another) holds as before. What the thread was
/// handed is let go as it ends, and what it made is the caller's alone: the thread object may
/// outlive the call (the system lets it go once the thread has ended), and an artwork's session
/// held by it would stay alive, all its memory with it, after the artwork is left.
func onBigStack<T>(qos: QualityOfService = .userInitiated, _ body: @escaping () -> T) -> T {
    let run = BigStackRun(body)
    let done = DispatchSemaphore(value: 0)
    let thread = Thread {
        run.run()
        done.signal()
    }
    thread.stackSize = 8 << 20
    thread.qualityOfService = qos
    thread.start()
    done.wait()
    // made before the signal
    return run.take()
}

/// What `onBigStack` hands its thread and gets back: touched by one thread at a time, the caller
/// waiting while the thread runs. Emptied as it is used, so whatever still holds it holds nothing.
private final class BigStackRun<T>: @unchecked Sendable {
    private var body: (() -> T)?
    private var made: T?

    init(_ body: @escaping () -> T) {
        self.body = body
    }

    /// On the thread: what `body` makes kept, and `body` (with all it captured) let go.
    func run() {
        let work = body
        body = nil
        made = work?()
    }

    /// What was made, handed over and no longer kept here.
    func take() -> T {
        let out = made
        made = nil
        // (run before this, always: the caller waits for the thread's signal)
        return out!
    }
}
