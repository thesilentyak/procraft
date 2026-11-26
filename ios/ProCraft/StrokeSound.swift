import AVFoundation
import Foundation

/// Brush sounds (a plugin's): plays the sound the core makes of the stroke being drawn, a little at
/// a time on a thread of its own, while `follow` says so. Silent for half a second (no stroke), the
/// audio rests, its thread waiting for the next touch (`wake`) instead of playing silence.
final class StrokeSound {
    /// The one the canvas open uses.
    static let shared = StrokeSound()

    /// The sound's rate, and how much is made at once (10 ms).
    private static let rate = 48000
    private static let frames = 480
    /// Pieces of silence in a row after which the audio rests (half a second), and how many pieces
    /// are queued ahead at most.
    private static let restAfter = 50
    private static let ahead = 3

    /// The strokes are heard now (as last followed, on the main thread): a hovering Pencil tells the
    /// canvas where it is only then.
    private(set) var sounding = false

    private let lock = NSCondition()
    /// The artwork whose strokes are heard; none: silence.
    private var heard: CanvasSession?
    private var running = false
    private var thread: Thread?
    /// Touches on the canvas so far, which the resting thread waits on.
    private var touched = 0

    /// Plays `session`'s strokes while `on` (a plugin makes their sound); stops otherwise.
    func follow(_ session: CanvasSession?, on: Bool) {
        let wanted = on && session != nil
        sounding = wanted
        lock.lock()
        heard = wanted ? session : nil
        let start = wanted && thread == nil
        let stop = !wanted && thread != nil
        if start {
            running = true
        }
        if stop {
            running = false
            lock.broadcast()
        }
        lock.unlock()
        if start {
            let made = Thread { [weak self] in self?.play() }
            made.name = "ProCraft stroke sound"
            made.qualityOfService = .userInteractive
            thread = made
            made.start()
        } else if stop {
            thread = nil
        }
    }

    /// A touch landed on the canvas: a stroke may begin, so the sound wakes if it rests.
    func wake() {
        lock.lock()
        touched += 1
        lock.broadcast()
        lock.unlock()
    }

    private func play() {
        guard let format = AVAudioFormat(standardFormatWithSampleRate: Double(Self.rate), channels: 1) else { return }
        let engine = AVAudioEngine()
        let player = AVAudioPlayerNode()
        engine.attach(player)
        engine.connect(player, to: engine.mainMixerNode, format: format)
        // mixed with what else plays, and quiet with the ring switch
        try? AVAudioSession.sharedInstance().setCategory(.ambient, options: [.mixWithOthers])
        do {
            try engine.start()
        } catch {
            return
        }
        player.play()
        let queued = DispatchSemaphore(value: Self.ahead)
        var silent = 0
        lock.lock()
        var seen = touched
        lock.unlock()
        while true {
            queued.wait()
            lock.lock()
            let session = heard
            let going = running
            lock.unlock()
            guard going else { break }
            let sound = session?.strokeSound(frames: UInt32(Self.frames), rate: UInt32(Self.rate)) ?? []
            let audible = sound.contains { $0 != 0 }
            if audible {
                silent = 0
                lock.lock()
                seen = touched
                lock.unlock()
            } else {
                silent += 1
                if silent >= Self.restAfter {
                    // nothing to play for a while: the audio rests until a touch (or the end)
                    queued.signal()
                    player.pause()
                    lock.lock()
                    while running, touched == seen {
                        lock.wait()
                    }
                    seen = touched
                    let still = running
                    lock.unlock()
                    guard still else { break }
                    silent = 0
                    player.play()
                    continue
                }
            }
            guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(Self.frames)),
                  let channel = buffer.floatChannelData?[0] else {
                queued.signal()
                continue
            }
            buffer.frameLength = AVAudioFrameCount(Self.frames)
            for i in 0 ..< Self.frames {
                channel[i] = i < sound.count ? sound[i] : 0
            }
            player.scheduleBuffer(buffer) { queued.signal() }
        }
        player.stop()
        engine.stop()
    }
}
