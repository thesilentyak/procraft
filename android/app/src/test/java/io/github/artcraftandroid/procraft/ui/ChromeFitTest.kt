package io.github.artcraftandroid.procraft.ui

import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ChromeFitTest {
    private val sideBarClear = Pc.SideBarWidth + Pc.PopoverGap

    private fun widths(from: Int, to: Int) = (from..to).map { it.dp }

    @Test
    fun aTabletsTopBarKeepsItsMeasuredSteps() {
        for ((width, tool) in listOf(720.dp to 8.dp, 744.dp to 8.dp, 1133.dp to 26.dp, 1200.dp to 26.dp)) {
            assertEquals(Pc.BarFit(40.dp, tool, 14.dp, 44.dp), Pc.barFit(width))
        }
        // wherever the measured step fits, it is the one used
        for (width in widths(705, 2000)) {
            val fit = Pc.barFit(width)
            assertEquals(40.dp, fit.margin)
            assertEquals(44.dp, fit.button)
            assertEquals(14.dp, fit.actionGap)
        }
    }

    @Test
    fun theTopBarsEndsNeverMeet() {
        // down to a phone's 300 across (#318: on a Galaxy A30s, narrower than 354, the tools had
        // covered Selection and Transform)
        for (width in widths(300, 2000)) assertTrue("$width", Pc.barFit(width).needs <= width)
        assertEquals(26.dp, Pc.barFit(300.dp).button)
        assertEquals(28.dp, Pc.barFit(340.dp).button)
        assertEquals(32.dp, Pc.barFit(360.dp).button)
    }

    @Test
    fun aTopBarButtonsGlyphShrinksOnlyBelowThirtyTwo() {
        for (width in listOf(32.dp, 36.dp, 44.dp)) assertEquals(1f, Pc.barGlyphScale(width))
        assertEquals(26f / 32f, Pc.barGlyphScale(26.dp), 1e-4f)
        // so the glyphs crowd no more than on the 32 step (the tools' 44 frame on a 32 pitch)
        for (width in widths(300, 353)) {
            val fit = Pc.barFit(width)
            assertTrue("$width", 44.dp * Pc.barGlyphScale(fit.button) <= fit.button * 1.375f + 0.01.dp)
        }
    }

    @Test
    fun theBrushStudioLaysItsPadBesideItsSettingsOnlyWithRoomForItsButtons() {
        // a tablet on its side, as measured: pages, settings and pad side by side
        assertTrue(studioSideBySide(landscape = true, 1133.dp))
        assertTrue(studioSideBySide(landscape = true, 1014.dp))
        // a phone on its side (#318: the pad had about 80 across, Done squeezed to nothing), and half
        // a tablet on its side: the pages as a rail, the pad over the settings
        for (width in listOf(640.dp, 732.dp, 780.dp, 850.dp, 1013.dp)) assertFalse("$width", studioSideBySide(landscape = true, width))
        // upright, always the rail
        assertFalse(studioSideBySide(landscape = false, 1280.dp))
        assertEquals(360.dp, PadLeast)
    }

    @Test
    fun panelsStillHangFromTheirButtons() {
        // the bar at 411: 8 margins and 36 buttons, nothing between them
        assertEquals(26.dp, Pc.toolCentreFromRight(0, 411.dp))
        assertEquals(170.dp, Pc.toolCentreFromRight(4, 411.dp))
        assertEquals(84.dp, Pc.actionCentre(0, 411.dp))
        // on a tablet, as ever
        assertEquals(62.dp, Pc.toolCentreFromRight(0, 1200.dp))
        assertEquals(342.dp, Pc.toolCentreFromRight(4, 1200.dp))
        assertEquals(134.dp, Pc.actionCentre(0, 1200.dp))
    }

    @Test
    fun aPanelInAWideEnoughWindowGoesWhereItAlwaysDid() {
        // as it was before narrow windows were thought of
        fun old(width: Dp, tip: Dp, screen: Dp, minLeft: Dp, minRight: Dp) =
            (tip + Pc.ArrowShift - width / 2).coerceIn(minLeft, (screen - minRight - width).coerceAtLeast(minLeft))
        for (screen in widths(451, 2000)) {
            val tip = screen - Pc.toolCentreFromRight(0, screen)
            assertEquals(old(376.dp, tip, screen, sideBarClear, Pc.PopoverGap), popoverPlace(376.dp, tip, screen, sideBarClear).left)
            assertEquals(old(376.dp, tip, screen, Pc.PopoverGap, sideBarClear), popoverPlace(376.dp, tip, screen, minRight = sideBarClear).left)
        }
    }

    @Test
    fun aPanelStaysInsideANarrowWindow() {
        for (panel in listOf(330.dp, 376.dp, 384.dp, 390.dp, 440.dp)) {
            for (screen in widths(panel.value.toInt(), 700)) {
                val tip = screen - Pc.toolCentreFromRight(0, screen)
                for (place in listOf(popoverPlace(panel, tip, screen, sideBarClear), popoverPlace(panel, tip, screen, minRight = sideBarClear))) {
                    assertTrue("$panel in $screen", place.left >= 0.dp && place.left + panel <= screen)
                }
            }
        }
        // the colour panel in a window 411 wide, which ran 28 off its right edge
        assertEquals(17.5.dp, popoverPlace(376.dp, 411.dp - 26.dp, 411.dp, sideBarClear).left)
    }
}
