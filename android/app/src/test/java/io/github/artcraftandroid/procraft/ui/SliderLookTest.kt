package io.github.artcraftandroid.procraft.ui

import io.github.artcraftandroid.procraft.core.SliderLook
import org.junit.Assert.assertEquals
import org.junit.Test

class SliderLookTest {
    @Test
    fun aPluginsOpacityTakesTheTracksAlphaAndNothingElse() {
        val own = Pc.SliderTrack
        Pc.sliderLook = SliderLook(0.25f, true)
        try {
            assertEquals(own.copy(alpha = 0.25f), Pc.SliderTrack)
        } finally {
            Pc.sliderLook = SliderLook(null, null)
        }
        // left as the app has it: its own
        assertEquals(own, Pc.SliderTrack)
        Pc.sliderLook = SliderLook(null, true)
        try {
            assertEquals(own, Pc.SliderTrack)
        } finally {
            Pc.sliderLook = SliderLook(null, null)
        }
    }
}
