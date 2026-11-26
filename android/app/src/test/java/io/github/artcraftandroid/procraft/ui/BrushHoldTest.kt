package io.github.artcraftandroid.procraft.ui

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import io.github.artcraftandroid.procraft.core.BrushDropOver
import io.github.artcraftandroid.procraft.core.BrushInfo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class BrushHoldTest {
    // the sets down the left, the shown set's (set 1) five brushes beside them in its list, 90 tall
    // and 10 apart
    private val sets = mapOf(0 to Rect(0f, 0f, 200f, 60f), 1 to Rect(0f, 70f, 200f, 130f), 2 to Rect(0f, 140f, 200f, 200f))
    private val list = Rect(210f, 0f, 630f, 800f)
    private val rows = (0 until 5).map { it to Rect(220f, it * 100f, 620f, it * 100f + 90f) }

    private fun over(x: Float, y: Float) = brushDropOver(Offset(x, y), sets, 1, 5, list, rows)

    // (what a held brush's move does, a drag or the list's, and when its menu shows, is the core's:
    // brush_hold.rs tests it)
    @Test
    fun aDraggedBrushLetGoIsOverTheHalfOfTheBrushUnderIt() {
        // over the fourth brush (300 to 390): its upper half, its lower half (the core puts it just
        // above or just below)
        assertEquals(BrushDropOver.OnBrush(1u, 3u, false), over(400f, 330f))
        assertEquals(BrushDropOver.OnBrush(1u, 3u, true), over(400f, 370f))
        // over a set's row, its own too (the core moves nothing there)
        assertEquals(BrushDropOver.OnSet(2u), over(100f, 150f))
        assertEquals(BrushDropOver.OnSet(1u), over(100f, 100f))
        // between the sets, or off the lists: nothing
        assertNull(over(100f, 65f))
        assertNull(over(700f, 145f))
    }

    @Test
    fun letGoInAGapOrBelowTheLastBrushTheNearestBrushTakesIt() {
        // the gap under the second brush (190 to 200): nearer it, below it; nearer the third, above it
        assertEquals(BrushDropOver.OnBrush(1u, 1u, true), over(400f, 193f))
        assertEquals(BrushDropOver.OnBrush(1u, 2u, false), over(400f, 198f))
        // the list's room below the last brush: after it
        assertEquals(BrushDropOver.OnBrush(1u, 4u, true), over(400f, 700f))
    }

    @Test
    fun anEmptySetsListTakesThemAndTheRecentSetsListNothing() {
        // a set opened under them with no brushes: its list itself
        assertEquals(BrushDropOver.InList(3u, 0u), brushDropOver(Offset(400f, 300f), sets, 3, 0, list, emptyList()))
        // its rows not laid out yet: the list, after the brushes it holds
        assertEquals(BrushDropOver.InList(1u, 5u), brushDropOver(Offset(400f, 300f), sets, 1, 5, list, emptyList()))
        // the Recent set's list (no set shown): nothing, its rows or not
        assertNull(brushDropOver(Offset(400f, 330f), sets, null, 5, list, rows))
    }

    /**
     * Why the drop reads the library afresh as it is let go: a state given a newer reference to the
     * same local function keeps the first (they are equal), so the drop the gesture held that way
     * saw the set shown when the library first appeared, not one opened since, and let go on a
     * brush found nothing there.
     */
    @Test
    fun aStateGivenANewerReferenceToALocalFunctionKeepsTheFirst() {
        fun dropFor(shown: Int): () -> Int {
            fun drop() = shown
            return ::drop
        }
        val held = androidx.compose.runtime.mutableStateOf(dropFor(1))
        held.value = dropFor(2)
        assertEquals(1, held.value())
    }

    @Test
    fun brushesHeldOverAnotherSetUntilItOpensAreOverItsBrushes() {
        // set 2 opened under brushes of set 1: over one of its brushes, that brush of set 2 (the core
        // copies them there, a different set from theirs)
        val opened = (0 until 3).map { it to Rect(220f, it * 100f, 620f, it * 100f + 90f) }
        assertEquals(BrushDropOver.OnBrush(2u, 1u, false), brushDropOver(Offset(400f, 125f), sets, 2, 3, list, opened))
        // it opens only after a hold as long as the original's (1.76 s measured)
        assertTrue(SPRING_LOAD_MS in 1600L..2000L)
    }

    /**
     * A row whose brush was held to its menu doesn't slide open: a sideways move late in the hold is
     * the list's (the core's hold rule), so let go it is as it was, its buttons shut and nothing
     * picked; a press that never showed the menu slides it as ever.
     */
    @Test
    fun aRowHeldToItsMenuLetGoSlidSidewaysStaysAsItWas() {
        // buttons 400 wide, the pick 60 to the right: slid 200 left (past a third, 133), let go
        assertEquals(false to true, brushSlideEnd(-200f, 400f, 60f, open = false, slides = true))
        assertEquals(false to false, brushSlideEnd(-200f, 400f, 60f, open = false, slides = false))
        // slid 40 right (past half the reach, 30): picked, unless held to the menu
        assertEquals(true to false, brushSlideEnd(40f, 400f, 60f, open = false, slides = true))
        assertEquals(false to false, brushSlideEnd(40f, 400f, 60f, open = false, slides = false))
        // open already and held: still open
        assertEquals(false to true, brushSlideEnd(-400f, 400f, 60f, open = true, slides = false))
        // a short slide (under a third) shuts again
        assertEquals(false to false, brushSlideEnd(-100f, 400f, 60f, open = false, slides = true))
    }

    @Test
    fun aRowSlidesNoFurtherThanItsButtonsOrThePick() {
        assertEquals(-400f, brushSlide(-390f, -30f, 400f, 60f, open = false), 0f)
        assertEquals(60f, brushSlide(50f, 30f, 400f, 60f, open = false), 0f)
        assertEquals(-150f, brushSlide(-120f, -30f, 400f, 60f, open = false), 0f)
        // open, it goes no further right than shut
        assertEquals(0f, brushSlide(-10f, 30f, 400f, 60f, open = true), 0f)
    }

    @Test
    fun aPickedBrushHeldTakesThePickedOnesWithItInTheirOrder() {
        val set = (0 until 6).map { BrushInfo(1u, it.toUInt(), "Brush $it") }
        // not picked: it alone; picked, or the current brush while others are: all of them in order
        assertEquals(listOf(set[2]), draggedTogether(set[2], emptyList(), false, set))
        assertEquals(listOf(set[2]), draggedTogether(set[2], listOf("1-4"), false, set))
        assertEquals(listOf(set[1], set[4], set[5]), draggedTogether(set[4], listOf("1-5", "1-1", "1-4"), false, set))
        assertEquals(listOf(set[0], set[3]), draggedTogether(set[0], listOf("1-3"), true, set))
    }
}
