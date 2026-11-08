package com.monokulo.till

import androidx.compose.animation.core.AnimationVector1D
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.SnapSpec
import androidx.compose.animation.core.SpringSpec
import androidx.compose.animation.core.TweenSpec
import androidx.compose.animation.core.VectorConverter
import androidx.compose.animation.core.VisibilityThreshold
import androidx.compose.ui.unit.IntOffset
import com.monokulo.till.ui.GLINT_MS
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.Preset
import kotlin.math.abs
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertSame
import kotlin.test.assertTrue

/** The motion presets (`docs/motion.md` §2, §7) as rules: production values, no test-only specs. */
class MotionTest {
    private val full = Motion.Full
    private val still = Motion.Still

    /** How long [spec] takes from 0 to 1 at rest, in ms. */
    private fun ms(spec: FiniteAnimationSpec<Float>): Long =
        spec.vectorize(Float.VectorConverter).getDurationNanos(AnimationVector1D(0f), AnimationVector1D(1f), AnimationVector1D(0f)) / 1_000_000

    @Test
    fun `the spatial presets are the spec's springs, told how close is finished`() {
        val surface = assertIs<SpringSpec<Float>>(full.surface.fraction)
        assertEquals(380f, surface.stiffness)
        assertEquals(0.86f, surface.dampingRatio)
        assertEquals(Preset.FRACTION_THRESHOLD, surface.visibilityThreshold, "a progress finishes within a thousandth")
        val settle = assertIs<SpringSpec<IntOffset>>(full.settle.spec(IntOffset.VisibilityThreshold))
        assertEquals(800f, settle.stiffness)
        assertEquals(1f, settle.dampingRatio)
        assertEquals(IntOffset.VisibilityThreshold, settle.visibilityThreshold, "the same physics for an offset, with its own threshold")
        val flick = assertIs<SpringSpec<Float>>(full.flick.px)
        assertEquals(600f, flick.stiffness)
        assertEquals(0.8f, flick.dampingRatio)
        assertEquals(Preset.PX_THRESHOLD, flick.visibilityThreshold, "a pixel value finishes within half a pixel")
        val sweep = assertIs<SpringSpec<Float>>(full.sweep.fraction)
        assertEquals(170f, sweep.stiffness)
        assertEquals(1f, sweep.dampingRatio)
    }

    @Test
    fun `fades are linear tweens, the quick one for exits`() {
        val fade = assertIs<TweenSpec<Float>>(full.fade.fraction)
        assertEquals(200, fade.durationMillis)
        val quick = assertIs<TweenSpec<Float>>(full.fadeQuick.fraction)
        assertEquals(120, quick.durationMillis)
    }

    @Test
    fun `the loops are one turn each, restarting`() {
        val pulse = assertIs<TweenSpec<Float>>(full.pulse!!.animation)
        assertEquals(1600, pulse.durationMillis)
        val glints = assertIs<TweenSpec<Float>>(full.glints!!.animation)
        assertEquals(GLINT_MS, glints.durationMillis, "one turn of the glints is the mark's own")
        val spin = assertIs<TweenSpec<Float>>(full.spin!!.animation)
        assertEquals(1600, spin.durationMillis, "the ring's arc turns a lap in 1.6 s")
        assertEquals(0.5f, spin.easing.transform(0.5f), "at one speed")
    }

    @Test
    fun `big things take longest, small things settle fast, a fade is quicker than a move`() {
        val fadeQuick = ms(full.fadeQuick.fraction)
        val fade = ms(full.fade.fraction)
        val settle = ms(full.settle.fraction)
        val surface = ms(full.surface.fraction)
        val sweep = ms(full.sweep.fraction)
        assertTrue(fadeQuick < fade, "an exit is quicker than an entry: $fadeQuick < $fade")
        assertTrue(fade < settle, "a fade is quicker than a move: $fade < $settle")
        assertTrue(settle < surface, "a row settles before a window would: $settle < $surface")
        assertTrue(surface < sweep, "the ring's sweep is slow enough to be seen: $surface < $sweep")
        assertTrue(sweep < 1_000, "but never slow enough to wait for: $sweep ms")
    }

