package com.monokulo.till.ui

import android.content.ContentResolver
import android.database.ContentObserver
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import androidx.compose.animation.core.AnimationVector
import androidx.compose.animation.core.DecayAnimationSpec
import androidx.compose.animation.core.Easing
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.InfiniteRepeatableSpec
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.LinearOutSlowInEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.TwoWayConverter
import androidx.compose.animation.core.VectorizedFiniteAnimationSpec
import androidx.compose.animation.core.exponentialDecay
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.platform.LocalContext

/**
 * How the till moves (`docs/motion.md` §2): every spring and duration,
 * named for its purpose, so two things of the same kind move the same way
 * and a timing is changed here and nowhere else. Springs for anything with
 * a position (they retarget from where they are, at the speed they're
 * going); tweens only for opacity, and [track], the straight line a
 * screen change's parts follow while a spring moves the stage. [Full] is the design; [Still] is what
 * "Remove animations" gets: spatial presets snap, crossfades are quick,
 * loops don't run, and the dwells, which are reading time, stay.
 */
@Immutable
class Motion private constructor(
    /** Whether the person asked for less motion: a transform becomes a crossfade, a back gesture shows no preview. */
    val still: Boolean,
) {
    /** A window-sized thing arriving or leaving: the panel out of the bubble, the enlarged code out of its card, a screen out of its row. */
    val surface: Preset = spatial(stiffness = 380f, dampingRatio = 0.86f)

    /** Small things finding their place with no bounce: a row moving, a pill resizing, the docked bubble sliding to its slot. */
    val settle: Preset = spatial(stiffness = 800f, dampingRatio = 1f)

    /** What the finger let go of, seeded with its velocity: the bubble snapping to its edge, the grabber springing back. */
    val flick: Preset = spatial(stiffness = 600f, dampingRatio = 0.8f)

    /**
     * What the finger threw, running on at its speed and slowing by itself while something else brings it home:
     * the panel flung down its grabber keeps going down as it collapses into the bubble (decision 247). The
     * platform's plain friction, no tuning of its own; null under [Still], where nothing is carried past the release.
     */
    val thrown: DecayAnimationSpec<Float>? = if (still) null else exponentialDecay()

    /**
     * The straight line every spatial part of a screen change follows (decision 169): the shared surfaces' bounds,
     * the rises and the slides, one [surface]'s length long, so the stage at progress p has every one of them p
     * of the way. The stage's progress itself moves on [surface] ([stage]), or under the finger while a back
     * gesture seeks it, so the physics is in one place and a scrub is a preview.
     */
    val track: Preset = if (still) Snapped else Timed(TRACK_MS)

    /**
     * How much of [track] a fade takes on it, read per draw, not as layers: a glyph-only surface's face fades in over the
     * first of it (out over the last), and a screen's words arrive over the last of it ([fadeOutOnTrack] for leaving).
     */
    val crossfadeOnTrack: Float = FADE_MS.toFloat() / TRACK_MS

    /** How much of [track] a screen's words take to fade out as it is left: [fadeQuick]'s share, so they are gone before the arriving words start. */
    val fadeOutOnTrack: Float = QUICK_MS.toFloat() / TRACK_MS

    /** How the stage's progress runs to a screen, the till's or the cashier's release: [surface] on [track]; under [Still], each part's own spec. */
    val stage: FiniteAnimationSpec<Float>? = if (still) null else surface.fraction

    /** The phase ring's arc growing a quarter: slow enough to read as a sweep, overdamped so it never passes a quarter mark. */
    val sweep: Preset = spatial(stiffness = 170f, dampingRatio = 1f)

    /** A crossfade between two faces of the same thing: the bubble's content, the status words, a row's badge. */
    val fade: Preset = Timed(if (still) QUICK_MS else FADE_MS)

    /** An exit under something arriving; every crossfade under [Still]. */
    val fadeQuick: Preset = Timed(QUICK_MS)

    /**
     * An entrance in the place of something leaving under [fadeQuick] that
     * isn't the same thing: it starts once the exit is done, so two labels
     * ("Copy address" and "Done") are never drawn over each other.
     */
    val fadeThrough: Preset = Timed(if (still) QUICK_MS else FADE_MS, delayMs = QUICK_MS)

    /** The lit bubble's pulse, one turn at a time; null when loops don't run. */
    val pulse: InfiniteRepeatableSpec<Float>? = loop(PULSE_MS, LinearOutSlowInEasing)

    /** The loading mark's glints, one turn of [GLINT_MS]; null when loops don't run. */
    val glints: InfiniteRepeatableSpec<Float>? = loop(GLINT_MS, LinearEasing)

    /**
     * The waiting bubble's border: its ink segment chasing round the pale track at one speed, a turn of [CHASE_MS]
     * (`docs/motion.md` §12.5, decision 267); null when loops don't run, where the border is whole ink instead.
     */
    val chase: InfiniteRepeatableSpec<Float>? = loop(CHASE_MS, LinearEasing)

    /**
     * The phase ring's arc turning while the payment is seen or confirming: a turn of [SPIN_MS] at one speed
     * (Board 5), read in the ring's draw; null when loops don't run, where the arc stands still and only its length steps.
     */
    val spin: InfiniteRepeatableSpec<Float>? = loop(SPIN_MS, LinearEasing)

    /**
     * The bubble's lens changing colour from the core out: ring `i` of the mark's [SPREAD_RINGS] (the core, the inner
     * six, the middle twelve with the base, the rim) is a [fade] starting [SPREAD_STEP_MS] after the ring inside it, so
     * the lens is done by `fade + 3 × step`. Under [Still] every ring is the one quick fade, no steps.
     */
    val spread: List<Preset> = List(SPREAD_RINGS) { ring -> if (still) fadeQuick else Timed(FADE_MS, delayMs = ring * SPREAD_STEP_MS) }

    /**
     * The paid bubble's tick drawn along its path: a [sweep] that starts where the core's ring of [spread] ends, so it
     * is never drawn on a lens still turning green; the start is in the spec, not a delay. Under [Still] it is whole at once.
     */
    val tick: Preset = if (still) Snapped else After(FADE_MS, sweep)

    /**
     * The paid bubble's tick leaving: its trim held where it is for [fadeQuick]'s length while the tick fades out
     * whole, then rewound to nothing, so a tick leaves by fading, never un-drawn from its tail, and the next paid
     * draws it in again (decision 293). The same under [Still]: the fade it waits for is the same quick one.
     */
    val untick: Preset = After(QUICK_MS, Snapped)

    private fun spatial(stiffness: Float, dampingRatio: Float): Preset = if (still) Snapped else Sprung(stiffness, dampingRatio)

    private fun loop(turnMs: Int, easing: Easing): InfiniteRepeatableSpec<Float>? =
        if (still) null else infiniteRepeatable(tween(turnMs, easing = easing), RepeatMode.Restart)

    /** How long a moment is held before the till moves on: reading time, the same under [Still]. */
    object Dwell {
        /** A sale shown paid (the sale screen, the panel, the green bubble, the chip's tick) before the till moves on. */
        const val PAID_MS = 5_000L
    }

    companion object {
        val Full = Motion(still = false)
        val Still = Motion(still = true)

        private const val FADE_MS = 200
        private const val QUICK_MS = 120
        private const val PULSE_MS = 1600

        /** One turn of the waiting bubble's border. */
        const val CHASE_MS = 1250

        /** One turn of the phase ring's arc while a payment is on its way. */
        const val SPIN_MS = 1600

        /** The lens's rings, core to rim, that [spread] colours one after another. */
        const val SPREAD_RINGS = 4

        /** How long after its inner neighbour a ring of [spread] starts. */
        const val SPREAD_STEP_MS = 60

        /** About as long as [surface] takes to settle, so a screen change takes as long whether seeked or not. */
        private const val TRACK_MS = 450
    }
}

