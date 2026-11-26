package io.github.artcraftandroid.procraft.canvas

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.ByteBuffer
import kotlin.math.abs

/**
 * A time-lapse frame written for an encoder and read back from a decoder, in the layouts devices'
 * codecs give (strides and slice heights padded to blocks, planar and semi-planar, U or V first,
 * planes apart), at sizes that aren't whole 16-pixel blocks, portrait and landscape: nothing of a
 * frame is left unfilled (green), and what is left unfilled is found.
 */
class YuvTest {
    private val sizes = listOf(904 to 1280, 1280 to 904, 958 to 1280, 802 to 1280, 1280 to 852, 806 to 1076, 6 to 4, 2 to 2)

    private fun align(v: Int, a: Int) = (v + a - 1) / a * a

    /** A frame as art is: 2 x 2 blocks of one colour (so 4:2:0 keeps them), gradients and strokes over white. */
    private fun art(w: Int, h: Int, seed: Int = 1): IntArray = IntArray(w * h) { i ->
        val x = (i % w) / 2 * 2
        val y = (i / w) / 2 * 2
        val stroke = (x + y * 3 + seed * 7) % 97 < 9
        val r = if (stroke) 30 else (x * 255 / maxOf(1, w - 1))
        val g = if (stroke) 60 else (y * 255 / maxOf(1, h - 1))
        val b = if (stroke) 200 else ((x + y + seed * 31) * 3) % 256
        (0xFF shl 24) or (r shl 16) or (g shl 8) or b
    }

    /** A codec's layouts of a [w] x [h] frame: what it says (format, stride, slice height) and the buffer it gives. */
    private data class Laid(val name: String, val format: Int, val stride: Int, val slice: Int, val capacity: Int)

    private fun layouts(w: Int, h: Int): List<Laid> {
        val tight = w * h + 2 * ((w + 1) / 2) * ((h + 1) / 2)
        val s16 = align(w, 16)
        val h16 = align(h, 16)
        val s64 = align(w, 64)
        val h32 = align(h, 32)
        return listOf(
            Laid("NV12 tight", Yuv.SEMI_PLANAR, w, h, tight),
            // rows and planes padded to 16, as some tablets' codecs lay them out
            Laid("NV12 stride and slice to 16", Yuv.SEMI_PLANAR, s16, h16, s16 * h16 * 3 / 2),
            // stride to 128, slice to 32, room after, as others do
            Laid("NV12 stride 128, slice 32", Yuv.SEMI_PLANAR, align(w, 128), h32, align(w, 128) * h32 * 3 / 2 + 4096),
            Laid("packed NV12, stride to 64", Yuv.PACKED_SEMI_PLANAR, s64, h, s64 * h * 3 / 2),
            Laid("I420 stride and slice to 16", Yuv.PLANAR, s16, h16, s16 * h16 + 2 * ((s16 + 1) / 2) * ((h16 + 1) / 2)),
            Laid("I420 tight", Yuv.PLANAR, w, h, tight),
            // says nothing of its layout
            Laid("NV12, stride and slice not said", Yuv.SEMI_PLANAR, 0, 0, tight),
            // says a padded layout of a buffer that is tight
            Laid("NV12 said padded, buffer tight", Yuv.SEMI_PLANAR, s16 + 32, h16 + 32, tight),
        )
    }

    /** [p]'s picture read back as ARGB, as the encoder's BT.601 video range. */
    private fun readBack(p: YuvPicture): IntArray = IntArray(p.width * p.height).also { Yuv.toArgb(p, it, Yuv.Matrix(bt709 = false, full = false)) }

    private fun most(a: IntArray, b: IntArray): Int {
        var most = 0
        for (i in a.indices) for (shift in intArrayOf(16, 8, 0)) {
            most = maxOf(most, abs((a[i] shr shift and 255) - (b[i] shr shift and 255)))
        }
        return most
    }

    @Test
    fun aFrameIsWrittenWholeInEveryLayoutACodecSays() {
        val worst = HashMap<String, Int>()
        for ((w, h) in sizes) for (laid in layouts(w, h)) {
            val buffer = ByteBuffer.allocateDirect(laid.capacity)
            val p = Yuv.planesIn(buffer, laid.format, w, h, laid.stride, laid.slice)
            assertNotNull("${laid.name} ${w}x$h", p)
            p!!
            val source = art(w, h)
            Yuv.write(source, w, h, p)
            val back = readBack(p)
            val error = most(source, back)
            worst.merge(laid.name, error, ::maxOf)
            assertTrue("${laid.name} ${w}x$h: off by $error", error <= 3)
            assertEquals("${laid.name} ${w}x$h: some of it unfilled", 0f, Yuv.unfilledShare(p), 0f)
        }
        println("most a channel is off after writing and reading back, by layout: $worst")
    }

