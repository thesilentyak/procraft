package io.github.artcraftandroid.procraft.canvas

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class StrokeSoundTest {
    @Test
    fun the_strokes_are_heard_only_with_the_canvas_shown_and_the_app_in_front() {
        val artwork = "artwork"
        assertEquals(artwork, strokeSoundHeard(artwork, inFront = true, onCanvas = true))
        // an artwork opening: the session is there before the canvas shows, so the sound must
        // follow again once it does (else it stays off until the plugins next change)
        assertNull(strokeSoundHeard(artwork, inFront = true, onCanvas = false))
        assertNull(strokeSoundHeard(artwork, inFront = false, onCanvas = true))
        assertNull(strokeSoundHeard<String>(null, inFront = true, onCanvas = true))
    }
}