/**
 * One preset, the same physics whatever type moves: [fraction] for a value
 * in 0..1 (a progress, an alpha), [px] for a value in pixels, [spec] for an
 * `IntOffset`, `IntSize`, `Rect` or `Color` with its own visibility
 * threshold. A spring is told how close is finished so it never runs an
 * invisible tail.
 */
@Immutable
sealed interface Preset {
    fun <T> spec(visibilityThreshold: T? = null): FiniteAnimationSpec<T>

    /** Within a thousandth of the way: a full-screen surface never jumps its last pixel. */
    val fraction: FiniteAnimationSpec<Float> get() = spec(FRACTION_THRESHOLD)

    /** Within half a pixel. */
    val px: FiniteAnimationSpec<Float> get() = spec(PX_THRESHOLD)

    companion object {
        const val FRACTION_THRESHOLD = 0.001f
        const val PX_THRESHOLD = 0.5f
    }
}

private class Sprung(private val stiffness: Float, private val dampingRatio: Float) : Preset {
    override fun <T> spec(visibilityThreshold: T?): FiniteAnimationSpec<T> = spring(dampingRatio, stiffness, visibilityThreshold)
}

private class Timed(private val durationMs: Int, private val delayMs: Int = 0) : Preset {
    override fun <T> spec(visibilityThreshold: T?): FiniteAnimationSpec<T> = tween(durationMs, delayMs, LinearEasing)
}

