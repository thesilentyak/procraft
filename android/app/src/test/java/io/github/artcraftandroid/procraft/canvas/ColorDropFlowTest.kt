package io.github.artcraftandroid.procraft.canvas

import androidx.compose.ui.geometry.Offset
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.Executors
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

class ColorDropFlowTest {
    /** The session's side of a drop, as slow as a whole layer's fill on a slow device. */
    private class Pours(
        val dropMs: Long = 150,
        val slideMs: Long = 40,
        @Volatile var outcome: DropPours.Dropped = DropPours.Dropped.POURED,
    ) : DropPours {
        val calls: MutableList<String> = java.util.Collections.synchronizedList(mutableListOf())

        /** The thread each touch's fill was made on. */
        val threads: MutableList<Thread> = java.util.Collections.synchronizedList(mutableListOf())

        override fun drop(at: Offset, colour: Int, threshold: Float): DropPours.Dropped {
            calls += "drop ${at.x.toInt()},${at.y.toInt()}"
            Thread.sleep(dropMs)
            return outcome
        }

        override fun pour(at: Offset, r: Float, g: Float, b: Float, threshold: Float): Boolean {
            calls += "pour ${at.x.toInt()},${at.y.toInt()}"
            threads += Thread.currentThread()
            Thread.sleep(dropMs)
            return outcome == DropPours.Dropped.POURED
        }

        override fun threshold(threshold: Float) {
            calls += "threshold $threshold"
            Thread.sleep(slideMs)
        }

        override fun end(keep: Boolean) {
            calls += "end $keep"
        }
    }

    /** What the fill's thread hands back to the main thread, run as the test drains it. */
    private val main = LinkedBlockingQueue<() -> Unit>()
    private val fillThread = Executors.newSingleThreadExecutor()

    private fun flow(pours: Pours) = ColorDropFlow(pours, run = { fillThread.execute(it) }, post = { main.put(it) })

    /** Runs what comes back on the main thread until [done] holds (a few seconds at most). */
    private fun drain(done: () -> Boolean) {
        val until = System.nanoTime() + 5_000_000_000L
        while (!done() && System.nanoTime() < until) main.poll(5, TimeUnit.MILLISECONDS)?.invoke()
        assertTrue("the fills didn't come back", done())
    }

    /** How long [block] held the calling thread, milliseconds. */
    private fun held(block: () -> Unit): Double {
        val t = System.nanoTime()
        block()
        return (System.nanoTime() - t) / 1e6
    }

    @After
    fun stop() {
        fillThread.shutdownNow()
    }

    @Test
    fun theMainThreadNeverWaitsForAWholeLayersFill() {
        // the code run once first (classes loaded, the fill's thread started), as an app that has run
        val warm = flow(Pours(dropMs = 0, slideMs = 0))
        var warmed = false
        warm.start()
        warm.begin(Offset(1f, 1f), 0, 0.3f, poured = {}, blocked = {})
        warm.slide(0.4f)
        warm.release(null, 0, 0.4f, blocked = {}) { warmed = true }
        drain { warmed && warm.pending == 0 }
        val pours = Pours()
        // as before: the main thread made the fill itself and waited for it
        val before = held { pours.drop(Offset(40f, 40f), 0xFF0000, 0.3f) }
        pours.calls.clear()
        // now: it hands the drop, five slides and the release over, and goes on
        val drops = flow(pours)
        var kept: Boolean? = null
        drops.start()
        val waits = mutableListOf<Double>()
        waits += held { drops.begin(Offset(40f, 40f), 0xFF0000, 0.3f, poured = {}, blocked = {}) }
        for (k in 1..5) waits += held { drops.slide(0.3f + k * 0.05f) }
        waits += held { drops.release(null, 0xFF0000, 0.55f, blocked = {}) { kept = it } }
        // and what comes back is quick to take in
        val until = System.nanoTime() + 5_000_000_000L
        while (kept == null && System.nanoTime() < until) main.poll(5, TimeUnit.MILLISECONDS)?.let { waits += held(it) }
        val longest = waits.max()
        println("the main thread's calls: " + waits.joinToString { "%.2f".format(java.util.Locale.ROOT, it) } + " ms")
        println("a whole layer's drop (${pours.dropMs} ms), five slides and its release: the main thread waited %.1f ms before, %.2f ms now".format(java.util.Locale.ROOT, before, longest))
        assertTrue("the main thread waited $longest ms", longest < 8.0)
        assertTrue(before >= pours.dropMs.toDouble())
        assertEquals(true, kept)
        assertEquals("drop 40,40", pours.calls.first())
        assertEquals("end true", pours.calls.last())
    }

