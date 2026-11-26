package io.github.artcraftandroid.procraft.ui

import androidx.compose.foundation.gestures.FlingBehavior
import androidx.compose.foundation.gestures.ScrollScope
import androidx.compose.runtime.withFrameNanos
import io.github.artcraftandroid.procraft.core.BlendStep
import io.github.artcraftandroid.procraft.core.blendCoast
import io.github.artcraftandroid.procraft.core.blendCoastAt
import kotlin.math.abs
import kotlin.math.roundToInt

/**
 * The blend list let go: it moves on as the core works it out, slowing straight onto a row in one
 * motion (flicked into an end, what speed is left goes to the list's overscroll). [rowPx] is the
 * rows' pitch in pixels, [density] pixels a point, [position] how far the list is scrolled.
 */
internal class BlendFling(
    private val rowPx: Int,
    private val density: Float,
    private val rows: Int,
    private val position: () -> Int,
) : FlingBehavior {
    override suspend fun ScrollScope.performFling(initialVelocity: Float): Float {
        val coast = blendCoast(position() / density, initialVelocity / density, rowPx / density, rows.toUInt())
        return followBlend(rowPx, density, position) { blendCoastAt(coast, it) }
    }
}

/**
 * Scrolls the blend list along [at] (where it is, in points, so many seconds in) a frame at a time
 * until it is done, then onto its row to the pixel. Returns the speed left (pixels a second) when
 * an end stops it first, else 0.
 */
internal suspend fun ScrollScope.followBlend(rowPx: Int, density: Float, position: () -> Int, at: (Float) -> BlendStep): Float {
    val start = withFrameNanos { it }
    var shown = position().toFloat()
    while (true) {
        val now = withFrameNanos { it }
        val step = at((now - start) / 1e9f)
        val target = if (step.done) (step.position * density / rowPx).roundToInt() * rowPx.toFloat() else step.position * density
        val wanted = target - shown
        val moved = scrollBy(wanted)
        shown += moved
        if (abs(moved - wanted) > 0.5f) return step.speed * density
        if (step.done) {
            val off = target - position()
            if (off != 0f) scrollBy(off)
            return 0f
        }
    }
}

/**
 * Where the band crosses blend row [index] with the list scrolled [position] pixels: its top and
 * bottom, pixels down from the row's top (outside 0 until [rowPx] it misses the row). The rows are
 * [rowPx] apart, and the band is a row tall where the first row is unscrolled.
 */
internal fun blendBandInRow(position: Int, index: Int, rowPx: Int): Pair<Float, Float> {
    val top = (position - index * rowPx).toFloat()
    return top to top + rowPx
}
