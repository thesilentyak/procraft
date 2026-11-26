package io.github.artcraftandroid.procraft.ui

import io.github.artcraftandroid.procraft.core.SideBarDragState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ModifyTouchTest {
    /** What the button's press heard. */
    private class Press : ModifyPress {
        val heard = mutableListOf<String>()
        override fun pressed() {
            heard += "pressed"
        }

        override fun released(cancelled: Boolean) {
            heard += if (cancelled) "cancelled" else "released"
        }
    }

    /**
     * The bar's drag as the core decides it, for a bar on the left: 15 points out from where the
     * button was pressed (more out than down) it moves, the bar's top following the finger down;
     * another way first, it is the press alone.
     */
    private class Bar(private val downX: Float, private val downY: Float, top: Float) : BarDrag {
        var state = SideBarDragState.WAITING
        var shownTop = top
        var lastY = downY
        var letGo = false
        val moves = mutableListOf<Pair<Float, Float>>()

        override fun moved(x: Float, y: Float, ms: Long): SideBarDragState {
            moves += x to y
            val (dx, dy) = (x - downX) to (y - downY)
            if (state == SideBarDragState.WAITING && kotlin.math.hypot(dx, dy) >= 15f) {
                state = if (dx > kotlin.math.abs(dy)) SideBarDragState.MOVING else SideBarDragState.PRESSING
            } else if (state == SideBarDragState.MOVING) {
                shownTop += y - lastY
            }
            lastY = y
            return state
        }

        override fun letGo() {
            letGo = true
        }
    }

    /** The bar at 143 down; the button pressed at (25, 400) by pointer 1. */
    private fun touch(press: Press, bar: Bar) = ModifyTouch(1L, press, bar)

    @Test
    fun aDragOutAndDownMovesTheBarAndLetsGoOfThePressUntapped() {
        val (press, bar) = Press() to Bar(25f, 400f, 143f)
        val touch = touch(press, bar)
        assertEquals(listOf("pressed"), press.heard)
        // a little out: still pressed
        touch.moved(1L, 32f, 400f, 10, onButton = true)
        assertFalse(touch.movesBar)
        assertEquals(listOf("pressed"), press.heard)
        // 15 out: the bar moves, and the press goes untapped (once)
        touch.moved(1L, 40f, 400f, 20, onButton = true)
        assertTrue(touch.movesBar)
        assertEquals(listOf("pressed", "cancelled"), press.heard)
        // down 120 with the finger, off the button's old place: the bar follows
        for (k in 1..6) touch.moved(1L, 45f, 400f + k * 20f, 20L + k * 10, onButton = false)
        assertEquals(263f, bar.shownTop, 0.01f)
        // lifted: the bar goes back to its edge, and nothing is tapped
        touch.lifted(1L, cancelled = false)
        assertTrue(bar.letGo)
        assertEquals(listOf("pressed", "cancelled"), press.heard)
    }

    @Test
    fun aStrokeStartedOnTheCanvasNeverMovesTheBar() {
        // the button pressed and held (pointer 1) while a stroke started on the canvas (pointer 2)
        // crosses the bar and the button: only the button's own pointer is the bar's
        val (press, bar) = Press() to Bar(25f, 400f, 143f)
        val touch = touch(press, bar)
        for (k in 0..20) touch.moved(2L, 600f - k * 30f, 380f + k * 2f, 10L + k * 10, onButton = k == 19)
        touch.lifted(2L, cancelled = false)
        assertTrue(bar.moves.isEmpty())
        assertFalse(bar.letGo)
        assertFalse(touch.movesBar)
        assertEquals(listOf("pressed"), press.heard)
        // the button still held: let go, it is a tap
        touch.lifted(1L, cancelled = false)
        assertEquals(listOf("pressed", "released"), press.heard)
        assertEquals(143f, bar.shownTop)
    }

    @Test
    fun aTapIsATapAndADragUpOrDownFirstIsThePressAlone() {
        // tapped
        val (press, bar) = Press() to Bar(25f, 400f, 143f)
        touch(press, bar).lifted(1L, cancelled = false)
        assertEquals(listOf("pressed", "released"), press.heard)
        // down the bar first: the press alone, which goes untapped as the finger leaves the button
        val (press2, bar2) = Press() to Bar(25f, 400f, 143f)
        val touch = touch(press2, bar2)
        touch.moved(1L, 25f, 416f, 10, onButton = true)
        assertFalse(touch.movesBar)
        assertEquals(listOf("pressed"), press2.heard)
        touch.moved(1L, 60f, 450f, 20, onButton = false)
        assertFalse(touch.movesBar)
        assertEquals(143f, bar2.shownTop)
        assertEquals(listOf("pressed", "cancelled"), press2.heard)
        // lifted: told once
        touch.lifted(1L, cancelled = false)
        assertEquals(listOf("pressed", "cancelled"), press2.heard)
        // the touch taken by the system while held on the button: let go, untapped
        val (press3, bar3) = Press() to Bar(25f, 400f, 143f)
        val taken = touch(press3, bar3)
        taken.lifted(1L, cancelled = true)
        taken.lifted(1L, cancelled = false)
        assertEquals(listOf("pressed", "cancelled"), press3.heard)
        assertTrue(bar3.letGo)
    }
}
