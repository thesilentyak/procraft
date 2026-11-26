package io.github.artcraftandroid.procraft.canvas

import android.os.Build
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import io.github.artcraftandroid.procraft.core.PointerKind
import io.github.artcraftandroid.procraft.core.QuickAction
import io.github.artcraftandroid.procraft.core.TouchSettings

/** The key Huawei's M-Pencil gives for a double-tap, twice over for each one. */
internal const val PEN_DOUBLE_TAP_KEY = 718

/** Presses of that key closer than this (ms) are one double-tap given twice. */
private const val DOUBLE_TAP_GIVEN_TWICE_MS = 300L

/** What a pen's button that comes as a key did: pressed, let go, or (a double-tap's key) tapped, never held. */
enum class PenKeyAction { PRESSED, LET_GO, DOUBLE_TAPPED }

/**
 * A pen's first side button, as a motion event's button state has it: Android's own bit, or the
 * mouse's right button, which older pens' drivers give it as (Android did before it had its own).
 */
internal const val PEN_FIRST = MotionEvent.BUTTON_STYLUS_PRIMARY or MotionEvent.BUTTON_SECONDARY

/** Its second, the same way: Android's own bit, or the mouse's middle button. */
internal const val PEN_SECOND = MotionEvent.BUTTON_STYLUS_SECONDARY or MotionEvent.BUTTON_TERTIARY

/** A pen's two side buttons, as a motion event's button state has them. */
internal const val PEN_BUTTONS = PEN_FIRST or PEN_SECOND

/** The pen's first and second side button, as [PenButtons] tells them: each whichever bit it came as. */
internal const val FIRST_BUTTON = 1
internal const val SECOND_BUTTON = 2

/** Which of the pen's side buttons ([FIRST_BUTTON], [SECOND_BUTTON]) [buttonState] has held. */
internal fun penButtonsIn(buttonState: Int): Int =
    (if (buttonState and PEN_FIRST != 0) FIRST_BUTTON else 0) or (if (buttonState and PEN_SECOND != 0) SECOND_BUTTON else 0)

/** Whether a pointer of [toolType] from [source] is a pen's (its tip or its eraser end): see [pointerKind]. */
internal fun isPen(toolType: Int, source: Int) = pointerKind(toolType, source).let { it == PointerKind.STYLUS || it == PointerKind.STYLUS_ERASER }

/** Whether an event's [source] says it came from a pen (a Bluetooth pen's says so too). */
private fun fromStylus(source: Int) = source and InputDevice.SOURCE_STYLUS == InputDevice.SOURCE_STYLUS

/**
 * A touchpad's gestures, as an event's classification says (Android 14's
 * `CLASSIFICATION_TWO_FINGER_SWIPE`, `CLASSIFICATION_MULTI_FINGER_SWIPE` and `CLASSIFICATION_PINCH`):
 * fingers on the pad moving the view, not the pointer.
 */
private val TOUCHPAD_GESTURES = setOf(3, 4, 5)

/**
 * What a pointer is to the canvas, by its [toolType], the [source] its event came from and the
 * event's [classification] (0 where the system has none): a pen's tip or eraser end, or a pointer
 * whose driver doesn't say what it is (an unknown tool type) in an event from a pen; a mouse, or a
 * touchpad moving the pointer (Android gives that one a finger's tool type), which does what a
 * touch is set to do, with no pressure, and is never taken for a hand resting by the pen; else a
 * finger. A touchpad's own swipes and pinches stay fingers'.
 */
internal fun pointerKind(toolType: Int, source: Int, classification: Int = 0): PointerKind = when {
    toolType == MotionEvent.TOOL_TYPE_STYLUS -> PointerKind.STYLUS
    toolType == MotionEvent.TOOL_TYPE_ERASER -> PointerKind.STYLUS_ERASER
    toolType == MotionEvent.TOOL_TYPE_MOUSE -> PointerKind.MOUSE
    toolType == MotionEvent.TOOL_TYPE_UNKNOWN && fromStylus(source) -> PointerKind.STYLUS
    source and InputDevice.SOURCE_MOUSE == InputDevice.SOURCE_MOUSE && classification !in TOUCHPAD_GESTURES -> PointerKind.MOUSE
    else -> PointerKind.FINGER
}

/** A palm's tool type, as a touchscreen that tells a palm from a finger gives it (Android keeps its own name for it hidden). */
internal const val TOOL_TYPE_PALM = 5

/** A finger's contact this far across (mm) or more is a palm: a fingertip is 8 to 15, a flat thumb under 20. */
internal const val PALM_MM = 25f

/**
 * Whether a contact is a hand resting on the screen, never handed to the shared touches (and taken
 * back from them if it becomes one): one its screen says is a palm ([toolType]), whatever painting
 * with a finger is set to, or a finger's ([kind]) [touchMajor] pixels across, [PALM_MM] or more on
 * a screen of [xdpi] pixels an inch (a screen that says nothing of a touch's size, 0, gives none).
 * A pen's or a mouse's never is.
 */
