package com.monokulo.till.ffi

import android.content.Context

/**
 * Certificate checks for the Rust core's requests (docs/decisions/core.md
 * 36): reqwest's rustls verifies through `rustls-platform-verifier`, which on
 * Android asks the system's trust manager (so a certificate authority the
 * store's owner installed still works) through the JVM, with the
 * `rustls-platform-verifier` Maven component this module brings. It needs
 * the app's context once, before the engine's first request, or that
 * request panics: [init] from `Application.onCreate`.
 */
object Tls {
    @Volatile
    private var done = false

    /** Hands the core the app's context. Safe to call again: only the first call counts. */
    @Synchronized
    fun init(context: Context) {
        if (done) return
        // The same library JNA loads for the bindings; System.loadLibrary binds this JNI entry to it.
        System.loadLibrary("till_ffi")
        initWithContext(context.applicationContext)
        done = true
    }

    @JvmStatic
    private external fun initWithContext(context: Context)
}
