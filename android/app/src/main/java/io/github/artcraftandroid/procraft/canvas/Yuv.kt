package io.github.artcraftandroid.procraft.canvas

import java.nio.ByteBuffer

/**
 * One plane of a YUV 4:2:0 picture: its samples from [buffer]'s start, [rowStride] bytes from one row
 * to the next and [pixelStride] from one sample to the next (2 where U and V take turns in one plane).
 */
class YuvPlane(val buffer: ByteBuffer, val rowStride: Int, val pixelStride: Int)

/**
 * A YUV 4:2:0 picture's planes as a codec lays them out, and where the picture is in them: from
 * [left], [top] (in luma samples), [width] x [height].
 */
class YuvPicture(
    val y: YuvPlane, val u: YuvPlane, val v: YuvPlane,
    val left: Int, val top: Int, val width: Int, val height: Int,
)

/**
 * Pictures to and from the YUV 4:2:0 layouts video codecs take and give, with nothing of Android's in
 * it: what the encoder is given ([write]), what a decoded frame shows ([toArgb]), whether a codec left
 * a frame unfilled ([unfilledShare]), and a frame's look in a few numbers to tell a frame made right
 * from one made wrong ([cells], [apart]).
 */
object Yuv {
    // the codecs' colour formats (MediaCodecInfo.CodecCapabilities' numbers)
    const val PLANAR = 19
    const val PACKED_PLANAR = 20
    const val SEMI_PLANAR = 21
    const val PACKED_SEMI_PLANAR = 39
    const val TI_PACKED_SEMI_PLANAR = 0x7f000100
    const val FLEXIBLE = 0x7f420888

    /**
     * The input format to ask of an encoder that lists [formats]: the flexible one where it takes it
     * (the codec then hands a picture laid out as it says), else a planar or semi-planar layout this
     * writes by the codec's stride and slice height; null when it lists none of these (tiled
     * layouts and vendors' own), when flexible is asked anyway.
     */
    fun inputFormat(formats: IntArray): Int? =
        listOf(FLEXIBLE, SEMI_PLANAR, PACKED_SEMI_PLANAR, TI_PACKED_SEMI_PLANAR, PLANAR, PACKED_PLANAR)
            .firstOrNull { it in formats }

    private fun semiPlanar(format: Int) = format == SEMI_PLANAR || format == PACKED_SEMI_PLANAR || format == TI_PACKED_SEMI_PLANAR
    private fun planar(format: Int) = format == PLANAR || format == PACKED_PLANAR

    /**
     * A [width] x [height] picture laid out in [buffer] as [colorFormat] says, rows [stride] bytes
     * apart and [sliceHeight] rows to the luma plane, as the codec's format says (the picture's own
     * size where it says less, or nothing: 0). Where the buffer is too small for that layout, the one
     * packed tight is taken if it fits (a codec that says a padded layout of a buffer that isn't).
     * Null for a layout this doesn't know (tiled, a vendor's) or that the buffer can't hold.
     */
    fun planesIn(buffer: ByteBuffer, colorFormat: Int, width: Int, height: Int, stride: Int, sliceHeight: Int): YuvPicture? {
        if (width <= 0 || height <= 0 || !(semiPlanar(colorFormat) || planar(colorFormat))) return null
        val half = (width + 1) / 2
        val rows = (height + 1) / 2
        val tries = listOf(maxOf(stride, width) to maxOf(sliceHeight, height), width to height).distinct()
        for ((s, sh) in tries) {
            val luma = s.toLong() * sh
            val (u, v, need) = if (semiPlanar(colorFormat)) {
                Triple(luma, luma + 1, luma + s.toLong() * (rows - 1) + 2L * half)
            } else {
                val cs = (s + 1) / 2
                val chroma = cs.toLong() * ((sh + 1) / 2)
                Triple(luma, luma + chroma, luma + chroma + cs.toLong() * (rows - 1) + half)
            }
            if (need > buffer.capacity()) continue
            val chromaStride = if (semiPlanar(colorFormat)) s else (s + 1) / 2
            val chromaStep = if (semiPlanar(colorFormat)) 2 else 1
            return YuvPicture(
                YuvPlane(slice(buffer, 0), s, 1),
                YuvPlane(slice(buffer, u.toInt()), chromaStride, chromaStep),
                YuvPlane(slice(buffer, v.toInt()), chromaStride, chromaStep),
                0, 0, width, height,
            )
        }
        return null
    }

