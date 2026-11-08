package com.monokulo.till.core

import java.security.MessageDigest

/**
 * Monokulo's proof of work (`challenge-required`): a nonce such that
 * SHA-256 of the challenge followed by the nonce starts with `difficulty`
 * zero bits, sent back as `Monokulo-Proof: <challenge>.<nonce>`.
 */
object Proof {
    fun solve(challenge: String, difficulty: Int): String {
        val sha = MessageDigest.getInstance("SHA-256")
        var nonce = 0L
        while (zeroBits(sha.digest("$challenge$nonce".toByteArray())) < difficulty) nonce++
        return "$challenge.$nonce"
    }

    internal fun zeroBits(hash: ByteArray): Int {
        var bits = 0
        for (byte in hash) {
            val b = byte.toInt() and 0xff
            if (b == 0) {
                bits += 8
            } else {
                return bits + Integer.numberOfLeadingZeros(b) - 24
            }
        }
        return bits
    }
}
