package io.github.artcraftandroid.procraft.ui

import androidx.compose.foundation.gestures.ScrollScope
import androidx.compose.runtime.MonotonicFrameClock
import io.github.artcraftandroid.procraft.core.BlendStep
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.math.roundToInt

class BlendFollowTest {
    /** A list that scrolls between 0 and [most] pixels, as far as it can each time. */
    private class Rows(var at: Float, val most: Float) : ScrollScope {
        override fun scrollBy(pixels: Float): Float {
            val to = (at + pixels).coerceIn(0f, most)
            val moved = to - at
            at = to
            return moved
        }

        val position get() = at.roundToInt()
    }

    /** Frames 1/60 s apart. */
    private val frames = object : MonotonicFrameClock {
        var now = 0L

        override suspend fun <R> withFrameNanos(onFrame: (Long) -> R): R {
            now += 16_666_667L
            return onFrame(now)
        }
    }

    // 38.8 points a row on a screen of 2.75 pixels a point: 107 pixels
    private val density = 2.75f
    private val rowPx = 107

    @Test
    fun aCoastComesToRestOnItsRowToThePixel() = runBlocking(frames) {
        val rows = Rows(250f, 26f * rowPx)
        val from = 250f / density
        val to = 5 * rowPx / density
        // half a second from where it was let go to row 5, the last step a fraction of a point off it
        val left = rows.followBlend(rowPx, density, { rows.position }) { t ->
            if (t >= 0.5f) BlendStep(to + 0.2f, 0f, true) else BlendStep(from + (to - from) * t * 2f, 100f, false)
        }
        assertEquals(0f, left)
        assertEquals(5 * rowPx, rows.position)
    }

    @Test
    fun stoppedByAnEndItHandsOnTheSpeedLeftInPixels() = runBlocking(frames) {
        val rows = Rows(200f, 300f)
        val from = 200f / density
        // heading 400 points a second past the end, 300 pixels in
        val left = rows.followBlend(rowPx, density, { rows.position }) { t -> BlendStep(from + 400f * t, 400f, false) }
        assertEquals(400f * density, left)
        assertEquals(300, rows.position)
    }
}
