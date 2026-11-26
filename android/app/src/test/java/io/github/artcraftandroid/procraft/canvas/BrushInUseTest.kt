package io.github.artcraftandroid.procraft.canvas

import io.github.artcraftandroid.procraft.core.BrushInfo
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class BrushInUseTest {
    private fun brush(set: Int, index: Int) = BrushInfo(set.toUInt(), index.toUInt(), "brush $set-$index")

    @Test
    fun onlyTheBrushInUseShowsHighlightedAndOpensTheStudio() {
        // painting with set 0's fourth brush, set 1 (last chosen: its third) opened by a drag
        // over it: none of set 1's brushes is the one in use, so none shows chosen, and a tap on
        // its third makes it the brush rather than opening the studio
        val current = brush(0, 3)
        assertTrue(isBrushInUse(brush(0, 3), current))
        assertFalse(isBrushInUse(brush(1, 2), current))
        assertFalse(isBrushInUse(brush(1, 3), current))
        assertFalse(isBrushInUse(brush(0, 2), current))
        // once set 1 is opened its choice is the brush in use, and only it shows chosen
        val opened = brush(1, 2)
        assertTrue(isBrushInUse(brush(1, 2), opened))
        assertFalse(isBrushInUse(brush(0, 3), opened))
        // no brush yet (the library not read): none
        assertFalse(isBrushInUse(brush(0, 0), null))
    }
}
