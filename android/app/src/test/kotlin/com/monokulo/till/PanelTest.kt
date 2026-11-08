package com.monokulo.till

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.TransformOrigin
import com.monokulo.till.core.SalePhase
import com.monokulo.till.overlay.Edge
import com.monokulo.till.overlay.EdgeSnap
import com.monokulo.till.overlay.Handoff
import com.monokulo.till.overlay.Life
import com.monokulo.till.overlay.PaidEvent
import com.monokulo.till.overlay.PaidMoment
import com.monokulo.till.overlay.PaidMoments
import com.monokulo.till.overlay.PaidStep
import com.monokulo.till.overlay.Panel
import com.monokulo.till.overlay.PanelFace
import com.monokulo.till.overlay.WindowAct
import com.monokulo.till.overlay.WindowEvent
import com.monokulo.till.overlay.Windows
import com.monokulo.till.overlay.Hero
import com.monokulo.till.overlay.PanelSale
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import uniffi.till_app.BubbleView
import uniffi.till_screens.BubbleTap
import uniffi.till_screens.OrderCard
import uniffi.till_screens.Press
import uniffi.till_screens.SaleView
import uniffi.till_screens.BubbleLook as EngineLook

class PanelTest {
    /**
     * The engine's bubble about one sale, its panel's words as
     * `till_screens::sale` and `bubble` write them: [hint] is the sale's,
     * [ended] its ended heading and line.
     */
    private fun bubble(
        id: String = "o1",
        step: Int = 1,
        spent: Boolean = false,
        hint: String = "Scan with any Monero wallet · 14:52 left",
        ended: Pair<String, String>? = null,
        order: OrderCard? = null,
    ) = BubbleView(
        shown = true,
        look = if (spent) EngineLook.PAID else EngineLook.LIT,
        orderId = id,
        label = "Till, \$79.21 waiting for payment",
        tap = BubbleTap.OPEN_PANEL,
        tapLabel = "Show the sale",
        tapOpen = BubbleTap.LOWER_PANEL,
        tapOpenLabel = "Lower to the bubble",
        panel = SaleView(
            title = "Sale #k4Rt", titleLine = "Front Counter · this till", amount = "\$79.21", xmr = "0.31684 XMR",
            code = "monero:4abc", codeSaid = "Payment code for \$79.21", spent = spent, status = "Waiting for payment",
            hint = hint, step = step.toUByte(), order = order, copyAddress = Press("Copy address", true), address = "4abc",
            backToPos = null, closes = true, ticks = step == 1,
        ),
        panelWhere = "Front Counter",
        endedHeading = ended?.first,
        endedLine = ended?.second,
        buzzes = true,
    )

    @Test
    fun `the card is gone and at its smallest when the panel is back in the bubble, and whole by a third of the way out`() {
        assertEquals(0f, Panel.shown(Panel.IN_BUBBLE))
        assertEquals(Panel.SMALLEST, Panel.scale(Panel.IN_BUBBLE))
        assertEquals(0f, Panel.opened(Panel.IN_BUBBLE / 2f))
        assertEquals(0f, Panel.shown(0f))
        val alone = Panel.IN_BUBBLE + (1f - Panel.IN_BUBBLE) * Panel.BUBBLE_ALONE
        assertEquals(0f, Panel.shown(alone), "the bubble's last stretch home has no card with it")
        assertTrue(Panel.shown(alone + 0.01f) > 0f)
        val third = Panel.IN_BUBBLE + (1f - Panel.IN_BUBBLE) * (Panel.BUBBLE_ALONE + 1f / 3f)
        assertEquals(1f, Panel.shown(third + 0.0001f))
        assertTrue(Panel.shown(third - 0.05f) < 1f)
        assertEquals(1f, Panel.scale(1f))
        assertEquals(1f, Panel.opened(1f))
    }