    @Test
    fun aDropItsThresholdSlidLetGoAndUndoneGoInThatOrder() {
        val pours = Pours()
        val drops = flow(pours)
        var kept: Boolean? = null
        drops.start()
        drops.begin(Offset(100f, 200f), 0x00FF00, 0.4f, poured = {}, blocked = {})
        assertEquals(ColorDropFlow.Fill.PENDING, drops.fill)
        // slid while the fill is being made: redone after it, at the newest threshold
        for (k in 1..6) drops.slide(0.4f + k * 0.05f)
        drops.release(null, 0x00FF00, 0.7f, blocked = {}) { kept = it }
        // undone before the fill is back: undone once it is (and so takes the fill back)
        var undone = false
        assertTrue(drops.pending > 0)
        drops.afterFills {
            pours.calls += "undo"
            undone = true
        }
        drain { undone }
        assertEquals(true, kept)
        val calls = pours.calls.toList()
        assertEquals("drop 100,200", calls.first())
        assertEquals(listOf("end true", "undo"), calls.takeLast(2))
        val slides = calls.filter { it.startsWith("threshold") }
        assertTrue("$slides", slides.size in 1..6)
        assertEquals(0.7f, slides.last().removePrefix("threshold ").toFloat(), 1e-4f)
        assertEquals(0, drops.pending)
    }

    @Test
    fun aSecondDropWhileTheFirstIsStillFillingWaitsItsTurnAndKeepsItsOwnState() {
        val pours = Pours()
        val drops = flow(pours)
        val shown = mutableListOf<Int>()
        val kept = mutableListOf<Pair<Int, Boolean>>()
        // the first comes to rest and is let go at once, its fill still being made
        drops.start()
        drops.begin(Offset(10f, 10f), 0xFF0000, 0.3f, poured = { shown += 1 }, blocked = {})
        drops.release(null, 0xFF0000, 0.3f, blocked = {}) { kept += 1 to it }
        // the second comes to rest meanwhile
        drops.start()
        drops.begin(Offset(500f, 500f), 0x0000FF, 0.3f, poured = { shown += 2 }, blocked = {})
        assertEquals(ColorDropFlow.Fill.PENDING, drops.fill)
        drain { shown.contains(2) }
        // the first was kept as it came back; its late answer left the second alone
        assertEquals(listOf(1 to true), kept)
        assertEquals(listOf(2), shown)
        assertEquals(ColorDropFlow.Fill.POURED, drops.fill)
        drops.release(null, 0x0000FF, 0.3f, blocked = {}) { kept += 2 to it }
        drain { kept.size == 2 && drops.pending == 0 }
        assertEquals(listOf("drop 10,10", "end true", "drop 500,500", "end true"), pours.calls.toList())
        assertEquals(listOf(1 to true, 2 to true), kept)
    }

    @Test
    fun aDropThatFillsNothingAtRestFillsWhereItIsLetGo() {
        val pours = Pours(dropMs = 20, outcome = DropPours.Dropped.NOTHING)
        val drops = flow(pours)
        var kept: Boolean? = null
        drops.start()
        drops.begin(Offset(10f, 10f), 0xFF0000, 0.3f, poured = {}, blocked = {})
        drain { drops.fill == ColorDropFlow.Fill.REFUSED }
        // refused: slides do nothing, and it isn't tried again while held
        drops.slide(0.9f)
        drops.begin(Offset(20f, 20f), 0xFF0000, 0.3f, poured = {}, blocked = {})
        pours.outcome = DropPours.Dropped.POURED
        drops.release(Offset(30f, 30f), 0xFF0000, 0.3f, blocked = {}) { kept = it }
        drain { kept != null && drops.pending == 0 }
        assertEquals(true, kept)
        assertEquals(listOf("drop 10,10", "drop 30,30", "end true"), pours.calls.toList())
    }