internal fun palmContact(toolType: Int, kind: PointerKind, touchMajor: Float, xdpi: Float): Boolean = when {
    toolType == TOOL_TYPE_PALM -> true
    kind != PointerKind.FINGER -> false
    xdpi <= 0f || !touchMajor.isFinite() -> false
    else -> touchMajor / xdpi * 25.4f >= PALM_MM
}

/** What pointer [index] of [e] is to the canvas (see [pointerKind]). */
internal fun pointerKindOf(e: MotionEvent, index: Int): PointerKind =
    pointerKind(e.getToolType(index), e.source, if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) e.classification else 0)

/**
 * [settings] as the tablet takes them: a mouse, or a touchpad moving the pointer, does what a finger
 * is set to do. With painting with a finger off it paints nothing (a drag does what a finger's
 * does), and on a model it turns the model; it still has no pressure and is never taken for a hand
 * resting by the pen. The pen's buttons do what Pen buttons set them to ([penButtons], the first
 * and the second; null, what Gesture controls say), or run a plugin's action ([penPlugins], its key;
 * null for none) before that.
 */
internal fun tabletTouchSettings(
    settings: TouchSettings,
    penButtons: List<QuickAction?> = emptyList(),
    penPlugins: List<String?> = emptyList(),
): TouchSettings = settings.copy(
    pointerFollowsTouch = true,
    penButtonDoes = penButtons.getOrNull(0),
    penSecondButtonDoes = penButtons.getOrNull(1),
    penButtonRuns = penPlugins.getOrNull(0),
    penSecondButtonRuns = penPlugins.getOrNull(1),
)

/**
 * Whether a mouse's press with [buttonState] held paints: its primary button's does (and a press
 * that says of no button, as a touchpad's tap may), its others' do nothing on the canvas.
 */
internal fun mousePressPaints(buttonState: Int): Boolean = buttonState == 0 || buttonState and MotionEvent.BUTTON_PRIMARY != 0

/** The pen's side buttons as its events say they are held, from one event to the next. */
internal class PenButtons {
    private var held = 0

    /** The pen's touch events have shown a button held: its driver puts them in, so one missing as it lifts was let go. */
    private var touchesShowButtons = false

    /**
     * The buttons pressed and let go ([FIRST_BUTTON], [SECOND_BUTTON]) since the last event, by this
     * one's [buttonState]. An event without the pen in it ([penIn] false: a hand's touch, whose buttons read
     * as none held) changes nothing, or a hand landing while a button is held would let it go. While
     * the pen is [touching], a button that goes missing isn't let go yet: some pens' touch events
     * leave out a button their hover shows held, which would hand the eraser back as the stroke
     * begins. As the pen [lifting] lifts it is let go, unless its touches have never shown a button:
     * then the lift leaves it out too, and the next hover says whether it is still held (let go
     * there and pressed again as the pen hovers on, one press would switch to the eraser and
     * straight back, and the size slider with it). Nor is it let go as the pen [leaving] leaves
     * hover, as it does on its way down onto the glass: some pens' hover exit leaves out a button
     * still held (the Galaxy Tab S6 Lite's), which handed the eraser back just before the stroke,
     * so a press only switched to the eraser and back. Leaving for good, its next hover or touch
     * says.
     */
    fun next(buttonState: Int, penIn: Boolean, touching: Boolean = false, lifting: Boolean = false, leaving: Boolean = false): Pair<Int, Int> {
        if (!penIn) return 0 to 0
        val shown = penButtonsIn(buttonState)
        if ((touching || lifting) && shown != 0) touchesShowButtons = true
        val kept = touching || leaving || (lifting && !touchesShowButtons)
        val now = shown or (if (kept) held else 0)
        val change = (now and held.inv()) to (held and now.inv())
        held = now
        return change
    }
}

/** Whether a motion event of [action] is the pen on the glass ([PenButtons.next]'s touching), or lifting from it (its lifting). */
internal fun penTouching(action: Int): Boolean = when (action) {
    MotionEvent.ACTION_DOWN, MotionEvent.ACTION_MOVE, MotionEvent.ACTION_POINTER_DOWN, MotionEvent.ACTION_POINTER_UP -> true
    else -> false
}

internal fun penLifting(action: Int): Boolean = action == MotionEvent.ACTION_UP

/** Whether a motion event of [action] is the pen leaving hover ([PenButtons.next]'s leaving). */
internal fun penLeaving(action: Int): Boolean = action == MotionEvent.ACTION_HOVER_EXIT

/** Tells a pen's next double-tap from the second press of the one just heard. */
internal class PenDoubleTaps {
    private var heardAt: Long? = null

    /** Whether a press at [at] (ms) is a double-tap of its own. */
    fun isNew(at: Long): Boolean {
        val last = heardAt
        if (last != null && at - last < DOUBLE_TAP_GIVEN_TWICE_MS) return false
        heardAt = at
        return true
    }
}

/** A device named as a pen, as a word of its own ("Smart Pen", not "Open Keyboard"). */
private val PEN_NAME = Regex("""\b(pen|stylus)\b""", RegexOption.IGNORE_CASE)

