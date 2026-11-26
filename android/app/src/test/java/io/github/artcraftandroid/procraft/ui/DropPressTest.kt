package io.github.artcraftandroid.procraft.ui

import androidx.compose.ui.geometry.Offset
import io.github.artcraftandroid.procraft.core.PressTurn
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.util.Locale

class DropPressTest {
    /**
     * The core's reading of a colour button press gone down at (0, 0), as it decides it: held at
     * 300 ms unless moved past 15 points first, dragged out at 44 points.
     */
    private class Reading : PressReading {
        val heard = mutableListOf<String>()
        var due: Double? = 300.0
        private var path = 0f
        private var last = 0f to 0f
        private var told = false

        override fun moved(x: Float, y: Float, ms: Double): PressTurn {
            heard += "moved %.2f,%.2f at %.0f".format(Locale.ROOT, x, y, ms)
            path += kotlin.math.hypot(x - last.first, y - last.second)
            last = x to y
            if (path > 15f) due = null
            return tell(if (path >= 44f) PressTurn.DRAG else if (held(ms)) PressTurn.HOLD else PressTurn.NONE)
        }

        override fun tick(ms: Double): PressTurn {
            heard += "tick %.0f".format(Locale.ROOT, ms)
            return tell(if (held(ms)) PressTurn.HOLD else PressTurn.NONE)
        }

        private fun held(ms: Double) = due?.let { ms >= it } == true

        override fun lifted(ms: Double): PressTurn {
            heard += "lifted %.0f".format(Locale.ROOT, ms)
            return tell(PressTurn.TAP)
        }

        override fun due() = due.takeIf { !told }

        private fun tell(turn: PressTurn): PressTurn {
            if (told) return PressTurn.NONE
            if (turn != PressTurn.NONE) told = true
            return turn
        }
    }

    @Test
    fun aPhonesPixelsGoToTheCoreAsPointsSoItDragsOutAt44PointsNot44Pixels() {
        // 2.75 pixels a point: 44 pixels down is 16 points, out of the hold's reach but no drag
        val reading = Reading()
        val press = DropPress(reading, perPoint = 2.75f)
        assertEquals(PressTurn.NONE, press.moved(Offset(0f, 44f), 40))
        assertEquals("moved 0.00,16.00 at 40", reading.heard.first())
        assertNull(press.wait(50))
        // 121 pixels: 44 points, out
        assertEquals(PressTurn.DRAG, press.moved(Offset(0f, 121f), 56))
        assertEquals("moved 0.00,44.00 at 56", reading.heard.last())
    }

    @Test
    fun itWaitsForThePointerUntilTheHoldIsDueAndThenAsksTheCore() {
        val reading = Reading()
        val press = DropPress(reading, perPoint = 2f)
        assertEquals(200L, press.wait(100))
        assertEquals(1L, press.wait(299))
        // a time between two milliseconds rounds up; one already past waits a millisecond
        reading.due = 300.4
        assertEquals(1L, press.wait(300))
        assertEquals(1L, press.wait(400))
        // the wait ran out: the core hears the time and holds
        reading.due = 300.0
        assertEquals(PressTurn.HOLD, press.waited(300))
        assertEquals("tick 300", reading.heard.last())
        // said, it waits for nothing more
        assertNull(press.wait(301))
    }

    @Test
    fun aLiftIsTheCoresToCall() {
        val reading = Reading()
        val press = DropPress(reading, perPoint = 3f)
        assertEquals(PressTurn.NONE, press.moved(Offset(3f, 3f), 30))
        assertEquals(PressTurn.TAP, press.lifted(90))
        assertEquals(listOf("moved 1.00,1.00 at 30", "lifted 90"), reading.heard)
    }
}