    @Test
    fun `the grabber lowers the panel past a quarter of its height, or on a fling`() {
        val fling = 2000f
        assertFalse(Panel.lowers(drag = 100f, height = 1000f, velocity = 0f, fling), "a short drag springs back")
        assertFalse(Panel.lowers(drag = 250f, height = 1000f, velocity = 0f, fling), "a quarter exactly isn't past it")
        assertTrue(Panel.lowers(drag = 251f, height = 1000f, velocity = 0f, fling))
        assertTrue(Panel.lowers(drag = 20f, height = 1000f, velocity = 2000f, fling), "a quick flick down lowers it")
        assertFalse(Panel.lowers(drag = 20f, height = 1000f, velocity = -3000f, fling), "flung up, it stays")
        assertFalse(Panel.lowers(drag = 20f, height = 0f, velocity = 0f, fling), "not measured yet")
    }

    @Test
    fun `the panel floats only on a screen whose smallest width is a tablet's`() {
        assertFalse(Panel.wide(411), "a phone, upright or on its side: its smallest width doesn't change")
        assertFalse(Panel.wide(599))
        assertTrue(Panel.wide(600), "a small tablet")
        assertTrue(Panel.wide(840))
    }

    @Test
    fun `the sale on the panel shows paid before the next one waiting comes up`() {
        val paid = PanelSale.of(bubble(id = "1044", step = 4, spent = true, hint = "0.31684 XMR received"))
        assertTrue(Panel.landed(paid, "1044", wasLive = true), "it was live a moment ago, now paid")
        assertFalse(Panel.landed(paid, "1044", wasLive = false), "already paid at the last look: not news")
        assertFalse(Panel.landed(PanelSale.of(bubble(id = "1043")), "1044", wasLive = true), "another sale, still waiting")
        assertFalse(Panel.landed(null, "1044", wasLive = true), "no sale at all")
    }

    @Test
    fun `the panel's status row counts down only while the code waits for its money`() {
        val waiting = PanelSale.of(bubble())!!
        assertEquals("· 14:52 left", waiting.left)
        assertEquals("· 1:05 left", PanelSale.of(bubble(hint = "Ask the customer to pay the rest to the same code · 1:05 left"))!!.left)
        assertNull(PanelSale.of(bubble(step = 3, hint = "The customer can go: it's being confirmed"))!!.left, "a payment on its way")
        val paid = PanelSale.of(bubble(step = 4, spent = true, hint = "0.31684 XMR received"))!!
        assertNull(paid.left)
        assertEquals("0.31684 XMR received", paid.received)
    }

    @Test
    fun `the free bubble pulses only while a code waits for its money and the panel is down`() {
        assertTrue(Panel.pulses(BubbleLook.Lit, SalePhase.Waiting, panelUp = false))
        assertTrue(Panel.pulses(BubbleLook.Lit, SalePhase.Partial, panelUp = false), "part paid, the rest still to come")
        assertFalse(Panel.pulses(BubbleLook.Lit, SalePhase.Waiting, panelUp = true), "docked above the panel: nothing to call for")
        assertFalse(Panel.pulses(BubbleLook.Lit, SalePhase.Seen, panelUp = false), "the payment is on its way")
        assertFalse(Panel.pulses(BubbleLook.Lit, SalePhase.Confirming, panelUp = false))
        assertFalse(Panel.pulses(BubbleLook.Paid, SalePhase.Paid, panelUp = false))
        assertFalse(Panel.pulses(BubbleLook.Idle, SalePhase.Waiting, panelUp = false))
    }

    @Test
    fun `a fling's speed carries into the collapse as progress a second`() {
        assertEquals(-2f, Panel.collapseVelocity(velocity = 3000f, height = 1500f), "flung down, the panel's progress falls")
        assertEquals(0f, Panel.collapseVelocity(velocity = 3000f, height = 0f), "not measured yet: no speed to carry")
        assertEquals(0f, Panel.collapseVelocity(velocity = 0f, height = 1500f), "dragged and let go")
    }

