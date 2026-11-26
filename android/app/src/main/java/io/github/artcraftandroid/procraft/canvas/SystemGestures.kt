package io.github.artcraftandroid.procraft.canvas

/** A cancel this soon (ms) after the last of the fingers landed can be the system taking them. */
internal const val SYSTEM_TOOK_FINGERS_MS = 1500L

/**
 * Whether the platform calling off the canvas's touches took three or more [fingers] for a gesture
 * of the tablet's own (many take three fingers swiped down for a screenshot, and some take three
 * fingers as soon as they land): the last of them landed [sinceLanded] ms before, and the window
 * kept the focus ([focused]: a dialog or another app coming up takes it). Three-finger taps, scrubs
 * and swipes then never reach the canvas.
 */
internal fun systemTookFingers(fingers: Int, sinceLanded: Long, focused: Boolean): Boolean =
    focused && fingers >= 3 && sinceLanded in 0..SYSTEM_TOOK_FINGERS_MS

/** Says yes the first time it is asked, and never again: for what is told once (each run of the app). */
internal class TellOnce {
    private var told = false

    fun first(): Boolean {
        if (told) return false
        told = true
        return true
    }
}
