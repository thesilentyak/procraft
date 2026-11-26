package io.github.artcraftandroid.procraft.canvas

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.nio.file.Files

class VideoWriterTest {
    /** A share of the frames kept in one video needs nothing of the device's media system. */
    @Test
    fun oneVideoIsCopiedAsItIs() {
        val dir = Files.createTempDirectory("join").toFile()
        val part = File(dir, "segment-1.mp4").apply { writeBytes(byteArrayOf(1, 2, 3)) }
        val out = File(dir, "out.mp4").apply { writeBytes(byteArrayOf(9)) }
        assertTrue(VideoWriter.join(listOf(part), out, 30f))
        assertArrayEquals(byteArrayOf(1, 2, 3), out.readBytes())
        dir.deleteRecursively()
    }

    /**
     * An encoder that stops taking frames or giving them back is given up on 10 s after it last did
     * (the export said failed in the log, the next encoder tried), never sooner; each frame it
     * takes or gives back starts the wait again. Before, an export waited on it for ever.
     */
    @Test
    fun aStalledEncoderIsGivenUpOnTenSecondsAfterItLastDidAnything() {
        var clock = 5_000L
        val patience = VideoWriter.Patience(now = { clock })
        assertEquals(10_000L, VideoWriter.Patience.MOST_MS)
        clock = 14_999
        assertFalse(patience.spent())
        clock = 15_000
        assertFalse(patience.spent())
        clock = 15_001
        assertTrue(patience.spent())
        // a frame taken: waited for afresh
        patience.progressed()
        clock = 25_001
        assertFalse(patience.spent())
        clock = 25_002
        assertTrue(patience.spent())
    }

    @Test
    fun oneVideoThatIsGoneMakesNothing() {
        val dir = Files.createTempDirectory("join").toFile()
        val out = File(dir, "out.mp4")
        assertFalse(VideoWriter.join(listOf(File(dir, "segment-1.mp4")), out, 30f))
        assertFalse(out.exists())
        dir.deleteRecursively()
    }
}
