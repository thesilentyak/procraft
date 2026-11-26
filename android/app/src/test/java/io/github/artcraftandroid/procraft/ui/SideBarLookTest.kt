package io.github.artcraftandroid.procraft.ui

import androidx.compose.ui.graphics.Color
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.abs

class SideBarLookTest {
    /** The grey (0..255) [top] shows over a [under] grey. */
    private fun over(top: Color, under: Float): Float = top.red * 255f * top.alpha + under * (1f - top.alpha)

    @After
    fun dark() {
        Pc.light = false
    }

    @Test
    fun aSavedValuesMarkShowsOnTheTrackInEitherLook() {
        for (light in listOf(false, true)) {
            Pc.light = light
            // over the white canvas, the black surround and a mid grey
            for (behind in listOf(255f, 0f, 128f)) {
                val track = over(Pc.SliderTrack, behind)
                val mark = over(Pc.NotchMark, track)
                assertTrue("light $light over $behind: $mark on $track", abs(mark - track) >= 15f)
            }
        }
    }

    @Test
    fun eachTrackHasALighterRimAndADarkerEdgeInEitherLook() {
        for (light in listOf(false, true)) {
            Pc.light = light
            val track = over(Pc.SliderTrack, 255f)
            val bar = over(Pc.SideBar, 255f)
            assertTrue("light $light", over(Pc.SliderRim, 255f) > track + 4f)
            assertTrue("light $light", over(Pc.SliderEdge, bar) < bar - 5f)
        }
        // as measured over white: the rims 150 and 249, the edges 38 and about 220
        Pc.light = false
        assertEquals(150f, over(Pc.SliderRim, 255f), 2f)
        assertEquals(38f, over(Pc.SliderEdge, over(Pc.SideBar, 255f)), 2f)
        Pc.light = true
        assertEquals(249f, over(Pc.SliderRim, 255f), 2f)
        assertEquals(220f, over(Pc.SliderEdge, over(Pc.SideBar, 255f)), 3f)
    }

    @Test
    fun theLightLooksThumbIsWhiteAndItsGlyphsAreTheDarkOnesTurnedOver() {
        Pc.light = false
        val dark = Pc.SideGlyph.red
        assertEquals(166f, Pc.SliderThumb.red * 255f, 0.5f)
        Pc.light = true
        assertEquals(Color.White, Pc.SliderThumb)
        assertEquals(1f - dark, Pc.SideGlyph.red, 0.5f / 255f)
        // a greyed out undo or redo, as measured over the light bar
        assertEquals(204f, over(Pc.SideGlyphDisabled, over(Pc.SideBar, 255f)), 2f)
    }

    @Test
    fun theMarkIsAsMeasuredOverWhite() {
        Pc.light = false
        assertEquals(103f, over(Pc.SliderTrack, 255f), 2f)
        assertEquals(119f, over(Pc.NotchMark, over(Pc.SliderTrack, 255f)), 2f)
        Pc.light = true
        assertEquals(244f, over(Pc.SliderTrack, 255f), 2f)
        assertEquals(162f, over(Pc.NotchMark, over(Pc.SliderTrack, 255f)), 2f)
    }
}