    /** [buffer] from [at] to its end, as a buffer of its own (the same memory). */
    private fun slice(buffer: ByteBuffer, at: Int): ByteBuffer = buffer.duplicate().apply {
        clear()
        position(at)
    }.slice()

    /**
     * Whether [second]'s samples are [first]'s memory one byte on (U and V taking turns in one plane):
     * a byte written through one is read back through the other.
     */
    private fun takeTurns(first: ByteBuffer, second: ByteBuffer): Boolean {
        if (first.capacity() < 2 || second.capacity() < 1) return false
        return listOf(0x5A, 0xA5).all { mark ->
            first.put(1, mark.toByte())
            second.get(0) == mark.toByte()
        }
    }

    /**
     * ARGB pixels ([w] x [h], rows top to bottom) into [p]'s planes (BT.601, video range), every
     * sample of the picture written: a row at a time where a plane's samples are packed, and where
     * the chroma take turns in one plane (U then V, or V then U, as encoders mostly take it); else
     * one by one.
     */
    fun write(argb: IntArray, w: Int, h: Int, p: YuvPicture) {
        val y = p.y
        val u = p.u
        val v = p.v
        val yb = y.buffer
        val ub = u.buffer
        val vb = v.buffer
        val luma = ByteArray(w)
        val half = (w + 1) / 2
        val cus = ByteArray(half)
        val cvs = ByteArray(half)
        val packedChroma = u.pixelStride == 1 && v.pixelStride == 1
        val interleaved = u.pixelStride == 2 && v.pixelStride == 2 && u.rowStride == v.rowStride
        val vAfterU = interleaved && takeTurns(ub, vb)
        val uAfterV = interleaved && !vAfterU && takeTurns(vb, ub)
        // the plane that starts first takes the row; the other's last sample goes after it
        val (lead, trail) = if (uAfterV) vb to ub else ub to vb
        val (leading, trailing) = if (uAfterV) cvs to cus else cus to cvs
        val turns = ByteArray(half * 2)
        for (row in 0 until h) {
            val even = row % 2 == 0
            for (col in 0 until w) {
                val c = argb[row * w + col]
                val r = (c shr 16) and 255
                val g = (c shr 8) and 255
                val b = c and 255
                luma[col] = (((66 * r + 129 * g + 25 * b + 128) shr 8) + 16).toByte()
                if (even && col % 2 == 0) {
                    cus[col / 2] = (((-38 * r - 74 * g + 112 * b + 128) shr 8) + 128).toByte()
                    cvs[col / 2] = (((112 * r - 94 * g - 18 * b + 128) shr 8) + 128).toByte()
                }
            }
            if (y.pixelStride == 1) {
                yb.position(row * y.rowStride)
                yb.put(luma, 0, w)
            } else {
                for (col in 0 until w) yb.put(row * y.rowStride + col * y.pixelStride, luma[col])
            }
            if (!even) continue
            val at = row / 2
            if (packedChroma) {
                ub.position(at * u.rowStride)
                ub.put(cus, 0, half)
                vb.position(at * v.rowStride)
                vb.put(cvs, 0, half)
            } else if (vAfterU || uAfterV) {
                for (i in 0 until half) {
                    turns[2 * i] = leading[i]
                    turns[2 * i + 1] = trailing[i]
                }
                lead.position(at * u.rowStride)
                lead.put(turns, 0, half * 2 - 1)
                trail.put(at * u.rowStride + (half - 1) * 2, trailing[half - 1])
            } else {
                for (i in 0 until half) {
                    ub.put(at * u.rowStride + i * u.pixelStride, cus[i])
                    vb.put(at * v.rowStride + i * v.pixelStride, cvs[i])
                }
            }
        }
    }

