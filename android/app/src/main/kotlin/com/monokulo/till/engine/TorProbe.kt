package com.monokulo.till.engine

import java.net.InetSocketAddress
import java.net.Socket
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.till_app.Action

/**
 * Whether Orbot runs: something listens at its SOCKS proxy (docs/decisions/core.md
 * 10; android.md 55). The probe stays on the platform; its answer goes to
 * the engine as [Action.TorProbed], which connects over Tor only once a probe
 * said yes and has Settings say when Orbot isn't running.
 */
object TorProbe {
    /** Orbot's SOCKS proxy, where it listens unless told otherwise. */
    private const val HOST = "127.0.0.1"
    private const val PORT = 9050

    /** Whether something listens at Orbot's proxy, within [timeoutMs]. Blocks: off the main thread. */
    fun running(timeoutMs: Int = 500): Boolean = runCatching {
        Socket().use { it.connect(InetSocketAddress(HOST, PORT), timeoutMs) }
    }.isSuccess

    /** Probes on the IO dispatcher and tells the engine through [send]. */
    suspend fun probe(send: (Action) -> Unit) {
        val running = withContext(Dispatchers.IO) { running() }
        send(Action.TorProbed(running))
    }
}
