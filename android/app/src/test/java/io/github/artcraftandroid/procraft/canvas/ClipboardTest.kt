package io.github.artcraftandroid.procraft.canvas

import org.junit.Assert.assertEquals
import org.junit.Test

class ClipboardTest {
    @Test
    fun anotherAppsPictureCopiedAfterTheCopyHereIsPasted() {
        assertEquals(true, pastesSystemPicture(false, systemAt = 2_000, ownCopy = true, ownAt = 1_000, ownPutAt = 1_005))
        // nothing copied here: whatever picture the system holds
        assertEquals(true, pastesSystemPicture(false, systemAt = 10, ownCopy = false, ownAt = 0, ownPutAt = null))
    }

    @Test
    fun theCopyHereIsPastedOverWhatCameBeforeIt() {
        assertEquals(false, pastesSystemPicture(false, systemAt = 900, ownCopy = true, ownAt = 1_000, ownPutAt = null))
        // its own picture there
        assertEquals(false, pastesSystemPicture(true, systemAt = 2_000, ownCopy = true, ownAt = 1_000, ownPutAt = 1_005))
    }

    @Test
    fun aSystemThatPutsItsOwnCopyInPlaceOfOursStillPastesOurs() {
        // put there after the copy was made, with a picture of the system's own: the time it gave
        // ours tells it is ours
        assertEquals(false, pastesSystemPicture(false, systemAt = 1_005, ownCopy = true, ownAt = 1_000, ownPutAt = 1_005))
    }

    @Test
    fun aCopyTheSystemRefusedIsStillPastedOverAnOlderPicture() {
        // the picture another app put there before, still there: the copy here is newer
        assertEquals(false, pastesSystemPicture(false, systemAt = 500, ownCopy = true, ownAt = 1_000, ownPutAt = null))
        // an earlier copy of ours went there, this one didn't
        assertEquals(false, pastesSystemPicture(false, systemAt = 800, ownCopy = true, ownAt = 1_000, ownPutAt = 800))
    }
}
