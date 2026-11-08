package com.monokulo.till.sales

import java.security.MessageDigest
import java.security.SecureRandom
import java.util.Base64

/** One sign-in attempt's PKCE verifier and state (RFC 7636). */
data class Pkce(val verifier: String, val state: String) {
    /** `code_challenge`: base64url(SHA-256(verifier)), no padding. */
    val challenge: String
        get() = base64Url(MessageDigest.getInstance("SHA-256").digest(verifier.toByteArray(Charsets.US_ASCII)))

    companion object {
        private val random = SecureRandom()

        fun create(): Pkce = Pkce(randomString(64), randomString(32))

        private fun randomString(bytes: Int): String = base64Url(ByteArray(bytes).also(random::nextBytes))

        internal fun base64Url(bytes: ByteArray): String =
            Base64.getUrlEncoder().withoutPadding().encodeToString(bytes)
    }
}
