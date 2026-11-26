package io.github.artcraftandroid.procraft.canvas

import android.graphics.Bitmap
import android.graphics.BitmapFactory

/**
 * What a picture [width] × [height] is read at to be kept [most] on its longer side: the most it
 * can be made smaller as it is read (a power of two) that leaves that side no more than an eighth
 * short of [most]. So a picture is read at less than twice the side it is kept at, and most
 * photos (4000 × 3000, 8000 × 6000) read in at about the size kept, with nothing to make smaller
 * after.
 */
internal fun pictureSample(width: Int, height: Int, most: Int): Int {
    val side = maxOf(width, height)
    val least = most - most / 8
    var sample = 1
    while (least > 0 && side / (sample * 2) >= least) sample *= 2
    return sample
}

/**
 * A picture for a Reference window, read by [decode] (it reads with the options given; null if it
 * can't): no bigger than the core keeps a window's picture (as big as the largest window shows
 * it), in 8 bits a channel (a picture of 16 bits reads in at twice the memory, past what the
 * screen will draw). Null if it can't be read.
 */
internal fun windowPicture(decode: (BitmapFactory.Options) -> Bitmap?): Bitmap? {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    decode(bounds)
    if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
    val most = io.github.artcraftandroid.procraft.core.referencePictureSize(bounds.outWidth.toUInt(), bounds.outHeight.toUInt())
    val read = decode(
        BitmapFactory.Options().apply {
            inSampleSize = pictureSample(bounds.outWidth, bounds.outHeight, maxOf(most.width, most.height).toInt())
            inPreferredConfig = Bitmap.Config.ARGB_8888
        },
    ) ?: return null
    val kept = io.github.artcraftandroid.procraft.core.referencePictureSize(read.width.toUInt(), read.height.toUInt())
    val (w, h) = kept.width.toInt().coerceAtLeast(1) to kept.height.toInt().coerceAtLeast(1)
    if (read.width == w && read.height == h && read.config == Bitmap.Config.ARGB_8888) return read
    // its colours as they were where it reads in 8 bits a channel; else as sRGB
    val space = read.colorSpace?.takeIf { read.config == Bitmap.Config.ARGB_8888 } ?: android.graphics.ColorSpace.get(android.graphics.ColorSpace.Named.SRGB)
    val out = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888, true, space)
    android.graphics.Canvas(out).drawBitmap(read, null, android.graphics.Rect(0, 0, w, h), android.graphics.Paint(android.graphics.Paint.FILTER_BITMAP_FLAG))
    read.recycle()
    return out
}
