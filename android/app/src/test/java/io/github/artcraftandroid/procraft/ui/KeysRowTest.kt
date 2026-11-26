package io.github.artcraftandroid.procraft.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class KeysRowTest {
    @Test
    fun theKeysEndWhereTheRowEndsAndTheLabelHasAllTheRoomBeforeThem() {
        // a plugin panel's row on the Legion: 720 px inside its padding, the keys 12 dp (30 px) on
        val (most, at) = keysRowPlaces(720, 105, 30)
        assertEquals(615, at)
        assertEquals(585, most)
        // "Copy the canvas" (about 300 px) shows whole beside Ctrl+A: half the room (292) cut it short
        assertTrue(300 <= most)
        // whatever the keys' width, they end where the row does, the label up to the gap before them
        for (keys in listOf(0, 24, 105, 458, 690, 720)) {
            val (labelMost, keysAt) = keysRowPlaces(720, keys, 30)
            assertEquals(720, keysAt + keys)
            assertEquals(maxOf(0, 720 - keys - 30), labelMost)
        }
        // keys wider than the row begin at its start, and leave the label no room
        assertEquals(0 to 0, keysRowPlaces(720, 900, 30))
    }
}
