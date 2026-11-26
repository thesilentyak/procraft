package io.github.artcraftandroid.procraft.gallery

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PaletteSampleTest {
    /** A picture for a palette is read whole, its colours exactly as they are, unless it's huge. */
    @Test
    fun aPictureIsReadWholeUnlessHuge() {
        // a Discord report's 512 x 512 test picture was read at half size before: now whole
        assertEquals(1, paletteSampleSize(512, 512))
        assertEquals(1, paletteSampleSize(2560, 1600))
        // 8 million pixels: still whole; one more row, halved
        assertEquals(1, paletteSampleSize(4000, 2000))
        assertEquals(2, paletteSampleSize(4000, 2001))
        // a 600 DPI 30 x 40 cm canvas's export (7087 x 9449): a quarter as many pixels and fewer
        val big = paletteSampleSize(7087, 9449)
        assertEquals(4, big)
        assertTrue((7087L / big) * (9449L / big) <= PALETTE_PICTURE_MOST)
        // a picture with no size (unreadable) needs no shrinking
        assertEquals(1, paletteSampleSize(0, 0))
    }
}
