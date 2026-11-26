package io.github.artcraftandroid.procraft.canvas

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack
import io.github.artcraftandroid.procraft.core.CanvasSession
import kotlin.concurrent.withLock

/** The artwork whose strokes are heard: [session], while the app is [inFront] and [onCanvas] shows it; else none. */
internal fun <S : Any> strokeSoundHeard(session: S?, inFront: Boolean, onCanvas: Boolean): S? = session?.takeIf { inFront && onCanvas }

/**
 * Brush sounds (a plugin's): plays the sound the core makes of the stroke being drawn, a little at a
 * time on a thread of its own, while [follow] says so. Silent for half a second (no stroke), the
 * audio rests, its thread waiting for the next touch ([wake]) instead of playing silence.
 */
class StrokeSound {
    /** The artwork whose strokes are heard; none: silence. */
    @Volatile
    private var heard: CanvasSession? = null

    @Volatile
    private var running = false
    private var thread: Thread? = null

    /** Touches on the canvas so far, and what the resting thread waits on. */
    private val touches = java.util.concurrent.locks.ReentrantLock()
    private val touchedMore = touches.newCondition()
    private var touched = 0L

    /** Plays [session]'s strokes while [on] (a plugin makes their sound); stops otherwise. */
    fun follow(session: CanvasSession?, on: Boolean) {
        val wanted = on && session != null
        heard = if (wanted) session else null
        if (wanted && thread == null) {
            running = true
            thread = Thread(::play, "procraft-stroke-sound").also { it.start() }
        } else if (!wanted && thread != null) {
            running = false
            wake()
            thread?.join(500)
            thread = null
        }
    }

    /** A touch landed on the canvas: a stroke may begin, so the sound wakes if it rests. */
    fun wake() {
        touches.withLock {
            touched++
            touchedMore.signalAll()
        }
    }

    private fun play() {
        val format = AudioFormat.Builder()
            .setEncoding(AudioFormat.ENCODING_PCM_FLOAT)
            .setSampleRate(RATE)
            .setChannelMask(AudioFormat.CHANNEL_OUT_MONO)
            .build()
        val least = AudioTrack.getMinBufferSize(RATE, AudioFormat.CHANNEL_OUT_MONO, AudioFormat.ENCODING_PCM_FLOAT)
        val track = runCatching {
            AudioTrack.Builder()
                .setAudioAttributes(
                    AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_GAME)
                        .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                        .build(),
                )
                .setAudioFormat(format)
                .setBufferSizeInBytes(maxOf(least, FRAMES * 4 * 3))
                .setTransferMode(AudioTrack.MODE_STREAM)
                .setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY)
                .build()
        }.getOrNull() ?: return
        val silence = FloatArray(FRAMES)
        // writes of silence in a row, and the touches as of the last sound (or rest)
        var silent = 0
        var seen = touches.withLock { touched }
        try {
            track.play()
            while (running) {
                val sound = heard?.let { s -> runCatching { s.strokeSound(FRAMES.toUInt(), RATE.toUInt()).toFloatArray() }.getOrNull() }
                val pcm = sound?.takeIf { it.isNotEmpty() }
                if (pcm != null) {
                    silent = 0
                    seen = touches.withLock { touched }
                } else if (++silent >= REST_AFTER) {
                    // nothing to play for a while: the audio rests until a touch (or the end)
                    track.pause()
                    track.flush()
                    touches.withLock {
                        while (running && touched == seen) touchedMore.await()
                        seen = touched
                    }
                    if (!running) break
                    silent = 0
                    track.play()
                    continue
                }
                val out = pcm ?: silence
                track.write(out, 0, out.size, AudioTrack.WRITE_BLOCKING)
            }
        } finally {
            runCatching { track.stop() }
            track.release()
        }
    }

    private companion object {
        /** The sound's rate, and how much is written at once (10 ms). */
        const val RATE = 48_000
        const val FRAMES = 480

        /** Writes of silence in a row after which the audio rests (half a second). */
        const val REST_AFTER = 50
    }
}
