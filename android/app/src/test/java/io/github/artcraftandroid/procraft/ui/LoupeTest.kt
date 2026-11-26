package io.github.artcraftandroid.procraft.ui

import androidx.compose.ui.unit.IntOffset
import io.github.artcraftandroid.procraft.canvas.LOUPE_LIFT_DP
import io.github.artcraftandroid.procraft.canvas.PickState
import io.github.artcraftandroid.procraft.canvas.loupeLift
import org.junit.Assert.assertEquals
import org.junit.Test

class LoupeTest {
    @Test
    fun aFingersLoupeIsFortyAboveItAndAPensOnItsTip() {
        assertEquals(40f, LOUPE_LIFT_DP)
        assertEquals(40f, loupeLift(pen = false))
        assertEquals(0f, loupeLift(pen = true))
        // a pick opened without saying is a finger's
        assertEquals(40f, PickState(0f, 0f, null, null).lift)
    }

    @Test
    fun theLoupesMiddleIsWhereThePointIsLifted() {
        // two pixels a dp: a finger's 80 pixels above the point, a pen's on it
        assertEquals(IntOffset(300, 120), loupeMiddle(PickState(300f, 200f, null, null, lift = loupeLift(pen = false)), 2f))
        assertEquals(IntOffset(300, 200), loupeMiddle(PickState(300f, 200f, null, null, lift = loupeLift(pen = true)), 2f))
        // on the reference window's picture as on the canvas
        assertEquals(IntOffset(10, 0), loupeMiddle(PickState(10f, 120f, null, null, fromReference = true, lift = loupeLift(pen = false)), 3f))
    }
}
