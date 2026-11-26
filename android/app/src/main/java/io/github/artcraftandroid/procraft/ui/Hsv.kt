package io.github.artcraftandroid.procraft.ui

import kotlin.math.roundToInt

// Hue, saturation and brightness worked out here, in a file of their own: a colour picker kept
// for several colours (HeldColor) uses them without the colour panel's file, whose pictures come
// from the core.

/** [rgb] (0xRRGGBB) as hue, saturation and brightness, 0..1 each (a grey's hue 0). */
internal fun rgbToHsv(rgb: Int): FloatArray {
    val r = ((rgb shr 16) and 0xFF) / 255f
    val g = ((rgb shr 8) and 0xFF) / 255f
    val b = (rgb and 0xFF) / 255f
    val most = maxOf(r, g, b)
    val span = most - minOf(r, g, b)
    val hue = when {
        span <= 0f -> 0f
        most == r -> ((g - b) / span / 6f + 1f) % 1f
        most == g -> ((b - r) / span + 2f) / 6f
        else -> ((r - g) / span + 4f) / 6f
    }
    return floatArrayOf(hue, if (most > 0f) span / most else 0f, most)
}

/** Hue, saturation and brightness (0..1 each) as 0xRRGGBB. */
internal fun hsvToRgb(h: Float, s: Float, v: Float): Int {
    val six = (((h % 1f) + 1f) % 1f) * 6f
    val sector = six.toInt().coerceIn(0, 5)
    val f = six - sector
    val p = v * (1f - s)
    val q = v * (1f - s * f)
    val t = v * (1f - s * (1f - f))
    val (r, g, b) = when (sector) {
        0 -> Triple(v, t, p)
        1 -> Triple(q, v, p)
        2 -> Triple(p, v, t)
        3 -> Triple(p, q, v)
        4 -> Triple(t, p, v)
        else -> Triple(v, p, q)
    }
    fun byte(x: Float) = (x.coerceIn(0f, 1f) * 255f).roundToInt()
    return (byte(r) shl 16) or (byte(g) shl 8) or byte(b)
}
