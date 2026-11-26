package io.github.artcraftandroid.procraft.canvas

import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The failure watch's timing: it runs only while there is something to watch and the app is in
 * front. It used to run every 3 s for as long as an artwork was open, the app away or not.
 */
class WhileNeededTest {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    @After
    fun end() = scope.cancel()

    private val period = 20L

    @Test
    fun nothingRunsOnceThereIsNothingToDo() {
        val steps = AtomicInteger()
        val watch = WhileNeeded(scope, period) { steps.incrementAndGet(); false }
        watch.poke()
        Thread.sleep(period * 15)
        // one look, then none for the rest of the 15 periods
        assertEquals(1, steps.get())
        // a change: one more look
        watch.poke()
        Thread.sleep(period * 5)
        assertEquals(2, steps.get())
    }

    @Test
    fun itRunsEachPeriodWhileThereIsSomethingToDo() {
        val pending = AtomicInteger(5)
        val steps = AtomicInteger()
        val watch = WhileNeeded(scope, period) { steps.incrementAndGet(); pending.decrementAndGet() > 0 }
        watch.poke()
        // pokes meanwhile start nothing more
        repeat(10) { watch.poke() }
        Thread.sleep(period * 25)
        assertEquals("five looks, the last finding nothing left", 5, steps.get())
    }

    @Test
    fun nothingRunsWhileTheAppIsAway() {
        val steps = AtomicInteger()
        val watch = WhileNeeded(scope, period) { steps.incrementAndGet(); true }
        watch.poke()
        Thread.sleep(period * 3)
        watch.pause()
        val atPause = steps.get()
        // changes while away start nothing
        repeat(20) {
            watch.poke()
            Thread.sleep(period / 2)
        }
        Thread.sleep(period * 5)
        val away = steps.get() - atPause
        assertTrue("paused", watch.paused)
        assertEquals("no look while away (in ${period * 15} ms)", 0, away)
        // back: it runs again
        watch.resume()
        Thread.sleep(period * 4)
        assertTrue("looking again once back", steps.get() > atPause)
    }
}
