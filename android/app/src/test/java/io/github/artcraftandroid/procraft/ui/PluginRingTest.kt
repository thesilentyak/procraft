package io.github.artcraftandroid.procraft.ui

import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.procraft.core.PanelBrush
import io.github.artcraftandroid.procraft.core.PanelRow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PluginRingTest {
    private val colours = PanelRow.Colours("C", listOf(1u, 2u, 3u), null)
    private val brushes = PanelRow.Brushes("B", listOf(PanelBrush(0u, 0u, "A")), null)

    @Test
    fun theFirstItemSitsAtTheTopAndTheRestGoClockwise() {
        val r = 60.dp
        assertEquals(0f, ringPlace(0, 4, r).x.value, 1e-3f)
        assertEquals(-60f, ringPlace(0, 4, r).y.value, 1e-3f)
        // a quarter round: to the right
        assertEquals(60f, ringPlace(1, 4, r).x.value, 1e-3f)
        assertEquals(0f, ringPlace(1, 4, r).y.value, 1e-3f)
        assertEquals(60f, ringPlace(2, 4, r).y.value, 1e-3f)
    }

    @Test
    fun itemsOnACrowdedRingShrinkSoTheyNeverTouch() {
        // few: as big as they come
        assertEquals(34f, ringItem(34.dp, ringRadius(0), 4).value, 1e-3f)
        // twelve colours on the first ring: smaller, with room between
        val small = ringItem(34.dp, ringRadius(0), 12)
        assertTrue("$small", small.value < 34f)
        val apart = ringPlace(0, 12, ringRadius(0)).let { a -> ringPlace(1, 12, ringRadius(0)).let { b -> kotlin.math.hypot((a.x - b.x).value, (a.y - b.y).value) } }
        assertTrue("$small apart $apart", apart - small.value >= 6f - 1e-3f)
    }

    @Test
    fun theRingStaysInTheWindowWhereItFits() {
        val reach = ringReach(listOf(colours, brushes))
        // its outer row of brushes and the margin past it
        assertEquals((62 + 58 + 26 + 12).toFloat(), reach.value, 1e-3f)
        assertEquals((62 + 17 + 12).toFloat(), ringReach(listOf(colours)).value, 1e-3f)
        // where the pen was, unless that's too near an edge
        assertEquals(DpOffset(500.dp, 400.dp), ringMiddle(DpOffset(500.dp, 400.dp), reach, 1200.dp, 800.dp))
        assertEquals(DpOffset(reach, 800.dp - reach), ringMiddle(DpOffset(3.dp, 799.dp), reach, 1200.dp, 800.dp))
        // a window too small for it: its middle
        assertEquals(100f, ringMiddle(DpOffset(3.dp, 3.dp), reach, 200.dp, 800.dp).x.value, 1e-3f)
    }
}
