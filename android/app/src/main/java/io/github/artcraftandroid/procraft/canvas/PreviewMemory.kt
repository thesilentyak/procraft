package io.github.artcraftandroid.procraft.canvas

/** Bytes of one of the library's sample strokes as the app keeps it: 530 x 162, four bytes a pixel. */
internal const val PREVIEW_BITMAP_BYTES = 530L * 162L * 4L

/** The fewest sample strokes kept as pictures (a few screens of rows) and the most (more than the stock library). */
internal const val PREVIEWS_KEPT_LEAST = 48
internal const val PREVIEWS_KEPT_MOST = 400

/**
 * How many of the library's sample strokes the app keeps as pictures (the last drawn; a row showing
 * one let go draws it again from the stroke the core keeps packed), on a device of [totalMem] bytes
 * with [availMem] bytes available now (ActivityManager.MemoryInfo's): a 64th of the device's memory's
 * worth, as the core's brushPreviewsKept gives, but no more than a 16th of what is available now, at
 * least [PREVIEWS_KEPT_LEAST] and at most [PREVIEWS_KEPT_MOST]. A phone of 4 GB with little left kept
 * 180 (62 MB) whatever it had available, and then closed opening a heavy set.
 */
internal fun previewBitmapsKept(totalMem: Long, availMem: Long, bitmapBytes: Long = PREVIEW_BITMAP_BYTES): Int {
    val each = bitmapBytes.coerceAtLeast(1)
    val budget = minOf(totalMem.coerceAtLeast(0) / 64, availMem.coerceAtLeast(0) / 16)
    return (budget / each).coerceIn(PREVIEWS_KEPT_LEAST.toLong(), PREVIEWS_KEPT_MOST.toLong()).toInt()
}
