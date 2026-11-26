package io.github.artcraftandroid.procraft.canvas

import org.junit.Assert.assertEquals
import org.junit.Test

class ImportFileSizeTest {
    @Test
    fun aBrushFilesSizeShowsInWholeMegabytesPastTenAndTenthsBelow() {
        assertEquals("199 MB", CanvasViewModel.fileSize(208_728_806))
        assertEquals("237 MB", CanvasViewModel.fileSize(248_500_000))
        assertEquals("4.2 MB", CanvasViewModel.fileSize(4_404_019))
        assertEquals("380 KB", CanvasViewModel.fileSize(389_120))
        assertEquals("1 KB", CanvasViewModel.fileSize(12))
    }
}
