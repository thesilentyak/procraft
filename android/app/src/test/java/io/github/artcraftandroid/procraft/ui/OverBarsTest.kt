package io.github.artcraftandroid.procraft.ui

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class OverBarsTest {
    private val side = Rect(0f, 300f, 115f, 1468f)

    @Test
    fun theTopBarAndTheSideBarAreOverTheCanvas() {
        // between two of the top bar's buttons, and on the side bar's body
        assertTrue(overBars(Offset(900f, 40f), 99f, side, hidden = false))
        assertTrue(overBars(Offset(60f, 700f), 99f, side, hidden = false))
        // the canvas below the bar and beside the side bar
        assertFalse(overBars(Offset(900f, 120f), 99f, side, hidden = false))
        assertFalse(overBars(Offset(200f, 700f), 99f, side, hidden = false))
    }

    @Test
    fun anOpenPanelIsOverTheCanvasUnderIt() {
        // the Actions panel hanging from its button: between its rows, and on its body
        val actions = Rect(120f, 99f, 900f, 1300f)
        assertTrue(overInterface(Offset(500f, 640f), 99f, side, hidden = false, covers = listOf(actions)))
        assertTrue(overInterface(Offset(130f, 1290f), 99f, side, hidden = false, covers = listOf(actions)))
        // the canvas beside and below it
        assertFalse(overInterface(Offset(1000f, 640f), 99f, side, hidden = false, covers = listOf(actions)))
        assertFalse(overInterface(Offset(500f, 1400f), 99f, side, hidden = false, covers = listOf(actions)))
        // closed: the canvas there again; and the bars count as before
        assertFalse(overInterface(Offset(500f, 640f), 99f, side, hidden = false, covers = emptyList()))
        assertTrue(overInterface(Offset(900f, 40f), 99f, side, hidden = false, covers = emptyList()))
        assertTrue(overInterface(Offset(60f, 700f), 99f, side, hidden = false, covers = listOf(actions)))
    }

    @Test
    fun withTheInterfaceHiddenItIsAllCanvas() {
        assertFalse(overBars(Offset(900f, 40f), 99f, side, hidden = true))
        assertFalse(overBars(Offset(60f, 700f), 99f, null, hidden = true))
    }
}
