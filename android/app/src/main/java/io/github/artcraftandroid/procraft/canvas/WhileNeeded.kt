package io.github.artcraftandroid.procraft.canvas

import kotlin.coroutines.CoroutineContext
import kotlin.coroutines.EmptyCoroutineContext
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * Work done every [periodMs] (in [scope], on [context]) only while there is a reason to and the
 * app is in front: [step] does it and says whether to go on. Nothing runs once it says not to, or
 * while the app is away: [poke] starts it again after a change, [pause] stops it as the app goes
 * away and [resume] as it comes back.
 */
internal class WhileNeeded(
    private val scope: CoroutineScope,
    private val periodMs: Long,
    private val context: CoroutineContext = EmptyCoroutineContext,
    private val step: suspend () -> Boolean,
) {
    @Volatile
    private var job: Job? = null

    /** The app is away: nothing runs until [resume]. */
    @Volatile
    var paused = false
        private set

    /** Something changed: the work runs again (a period from now), unless it is running or the app is away. */
    fun poke() {
        if (paused || job?.isActive == true) return
        job = scope.launch(context) {
            do {
                delay(periodMs)
            } while (!paused && step())
        }
    }

    /** The app went away: what runs stops, and nothing starts until [resume]. */
    fun pause() {
        paused = true
        job?.cancel()
        job = null
    }

    /** The app is back: the work runs again, as after a change. */
    fun resume() {
        paused = false
        poke()
    }
}
