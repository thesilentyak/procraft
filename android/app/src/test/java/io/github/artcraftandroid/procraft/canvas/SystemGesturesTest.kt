package io.github.artcraftandroid.procraft.canvas

import org.junit.Assert.assertEquals
import org.junit.Test

class SystemGesturesTest {
    @Test
    fun threeFingersCalledOffSoonAfterTheyLandAreTheTabletsOwnGesture() {
        // as they land, or a moment into a swipe down
        assertEquals(true, systemTookFingers(3, 0, focused = true))
        assertEquals(true, systemTookFingers(3, 180, focused = true))
        assertEquals(true, systemTookFingers(4, 600, focused = true))
    }

    @Test
    fun otherCancelsAreNot() {
        // two fingers (the back gesture, a pinch called off), or a window that lost the focus to a
        // dialog or another app
        assertEquals(false, systemTookFingers(2, 50, focused = true))
        assertEquals(false, systemTookFingers(3, 50, focused = false))
        // fingers held long before it came
        assertEquals(false, systemTookFingers(3, SYSTEM_TOOK_FINGERS_MS + 1, focused = true))
        // a clock gone backwards says nothing
        assertEquals(false, systemTookFingers(3, -5, focused = true))
    }

    @Test
    fun whatIsToldOnceIsToldOnce() {
        val once = TellOnce()
        assertEquals(true, once.first())
        assertEquals(false, once.first())
        assertEquals(false, once.first())
    }
}