    @Test
    fun `the docked bubble is at the free bubble in the bubble, at the dock once open, and the card's pivot rides with it`() {
        val dock = Offset(1100f, 200f)
        val far = Offset(60f, 1400f)
        assertEquals(far, Panel.docked(dock, far, 0f), "collapsed: where the free bubble is")
        assertEquals(far, Panel.docked(dock, far, Panel.IN_BUBBLE), "in the bubble: still the free one's place")
        assertEquals(dock, Panel.docked(dock, far, 1f), "open: in its slot")
        val half = Panel.docked(dock, far, Panel.IN_BUBBLE + (1f - Panel.IN_BUBBLE) / 2f)
        assertEquals(580f, half.x, 0.01f)
        assertEquals(800f, half.y, 0.01f)
        // The card scales about that point, so its scaled bounds keep the point on every frame (decision 246).
        val card = Rect(40f, 280f, 1240f, 2400f)
        for (step in 0..20) {
            val raise = step / 20f
            val at = Panel.docked(dock, far, raise)
            val o = Panel.origin(at, card, corner = false)
            val s = Panel.scale(raise)
            val pivot = Offset(card.left + o.pivotFractionX * card.width, card.top + o.pivotFractionY * card.height)
            assertEquals(at.x, pivot.x, 0.5f, "raise $raise: the pivot is the bubble")
            assertEquals(at.y, pivot.y, 0.5f, "raise $raise: the pivot is the bubble")
            val scaled = Rect(pivot.x + (card.left - pivot.x) * s, pivot.y + (card.top - pivot.y) * s, pivot.x + (card.right - pivot.x) * s, pivot.y + (card.bottom - pivot.y) * s)
            assertTrue(scaled.contains(at) || at.y <= scaled.top, "raise $raise: the scaled card $scaled holds or sits under the bubble $at")
        }
    }

    @Test
    fun `a phone's panel grows about its docked bubble, a tablet's from its side nearest the bubble, at the bubble's height`() {
        val card = Rect(100f, 200f, 500f, 1000f)
        assertEquals(TransformOrigin(0.875f, -0.05f), Panel.origin(Offset(450f, 160f), card, corner = false), "above the card's end: the dock")
        assertEquals(TransformOrigin(1f, 0.125f), Panel.origin(Offset(1200f, 300f), card, corner = true), "bubble beside the card's right side")
        assertEquals(TransformOrigin(0f, 1f), Panel.origin(Offset(20f, 1400f), card, corner = true), "bubble below the card on the left: its corner")
        assertEquals(TransformOrigin(0f, 0f), Panel.origin(Offset(20f, 100f), card, corner = true), "above it: the top corner")
        assertEquals(TransformOrigin.Center, Panel.origin(Offset(20f, 900f), Rect(0f, 0f, 0f, 0f), corner = true), "not placed yet")
    }

    private fun want(w: Windows, usable: Boolean, open: Boolean, wide: Boolean = false) = Handoff.next(w, WindowEvent.Want(usable, open, wide))

    @Test
    fun `a failed poll or a dropped stream leaves the overlay up, and a raise is never closed for it`() {
        val running = RunnerState(running = true, live = true)
        assertTrue(Handoff.usable(running, appVisible = false, wanted = true, allowed = true))
        // What the runner's state is after a stream or poll fails that isn't the store's 401 (TillRunner.follow).
        val failed = running.copy(live = false, streamProblem = "Monokulo didn't answer in time")
        assertTrue(Handoff.usable(failed, appVisible = false, wanted = true, allowed = true), "a slow connection doesn't lower the panel")
        assertFalse(Handoff.usable(running.copy(live = false, disconnected = true), appVisible = false, wanted = true, allowed = true), "the store's 401 does")
        assertFalse(Handoff.usable(running, appVisible = true, wanted = true, allowed = true), "Till on screen lowers it at once")
        assertFalse(Handoff.usable(running.copy(running = false), appVisible = false, wanted = true, allowed = true))
        assertFalse(Handoff.usable(running, appVisible = false, wanted = false, allowed = true))
        assertFalse(Handoff.usable(running, appVisible = false, wanted = true, allowed = false))
        // A tablet's panel raised and growing: the failure's new state asks for the same windows, so the raise runs on.
        for (wide in listOf(true, false)) {
            var w = want(Windows(), usable = true, open = false, wide = wide).windows
            w = want(w, usable = true, open = true, wide = wide).windows
            w = Handoff.next(w, WindowEvent.PanelDrawn).also { assertTrue(WindowAct.OpenPanel in it.acts) }.windows
            val again = want(w, Handoff.usable(failed, appVisible = false, wanted = true, allowed = true), open = true, wide = wide)
            assertTrue(again.acts.isEmpty(), "wide=$wide: no close, no reopen, so the raise is never retargeted")
            assertEquals(w, again.windows)
        }
    }

