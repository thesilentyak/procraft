package io.github.artcraftandroid.procraft.gallery

/** Thirty swatches a palette holds: ten across, three down (the shared core keeps them). */
const val PALETTE_SLOTS = 30

/** The most pixels a picture is read at for a palette made from it: past it, it's read smaller. */
const val PALETTE_PICTURE_MOST = 8_000_000L

/**
 * How much smaller a [width] × [height] picture is read for a palette made from it: whole (1), so
 * its colours come exactly as they are, unless it has more than [PALETTE_PICTURE_MOST] pixels;
 * then the least power of two that brings it within them.
 */
fun paletteSampleSize(width: Int, height: Int): Int {
    var sample = 1
    while ((width / sample).toLong() * (height / sample) > PALETTE_PICTURE_MOST) sample *= 2
    return sample
}

/**
 * [bitmap]'s pixels as the shared core takes a picture for a palette: four bytes a pixel (alpha,
 * red, green, blue), straight, in sRGB; read a strip of rows at a time.
 */
fun paletteArgb(bitmap: android.graphics.Bitmap): ByteArray {
    val (w, h) = bitmap.width to bitmap.height
    val out = java.nio.ByteBuffer.allocate(w * h * 4)
    val ints = out.asIntBuffer()
    val rows = maxOf(1, 65_536 / maxOf(1, w))
    val strip = IntArray(w * rows)
    var y = 0
    while (y < h) {
        val n = minOf(rows, h - y)
        bitmap.getPixels(strip, 0, w, 0, y, w, n)
        ints.put(strip, 0, w * n)
        y += n
    }
    return out.array()
}

/** A named set of swatches (0xRRGGBB, or null for an empty slot). */
data class Palette(val name: String, val colors: List<Int?>) {
    init {
        require(colors.size == PALETTE_SLOTS)
    }
}
