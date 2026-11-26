package io.github.artcraftandroid.procraft.ui

import org.junit.Assert.assertEquals
import org.junit.Test

class SideBarStretchTest {
    /** The side bar's body on the window ([density] pixels a dp), placed out by [stretch] dp as the canvas screen places it. */
    private fun onWindow(stretch: Float, right: Boolean, window: Float = 1600f, density: Float = 2.5f): Pair<Float, Float> {
        val width = 51f * density
        val placed = if (right) window - width - stretch * density else stretch * density
        val (l, r) = sideBarBody(width, stretch * density, right)
        return placed + l to placed + r
    }

    @Test
    fun stretchedItsEdgeSideStaysAgainstTheWindowsEdgeAndItsFarSideGoesOut() {
        // as measured: the finger 20 dp out stretches it 5; 8 at most
        for (stretch in listOf(0f, 5f, 8f)) {
            val (left, right) = onWindow(stretch, right = false)
            println("left-handed, stretched $stretch dp: the body $left..$right px")
            assertEquals(0f, left, 0.001f)
            assertEquals((51f + stretch) * 2.5f, right, 0.001f)
            val (l, r) = onWindow(stretch, right = true)
            println("right-handed, stretched $stretch dp: the body $l..$r px")
            assertEquals(1600f, r, 0.001f)
            assertEquals(1600f - (51f + stretch) * 2.5f, l, 0.001f)
        }
    }

    @Test
    fun aStretchBelowNothingIsNone() {
        assertEquals(0f to 100f, sideBarBody(100f, -3f, right = false))
        assertEquals(0f to 100f, sideBarBody(100f, -3f, right = true))
    }
}