    @Test
    fun `on a phone the free bubble goes only once the panel has drawn the docked one, and comes back before the panel goes`() {
        var w = want(Windows(), usable = true, open = false).also { assertEquals(listOf(WindowAct.AddBubble), it.acts) }.windows
        assertEquals(Windows(usable = true, bubble = Life.Up), w)
        w = want(w, usable = true, open = true).also { assertEquals(listOf(WindowAct.AddPanel), it.acts, "the panel goes up at 0 over the bubble") }.windows
        assertEquals(Life.Up, w.bubble, "no frame with no bubble")
        assertEquals(w, want(w, usable = true, open = true).windows, "asked again, nothing changes")
        assertTrue(want(w, usable = true, open = true).acts.isEmpty())
        w = Handoff.next(w, WindowEvent.PanelDrawn).also {
            assertEquals(listOf(WindowAct.RemoveBubble, WindowAct.OpenPanel), it.acts, "then the free bubble goes and the panel grows")
        }.windows
        w = want(w, usable = true, open = false).also { assertEquals(listOf(WindowAct.ClosePanel), it.acts, "no free bubble yet") }.windows
        assertEquals(Life.Gone, w.bubble)
        w = Handoff.next(w, WindowEvent.PanelLanded).also { assertEquals(listOf(WindowAct.AddBubbleAtRest), it.acts) }.windows
        assertEquals(Life.Landed, w.panel, "the panel stays until the free bubble has drawn over its docked one")
        w = Handoff.next(w, WindowEvent.BubbleDrawn).also { assertEquals(listOf(WindowAct.RemovePanel), it.acts) }.windows
        assertEquals(Windows(usable = true, bubble = Life.Up), w)
    }

    @Test
    fun `a tap mid-collapse grows the panel again from where it is`() {
        var w = Windows(usable = true, bubble = Life.Gone, panel = Life.Up, panelDrawn = true)
        w = want(w, usable = true, open = false).windows
        w = want(w, usable = true, open = true).also { assertEquals(listOf(WindowAct.OpenPanel), it.acts) }.windows
        assertEquals(Life.Up, w.panel)
        assertTrue(Handoff.next(w, WindowEvent.PanelLanded).acts.isEmpty(), "the interrupted collapse's landing is nothing")
        // Reopened in the frame the free bubble came back: it goes again at once, the panel being drawn.
        val landed = Windows(usable = true, bubble = Life.Up, panel = Life.Landed, panelDrawn = true)
        assertEquals(listOf(WindowAct.OpenPanel, WindowAct.RemoveBubble), want(landed, usable = true, open = true).acts)
    }

    @Test
    fun `a fling's speed reaches the collapse whatever lands between the fling and the lower`() {
        // A phone's panel up with its docked bubble; the free bubble is gone.
        var w = Windows(usable = true, bubble = Life.Gone, panel = Life.Up, panelDrawn = true)
        w = Handoff.next(w, WindowEvent.Flung(-2.5f)).windows
        // The panel's pre-draw and a bubble's exit land before the overlay's flow asks for the panel down.
        w = Handoff.next(w, WindowEvent.PanelDrawn).windows
        w = Handoff.next(w, WindowEvent.BubbleLeft).windows
        val lowered = want(w, usable = true, open = false)
        assertEquals(listOf(WindowAct.ClosePanel), lowered.acts)
        assertEquals(-2.5f, lowered.closeVelocity, "the collapse starts at the grabber's speed")
        assertNull(lowered.windows.flung, "and spends it")
        // A collapse that isn't a fling keeps its own velocity.
        val tapped = want(want(lowered.windows, usable = true, open = true).windows, usable = true, open = false)
        assertNull(tapped.closeVelocity)
    }