private object Snapped : Preset {
    override fun <T> spec(visibilityThreshold: T?): FiniteAnimationSpec<T> = snap()
}

/** [inner] started [delayMs] later: a spring carries no delay of its own, so it is played from a later start. */
private class After(private val delayMs: Int, private val inner: Preset) : Preset {
    override fun <T> spec(visibilityThreshold: T?): FiniteAnimationSpec<T> = Delayed(delayMs * 1_000_000L, inner.spec(visibilityThreshold))
}

/**
 * [inner] held at its start for [delayNanos], then run whole: what a
 * tween's `delayMillis` does, for any finite spec. Retargeted mid-hold it
 * starts again from where it stands, as the spec it wraps would.
 */
private class Delayed<T>(private val delayNanos: Long, private val inner: FiniteAnimationSpec<T>) : FiniteAnimationSpec<T> {
    override fun <V : AnimationVector> vectorize(converter: TwoWayConverter<T, V>): VectorizedFiniteAnimationSpec<V> {
        val run = inner.vectorize(converter)
        return object : VectorizedFiniteAnimationSpec<V> {
            override fun getDurationNanos(initialValue: V, targetValue: V, initialVelocity: V): Long =
                delayNanos + run.getDurationNanos(initialValue, targetValue, initialVelocity)

            override fun getValueFromNanos(playTimeNanos: Long, initialValue: V, targetValue: V, initialVelocity: V): V =
                if (playTimeNanos < delayNanos) initialValue else run.getValueFromNanos(playTimeNanos - delayNanos, initialValue, targetValue, initialVelocity)

            override fun getVelocityFromNanos(playTimeNanos: Long, initialValue: V, targetValue: V, initialVelocity: V): V =
                run.getVelocityFromNanos((playTimeNanos - delayNanos).coerceAtLeast(0L), initialValue, targetValue, initialVelocity)
        }
    }
}

val LocalMotion = staticCompositionLocalOf { Motion.Full }

/** The current motion presets, as [mk] is the current colours. */
val motion: Motion @Composable get() = LocalMotion.current

/**
 * The motion the person asked for, kept up to date: [Motion.Still] while
 * the animator duration scale is 0 ("Remove animations" in Accessibility,
 * or the developer option), read once here for every window and followed
 * through a `ContentObserver`, so a change takes effect without a restart.
 */
@Composable
fun rememberMotion(): Motion {
    val resolver = LocalContext.current.contentResolver
    val still by produceState(remember(resolver) { animationsOff(resolver) }, resolver) {
        val observer = object : ContentObserver(Handler(Looper.getMainLooper())) {
            override fun onChange(selfChange: Boolean) {
                value = animationsOff(resolver)
            }
        }
        resolver.registerContentObserver(Settings.Global.getUriFor(Settings.Global.ANIMATOR_DURATION_SCALE), false, observer)
        awaitDispose { resolver.unregisterContentObserver(observer) }
    }
    return if (still) Motion.Still else Motion.Full
}

private fun animationsOff(resolver: ContentResolver): Boolean =
    Settings.Global.getFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
