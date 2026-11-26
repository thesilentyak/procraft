package io.github.artcraftandroid.procraft.canvas

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SliderPanelTest {
    // the side bar down the left edge, the size slider's panel beside it, reaching past the bar
    private val bar = Rect(0f, 300f, 102f, 1338f)
    private val panel = Rect(106f, 292f, 486f, 672f)

    @Test
    fun aTouchOnThePanelOrTheSideBarLeavesThePanelUp() {
        // its + (where a finger lifted off the slider goes next), the slider, undo
        assertTrue(sliderPanelKeeps(Offset(438f, 336f), bar, panel))
        assertTrue(sliderPanelKeeps(Offset(50f, 400f), bar, panel))
        assertTrue(sliderPanelKeeps(Offset(50f, 1250f), bar, panel))
    }

    @Test
    fun aTouchAnywhereElseTakesThePanelAway() {
        // the canvas, just past the panel's edge, the top bar
        assertFalse(sliderPanelKeeps(Offset(900f, 800f), bar, panel))
        assertFalse(sliderPanelKeeps(Offset(487f, 400f), bar, panel))
        assertFalse(sliderPanelKeeps(Offset(300f, 40f), bar, panel))
    }
}
