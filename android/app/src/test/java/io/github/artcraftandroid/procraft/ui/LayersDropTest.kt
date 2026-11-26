package io.github.artcraftandroid.procraft.ui

import androidx.compose.ui.geometry.Rect
import io.github.artcraftandroid.procraft.core.RowDrop
import org.junit.Assert.assertEquals
import org.junit.Test

class LayersDropTest {
    // four rows 69 high, 1 apart, top down: 1, 2, 3, 4; row 2 dragged
    private val rows = (1..4).associate { k -> k.toULong() to Rect(0f, (k - 1) * 70f, 360f, (k - 1) * 70f + 69f) }
    private val unit = setOf(2uL)

    private fun aside(place: RowDrop?) = (1..4).map { makingWay(it.toULong(), unit, place, rows, 1f) }

    @Test
    fun theRowsBetweenWhereTheDraggedRowWasAndWhereItLandsMakeRoomForIt() {
        // down above 4: 3 goes up into its place
        assertEquals(listOf(0f, 0f, -70f, 0f), aside(RowDrop.Above(4uL)))
        // up above 1: 1 goes down
        assertEquals(listOf(70f, 0f, 0f, 0f), aside(RowDrop.Above(1uL)))
        // under the last row of a group (3): 3 goes up, 4 stays
        assertEquals(listOf(0f, 0f, -70f, 0f), aside(RowDrop.Below(3uL)))
        // at the bottom: both under it go up
        assertEquals(listOf(0f, 0f, -70f, -70f), aside(RowDrop.Bottom))
    }

    @Test
    fun heldOntoARowOrOverItsOwnPlaceNoRowMoves() {
        assertEquals(listOf(0f, 0f, 0f, 0f), aside(RowDrop.Onto(3uL)))
        assertEquals(listOf(0f, 0f, 0f, 0f), aside(RowDrop.Stays))
        assertEquals(listOf(0f, 0f, 0f, 0f), aside(RowDrop.Above(3uL)))
        assertEquals(listOf(0f, 0f, 0f, 0f), aside(null))
    }
}
