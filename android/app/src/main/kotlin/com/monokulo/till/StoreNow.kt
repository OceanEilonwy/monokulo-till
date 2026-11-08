package com.monokulo.till

import com.monokulo.till.core.AppConnection
import com.monokulo.till.core.Connection
import com.monokulo.till.core.MonokuloApi
import com.monokulo.till.core.MonokuloException
import com.monokulo.till.core.Problems
import com.monokulo.till.core.Store
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * The store and this device as last read from Monokulo, this session.
 * Held in memory and never saved (`docs/decisions/android.md` 50): shown
 * as "last seen" while the next read is under way.
 */
data class StoreNow(
    val store: Store? = null,
    val device: AppConnection? = null,
    /** The entry on the store's onion address (`mk:onion-api`), while it has one. */
    val onionApi: String? = null,
    /** A read is under way. */
    val reading: Boolean = false,
    /** Why the last read failed, if it did. */
    val problem: String? = null,
)

/** Reads the store live: on app start, after connecting, and each time Settings opens. */
class LiveStore(
    private val monokulo: MonokuloApi,
    /** The store answered 401: it disconnected this device. */
    private val onDisconnected: () -> Unit,
    /** Where the store's onion entry is now, or null for none: kept with the connection. */
    private val onOnionApi: (String?) -> Unit = {},
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val state = MutableStateFlow(StoreNow())
    val now: StateFlow<StoreNow> = state.asStateFlow()

    /** Reads the store and this device's connection again; what was last seen stays meanwhile. */
    fun read(connection: Connection?) {
        connection ?: return
        state.update { it.copy(reading = true, problem = null) }
        scope.launch {
            try {
                // coroutineScope, so a failed read lands in the catch below rather than the scope.
                val read = coroutineScope {
                    val device = async { monokulo.app(connection) }
                    val onion = async { monokulo.onionApi(connection) }
                    StoreNow(monokulo.store(connection), device.await(), onion.await())
                }
                // The connection keeps the onion entry before the read shows as done, so
                // whoever sees `reading` drop also sees the connection that came with it.
                onOnionApi(read.onionApi)
                state.value = read
            } catch (e: MonokuloException) {
                if (e.status == 401) onDisconnected()
                state.update { it.copy(reading = false, problem = Problems.say(e)) }
            } catch (e: Exception) {
                state.update { it.copy(reading = false, problem = "Couldn't reach the store: ${e.message}") }
            }
        }
    }

    /** Forgets what was seen (the till was disconnected). */
    fun forget() {
        state.value = StoreNow()
    }
}
