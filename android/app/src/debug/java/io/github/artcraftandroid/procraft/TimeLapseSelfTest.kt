package io.github.artcraftandroid.procraft

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.media.MediaFormat
import android.media.MediaMetadataRetriever
import android.util.Log
import io.github.artcraftandroid.procraft.canvas.VideoFrames
import io.github.artcraftandroid.procraft.canvas.VideoWriter
import io.github.artcraftandroid.procraft.canvas.Yuv
import java.io.File
import kotlin.concurrent.thread

/**
 * Debug builds only: time-lapse videos made and read back on the device, every encoder for H.264
 * and HEVC at portrait and landscape sizes that aren't whole 16-pixel blocks, and every frame of
 * each read back with the app's decoder and with the system's reader. Each line in the log (tag
 * `TimeLapseCheck`): what was made, frames left unfilled, frames apart from what they were made of,
 * the largest jump between neighbours that aren't neighbours in what was drawn, and the times.
 *
 * `adb shell am broadcast -n io.github.artcraftandroid.procraft.debug/io.github.artcraftandroid.procraft.TimeLapseSelfTest`
 * (the app open, so its process stays while it runs; `--ei frames N` for another length).
 */
class TimeLapseSelfTest : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val frames = intent.getIntExtra("frames", 75)
        val dir = File(context.cacheDir, "lapse-check")
        thread(name = "time-lapse check") {
            try {
                run(dir, frames)
            } catch (e: Throwable) {
                Log.e(TAG, "the check stopped", e)
            } finally {
                dir.deleteRecursively()
            }
        }
    }

    private fun run(dir: File, frames: Int) {
        dir.deleteRecursively()
        dir.mkdirs()
        var wrong = 0
        val sizes = listOf(904 to 1280, 1280 to 904, 958 to 1280, 1280 to 852, 806 to 1076, 896 to 1280, 1280 to 1280)
        for (mime in listOf(MediaFormat.MIMETYPE_VIDEO_AVC, MediaFormat.MIMETYPE_VIDEO_HEVC)) for ((w, h) in sizes) {
            for (name in VideoWriter.encoderNames(mime, w, h)) {
                val out = File(dir, "$name-${w}x$h.mp4")
                val pixels = IntArray(w * h)
                val t0 = System.nanoTime()
                val made = VideoWriter.make(out, w, h, 30f, frames, mime, check = true, only = name) { i, px -> drawn(w, h, i, px); true }
                val t1 = System.nanoTime()
                if (made == VideoWriter.Made.NONE) {
                    Log.i(TAG, "$mime $name ${w}x$h: not made")
                    continue
                }
                // the app's decoder
                var unfilled = 0
                var apart = 0
                var read = 0
                var worstUnfilled = 0f
                VideoFrames.open(out.path)?.use { f ->
                    for (i in 0 until f.count) {
                        drawn(w, h, i, pixels)
                        val look = Yuv.cells(pixels, w, h)
                        f.read(i) { p, m ->
                            read++
                            val share = Yuv.unfilledShare(p)
                            worstUnfilled = maxOf(worstUnfilled, share)
                            if (share > Yuv.UNFILLED_MOST) unfilled++
                            if (Yuv.apart(Yuv.cells(p, m), look)) apart++
                        }
                    }
                }
                val t2 = System.nanoTime()
                // the system's reader, as replays read before
                val system = systemReader(out, w, h, frames, pixels)
                val t3 = System.nanoTime()
                val bad = made != VideoWriter.Made.CHECKED || read != frames || unfilled > 0 || apart > 0
                if (bad) wrong++
                Log.i(
                    TAG,
                    "$mime $name ${w}x$h: $made, decoder read $read of $frames, unfilled $unfilled (most ${"%.3f".format(worstUnfilled)}), " +
                        "apart $apart; system reader $system; make ${(t1 - t0) / 1_000_000} ms, decode ${(t2 - t1) / 1_000_000} ms, " +
                        "system ${(t3 - t2) / 1_000_000} ms" + if (bad) "  WRONG" else "",
                )
                out.delete()
            }
        }
        Log.i(TAG, "done: $wrong wrong")
    }

    /** Every frame read by the system's reader in runs, as the replay read them: frames read, green ones, ones apart. */
    private fun systemReader(out: File, w: Int, h: Int, frames: Int, pixels: IntArray): String {
        // (before Android 9 it reads no frames by number)
        if (android.os.Build.VERSION.SDK_INT < android.os.Build.VERSION_CODES.P) return "not before Android 9"
        val r = MediaMetadataRetriever()
        return try {
            r.setDataSource(out.path)
            var read = 0
            var green = 0
            var apart = 0
            var i = 0
            while (i < frames) {
                val run = runCatching { r.getFramesAtIndex(i, minOf(13, frames - i)) }.getOrNull().orEmpty()
                if (run.isEmpty()) break
                for ((k, b) in run.withIndex()) {
                    read++
                    val argb = IntArray(b.width * b.height)
                    b.copy(Bitmap.Config.ARGB_8888, false).also { it.getPixels(argb, 0, b.width, 0, 0, b.width, b.height) }.recycle()
                    if (argb.count { c -> (c shr 16 and 255) < 24 && (c and 255) < 24 && (c shr 8 and 255) in 105..165 } > argb.size / 200) green++
                    drawn(w, h, i + k, pixels)
                    if (b.width == w && b.height == h && Yuv.apart(Yuv.cells(argb, w, h), Yuv.cells(pixels, w, h))) apart++
                    b.recycle()
                }
                i += run.size
            }
            "read $read, green $green, apart $apart"
        } catch (e: Exception) {
            "failed: $e"
        } finally {
            r.release()
        }
    }

    /** Frame [i] of a drawing: white, a band of colour, and a stroke more each frame. */
    private fun drawn(w: Int, h: Int, i: Int, into: IntArray) {
        java.util.Arrays.fill(into, -1)
        for (y in h / 3 until h / 3 + h / 10) for (x in 0 until w) into[y * w + x] = (0xFF shl 24) or ((x * 255 / w) shl 16) or (90 shl 8) or 200
        for (k in 0..i) {
            val x0 = (k * 37) % (w - 40)
            val y0 = (k * 53) % (h - 120)
            val c = (0xFF shl 24) or (((k * 71) % 200) shl 16) or (((k * 29) % 120) shl 8) or ((k * 13) % 256)
            for (y in y0 until y0 + 120) for (x in x0 until x0 + 12) into[y * w + x] = c
        }
    }

    companion object {
        private const val TAG = "TimeLapseCheck"
    }
}