    @Test
    fun `the sweep never passes a quarter mark`() {
        val spring = full.sweep.fraction.vectorize(Float.VectorConverter)
        val from = AnimationVector1D(0f)
        val to = AnimationVector1D(0.25f)
        val rest = AnimationVector1D(0f)
        val duration = spring.getDurationNanos(from, to, rest)
        var passedHalfWay = false
        var t = 0L
        while (t <= duration) {
            val value = spring.getValueFromNanos(t, from, to, rest).value
            assertTrue(value <= 0.25f, "at $t ns the arc is at $value, past the quarter")
            if (value > 0.125f) passedHalfWay = true
            t += 8_000_000
        }
        assertTrue(passedHalfWay, "the arc is seen on its way, not just at its ends")
    }

    @Test
    fun `still motion snaps what moves, keeps the quick fade, and runs no loop`() {
        assertTrue(still.still)
        assertIs<SnapSpec<Float>>(still.surface.fraction)
        assertIs<SnapSpec<IntOffset>>(still.settle.spec(IntOffset.VisibilityThreshold))
        assertIs<SnapSpec<Float>>(still.flick.px)
        assertIs<SnapSpec<Float>>(still.sweep.fraction)
        assertEquals(120, assertIs<TweenSpec<Float>>(still.fade.fraction).durationMillis, "a crossfade is not motion: a quick one stays")
        assertEquals(120, assertIs<TweenSpec<Float>>(still.fadeQuick.fraction).durationMillis)
        assertNull(still.pulse)
        assertNull(still.glints)
        assertNull(still.spin, "the ring's arc stands still")
        assertNull(still.thrown, "nothing is carried past a release")
        assertNotNull(full.thrown)
        assertSame(Motion.Full, full, "one instance each, so a window reads the same presets as the next")
    }

    @Test
    fun `a fade-through starts once the quick exit under it is done`() {
        val through = assertIs<TweenSpec<Float>>(full.fadeThrough.fraction)
        val quick = assertIs<TweenSpec<Float>>(full.fadeQuick.fraction)
        assertEquals(quick.durationMillis, through.delay, "the entrance waits for the exit, no longer")
        assertEquals(200, through.durationMillis)
        val stillThrough = assertIs<TweenSpec<Float>>(still.fadeThrough.fraction)
        assertEquals(120, stillThrough.durationMillis, "under Still every crossfade is the quick one")
        // Not a crossfade: two different labels in one place. Under Still it keeps its wait, so they never overlap there
        // either, and takes 240 ms where a Still crossfade takes 120 (decision 198).
        assertEquals(quick.durationMillis, stillThrough.delay)
    }

    @Test
    fun `a screen change's parts follow one straight line, and its progress springs`() {
        val track = assertIs<TweenSpec<Float>>(full.track.fraction)
        assertEquals(450, track.durationMillis)
        assertEquals(0.5f, track.easing.transform(0.5f), "a line: half the time is half the way")
        assertIs<SnapSpec<Float>>(still.track.fraction)
        assertSame(full.surface.fraction.javaClass, full.stage!!.javaClass)
        val stage = assertIs<SpringSpec<Float>>(full.stage)
        assertEquals(380f, stage.stiffness, "the stage moves on the surface spring")
        assertNull(still.stage, "under Still each part runs its own quick spec")
        val surface = ms(full.surface.fraction)
        assertTrue(abs(surface - 450) < 120, "the track is about as long as the spring takes to settle: $surface ms")
    }

    @Test
    fun `on the track, words fade out before the arriving ones fade in`() {
        assertEquals(200f / 450f, full.crossfadeOnTrack)
        assertEquals(120f / 450f, full.fadeOutOnTrack)
        assertTrue(full.fadeOutOnTrack + full.crossfadeOnTrack < 1f, "a gap between the leaving words and the arriving ones")
        var p = 0f
        while (p <= 1f) {
            val going = Nav.through(p, isArriving = false, full.fadeOutOnTrack, full.crossfadeOnTrack)
            val coming = Nav.through(p, isArriving = true, full.fadeOutOnTrack, full.crossfadeOnTrack)
            assertTrue(going == 0f || coming == 0f, "at $p only one screen's words are drawn: $going and $coming")
            p += 0.01f
        }
        assertEquals(1f, Nav.through(0f, isArriving = false, full.fadeOutOnTrack, full.crossfadeOnTrack), "at rest the leaving screen is whole")
        assertEquals(1f, Nav.through(1f, isArriving = true, full.fadeOutOnTrack, full.crossfadeOnTrack), "and so is the arrived one")
    }

    @Test
    fun `one paid dwell, five seconds, which reduced motion leaves alone`() {
        assertEquals(5_000L, Motion.Dwell.PAID_MS)
    }

