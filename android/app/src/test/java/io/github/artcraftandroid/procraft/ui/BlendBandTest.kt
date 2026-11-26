package io.github.artcraftandroid.procraft.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class BlendBandTest {
    private val row = 78

    @Test
    fun aNameHalfUnderTheBandIsWhiteJustWhereTheBandCoversIt() {
        // scrolled two and a half rows: the band covers row 2's lower half and row 3's upper half
        val at = 2 * row + row / 2
        assertEquals(39f to 117f, blendBandInRow(at, 2, row))
        assertEquals(-39f to 39f, blendBandInRow(at, 3, row))
        // rows away from the band: it misses them, all grey
        for (index in listOf(0, 1, 4, 5)) {
            val (top, bottom) = blendBandInRow(at, index, row)
            assertTrue("row $index", bottom <= 0f || top >= row)
        }
    }

    @Test
    fun atRestTheBandCoversItsRowWholeAndNoOther() {
        assertEquals(0f to 78f, blendBandInRow(4 * row, 4, row))
        assertEquals(78f to 156f, blendBandInRow(4 * row, 3, row))
        assertEquals(-78f to 0f, blendBandInRow(4 * row, 5, row))
    }
}