    /** The planes a codec hands as pictures: U and V taking turns in one plane (either first), or each in its own. */
    @Test
    fun aFrameIsWrittenWholeIntoTheCodecsOwnPlanes() {
        for ((w, h) in sizes) {
            val stride = align(w, 16)
            val rows = align(h, 16)
            val half = (w + 1) / 2
            val chromaRows = (h + 1) / 2
            val memory = ByteBuffer.allocateDirect(stride * rows * 3 / 2)
            fun at(offset: Int) = memory.duplicate().apply { clear(); position(offset) }.slice()
            val y = YuvPlane(at(0), stride, 1)
            val luma = stride * rows
            val kinds = mapOf(
                "U then V" to Pair(YuvPlane(at(luma), stride, 2), YuvPlane(at(luma + 1), stride, 2)),
                "V then U" to Pair(YuvPlane(at(luma + 1), stride, 2), YuvPlane(at(luma), stride, 2)),
                "apart" to Pair(
                    YuvPlane(ByteBuffer.allocateDirect(stride * chromaRows * 2), stride * 2, 2),
                    YuvPlane(ByteBuffer.allocateDirect(stride * chromaRows * 2), stride * 2, 2),
                ),
                "planar" to Pair(YuvPlane(ByteBuffer.allocateDirect(half * chromaRows), half, 1), YuvPlane(ByteBuffer.allocateDirect(half * chromaRows), half, 1)),
            )
            for ((kind, uv) in kinds) {
                val p = YuvPicture(y, uv.first, uv.second, 0, 0, w, h)
                val source = art(w, h, seed = kind.length)
                Yuv.write(source, w, h, p)
                val error = most(source, readBack(p))
                assertTrue("$kind ${w}x$h: off by $error", error <= 3)
                assertEquals("$kind ${w}x$h", 0f, Yuv.unfilledShare(p), 0f)
            }
        }
    }

    /**
     * A frame a codec reads only part of (given less than its padded layout holds, as a buffer
     * given back as width x height x 1.5 bytes is): the rest stays unfilled, green, and is found.
     * Given back whole, none is.
     */
    @Test
    fun aFrameGivenBackShortIsLeftPartlyUnfilledAndFound() {
        for ((w, h) in listOf(904 to 1280, 1280 to 852, 958 to 1280)) {
            val stride = align(w, 16) + 16
            val slice = align(h, 16)
            val capacity = stride * slice * 3 / 2
            val written = ByteBuffer.allocateDirect(capacity)
            val p = Yuv.planesIn(written, Yuv.SEMI_PLANAR, w, h, stride, slice)!!
            Yuv.write(art(w, h), w, h, p)
            // what a codec reading only the bytes it was told of has
            val told = w * h * 3 / 2
            val read = ByteBuffer.allocateDirect(capacity)
            for (i in 0 until told) read.put(i, written.get(i))
            val short = Yuv.unfilledShare(Yuv.planesIn(read, Yuv.SEMI_PLANAR, w, h, stride, slice)!!)
            // told of the whole buffer
            val whole = Yuv.unfilledShare(p)
            println("${w}x$h, stride $stride, slice $slice: given back $told of $capacity bytes, ${"%.1f".format(short * 100)}% of the frame unfilled; whole, ${"%.1f".format(whole * 100)}%")
            assertTrue("${w}x$h: $short", short > Yuv.UNFILLED_MOST)
            assertEquals(0f, whole, 0f)
        }
    }

    /** A frame no codec filled (all zero), or one with a band unfilled, is found; no colour at all is. */
    @Test
    fun unfilledIsFoundAndNoColourIsTakenForIt() {
        val (w, h) = 904 to 1280
        val all = ByteBuffer.allocateDirect(w * h * 3 / 2)
        assertEquals(1f, Yuv.unfilledShare(Yuv.planesIn(all, Yuv.SEMI_PLANAR, w, h, w, h)!!), 0f)
        // a band at the bottom, a tenth of the frame
        val band = ByteBuffer.allocateDirect(w * h * 3 / 2)
        val p = Yuv.planesIn(band, Yuv.SEMI_PLANAR, w, h, w, h)!!
        Yuv.write(art(w, h), w, h, p)
        for (i in w * h + w * (h / 2 * 9 / 10) until w * h * 3 / 2) band.put(i, 0)
        val share = Yuv.unfilledShare(p)
        assertTrue("$share", share in 0.08f..0.12f)
        // every colour (in steps of 5), as frames of one colour each
        var nearest = 255
        for (r in 0..255 step 5) for (g in 0..255 step 5) for (b in 0..255 step 5) {
            val c = (0xFF shl 24) or (r shl 16) or (g shl 8) or b
            val one = ByteBuffer.allocateDirect(6)
            val q = Yuv.planesIn(one, Yuv.SEMI_PLANAR, 2, 2, 2, 2)!!
            Yuv.write(intArrayOf(c, c, c, c), 2, 2, q)
            nearest = minOf(nearest, maxOf(one.get(4).toInt() and 255, one.get(5).toInt() and 255))
            assertEquals("rgb($r, $g, $b)", 0f, Yuv.unfilledShare(q, grid = 1), 0f)
        }
        println("the nearest any colour comes to unfilled chroma: $nearest (unfilled below 28)")
        assertTrue(nearest >= 40)
    }

