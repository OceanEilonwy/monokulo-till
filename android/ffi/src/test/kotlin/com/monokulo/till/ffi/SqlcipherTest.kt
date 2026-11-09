package com.monokulo.till.ffi

import java.nio.file.Files
import kotlin.test.Test
import kotlin.test.assertTrue
import uniffi.till_ffi.sqlcipherCheck

/**
 * The library loads through the bindings and carries SQLCipher, which
 * encrypts and reads back as the ledger will use it (docs/decisions/core.md
 * 143). This runs against the library built for this machine; the ABIs'
 * builds link the same SQLCipher with OpenSSL built in.
 */
class SqlcipherTest {
    @Test
    fun `the library carries SQLCipher`() {
        val dir = Files.createTempDirectory("sqlcipher").toFile()
        try {
            val version = sqlcipherCheck(dir.path)
            assertTrue(version.startsWith("4"), "cipher_version $version")
        } finally {
            dir.deleteRecursively()
        }
    }
}
