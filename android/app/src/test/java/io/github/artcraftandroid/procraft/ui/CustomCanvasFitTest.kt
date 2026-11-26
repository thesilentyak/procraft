package io.github.artcraftandroid.procraft.ui

import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CustomCanvasFitTest {
    private fun near(expected: Dp, actual: Dp, what: String) = assertEquals(what, expected.value, actual.value, 0.01f)

    /** The keypad's rows across: units, three digits, the commands, and the gaps. */
    private fun across(k: KeypadFit) = k.keyWidth * 5 + k.across * 2 + k.groups * 2

    @Test
    fun aTabletAtFullSizeKeepsItsMeasuredSheet() {
        for (width in listOf(744.dp, 800.dp, 1133.dp, 1280.dp)) {
            val fit = CanvasSheetFit(width)
            assertFalse("$width", fit.narrow)
            assertEquals(319.dp, fit.railWidth)
            assertEquals(10.dp, fit.railInset)
            assertEquals(20.dp, fit.railCorner)
            assertEquals(304.dp, fit.cellWidth)
            assertEquals(372.dp, fit.pageStart)
        }
    }

    @Test
    fun aTabletAtFullSizeKeepsItsMeasuredKeypad() {
        // upright: 120 x 71 keys, 8 apart, in a 342 panel
        val upright = KeypadFit(800.dp, wide = false, room = 1280.dp - 286.dp)
        assertFalse(upright.compact)
        assertEquals(1f, upright.scale)
        assertEquals(120.dp, upright.keyWidth)
        assertEquals(71.dp, upright.keyHeight)
        assertEquals(8.dp, upright.down)
        assertEquals(9.dp, upright.across)
        assertEquals(30.dp, upright.groups)
        assertEquals(342.dp, upright.height)
        assertTrue(upright.centred)
        // wide: 150 x 87.5, 9 apart, in a 457 panel from 47 down
        val wide = KeypadFit(1280.dp, wide = true, room = 800.dp - 286.dp)
        assertEquals(1f, wide.scale)
        assertEquals(150.dp, wide.keyWidth)
        assertEquals(87.5.dp, wide.keyHeight)
        assertEquals(9.dp, wide.down)
        assertEquals(457.dp, wide.height)
        assertEquals(47.dp, wide.top)
        // Backspace, Next and Done on the rows
        for (k in listOf(upright, wide)) {
            assertEquals(k.keyHeight, k.backspaceHeight)
            assertEquals(k.keyHeight, k.nextHeight)
            assertEquals(k.down, k.commandGap)
        }
    }

    @Test
    fun aNarrowWindowShowsTheSectionsAsARailOfIcons() {
        // a phone, split screen on a tablet (upright and on its side), the widest that still is
        for (width in listOf(360.dp, 411.dp, 400.dp, 636.dp, 700.dp)) {
            val fit = CanvasSheetFit(width)
            assertTrue("$width", fit.narrow)
            assertEquals(81.dp, fit.railWidth)
            assertEquals(0.dp, fit.railInset)
            assertEquals(0.dp, fit.railCorner)
            assertEquals(65.dp, fit.cellWidth)
            assertEquals(123.dp, fit.pageStart)
            // the page and the name start inside the window, the name clear of Cancel and Done
            assertTrue("$width", fit.pageStart + 160.dp <= width - 32.dp)
            assertTrue("$width", fit.nameWidth > 0.dp && fit.nameStart + fit.nameWidth <= width - 180.dp)
        }
        assertFalse(CanvasSheetFit(701.dp).narrow)
    }

    @Test
    fun theKeypadFitsEveryWindowAcross() {
        for (w in 300..2000) {
            val width = w.dp
            for (wide in listOf(false, true)) {
                val k = KeypadFit(width, wide, room = 600.dp)
                assertTrue("$width $wide: ${across(k)}", across(k) <= width - 16.dp + 0.01.dp)
                assertEquals("$width", width <= 420.dp, k.compact)
            }
        }
    }

    @Test
    fun aCompactKeypadHasFiveColumnsOfItsOwn() {
        // the reported phone, 360 across: 60 wide keys in a 320 cluster
        val phone = KeypadFit(360.dp, wide = false, room = 780.dp - 286.dp)
        assertTrue(phone.compact)
        near(60.dp, phone.keyWidth, "key")
        assertEquals(5.dp, phone.across)
        assertEquals(5.dp, phone.groups)
        assertEquals(60.dp, phone.keyHeight)
        assertEquals(5.dp, phone.down)
        assertEquals(12.dp, phone.top)
        assertEquals(276.dp, phone.height)
        near(320.dp, across(phone), "cluster")
        // Backspace a row tall, Next 9 under it to the second row's foot, Done two rows tall
        assertEquals(phone.keyHeight, phone.backspaceHeight)
        assertEquals(9.dp, phone.commandGap)
        assertEquals(56.dp, phone.nextHeight)
        near(phone.rows, phone.backspaceHeight + phone.commandGap + phone.nextHeight + phone.down + phone.keyHeight * 2 + phone.down, "commands")
        // narrower than 336, the cluster keeps 8 from each side
        near(304.dp, across(KeypadFit(320.dp, wide = false, room = 600.dp)), "slide over")
        // the rows it types into stay above it on a phone
        assertTrue(phone.height <= 780.dp - 286.dp)
        // its words: units short, ⌫ for Backspace at 28, Next and Done at 16 (of the tablet's 18)
        assertEquals(listOf("MM", "CM", "IN", "PX"), SizeUnit.entries.map { it.short })
        assertEquals("⌫", phone.backspace)
        assertEquals(28f / 18f, phone.backspaceText, 1e-6f)
        assertEquals(16f / 18f, phone.commandsText, 1e-6f)
    }

    @Test
    fun theSheetAndTheKeypadTurnCompactWhereTheReferenceDoes() {
        // the sections' rail at 700 across and narrower, the compact keypad at 420 and narrower
        assertTrue(CanvasSheetFit(700.dp).narrow)
        assertFalse(CanvasSheetFit(700.5.dp).narrow)
        assertTrue(KeypadFit(420.dp, wide = false, room = 600.dp).compact)
        assertFalse(KeypadFit(420.5.dp, wide = false, room = 600.dp).compact)
        // a tablet's keypad keeps its words
        val tablet = KeypadFit(800.dp, wide = false, room = 600.dp)
        assertEquals(listOf("Millimeters", "Centimeters", "Inches", "Pixels"), SizeUnit.entries.map { it.label })
        assertEquals("Backspace", tablet.backspace)
        assertEquals(1f, tablet.backspaceText)
        assertEquals(1f, tablet.commandsText)
    }

    @Test
    fun betweenCompactAndFullTheTabletsKeysNarrow() {
        // half a tablet on its side: the upright keypad a little narrower, as tall as ever
        val half = KeypadFit(636.dp, wide = false, room = 800.dp - 286.dp)
        assertFalse(half.compact)
        assertEquals((636f - 16f) / 678f, half.scale, 1e-4f)
        near(120.dp * half.scale, half.keyWidth, "key")
        assertEquals(71.dp, half.keyHeight)
        near(636.dp - 16.dp, across(half), "across")
        // short of room, shorter (to half)
        assertEquals(0.5f * 342f, KeypadFit(800.dp, wide = false, room = 100.dp).height.value, 0.01f)
    }
}
