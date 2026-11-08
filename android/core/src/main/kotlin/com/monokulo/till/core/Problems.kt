package com.monokulo.till.core

/**
 * What the app says when Monokulo answers one of its routes with a
 * problem: one message per problem Monokulo's OpenAPI document lists for
 * the routes the app calls (the handshake, the entry, the store, the app,
 * orders and the event stream), saying what happened and, when someone
 * has something to do, who and what. MonokuloContractTest checks every
 * listed problem has one here, so a problem added at a re-pin fails there.
 */
object Problems {
    /** The message for [e], else its own detail for a problem not listed here. */
    fun say(e: MonokuloException): String = e.problem?.let { said(it, e) } ?: e.message.orEmpty().ifBlank { "HTTP ${e.status}" }

    /** Whether [problem] has a message of its own. */
    fun knows(problem: String): Boolean = said(problem, MonokuloException(0, "", problem)) != null

    private fun said(problem: String, e: MonokuloException): String? = when (problem) {
        // The handshake.
        "invalid-connect-code" -> "That code has been used or has expired. Make a new one on the store's Apps card."
        "no-public-address" ->
            "The store's Monokulo has no web address yet, so nothing can connect to it. Ask its admin to set one, then scan the same code again."
        // Every route.
        "unauthorized" -> "The store disconnected this device. Connect it again from the store's Apps card."
        "forbidden" ->
            "The store's Monokulo doesn't let this device's key do that. Disconnect it and connect again from the store's Apps card."
        "rate-limited" -> "The store is busy and asked for ${e.retryAfter ?: 60} seconds' wait before the next try."
        "challenge-required" -> "The store is busy and asked for a check this device couldn't pass. Try again in a moment."
        "internal" ->
            "Something went wrong on the store's Monokulo. Try again in a moment; if it keeps happening, its admin can find it in Monokulo's logs."
        "invalid-request", "too-large", "unsupported-media-type" ->
            "Monokulo didn't take what this app sent (${e.message}). Update Till; if it keeps happening, tell its makers."
        // Making an order: no order was made, so the till asks again with the same key.
        "rates-unavailable" ->
            "Monokulo has no exchange rate right now, so there's no payment code yet. Trying again shortly."
        "engine-unavailable" ->
            "Monokulo's payment engine is busy, so there's no payment code yet. Trying again shortly."
        "store-unavailable" ->
            "Monokulo can't read this store's settings right now, so there's no payment code yet. Trying again shortly; if it keeps happening, ask Monokulo's admin to check its database."
        "invalid-amount" ->
            "Monokulo can't price this sale (${e.message}), so it has no payment code. Take another payment for it and refund it in the POS."
        "idempotency-conflict" ->
            "Monokulo already has a different order for this POS sale, so it made no payment code. Take another payment for it and refund it in the POS."
        // Reading an order, and the event stream.
        "not-found" -> "The store has no such order any more."
        "too-many-streams" ->
            "Too many devices are following this store's sales at once. Close Till on one you aren't using, or disconnect it from the store's Apps card."
        else -> null
    }
}