    @Test
    fun `the bubble's border chases at one speed, a turn and a quarter of a second, and not at all under Still`() {
        val chase = assertIs<TweenSpec<Float>>(full.chase!!.animation)
        assertEquals(1250, chase.durationMillis)
        assertEquals(Motion.CHASE_MS, chase.durationMillis)
        assertEquals(0.5f, chase.easing.transform(0.5f), "constant speed: half the turn is half the time")
        assertNull(still.chase, "under Still the border is whole ink, no loop")
    }

    @Test
    fun `the lens spreads ring by ring from the core, each a fade a step after the last, done well inside the paid dwell`() {
        assertEquals(4, Motion.SPREAD_RINGS, "the mark's rings: the core, the inner six, the middle twelve, the rim")
        assertEquals(Motion.SPREAD_RINGS, full.spread.size)
        val fade = assertIs<TweenSpec<Float>>(full.fade.fraction)
        for (ring in 0 until Motion.SPREAD_RINGS) {
            val step = assertIs<TweenSpec<Float>>(full.spread[ring].fraction)
            assertEquals(fade.durationMillis, step.durationMillis, "ring $ring is a fade")
            assertEquals(ring * Motion.SPREAD_STEP_MS, step.delay, "ring $ring starts a step after ring ${ring - 1}")
        }
        assertEquals(60, Motion.SPREAD_STEP_MS)
        val done = ms(full.spread.last().fraction)
        assertEquals(380, done.toInt(), "the rim is green by 380 ms")
        assertTrue(done < Motion.Dwell.PAID_MS / 10, "the lens is long done before the paid moment ends")
        for (ring in 0 until Motion.SPREAD_RINGS) {
            val step = assertIs<TweenSpec<Float>>(still.spread[ring].fraction)
            assertEquals(120, step.durationMillis, "under Still every ring is the quick fade")
            assertEquals(0, step.delay, "and none waits: one fade, no spread")
        }
    }

    @Test
    fun `the tick sweeps on once the core is green, waiting in its spec and never overshooting`() {
        val coreDone = ms(full.spread[0].fraction)
        val sweep = ms(full.sweep.fraction)
        val tick = full.tick.fraction.vectorize(Float.VectorConverter)
        val from = AnimationVector1D(0f)
        val to = AnimationVector1D(1f)
        val rest = AnimationVector1D(0f)
        assertEquals(coreDone + sweep, tick.getDurationNanos(from, to, rest) / 1_000_000, "the core's fade, then the sweep")
        var t = 0L
        while (t < coreDone * 1_000_000) {
            assertEquals(0f, tick.getValueFromNanos(t, from, to, rest).value, "at $t ns the tick waits for the core")
            assertEquals(0f, tick.getVelocityFromNanos(t, from, to, rest).value, "and is not moving")
            t += 8_000_000
        }
        val sprung = full.sweep.fraction.vectorize(Float.VectorConverter)
        var later = coreDone * 1_000_000
        var seenOnItsWay = false
        while (later <= (coreDone + sweep) * 1_000_000) {
            val value = tick.getValueFromNanos(later, from, to, rest).value
            assertEquals(sprung.getValueFromNanos(later - coreDone * 1_000_000, from, to, rest).value, value, "from the core's end the tick is the sweep itself")
            assertTrue(value <= 1f, "the tick never runs past its end: $value")
            if (value > 0.1f && value < 0.9f) seenOnItsWay = true
            later += 8_000_000
        }
        assertTrue(seenOnItsWay, "the tick is drawn on its way, not only whole")
        assertIs<SnapSpec<Float>>(still.tick.fraction, "under Still the tick is whole at once")
    }

    @Test
    fun `the tick leaves whole, its trim holds while it fades on fadeQuick, then rewinds, the same under Still`() {
        val fade = ms(full.fadeQuick.fraction)
        for (presets in listOf(full, still)) {
            val untick = presets.untick.fraction.vectorize(Float.VectorConverter)
            val from = AnimationVector1D(1f)
            val to = AnimationVector1D(0f)
            val rest = AnimationVector1D(0f)
            assertEquals(fade, untick.getDurationNanos(from, to, rest) / 1_000_000, "the trim rewinds as the quick fade ends")
            var t = 0L
            while (t < fade * 1_000_000) {
                assertEquals(1f, untick.getValueFromNanos(t, from, to, rest).value, "at $t ns the tick is still whole: it fades, it is not un-drawn")
                t += 8_000_000
            }
            assertEquals(0f, untick.getValueFromNanos(fade * 1_000_000, from, to, rest).value, "and once invisible it is rewound for the next paid")
        }
    }
}