    @Test
    fun `a fling with no panel up, or before the panel is raised again, carries nothing`() {
        assertNull(Handoff.next(Windows(usable = true, bubble = Life.Up), WindowEvent.Flung(-3f)).windows.flung)
        val up = Windows(usable = true, bubble = Life.Gone, panel = Life.Up, panelDrawn = true)
        val flung = Handoff.next(up, WindowEvent.Flung(-3f)).windows
        assertNull(want(flung, usable = true, open = true).windows.flung, "raised again before it lowered: spent")
    }

    @Test
    fun `a tablet's panel sits beside a bubble on its edge, never over it`() {
        val beside = Panel.floatingInsetDp(bubbleOnEnd = true)
        // The card is drawn its shadow's width in from the window's edge.
        assertTrue(beside + Panel.FLOATING_SHADOW_DP > Panel.BUBBLE_WINDOW_DP, "the card starts past the bubble's window")
        assertTrue(beside + Panel.FLOATING_SHADOW_DP - Panel.BUBBLE_WINDOW_DP <= 16, "and close beside it, so it grows from the bubble")
        assertTrue(Panel.floatingInsetDp(bubbleOnEnd = false) < beside, "with the bubble on the other edge the panel keeps to its own")
    }

    @Test
    fun `on a tablet the bubble stays beside the panel`() {
        var w = want(Windows(), usable = true, open = true, wide = true).also {
            assertEquals(listOf(WindowAct.AddPanel, WindowAct.AddBubble), it.acts)
        }.windows
        w = Handoff.next(w, WindowEvent.PanelDrawn).also { assertEquals(listOf(WindowAct.OpenPanel), it.acts) }.windows
        w = want(w, usable = true, open = false, wide = true).also { assertEquals(listOf(WindowAct.ClosePanel), it.acts) }.windows
        w = Handoff.next(w, WindowEvent.PanelLanded).also { assertEquals(listOf(WindowAct.RemovePanel), it.acts) }.windows
        assertEquals(Windows(usable = true, wide = true, bubble = Life.Up), w)
    }

    @Test
    fun `a panel of the other kind is swapped without motion`() {
        val phone = Windows(usable = true, panel = Life.Up, panelDrawn = true)
        val step = want(phone, usable = true, open = true, wide = true)
        assertEquals(listOf(WindowAct.RemovePanel, WindowAct.AddPanelOpen, WindowAct.AddBubble), step.acts)
    }

    @Test
    fun `Till coming to the front collapses the panel and the bubble goes with it`() {
        var w = Windows(usable = true, panel = Life.Up, panelDrawn = true)
        w = want(w, usable = false, open = false).also { assertEquals(listOf(WindowAct.ClosePanel), it.acts) }.windows
        w = Handoff.next(w, WindowEvent.PanelLanded).also { assertEquals(listOf(WindowAct.RemovePanel), it.acts, "no free bubble over the app") }.windows
        assertEquals(Windows(), w)
        var free = Windows(usable = true, bubble = Life.Up)
        free = want(free, usable = false, open = false).also { assertEquals(listOf(WindowAct.HideBubble), it.acts) }.windows
        assertEquals(listOf(WindowAct.ShowBubble), want(free, usable = true, open = false).acts, "back before it went: it grows again")
        assertEquals(listOf(WindowAct.RemoveBubble), Handoff.next(free, WindowEvent.BubbleLeft).acts)
    }

    @Test
    fun `a let-go bubble goes flush against the nearer side`() {
        val width = 264
        val screen = 1280
        assertEquals(EdgeSnap(left = true, x = 0), Edge.snap(x = 300, width, screen))
        assertEquals(EdgeSnap(left = false, x = 1016), Edge.snap(x = 600, width, screen), "its centre past the middle")
        assertEquals(EdgeSnap(left = true, x = 0), Edge.snap(x = 507, width, screen), "centre at 639: still the left")
        assertEquals(EdgeSnap(left = false, x = 1016), Edge.snap(x = 508, width, screen), "centre at 640: the right")
    }

