package io.github.artcraftandroid.procraft.ui

import io.github.artcraftandroid.procraft.core.LayoutPiece
import org.junit.Assert.assertEquals
import org.junit.Test

class FloatingSlotsTest {
    private val layers = LayoutPiece.LAYERS
    private val brushes = LayoutPiece.BRUSHES
    private val actions = LayoutPiece.ACTIONS
    private val adjustments = LayoutPiece.ADJUSTMENTS

    /** Where [piece]'s window is composed among the windows, with [order] showing. */
    private fun slot(piece: LayoutPiece, order: List<LayoutPiece>): Int = floatingWindowSlots(order).indexOfFirst { it.first == piece }

    @Test
    fun aTabTornOutOfTheLibrarysWindowLeavesItWhereItIsComposed() {
        // the library showing as a tab of the Layers panel's window: its window the only one drawn
        val joined = listOf(brushes)
        // either tab torn out: the Layers panel's window comes to show too (before the library's, as
        // the Layers panel comes first); the library's window, whose strip the drag is on, stays in its
        // slot (it was composed anew, ending the drag, while only the windows showing had slots)
        for (torn in listOf(listOf(brushes, layers), listOf(layers, brushes))) {
            assertEquals(slot(brushes, joined), slot(brushes, torn))
        }
        assertEquals(listOf(false, true), floatingWindowSlots(joined).take(2).map { it.second })
        assertEquals(listOf(true, true), floatingWindowSlots(listOf(brushes, layers)).take(2).map { it.second })
    }

    @Test
    fun eachWindowKeepsOneSlotWhicheverOthersShow() {
        val all = listOf(layers, brushes, actions, adjustments)
        // every set of windows showing, in either stacking order
        val sets = (0 until 16).map { bits -> all.filterIndexed { i, _ -> bits and (1 shl i) != 0 } }
        for (piece in all) {
            val slots = sets.flatMap { listOf(it, it.reversed()) }.map { slot(piece, it) }.toSet()
            assertEquals("$piece", 1, slots.size)
        }
        // and each shows exactly while it is in the order
        for (set in sets) {
            assertEquals(all.map { it in set }, floatingWindowSlots(set).map { it.second })
        }
    }
}