    /**
     * How a frame's YUV becomes RGB: BT.709 or BT.601 ([bt709]), video range or [full], as 16.16
     * fixed-point factors.
     */
    class Matrix(bt709: Boolean, full: Boolean) {
        val lumaScale: Int
        val lumaFloor: Int
        val rv: Int
        val gu: Int
        val gv: Int
        val bu: Int

        init {
            val one = 65536.0
            val (kr, kb) = if (bt709) 0.2126 to 0.0722 else 0.299 to 0.114
            val kg = 1 - kr - kb
            // video range spreads 219 steps of luma and 224 of chroma over 255
            val ys = if (full) 1.0 else 255.0 / 219.0
            val cs = if (full) 1.0 else 255.0 / 224.0
            lumaScale = (ys * one).toInt()
            lumaFloor = if (full) 0 else 16
            rv = (2 * (1 - kr) * cs * one).toInt()
            bu = (2 * (1 - kb) * cs * one).toInt()
            gu = (2 * (1 - kb) * kb / kg * cs * one).toInt()
            gv = (2 * (1 - kr) * kr / kg * cs * one).toInt()
        }

        /** The opaque ARGB of luma [y] and chroma [u], [v] (0 to 255 each). */
        fun argb(y: Int, u: Int, v: Int): Int {
            val l = (y - lumaFloor) * lumaScale + 32768
            val cu = u - 128
            val cv = v - 128
            val r = ((l + rv * cv) shr 16).coerceIn(0, 255)
            val g = ((l - gu * cu - gv * cv) shr 16).coerceIn(0, 255)
            val b = ((l + bu * cu) shr 16).coerceIn(0, 255)
            return (0xFF shl 24) or (r shl 16) or (g shl 8) or b
        }
    }

    /** [p]'s picture as ARGB into [out] (its width x height, rows top to bottom), by [m]. */
    fun toArgb(p: YuvPicture, out: IntArray, m: Matrix) {
        val w = p.width
        val firstChroma = p.left shr 1
        val chromaCols = ((p.left + w - 1) shr 1) - firstChroma + 1
        val yRow = ByteArray((w - 1) * p.y.pixelStride + 1)
        val uRow = ByteArray((chromaCols - 1) * p.u.pixelStride + 1)
        val vRow = ByteArray((chromaCols - 1) * p.v.pixelStride + 1)
        var lastChromaRow = -1
        for (row in 0 until p.height) {
            val sy = p.top + row
            read(p.y, sy * p.y.rowStride + p.left * p.y.pixelStride, yRow)
            val cy = sy shr 1
            if (cy != lastChromaRow) {
                read(p.u, cy * p.u.rowStride + firstChroma * p.u.pixelStride, uRow)
                read(p.v, cy * p.v.rowStride + firstChroma * p.v.pixelStride, vRow)
                lastChromaRow = cy
            }
            val base = row * w
            for (col in 0 until w) {
                val c = ((p.left + col) shr 1) - firstChroma
                out[base + col] = m.argb(
                    yRow[col * p.y.pixelStride].toInt() and 255,
                    uRow[c * p.u.pixelStride].toInt() and 255,
                    vRow[c * p.v.pixelStride].toInt() and 255,
                )
            }
        }
    }

    /** [into]'s length of [plane]'s bytes from [at] (what the plane holds past its end left as it was). */
    private fun read(plane: YuvPlane, at: Int, into: ByteArray) {
        val b = plane.buffer
        if (at >= b.limit()) return
        b.position(at)
        b.get(into, 0, minOf(into.size, b.limit() - at))
    }

    /**
     * The share of [grid] x [grid] points across [p]'s picture whose chroma are both near zero: what
     * a frame (or a part of one) a codec never filled shows, green on screen. No colour gives it: U
     * near zero is yellow and V near zero cyan, and the nearest any colour comes to both is about 38
     * (full range; 49 in video range), pure green's.
     */
    fun unfilledShare(p: YuvPicture, grid: Int = 64): Float {
        var unfilled = 0
        for (j in 0 until grid) {
            val cy = (p.top + point(j, grid, p.height)) shr 1
            for (i in 0 until grid) {
                val cx = (p.left + point(i, grid, p.width)) shr 1
                val u = sample(p.u, cx, cy)
                val v = sample(p.v, cx, cy)
                if (u in 0 until UNFILLED && v in 0 until UNFILLED) unfilled++
            }
        }
        return unfilled / (grid * grid).toFloat()
    }