    @Test
    fun `a paid moment is held for the dwell, then the panel collapses into the green bubble, which fades once it lands`() {
        val held = PaidMoments.next(PaidMoment.None, PaidEvent.Paid, panelUp = true, nextWaiting = false)
        assertEquals(PaidStep(PaidMoment.Held), held)
        assertTrue(held.moment.paid, "the bubble is green")
        val dwelt = PaidMoments.next(PaidMoment.Held, PaidEvent.DwellEnded, panelUp = true, nextWaiting = false)
        assertEquals(PaidStep(PaidMoment.Lowering, lower = true), dwelt, "the panel collapses")
        assertTrue(dwelt.moment.paid, "into a bubble still green")
        assertEquals(PaidStep(PaidMoment.None), PaidMoments.next(PaidMoment.Lowering, PaidEvent.ExitLanded, panelUp = false, nextWaiting = false))
    }

    @Test
    fun `Done during the dwell ends it with the same collapse`() {
        val done = PaidMoments.next(PaidMoment.Held, PaidEvent.Lowered, panelUp = true, nextWaiting = false)
        assertEquals(PaidStep(PaidMoment.Lowering), done, "the cashier's lower is the collapse: no second one")
        assertEquals(PaidStep(PaidMoment.Lowering), PaidMoments.next(done.moment, PaidEvent.DwellEnded, panelUp = false, nextWaiting = false), "the dwell was cancelled; a late one changes nothing")
        assertEquals(PaidMoment.None, PaidMoments.next(done.moment, PaidEvent.ExitLanded, panelUp = false, nextWaiting = false).moment)
    }

    @Test
    fun `with another sale waiting, or no panel up, the moment ends at the dwell without a collapse`() {
        assertEquals(PaidStep(PaidMoment.None), PaidMoments.next(PaidMoment.Held, PaidEvent.DwellEnded, panelUp = true, nextWaiting = true), "the next code comes up in the panel")
        assertEquals(PaidStep(PaidMoment.None), PaidMoments.next(PaidMoment.Held, PaidEvent.DwellEnded, panelUp = false, nextWaiting = false), "the bubble alone: green for the dwell, then it fades")
        assertEquals(PaidStep(PaidMoment.Held), PaidMoments.next(PaidMoment.Lowering, PaidEvent.Paid, panelUp = false, nextWaiting = false), "another payment mid-collapse: green again")
        assertEquals(PaidStep(PaidMoment.None), PaidMoments.next(PaidMoment.None, PaidEvent.ExitLanded, panelUp = false, nextWaiting = false), "a collapse with nothing paid")
        assertEquals(PaidStep(PaidMoment.None), PaidMoments.next(PaidMoment.None, PaidEvent.Lowered, panelUp = true, nextWaiting = false))
    }

    @Test
    fun `the panel shows the code while live, the tick once paid, and where the code was once it ended`() {
        fun face(b: BubbleView) = PanelSale.of(b)!!.face
        assertEquals(PanelFace.Code, face(bubble(step = 1)))
        assertEquals(PanelFace.Code, face(bubble(step = 3)))
        assertEquals(PanelFace.Paid, face(bubble(step = 4, spent = true)))
        val expired = bubble(step = 0, ended = "Code expired · not paid" to "Open the shift to refund it in Square.")
        assertEquals(PanelFace.Ended, face(expired))
        assertEquals(Hero.Ended, PanelSale.of(expired)!!.hero)
        val reversed = bubble(step = 0, ended = "Payment reversed · don't hand it over" to "Open the shift to refund it in Square.")
        assertEquals(PanelFace.Ended, face(reversed))
        assertEquals(Hero.Reversed, PanelSale.of(reversed)!!.hero)
        val refunded = bubble(step = 0, ended = "Refunded in Square" to "Nothing more to do.")
        assertEquals(Hero.Refunded, PanelSale.of(refunded)!!.hero)
        assertEquals(Hero.Paid, PanelSale.of(bubble(step = 4, spent = true))!!.hero)
        assertNull(PanelSale.of(bubble().copy(panel = null)), "no sale: no panel")
    }
}
