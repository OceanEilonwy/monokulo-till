package com.monokulo.till

/**
 * A shift row swiped left (the owner's "left swipe a sale"): the row follows the finger over the X's danger
 * ground, and let go past a third of its width, or flung that way, it asks what the sale page's X asks (the close
 * sheet); either way the row settles back. All in dp, so it is the same on any screen; [pulled] is how far the row has
 * moved, negative to the left. Which rows swipe is the engine's (a shift row's `closes`).
 */
object RowSwipe {
    /** How fast a fling to the left has to be to ask, in dp a second (the fold stack's and the panel's grabber's). */
    const val FLING_DP_PER_S = FoldStack.FLING_DP_PER_S

    /** The share of the row's width a release has to be past to ask. */
    const val REACH = 1f / 3f

    /** How far the row is moved when the finger has gone [raw] dp along it: to the left only, never past its [width]. */
    fun pulled(raw: Float, width: Float): Float = raw.coerceIn(-width, 0f)

    /**
     * Whether letting go of the row at [pull] dp, moving [velocity] dp a second along it, asks: flung left, or
     * past [REACH] of its [width] and not flung back.
     */
    fun asks(pull: Float, width: Float, velocity: Float): Boolean =
        velocity <= -FLING_DP_PER_S || (pull < -width * REACH && velocity < FLING_DP_PER_S)
}
