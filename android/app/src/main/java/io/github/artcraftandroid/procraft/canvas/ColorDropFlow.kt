package io.github.artcraftandroid.procraft.canvas

import androidx.compose.ui.geometry.Offset
import java.util.concurrent.ConcurrentHashMap

/** What a colour dragged onto the canvas fills with: the session's own calls, made on the fill's thread. */
interface DropPours {
    /** What a drop at a window point did. */
    enum class Dropped { POURED, BLOCKED, NOTHING }

    /**
     * Fills from window point [at] with [colour] (0xRRGGBB) at [threshold]: poured, turned away by
     * the current layer (a dialog says why), or nothing filled.
     */
    fun drop(at: Offset, colour: Int, threshold: Float): Dropped

    /**
     * Continue filling's touch: fills from window point [at] with ([r], [g], [b], 0..1) at
     * [threshold], shown at once (not spreading); false if nothing was filled.
     */
    fun pour(at: Offset, r: Float, g: Float, b: Float, threshold: Float): Boolean

    /** The open fill redone at [threshold]. */
    fun threshold(threshold: Float)

    /** The open fill ends: [keep] makes it an undo step. */
    fun end(keep: Boolean)
}

/**
 * The fills of a colour dragged out and onto the canvas, as the drag goes: it comes to rest (or its
 * threshold is slid) and fills there, the threshold slid redoes the fill, and let go it is kept;
 * let go where it hadn't filled, it fills where it is let go and is kept.
 *
 * The fills are made on a thread of their own ([run], in order) and the main thread never waits
 * for one: what each did comes back on it ([post]). A large fill takes a moment on a slow device,
 * and the screen goes on meanwhile. What comes after them waits for them ([afterFills]), so undo
 * takes back the fill and not what was before it.
 *
 * Continue filling's touches fill the same way ([tap], [tapEnd]), but the screen goes on taking
 * touches meanwhile: the next touch's fill waits its turn, and a second finger still takes a fill
 * back.
 *
 * Main thread only, but for [run]'s blocks.
 */
class ColorDropFlow(
    private val pours: DropPours,
    private val run: (() -> Unit) -> Unit,
    private val post: (() -> Unit) -> Unit,
) {
    /** How far the drag's fill has got: none tried, being made, made, or none to be made. */
    enum class Fill { NONE, PENDING, POURED, REFUSED }

    var fill = Fill.NONE
        private set

    /** Fills handed to the fill's thread and not yet back. */
    var pending = 0
        private set

    /** Which drag the flow is on: what comes back for an earlier one leaves this one alone. */
    private var drag = 0

    /** What the fill's thread last made of a drop, by drag (read and written on that thread only). */
    @Volatile
    private var lastDrop: Pair<Int, DropPours.Dropped>? = null

    /** The newest threshold for the fill's thread, and whether one is on its way there. */
    private val threshold = ConcurrentHashMap<String, Float>()

    /** Told whether a dragged colour's fills are being made (for the screen to take no touches meanwhile). */
    var onBusy: (Boolean) -> Unit = {}

    /** Fills handed over that hold the screen (a dragged colour's, not Continue filling's) and not yet back. */
    private var holding = 0

    /** A drag begins: nothing filled yet. */
    fun start() {
        drag += 1
        fill = Fill.NONE
    }

    /**
     * The drag comes to rest at [at], or its threshold first slides there: it fills there with
     * [colour] at [threshold], once. [poured] is told once it has, for this drag; [blocked] if the
     * current layer turned it away.
     */
    fun begin(at: Offset, colour: Int, threshold: Float, poured: () -> Unit, blocked: () -> Unit) {
        if (fill != Fill.NONE) return
        fill = Fill.PENDING
        val d = drag
        submit {
            val did = pours.drop(at, colour, threshold)
            lastDrop = d to did
            post {
                if (d == drag && fill == Fill.PENDING) {
                    fill = if (did == DropPours.Dropped.POURED) Fill.POURED else Fill.REFUSED
                    if (did == DropPours.Dropped.POURED) poured()
                }
                if (did == DropPours.Dropped.BLOCKED) blocked()
            }
        }
    }

    /** It can't fill (a text layer current): nothing is tried until it is let go. */
    fun refuse() {
        if (fill == Fill.NONE) fill = Fill.REFUSED
    }

    /** The threshold slid to [t] with the fill made or being made: redone at the newest, in turn. */
    fun slide(t: Float) {
        if (fill != Fill.PENDING && fill != Fill.POURED) return
        // the newest only: one still waiting for the fill's thread takes it
        if (threshold.put(KIND, t) != null) return
        submit { threshold.remove(KIND)?.let(pours::threshold) }
    }

    /**
     * Let go at [at] (null where it fills nothing, or where a fill isn't wanted): a fill made is
     * kept; else it fills there with [colour] at [threshold] and that is kept. [done] is told
     * whether one was kept.
     */
    fun release(at: Offset?, colour: Int, threshold: Float, blocked: () -> Unit, done: (Boolean) -> Unit) {
        val made = fill
        val d = drag
        fill = Fill.NONE
        // let go before its fill came back, the fill's thread knows how that went
        submit {
            val poured = when (made) {
                Fill.POURED -> true
                Fill.PENDING -> lastDrop?.let { (of, did) -> of == d && did == DropPours.Dropped.POURED } == true
                else -> false
            }
            if (poured) {
                pours.end(true)
                post { done(true) }
                return@submit
            }
            val did = at?.let { pours.drop(it, colour, threshold) } ?: DropPours.Dropped.NOTHING
            if (did == DropPours.Dropped.POURED) pours.end(true)
            post {
                if (did == DropPours.Dropped.BLOCKED) blocked()
                done(did == DropPours.Dropped.POURED)
            }
        }
    }

    /**
     * Continue filling's touch landed at [at]: it fills there with ([r], [g], [b]) at [threshold] on
     * the fill's thread, in turn (its threshold slid is redone after it there).
     */
    fun tap(at: Offset, r: Float, g: Float, b: Float, threshold: Float) {
        submit(holds = false) { pours.pour(at, r, g, b, threshold) }
    }

    /** Continue filling's touch let go: its fill is kept ([keep]) or taken back, once it is made. */
    fun tapEnd(keep: Boolean) {
        submit(holds = false) { pours.end(keep) }
    }

    /** What waits for the fills under way to be done, in the order it came. */
    private val waiting = ArrayDeque<() -> Unit>()

    /** Runs [block] once the fills handed over are done, at once if there are none. */
    fun afterFills(block: () -> Unit) {
        if (pending == 0) block() else waiting.addLast(block)
    }

    /** Hands [block] to the fill's thread, counting it until it is back; [holds]: the screen takes no touches meanwhile. */
    private fun submit(holds: Boolean = true, block: () -> Unit) {
        pending += 1
        if (holds) {
            holding += 1
            if (holding == 1) onBusy(true)
        }
        run {
            try {
                block()
            } finally {
                post {
                    pending -= 1
                    if (holds) {
                        holding -= 1
                        if (holding == 0) onBusy(false)
                    }
                    if (pending == 0) {
                        while (pending == 0) waiting.removeFirstOrNull()?.invoke() ?: break
                    }
                }
            }
        }
    }

    private companion object {
        const val KIND = "threshold"
    }
}