    /** A frame read back through a video (a little loss) is its picture; one gone green, or a band of it, isn't. */
    @Test
    fun aFrameGoneWrongIsApartFromWhatItWasMadeOf() {
        for ((w, h) in listOf(904 to 1280, 1280 to 852)) {
            val source = art(w, h)
            val look = Yuv.cells(source, w, h)
            // through the YUV a video keeps
            val buffer = ByteBuffer.allocateDirect(w * h * 3 / 2)
            val p = Yuv.planesIn(buffer, Yuv.SEMI_PLANAR, w, h, w, h)!!
            Yuv.write(source, w, h, p)
            assertFalse(Yuv.apart(Yuv.cells(p, Yuv.Matrix(bt709 = false, full = false)), look))
            // a video's loss: each channel off by up to 8
            var seed = 7
            val lossy = IntArray(source.size) { i ->
                seed = seed * 1103515245 + 12345
                val d = (seed ushr 16) % 17 - 8
                val c = source[i]
                val ch = { s: Int -> ((c shr s and 255) + d).coerceIn(0, 255) }
                (0xFF shl 24) or (ch(16) shl 16) or (ch(8) shl 8) or ch(0)
            }
            assertFalse(Yuv.apart(Yuv.cells(lossy, w, h), look))
            // the next frame, a stroke more (40 x 200 pixels)
            val next = source.copyOf().also { for (y in 300 until 500) for (x in 200 until 240) it[y * w + x] = 0xFF101040.toInt() }
            assertFalse(Yuv.apart(Yuv.cells(next, w, h), look))
            // green all over, as a frame left unfilled shows
            val green = IntArray(source.size) { (0xFF shl 24) or (0 shl 16) or (135 shl 8) }
            assertTrue(Yuv.apart(Yuv.cells(green, w, h), look))
            // the bottom twentieth green
            val band = source.copyOf().also { for (i in w * (h * 19 / 20) until it.size) it[i] = (0xFF shl 24) or (135 shl 8) }
            assertTrue(Yuv.apart(Yuv.cells(band, w, h), look))
        }
    }

    /** What an encoder lists picks its input: flexible where it takes it, else a layout this writes; none for tiled ones only. */
    @Test
    fun anEncodersInputIsPickedFromWhatItLists() {
        // (0x7FC00002: a vendor's tiled NV12; 0x7F000789: the opaque surface format)
        assertEquals(Yuv.FLEXIBLE, Yuv.inputFormat(intArrayOf(0x7F000789, Yuv.SEMI_PLANAR, Yuv.FLEXIBLE)))
        assertEquals(Yuv.SEMI_PLANAR, Yuv.inputFormat(intArrayOf(0x7F000789, 0x7FC00002, Yuv.SEMI_PLANAR)))
        assertEquals(Yuv.PLANAR, Yuv.inputFormat(intArrayOf(0x7FC00002, Yuv.PLANAR)))
        assertNull(Yuv.inputFormat(intArrayOf(0x7FC00002, 0x7F000789)))
        // a tiled layout isn't written as if it were linear
        assertNull(Yuv.planesIn(ByteBuffer.allocateDirect(1 shl 20), 0x7FC00002, 64, 64, 64, 64))
        // nor one the buffer can't hold
        assertNull(Yuv.planesIn(ByteBuffer.allocateDirect(100), Yuv.SEMI_PLANAR, 64, 64, 64, 64))
    }

    /** The time-lapse's videos are made in whole 16-pixel blocks, the shape kept within a block. */
    @Test
    fun videoSizesAreWholeBlocks() {
        assertEquals(896 to 1280, VideoWriter.aligned(904f, 1280f, 16))
        assertEquals(1280 to 848, VideoWriter.aligned(1280f, 852.5f, 16))
        assertEquals(944 to 1280, VideoWriter.aligned(958.7f, 1280f, 16))
        assertEquals(904 to 1280, VideoWriter.aligned(904f, 1280f, 2))
        assertEquals(16 to 16, VideoWriter.aligned(3f, 5f, 16))
    }
}
