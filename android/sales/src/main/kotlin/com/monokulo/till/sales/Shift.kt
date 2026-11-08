package com.monokulo.till.sales

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.selects.onTimeout
import kotlinx.coroutines.selects.select
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

/** Why the till checked the POS: what [Shift.run] tells its `onChecked`. */
enum class WakeCause {
    /** The shift starting: the first check, for whatever came in while nothing watched. */
    Started,

    /** The shift's timer, at its pace. */
    Timer,

    /** A ding from the Monokulo Bell (`docs/design.md` §3). */
    Ding,

    /** The app came to the front. */
    Opened,
}

/**
 * What makes the till check the POS (`docs/design.md` §3): a timer during
 * a shift, and anything that calls [wake] (the app opening, a ding from a
 * Monokulo Bell). Every wake-up does the same idempotent check.
 */
class Shift(
    private val till: Till,
    /**
     * How often to check without a wake-up: 2 s alone, 30 s with a Bell.
     * Read on every tick, so a change takes effect within [TICK].
     */
    private val interval: () -> Duration = { 2.seconds },
    /** After every check: why it ran, and how many sales it found that no check had found before. */
    private val onChecked: (WakeCause, Int) -> Unit = { _, _ -> },
) {
    /** One wake-up kept while a check runs: ten dings in a second make at most two checks (acceptance A8.37). */
    private val wakeups = Channel<WakeCause>(Channel.CONFLATED)

    /** Check now, rather than at the next tick. */
    fun wake(cause: WakeCause) {
        wakeups.trySend(cause)
    }

    /** Checks until cancelled. */
    suspend fun run() {
        var cause = WakeCause.Started
        while (true) {
            onChecked(cause, till.check())
            cause = next()
        }
    }

    /**
     * Waits for a wake-up or the timer. The wait goes in ticks of [TICK] at
     * most, [interval] read again at each, so the pace dropping from 30 s
     * to 2 s mid-wait checks within a tick rather than at the end of the
     * 30 s (acceptance A5.24). Counted in waits, not read off a clock, so
     * virtual time drives it in tests. A `select`, not a timeout around
     * `receive`: a wake-up landing as a tick ends is taken or left, never
     * taken and dropped.
     */
    @OptIn(ExperimentalCoroutinesApi::class)
    private suspend fun next(): WakeCause {
        var waited = Duration.ZERO
        while (true) {
            val left = interval() - waited
            if (left <= Duration.ZERO) return WakeCause.Timer
            val step = minOf(left, TICK)
            select {
                wakeups.onReceive { it }
                onTimeout(step) { null }
            }?.let { return it }
            waited += step
        }
    }

    companion object {
        /** The longest the timer waits before reading [interval] again: the shortest pace. */
        private val TICK: Duration = 2.seconds
    }
}
