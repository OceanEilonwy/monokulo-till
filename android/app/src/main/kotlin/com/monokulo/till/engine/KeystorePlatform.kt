package com.monokulo.till.engine

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import uniffi.till_app.Platform
import uniffi.till_app.SecretException

/**
 * The engine's [Platform] on Android: its secrets kept as `SecureStore` kept
 * them, each value sealed with AES-GCM under a key that never leaves the
 * Android Keystore, in the same preferences file under the same key alias,
 * so an upgraded till reads everything it had (docs/decisions/core.md 42).
 * The engine keeps each value as the UTF-8 of the string the Kotlin stores
 * wrote, which is exactly what was sealed.
 *
 * A secret that is there but can't be opened, or a Keystore that can't be
 * reached, is [SecretException.Unavailable], never "none" (decision 17):
 * taking it for none would replace a saved connection with nothing.
 */
class KeystorePlatform(context: Context) : Platform {
    private val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    override fun secretGet(key: String): ByteArray? {
        val stored = prefs.getString(key, null) ?: return null
        return unavailable("read $key") {
            val bytes = Base64.decode(stored, Base64.NO_WRAP)
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(128, bytes, 0, IV_BYTES))
            cipher.doFinal(bytes, IV_BYTES, bytes.size - IV_BYTES)
        }
    }

    override fun secretPut(key: String, value: ByteArray) {
        val sealed = unavailable("seal $key") {
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.ENCRYPT_MODE, secretKey())
            cipher.iv + cipher.doFinal(value)
        }
        // commit, not apply: a spent refresh token must never be the one left on disk.
        val written = prefs.edit().putString(key, Base64.encodeToString(sealed, Base64.NO_WRAP)).commit()
        if (!written) throw SecretException.Unavailable("couldn't write $key")
    }

    override fun secretRemove(key: String) {
        val written = prefs.edit().remove(key).commit()
        if (!written) throw SecretException.Unavailable("couldn't remove $key")
    }

    override fun formatMoney(minor: Long, currency: String): String = Money.format(minor, currency)

    private inline fun <T> unavailable(what: String, block: () -> T): T = try {
        block()
    } catch (e: Exception) {
        throw SecretException.Unavailable("couldn't $what: ${e.javaClass.simpleName}: ${e.message}")
    }

    private fun secretKey(): SecretKey {
        val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (keyStore.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return generator.generateKey()
    }

    private companion object {
        /** `SecureStore`'s file and key: what an upgraded till already holds. */
        const val PREFS = "till"
        const val ALIAS = "till-store"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val IV_BYTES = 12
    }
}