    @Test
    fun aLayerThatTurnsTheColourAwaySaysSoAndKeepsNothing() {
        val pours = Pours(dropMs = 10, outcome = DropPours.Dropped.BLOCKED)
        val drops = flow(pours)
        var said = 0
        var kept: Boolean? = null
        drops.start()
        drops.begin(Offset(10f, 10f), 0xFF0000, 0.3f, poured = {}, blocked = { said += 1 })
        drops.release(Offset(12f, 12f), 0xFF0000, 0.3f, blocked = { said += 1 }) { kept = it }
        drain { kept != null && drops.pending == 0 }
        assertEquals(false, kept)
        assertEquals(2, said)
        assertTrue(pours.calls.none { it.startsWith("end") })
    }

    /**
     * Continue filling's touches, as the app did them on the main thread (each touch's whole-layer
     * fill waited for there) and now (handed to the fill's thread): a touch, its threshold slid and
     * let go, then a second touch let go at once, a third taken back by a second finger, and an undo.
     * Printed: the main thread's longest wait before and now. Bound: no fill made on the main thread,
     * the longest wait under 8 ms in the quickest of three runs after a warm-up (a machine busy
     * with other work may stall any one run; a main thread waiting on a fill waits its 150 ms in
     * every run), the screen never held (a touch may come meanwhile), the fills and the undo in
     * order.
     */
    @Test
    fun continueFillingsTouchesFillOnTheFillsThreadAndTheScreenGoesOn() {
        val testThread = Thread.currentThread()
        val pours = Pours()
        // as before: the main thread poured and ended each touch's fill itself
        val before = held {
            pours.pour(Offset(40f, 40f), 1f, 0f, 0f, 0.5f)
            pours.end(true)
        }
        // the code run once first (classes loaded, the fill's thread started), as an app that has run
        continueFilling(Pours(dropMs = 0, slideMs = 0))
        val runs = List(3) {
            val run = Pours()
            val (longest, busy) = continueFilling(run)
            assertTrue("the screen was held: $busy", busy.isEmpty())
            assertEquals(
                listOf("pour 40,40", "end true", "pour 300,40", "end true", "pour 600,40", "end false", "undo"),
                run.calls.toList(),
            )
            assertTrue("a fill was made on the main thread", run.threads.none { it == testThread })
            longest
        }
        val quickest = runs.min()
        println(
            "Continue filling's three touches (each a ${pours.dropMs} ms fill) and an undo: the main thread waited up to %.1f ms before, now %s ms (quickest %.2f)"
                .format(java.util.Locale.ROOT, before, runs.joinToString { "%.2f".format(java.util.Locale.ROOT, it) }, quickest),
        )
        assertTrue("the main thread waited $quickest ms in the quickest run", quickest < 8.0)
        assertTrue(before >= pours.dropMs.toDouble())
    }

    /**
     * Continue filling's three touches and an undo through a new flow on [pours]: the main thread's
     * longest wait (milliseconds), and whether the screen was held each time it was asked.
     */
    private fun continueFilling(pours: Pours): Pair<Double, List<Boolean>> {
        val drops = flow(pours)
        val busy = mutableListOf<Boolean>()
        drops.onBusy = { busy += it }
        val waits = mutableListOf<Double>()
        waits += held { drops.tap(Offset(40f, 40f), 1f, 0f, 0f, 0.5f) }
        waits += held { drops.tapEnd(true) }
        waits += held { drops.tap(Offset(300f, 40f), 0f, 1f, 0f, 0.6f) }
        waits += held { drops.tapEnd(true) }
        waits += held { drops.tap(Offset(600f, 40f), 0f, 0f, 1f, 0.6f) }
        waits += held { drops.tapEnd(false) }
        var undone = false
        assertTrue(drops.pending > 0 || pours.dropMs == 0L)
        waits += held {
            drops.afterFills {
                pours.calls += "undo"
                undone = true
            }
        }
        val until = System.nanoTime() + 5_000_000_000L
        while (!(undone && drops.pending == 0) && System.nanoTime() < until) main.poll(5, TimeUnit.MILLISECONDS)?.let { waits += held(it) }
        assertTrue("the fills didn't come back", undone && drops.pending == 0)
        return waits.max() to busy
    }

    @Test
    fun withNoFillUnderWayWhatWaitsForThemRunsAtOnce() {
        val drops = flow(Pours())
        var ran = false
        drops.afterFills { ran = true }
        assertTrue(ran)
        // let go off the artwork with nothing filled: nothing to keep
        var kept: Boolean? = null
        drops.start()
        drops.release(null, 0, 0.5f, blocked = {}) { kept = it }
        drain { kept != null }
        assertEquals(false, kept)
    }
}
