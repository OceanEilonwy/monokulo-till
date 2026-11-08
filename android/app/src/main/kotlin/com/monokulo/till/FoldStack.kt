package com.monokulo.till

/**
 * The folded tray's stack (the fixes canvas's board 4): the hidden sales peek as two card edges under the third row,
 * like a notification stack, over a "+N" pill. Pulled down, the edges follow the finger, each taking half the pull,
 * widening toward the card and growing toward a row's height; let go past a third of [REACH_DP], or flung down, the
 * tray opens, else the stack settles back. All in dp, so it is the same on any screen; [pull] is how far the stack
 * is pulled past its rest (negative while it grows back in as the tray folds).
 */
object FoldStack {
    /** The two edges at rest, top first: their heights, their inset from the card each side, their bottom corners. */
    private val HEIGHTS = floatArrayOf(8f, 7f)
    private val INSETS = floatArrayOf(10f, 20f)
    private val CORNERS = floatArrayOf(14f, 12f)

    /** How many edges peek. */
    const val EDGES = 2

    /** The edges together at rest. */
    const val REST_DP = 15f

    /** The pull at which both edges are a row tall and as wide as the card: the stack fanned out whole. */
    const val REACH_DP = 104f

    /** How fast a downward fling on the stack has to be to open the tray, in dp a second (the panel's grabber's). */
    const val FLING_DP_PER_S = 800f

    /**
     * How far apart the pill sits in the two layouts: folded it is under the edges and 8 dp of tray, open it has 10
     * dp above it. A change of fold starts the pull this far off, so the pill does not jump as the edges come or go.
     */
    const val GAP_DP = REST_DP + 8f - 10f

    /** How far the stack is pulled when the finger has gone [raw] dp down: one to one up to [REACH_DP], a third past it. */
    fun pulled(raw: Float): Float = when {
        raw <= 0f -> 0f
        raw <= REACH_DP -> raw
        else -> REACH_DP + (raw - REACH_DP) / 3f
    }

    /** Whether letting go of the stack at [pull] dp, moving [velocity] dp a second down, opens the tray. */
    fun opens(pull: Float, velocity: Float): Boolean = velocity >= FLING_DP_PER_S || pull > REACH_DP / 3f

    /** How tall the stack's edges are together at [pull]: never less than nothing. */
    fun area(pull: Float): Float = (REST_DP + pull).coerceAtLeast(0f)

    /** Edge [i]'s height at [pull]: shrunk in proportion while the stack grows back in, half the pull past rest. */
    fun height(i: Int, pull: Float): Float =
        if (pull <= 0f) HEIGHTS[i] * area(pull) / REST_DP else HEIGHTS[i] + pull / 2f

    /** Edge [i]'s inset each side at [pull]: its rest inset, narrowing to the card's width as the pull reaches [REACH_DP]. */
    fun inset(i: Int, pull: Float): Float = INSETS[i] * (1f - (pull / REACH_DP).coerceIn(0f, 1f))

    /** Edge [i]'s bottom corners: its top is square, under the card or the edge above it. */
    fun corner(i: Int): Float = CORNERS[i]

    /** Where edge [i]'s top is at [pull]: under the edges above it. */
    fun top(i: Int, pull: Float): Float {
        var at = 0f
        for (j in 0 until i) at += height(j, pull)
        return at
    }
}
