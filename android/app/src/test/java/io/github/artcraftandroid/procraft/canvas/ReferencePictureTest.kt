package io.github.artcraftandroid.procraft.canvas

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ReferencePictureTest {
    /** The longer side the core keeps a window's picture at (its PICTURE_MOST). */
    private val most = 2048

    /** MiB that [w] × [h] pixels take, 8 bits a channel. */
    private fun mib(w: Int, h: Int) = w.toDouble() * h * 4 / (1 shl 20)

    /** What the window held before: the picture read at the least power of two that made it 4096 or less. */
    private fun heldBefore(w: Int, h: Int): Double {
        var sample = 1
        while (maxOf(w, h) / sample > 4096) sample *= 2
        return mib(w / sample, h / sample)
    }

    @Test
    fun aPictureIsReadAtAboutTheSizeAWindowKeepsIt() {
        // a 4000 square, 12, 48 and 200 megapixel photos, one at 4096, one read whole and made smaller, a small one
        val cases = listOf(
            Triple(4000, 4000, 2),
            Triple(4000, 3000, 2),
            Triple(8000, 6000, 4),
            Triple(16320, 12240, 8),
            Triple(4096, 4096, 2),
            Triple(3500, 3500, 1),
            Triple(1000, 500, 1),
        )
        for ((w, h, sample) in cases) {
            assertEquals("$w x $h", sample, pictureSample(w, h, most))
            val (rw, rh) = w / sample to h / sample
            val side = maxOf(rw, rh)
            // read at no more than an eighth short of the side kept (a small one whole), and under twice it
            assertTrue("$w x $h read at $side", side >= minOf(maxOf(w, h), most - most / 8) && side < 2 * (most - most / 8))
            // held: what is read, made the side kept if it is longer
            val scale = minOf(1.0, most.toDouble() / side)
            val held = mib((rw * scale).toInt(), (rh * scale).toInt())
            val read = mib(rw, rh)
            println("$w x $h: read at $rw x $rh (%.1f MiB), held %.1f MiB; before held %.1f MiB".format(read, held, heldBefore(w, h)))
            assertTrue("$w x $h holds $held MiB", held <= 16.0)
            assertTrue("$w x $h reads $read MiB", read <= 49.0)
        }
        // a 4000 square was held whole: 61 MiB, now 15.3
        assertEquals(61.0, heldBefore(4000, 4000), 0.1)
    }

    @Test
    fun aSmallPictureOrNoneIsReadWhole() {
        assertEquals(1, pictureSample(300, 200, 300))
        assertEquals(1, pictureSample(2048, 2048, most))
        assertEquals(1, pictureSample(0, 0, 0))
        assertEquals(1, pictureSample(5000, 5000, 0))
    }
}