/**
 * Which of a pen's two side buttons [keyCode] is (true: the second), from a device that has
 * [sources], is of [keyboardType] and is called [name]; null if it is no pen's. Some pens, as
 * Redmi's Smart Pen, give their buttons as Page Up and Page Down. A key's own source says keyboard
 * whatever else its device is, so this goes by the device: a stylus, one without letters, or one
 * named as a pen (a pen paired over Bluetooth may list a whole keyboard's keys). Those keys stay a
 * typing keyboard's own.
 */
internal fun penKeyButton(keyCode: Int, sources: Int, keyboardType: Int, name: String): Boolean? {
    val second = when (keyCode) {
        KeyEvent.KEYCODE_PAGE_UP -> false
        KeyEvent.KEYCODE_PAGE_DOWN -> true
        else -> return null
    }
    val stylus = sources and InputDevice.SOURCE_STYLUS == InputDevice.SOURCE_STYLUS
    val pen = stylus || keyboardType != InputDevice.KEYBOARD_TYPE_ALPHABETIC || PEN_NAME.containsMatchIn(name)
    return second.takeIf { pen }
}

/**
 * Whether a key from a device with [sources] called [name] may be a pen's: a stylus, or one named
 * as a pen. What a pen's button sends that the canvas doesn't take goes in the log, once a key.
 */
internal fun maybePenKeys(sources: Int, name: String): Boolean =
    sources and InputDevice.SOURCE_STYLUS == InputDevice.SOURCE_STYLUS || PEN_NAME.containsMatchIn(name)

/** Each key from each device told once in the log. */
internal class KeysTold {
    private val told = HashSet<Pair<Int, Int>>()

    /** Whether [keyCode] from device [deviceId] is yet to be told (it counts as told now). */
    fun isNew(deviceId: Int, keyCode: Int): Boolean = told.add(deviceId to keyCode)
}

/** Each kind of pointer the canvas has seen from each device, told once in the log. */
internal class PointersTold {
    private val told = HashSet<Triple<Int, Int, Int>>()

    /** Whether a pointer of [toolType] in an event from [source] of device [deviceId] is yet to be told (it counts as told now). */
    fun isNew(deviceId: Int, toolType: Int, source: Int): Boolean = told.add(Triple(deviceId, toolType, source))
}

/** Android's tool types by number. */
private val TOOL_NAMES = listOf("unknown", "finger", "stylus", "mouse", "eraser", "palm")

/**
 * What the log says of a kind of pointer the first time it comes ([what]: down, or hovering): its
 * [device] (its vendor and product, and a built-in one's name), [toolType] and event [source],
 * what the canvas takes it for ([kind]), its first [pressure] and the pressure its device says it
 * gives ([pressureAxis], least to most; null, none), the [buttons] held and how big its touch is
 * ([touchMajor] pixels): what a report of a pen that doesn't paint needs.
 */
internal fun pointerFacts(
    what: String,
    device: String,
    toolType: Int,
    source: Int,
    kind: PointerKind,
    pressure: Float,
    pressureAxis: ClosedFloatingPointRange<Float>?,
    buttons: Int,
    touchMajor: Float,
): String {
    val tool = TOOL_NAMES.getOrNull(toolType) ?: "$toolType"
    val axis = pressureAxis?.let { "%.2f to %.2f".format(java.util.Locale.ROOT, it.start, it.endInclusive) } ?: "none"
    return "a pointer %s: %s, tool %s, source 0x%x, taken for %s; pressure %.3f (its device gives %s); buttons 0x%x; touch %.1f px".format(
        java.util.Locale.ROOT, what, device, tool, source, kind.name.lowercase(java.util.Locale.ROOT), pressure, axis, buttons, touchMajor,
    )
}

/** Each device's first pen stroke, its pressures summed up for the log: whether the pen gives a pressure, and how much. */
internal class PenStrokesTold {
    private val told = HashSet<Int>()

    /** The device whose first stroke this is (none: not being summed up), its samples and its least and most pressure. */
    private var device: Int? = null
    private var count = 0
    private var least = 0f
    private var most = 0f

    /** A pen's stroke began on device [deviceId]: summed up if it is that device's first. */
    fun began(deviceId: Int) {
        device = deviceId.takeIf { it !in told }
        count = 0
        least = Float.POSITIVE_INFINITY
        most = Float.NEGATIVE_INFINITY
    }

    /** Whether the stroke's samples are being summed up. */
    val summing get() = device != null

    /** One of its samples' [pressure]. */
    fun sample(pressure: Float) {
        if (device == null) return
        count++
        least = minOf(least, pressure)
        most = maxOf(most, pressure)
    }

    /** The stroke lifted (or was called off): the line for the log, if it was its device's first. */
    fun lifted(): String? {
        val d = device ?: return null
        device = null
        told += d
        if (count == 0) return "the first pen stroke from device $d: no samples"
        return "the first pen stroke from device %d: %d samples, pressure %.3f to %.3f".format(java.util.Locale.ROOT, d, count, least, most)
    }
}
