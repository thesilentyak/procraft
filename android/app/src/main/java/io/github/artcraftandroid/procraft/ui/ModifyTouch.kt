package io.github.artcraftandroid.procraft.ui

import io.github.artcraftandroid.procraft.core.SideBarDragState

/** The modify button's press: held, a canvas touch does what □ + Touch is set to; tapped, Tap □. */
interface ModifyPress {
    fun pressed()

    /** Let go: a tap unless [cancelled] (slid off, taken, or moving the side bar instead). */
    fun released(cancelled: Boolean)
}

/** The side bar's part in a touch on the modify button: the core's drag, which says when it moves the bar. */
interface BarDrag {
    /** The finger moved to ([x], [y]) at [ms] (the bar's points, on the window). */
    fun moved(x: Float, y: Float, ms: Long): SideBarDragState

    /** The finger lifted, or the touch was taken: a bar it moved settles into its width, its place kept. */
    fun letGo()
}

/**
 * One touch on the side bar's modify button, made as it goes down there: it presses the button
 * ([press]), and dragged out from the bar's edge and then up or down it moves the bar ([bar])
 * instead, the press let go untapped. Slid off the button without moving the bar, the press is let
 * go untapped, as before. Only the pointer that went down on the button counts: one that went down
 * on the canvas (a stroke crossing the bar) never reaches it.
 */
class ModifyTouch(private val pointer: Long, private val press: ModifyPress, private val bar: BarDrag) {
    private var pressing = true
    private var moving = false
    private var done = false

    init {
        press.pressed()
    }

    /** It moves the bar. */
    val movesBar: Boolean get() = moving

    /**
     * Pointer [id] moved to ([x], [y]) at [ms] (the bar's points, on the window), on the button
     * still or not ([onButton], as the button stood before the bar moved).
     */
    fun moved(id: Long, x: Float, y: Float, ms: Long, onButton: Boolean) {
        if (id != pointer || done) return
        val state = bar.moved(x, y, ms)
        if (state == SideBarDragState.MOVING) moving = true
        if (pressing && (moving || !onButton)) {
            pressing = false
            press.released(cancelled = true)
        }
    }

    /** Pointer [id] lifted, or (it was [cancelled]) the system took it. */
    fun lifted(id: Long, cancelled: Boolean) {
        if (id != pointer || done) return
        done = true
        bar.letGo()
        if (pressing) {
            pressing = false
            press.released(cancelled)
        }
    }
}