    /** Chroma below this in both is never a colour's (see [unfilledShare]). */
    private const val UNFILLED = 28

    /** A frame with more of it unfilled than this is one the codec got wrong. */
    const val UNFILLED_MOST = 0.005f

    /** Plane [plane]'s sample at column [x], row [y]; -1 past its end. */
    private fun sample(plane: YuvPlane, x: Int, y: Int): Int {
        val at = y * plane.rowStride + x * plane.pixelStride
        return if (at in 0 until plane.buffer.limit()) plane.buffer.get(at).toInt() and 255 else -1
    }

    /** The [i]th of [n] points spread evenly over [size] pixels, each in the middle of its share. */
    private fun point(i: Int, n: Int, size: Int): Int = ((2L * i + 1) * size / (2L * n)).toInt().coerceIn(0, size - 1)

    /** A frame's look is told by [CELLS] x [CELLS] cells, [POINTS] x [POINTS] points sampled in each. */
    const val CELLS = 16
    private const val POINTS = 6

    /**
     * A picture's look: the average R, G and B of each of [CELLS] x [CELLS] cells over it (as
     * [POINTS] x [POINTS] points in each sample it), whatever its size: a frame and a video made of it
     * have (nearly) the same.
     */
    fun cells(argb: IntArray, w: Int, h: Int): ByteArray =
        cellsBy(w, h) { x, y -> argb[y * w + x] }

    /** [cells] of [p]'s picture, as [m] makes it RGB. */
    fun cells(p: YuvPicture, m: Matrix): ByteArray = cellsBy(p.width, p.height) { x, y ->
        val sx = p.left + x
        val sy = p.top + y
        m.argb(
            sample(p.y, sx, sy).coerceAtLeast(0),
            sample(p.u, sx shr 1, sy shr 1).coerceAtLeast(0),
            sample(p.v, sx shr 1, sy shr 1).coerceAtLeast(0),
        )
    }

    private inline fun cellsBy(w: Int, h: Int, at: (Int, Int) -> Int): ByteArray {
        val n = CELLS * POINTS
        val sums = IntArray(CELLS * CELLS * 3)
        for (j in 0 until n) {
            val y = point(j, n, h)
            val row = j / POINTS * CELLS
            for (i in 0 until n) {
                val c = at(point(i, n, w), y)
                val cell = (row + i / POINTS) * 3
                sums[cell] += (c shr 16) and 255
                sums[cell + 1] += (c shr 8) and 255
                sums[cell + 2] += c and 255
            }
        }
        return ByteArray(sums.size) { (sums[it] / (POINTS * POINTS)).toByte() }
    }

    /**
     * Whether two pictures' [cells] are too far apart for one to be the other through a video's
     * loss: on average more than [MEAN_MOST] a channel, or more than [CELL_SHARE] of the cells off by
     * more than [CELL_MOST] in a channel (a band of a frame gone wrong).
     */
    fun apart(a: ByteArray, b: ByteArray): Boolean {
        if (a.size != b.size || a.isEmpty()) return true
        var total = 0L
        var off = 0
        for (cell in 0 until a.size / 3) {
            var most = 0
            for (k in 0 until 3) {
                val d = kotlin.math.abs((a[cell * 3 + k].toInt() and 255) - (b[cell * 3 + k].toInt() and 255))
                total += d
                most = maxOf(most, d)
            }
            if (most > CELL_MOST) off++
        }
        return total > MEAN_MOST * a.size || off > CELL_SHARE * (a.size / 3)
    }

    private const val MEAN_MOST = 10
    private const val CELL_MOST = 48
    private const val CELL_SHARE = 0.02f
}
