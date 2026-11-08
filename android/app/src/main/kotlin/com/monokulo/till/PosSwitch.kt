package com.monokulo.till

import com.monokulo.till.core.PointOfSale

/**
 * What changing the point of sale touches, in the order [PosSwitch] does it:
 * the app's graph and stores behind it, a recording fake in `PosSwitchTest`.
 */
interface PosSwitchSide {
    /** The Bell's registration taken back, awaited, while the old sign-in is still here (acceptance T5.5). */
    suspend fun takeBellRegistrationBack()

    /** Signed out of [pos]: its tokens, the application they came through, its location or merchant and its remembered tender id forgotten. */
    fun signOut(pos: PointOfSale)

    /** Where [pos]'s search for sales had got to forgotten, so a sign-in to it later starts from then. */
    fun forgetCursor(pos: PointOfSale)

    /** [pos] saved as the till's point of sale. */
    fun choose(pos: PointOfSale)
}

/**
 * Choosing a point of sale (acceptance D6, decision 138), from step 4's rows
 * or Settings' "Change the point of sale". Choosing the one chosen changes
 * nothing. Choosing another takes the Bell's registration back first when
 * still signed in to the first, then always signs out of the old one, so a
 * sign-in that had already ended leaves no location, merchant, cursor or
 * tender id behind, then saves the new;
 * the Wake-ups choice and the shift's saved sales are never touched, so the
 * Bell registers again for the new point of sale once it is signed in, and
 * the old sales' pages still open, named by the point of sale that took them
 * (`FoundSale.pointOfSale`).
 */
object PosSwitch {
    /** Whether anything changed: the caller then shows the new point of sale's sign-in. */
    suspend fun choose(side: PosSwitchSide, from: PointOfSale?, signedIn: Boolean, to: PointOfSale): Boolean {
        if (from == to) return false
        if (from != null) {
            if (signedIn) side.takeBellRegistrationBack()
            side.signOut(from)
            side.forgetCursor(from)
        }
        side.choose(to)
        return true
    }

    /**
     * Settings' "Sign out of …": the Bell's registration taken back, awaited, then the sign-in
     * forgotten. The cursor stays, as Square's sign-out always kept it: signing in to the same
     * point of sale again carries on from where the till had got to rather than reading the
     * merchant's earlier payments again (decision 138).
     */
    suspend fun signOut(side: PosSwitchSide, pos: PointOfSale) {
        side.takeBellRegistrationBack()
        side.signOut(pos)
    }
}
