package io.github.artcraftandroid.procraft.ui

import androidx.compose.ui.geometry.Offset
import io.github.artcraftandroid.procraft.core.ColourPress
import io.github.artcraftandroid.procraft.core.PressTurn
import kotlin.math.ceil

/** The core's reading of one press on a colour that can be dragged out, in points and milliseconds. */
interface PressReading {
    fun moved(x: Float, y: Float, ms: Double): PressTurn

    fun tick(ms: Double): PressTurn

    fun lifted(ms: Double): PressTurn

    /** When (ms) the press, kept still, may turn by itself; null when it waits for nothing. */
    fun due(): Double?
}

/**
 * One press on the colour button or a swatch: its pointer's positions (pixels, [perPoint] of them
 * to a point) and times (uptime) go to the core's reading of it ([reading]), which says once what
 * the press is: a tap, a hold, or a drag pulling the colour out.
 */
class DropPress(private val reading: PressReading, private val perPoint: Float) {
    /** The pointer moved to [at] at [ms]. */
    fun moved(at: Offset, ms: Long): PressTurn = reading.moved(at.x / perPoint, at.y / perPoint, ms.toDouble())

    /** Nothing was heard until [now]. */
    fun waited(now: Long): PressTurn = reading.tick(now.toDouble())

    /** Lifted at [ms]. */
    fun lifted(ms: Long): PressTurn = reading.lifted(ms.toDouble())

    /** How long (ms, at least 1) to wait for the pointer from [now] before the press may turn by itself; null: for as long as it takes. */
    fun wait(now: Long): Long? = reading.due()?.let { (ceil(it).toLong() - now).coerceAtLeast(1L) }

    companion object {
        /**
         * A press going down at [at] (pixels, [perPoint] to a point) at [ms]: on the colour button,
         * or on a swatch, which holds after [holdMs] (when it holds) and drags past [slop] pixels.
         */
        fun of(at: Offset, ms: Long, perPoint: Float, swatch: Boolean, holdMs: Long?, slop: Float): DropPress {
            val (x, y) = at.x / perPoint to at.y / perPoint
            val press = if (swatch) {
                ColourPress.swatch(x, y, ms.toDouble(), holdMs?.toUInt(), slop / perPoint)
            } else {
                ColourPress.button(x, y, ms.toDouble())
            }
            val reading = object : PressReading {
                override fun moved(x: Float, y: Float, ms: Double) = press.moved(x, y, ms)

                override fun tick(ms: Double) = press.tick(ms)

                override fun lifted(ms: Double) = press.lifted(ms)

                override fun due() = press.due()
            }
            return DropPress(reading, perPoint)
        }
    }
}
