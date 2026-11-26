package io.github.artcraftandroid.procraft.ui

import androidx.compose.ui.geometry.Offset
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TextTouchTest {
    @Test
    fun aTouchOnTheTextLeavesTheStyleSheetInTheKeyboardsPlace() {
        assertFalse(textTouchBringsKeyboard(styleOpen = true))
        assertTrue(textTouchBringsKeyboard(styleOpen = false))
    }

    @Test
    fun quickTapsOnTheTextCountOneTwoThreeThenStartAgain() {
        val at = Offset(100f, 50f)
        fun next(before: Int, gap: Long, where: Offset = at) = tapsCounted(before, 1000L, at, 1000L + gap, where, 300L, 50f)
        assertEquals(1, next(0, 100))
        assertEquals(2, next(1, 100))
        assertEquals(3, next(2, 300))
        // after three, the fourth is a first again
        assertEquals(1, next(3, 100))
        // too late or too far: a first
        assertEquals(1, next(1, 301))
        assertEquals(1, next(2, 100, Offset(100f, 101f)))
        assertEquals(2, next(1, 100, Offset(130f, 90f)))
    }
}
