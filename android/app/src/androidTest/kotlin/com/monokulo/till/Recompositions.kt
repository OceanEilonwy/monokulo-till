package com.monokulo.till

import androidx.compose.runtime.Composer
import androidx.compose.runtime.CompositionTracer
import androidx.compose.runtime.InternalComposeTracingApi

/**
 * Counts how often each composable's body runs (a recomposition that
 * skips doesn't run it), by the compiler's trace markers: Layout
 * Inspector's recomposition counts, in a test. [count] takes the
 * function's qualified name, `com.monokulo.till.ui.SaleRow`.
 */
@OptIn(InternalComposeTracingApi::class)
class Recompositions : AutoCloseable {
    private val counts = HashMap<String, Int>()

    init {
        Composer.setTracer(object : CompositionTracer {
            override fun isTraceInProgress() = true

            override fun traceEventStart(key: Int, dirty1: Int, dirty2: Int, info: String) {
                val name = info.substringBefore(' ')
                counts[name] = (counts[name] ?: 0) + 1
            }

            override fun traceEventEnd() = Unit
        })
    }

    fun count(name: String): Int = counts[name] ?: 0

    fun reset() = counts.clear()

    override fun close() = Composer.setTracer(object : CompositionTracer {
        override fun isTraceInProgress() = false
        override fun traceEventStart(key: Int, dirty1: Int, dirty2: Int, info: String) = Unit
        override fun traceEventEnd() = Unit
    })
}
