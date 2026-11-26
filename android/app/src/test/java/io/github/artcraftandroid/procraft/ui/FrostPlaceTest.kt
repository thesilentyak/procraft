package io.github.artcraftandroid.procraft.ui

import androidx.compose.ui.geometry.Offset
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test

class FrostPlaceTest {
    /** The frosted box of a node at [onScreen], [width] by [height], with the canvas's corner at [canvas]. */
    private fun box(onScreen: Offset, width: Int, height: Int, canvas: Offset): FloatArray {
        val backdrops = Backdrops { _, _ -> }
        backdrops.canvasOnScreen = { canvas }
        return frostBox(backdrops.onCanvas(onScreen), width, height, 0f, 0f, 0f, 0f)
    }

    @Test
    fun theTopBarsGlassLiesUnderTheBarInAWindowOfItsOwn() {
        // a window 300 in and 100 down the screen, its frame's caption bar 66 tall over the content:
        // the top bar at the content's top has its glass at the canvas's top, not a caption lower
        val content = Offset(300f, 166f)
        val canvas = canvasCorner(content, Clearance())
        assertEquals(content, canvas)
        assertArrayEquals(floatArrayOf(0f, 0f, 1200f, 88f, 0f), box(content, 1200, 88, canvas), 0f)
        // a panel 40 under it and 500 across
        assertArrayEquals(floatArrayOf(500f, 128f, 876f, 728f, 0f), box(content + Offset(500f, 128f), 376, 600, canvas), 0f)
    }

    @Test
    fun theBandKeptClearMovesTheCanvasInFromTheContentsCorner() {
        // split screen, the app in the lower half; the interface kept clear of a 24 band above it
        val content = Offset(0f, 1000f)
        val canvas = canvasCorner(content, Clearance(top = 24))
        assertEquals(Offset(0f, 1024f), canvas)
        assertArrayEquals(floatArrayOf(0f, 0f, 1600f, 88f, 0f), box(Offset(0f, 1024f), 1600, 88, canvas), 0f)
    }

    @Test
    fun fullScreenIsAsBefore() {
        val canvas = canvasCorner(Offset.Zero, Clearance())
        assertArrayEquals(floatArrayOf(10f, 20f, 40f, 60f, 0f), box(Offset(10f, 20f), 30, 40, canvas), 0f)
        // a bar's box taken past its sides and below
        assertArrayEquals(floatArrayOf(-5f, 20f, 45f, 63f, 12f), frostBox(Offset(10f, 20f), 30, 40, 15f, 5f, 3f, 12f), 0f)
    }

    @Test
    fun aPanelThatMayLieOverAWindowHidesItWhole() {
        // the dark frost lets 13% of what is behind it through: over the canvas, blurred
        val frost = 0xDE / 255f
        assertEquals(frost, frostAlpha(frost, solid = false), 0f)
        // solid (a floating window, or any panel while the panels float): a window's white text (255) on
        // its dark ground (30) under it moves its colour by no level at all (0.98, as before, let 4.5
        // levels through: "Layer 1" and the checkboxes faintly readable through the brush library)
        val solid = frostAlpha(frost, solid = true)
        assertEquals(1f, solid, 0f)
        val through = (1f - solid) * (255 - 30)
        println("solid panel: $through levels of a window's text through it (was ${(1f - 0.98f) * (255 - 30)})")
        org.junit.Assert.assertTrue("$through levels show through", through < 0.5f)
    }
}
