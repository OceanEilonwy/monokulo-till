package com.monokulo.till.sales

import com.monokulo.till.core.AppIdentity
import com.monokulo.till.core.Connection
import com.monokulo.till.core.MonokuloApi
import com.monokulo.till.core.PointOfSale
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import okhttp3.OkHttpClient
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

/** A POS with nothing new that takes [takes] to say so. */
private class SlowSource(private val takes: Duration = Duration.ZERO) : SaleSource {
    override val pointOfSale = PointOfSale.Square
    override suspend fun newSales(): List<FoundSale> {
        delay(takes)
        return emptyList()
    }
    override suspend fun order(sale: FoundSale) = SaleOrder(null)
    override val refunds = Refunds.ByTill {}
    override fun handled(sale: FoundSale) = Unit
}

class ShiftTest {
    /** A till over [source]: with no sales found, Monokulo is never asked. */
    private fun till(source: SaleSource) = Till(
        source,
        MonokuloApi(OkHttpClient(), AppIdentity("till", "Monokulo Till", "0.1.0", "android", "Pixel")),
        Connection("ak_1", "http://127.0.0.1:1/api/v1"),
    )

    /** Runs a shift in the background, recording each check's virtual time (ms) and cause. */
    private fun TestScope.shift(
        source: SaleSource = SlowSource(),
        interval: () -> Duration,
    ): Pair<Shift, MutableList<Pair<Long, WakeCause>>> {
        val checks = mutableListOf<Pair<Long, WakeCause>>()
        val shift = Shift(till(source), interval) { cause, _ -> checks += currentTime to cause }
        backgroundScope.launch { shift.run() }
        runCurrent()
        return shift to checks
    }

    @Test
    fun `the first check is the shift starting, then the timer at its pace`() = runTest {
        val (_, checks) = shift { 2.seconds }
        advanceTimeBy(4_001)
        assertEquals(listOf(0L to WakeCause.Started, 2_000L to WakeCause.Timer, 4_000L to WakeCause.Timer), checks)
    }

    @Test
    fun `a pace that flips from 2 s to 30 s and back mid-run takes effect on the next tick, without a restart`() = runTest {
        var pace = 2.seconds
        val (_, checks) = shift { pace }
        advanceTimeBy(5_000)
        pace = 30.seconds // the Bell connected, during the wait that ends at 6 s
        advanceTimeBy(36_000) // to 41 s
        assertEquals(listOf(0L, 2_000L, 4_000L, 34_000L), checks.map { it.first }, "30 s from the last check, not another 2 s tick")
        pace = 2.seconds // the Bell's wake-ups stopped, 7 s into a 30 s wait
        advanceTimeBy(1_001) // to 42 s: the next tick
        assertEquals(42_000L to WakeCause.Timer, checks.last(), "back at 2 s within a tick, not at 64 s")
        advanceTimeBy(2_000)
        assertEquals(44_000L, checks.last().first)
    }

    @Test
    fun `a wake checks at once and says why, and the timer starts again from it`() = runTest {
        val (shift, checks) = shift { 30.seconds }
        advanceTimeBy(10_000)
        shift.wake(WakeCause.Ding)
        runCurrent()
        advanceTimeBy(5_000)
        shift.wake(WakeCause.Opened)
        runCurrent()
        advanceTimeBy(30_001)
        assertEquals(
            listOf(0L to WakeCause.Started, 10_000L to WakeCause.Ding, 15_000L to WakeCause.Opened, 45_000L to WakeCause.Timer),
            checks,
        )
    }

    @Test
    fun `ten dings in a second make at most two checks, one in flight and one queued`() = runTest {
        val (shift, checks) = shift(SlowSource(takes = 300.milliseconds)) { 30.seconds }
        advanceTimeBy(1_000)
        repeat(10) {
            shift.wake(WakeCause.Ding)
            advanceTimeBy(10) // all ten inside the first one's check
        }
        advanceTimeBy(10_000)
        assertEquals(listOf(WakeCause.Started, WakeCause.Ding, WakeCause.Ding), checks.map { it.second })
    }
}
