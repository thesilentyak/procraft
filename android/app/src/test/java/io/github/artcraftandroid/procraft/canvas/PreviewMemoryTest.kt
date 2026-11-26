package io.github.artcraftandroid.procraft.canvas

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PreviewMemoryTest {
    private val gb = 1L shl 30
    private val mb = 1L shl 20

    @Test
    fun withMemoryToSpareTheDevicesSizeSetsHowManyStrokesAreKept() {
        // as the core's brushPreviewsKept: a 64th of the memory, in 530 x 162 pictures
        assertEquals(195, previewBitmapsKept(4 * gb, 3 * gb))
        assertEquals(390, previewBitmapsKept(8 * gb, 6 * gb))
        assertEquals(PREVIEWS_KEPT_MOST, previewBitmapsKept(16 * gb, 12 * gb))
        assertEquals(PREVIEWS_KEPT_LEAST, previewBitmapsKept(1 * gb, 900 * mb))
    }

    @Test
    fun aPhoneShortOfMemoryKeepsFewerAndNeverFewerThanAFewScreens() {
        // a 4 GB phone with 800 MB available: a 16th of that, 50 MB (152 pictures), not the 67 MB its size gives
        val short = previewBitmapsKept(4 * gb, 800 * mb)
        assertEquals(152, short)
        assertTrue(short * PREVIEW_BITMAP_BYTES <= 800 * mb / 16)
        // with 400 MB available, 76; with next to nothing, the fewest
        assertEquals(76, previewBitmapsKept(4 * gb, 400 * mb))
        assertEquals(PREVIEWS_KEPT_LEAST, previewBitmapsKept(4 * gb, 100 * mb))
        assertEquals(PREVIEWS_KEPT_LEAST, previewBitmapsKept(4 * gb, 0))
        // what it keeps only falls as what is available does
        var last = Int.MAX_VALUE
        for (available in (4 * gb) downTo 0 step 64 * mb) {
            val kept = previewBitmapsKept(4 * gb, available)
            assertTrue("$kept at ${available / mb} MB", kept <= last)
            last = kept
        }
    }

    @Test
    fun nonsenseFromTheSystemKeepsTheFewest() {
        assertEquals(PREVIEWS_KEPT_LEAST, previewBitmapsKept(-1, -1))
        assertEquals(PREVIEWS_KEPT_LEAST, previewBitmapsKept(0, 4 * gb))
        assertEquals(PREVIEWS_KEPT_MOST, previewBitmapsKept(Long.MAX_VALUE, Long.MAX_VALUE))
        // (a picture of no bytes divides nothing by nothing)
        assertEquals(PREVIEWS_KEPT_MOST, previewBitmapsKept(4 * gb, 3 * gb, bitmapBytes = 0))
    }
}
