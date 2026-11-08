package com.monokulo.till.core

/**
 * A point of sale Till watches for Monero sales (`docs/design.md` §1,
 * "POS sources"), named as the Monokulo Bell names it (`docs/bell.md`:
 * a registration's `point_of_sale`, `/v1/info`'s `points_of_sale` keys and
 * a ding's `{"wake":"<pos>"}`). The `sales` module has a source for each;
 * the `push` module registers the till with a Bell for one.
 */
enum class PointOfSale(
    /** The name on the wire, between the till and a Bell. */
    val wire: String,
    /** The name on screen: "Sign in to Square", "Refund it in Clover". */
    val displayName: String,
) {
    Square("square", "Square"),
    Clover("clover", "Clover"),
    ;

    companion object {
        /** The point of sale [wire] names, or null for one this build doesn't know. */
        fun fromWire(wire: String): PointOfSale? = entries.firstOrNull { it.wire == wire }
    }
}
