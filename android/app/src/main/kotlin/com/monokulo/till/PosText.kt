package com.monokulo.till

import androidx.annotation.DrawableRes
import com.monokulo.till.core.PointOfSale
import com.monokulo.till.sales.TillSale

/**
 * Every word on screen that names the point of sale (acceptance D5, decision
 * 136): Square's and Clover's from one exhaustive `when` each, so a point of
 * sale added later doesn't compile until it has its words, and no screen
 * spells "Square" itself. A null point of sale is a till that runs without
 * one (or hasn't chosen yet): "your point of sale". Pure: tested in
 * `PosTextTest`, and through the screens' own texts.
 */
object PosText {
    /** The point of sale's mark beside its name (`res/drawable/ic_pos_*`, each brand's own art), drawn by [com.monokulo.till.ui.PosMark]. */
    @DrawableRes
    fun mark(pos: PointOfSale): Int = when (pos) {
        PointOfSale.Square -> R.drawable.ic_pos_square
        PointOfSale.Clover -> R.drawable.ic_pos_clover
    }

    /** Whether [pos]'s mark is drawn in the theme's ink: Square's is one colour, black or white; Clover's keeps its green. */
    fun markInInk(pos: PointOfSale): Boolean = when (pos) {
        PointOfSale.Square -> true
        PointOfSale.Clover -> false
    }

    /** "Square", "Clover", or "your point of sale" with none. */
    fun name(pos: PointOfSale?): String = pos?.displayName ?: "your point of sale"

    /** [name] at the start of a sentence: "Your point of sale" with none. */
    fun nameFirst(pos: PointOfSale?): String = name(pos).replaceFirstChar(Char::uppercase)

    /** What the till watches at the point of sale: a Square location, a Clover merchant (its only location, acceptance D1). */
    fun place(pos: PointOfSale?): String = when (pos) {
        PointOfSale.Square, null -> "location"
        PointOfSale.Clover -> "merchant"
    }

    /** [place] as a row's name: "Location", "Merchant". */
    fun placeRow(pos: PointOfSale?): String = place(pos).replaceFirstChar(Char::uppercase)

    /** "Square location", "Clover merchant". */
    fun placeAt(pos: PointOfSale?): String = pos?.let { "${it.displayName} ${place(it)}" } ?: place(null)

    /** "Square sign-in", "point-of-sale sign-in". */
    fun signInOf(pos: PointOfSale?): String = "${pos?.displayName ?: "point-of-sale"} sign-in"

    fun signIn(pos: PointOfSale?): String = "Sign in to ${name(pos)}"

    fun signInAgain(pos: PointOfSale?): String = "Sign in to ${name(pos)} again"

    fun signOut(pos: PointOfSale): String = "Sign out of ${pos.displayName}"

    /** "Signing in…" on the sign-in button while it waits. */
    const val SIGNING_IN = "Signing in…"

    /** Settings' section and onboarding step 4's heading. */
    const val POINT_OF_SALE = "Point of sale"
    const val CONNECT_HEADING = "Connect your point of sale"
    const val CHANGE = "Change the point of sale"

    /** The choice rows' details (acceptance T1.1). */
    fun choiceDetail(pos: PointOfSale): String = when (pos) {
        PointOfSale.Square -> "Square Point of Sale on a phone, tablet or Register"
        PointOfSale.Clover -> "A Clover device"
    }

    /** The Welcome screen's first feature: every point of sale the till knows. */
    val WELCOME_FEATURE = "Ring up in ${PointOfSale.entries.joinToString(" or ") { it.displayName }} as usual and choose Monero as the payment."

    // Step 4's card, for the point of sale chosen.

    fun connect(pos: PointOfSale): String = "Connect ${pos.displayName}"

    fun lead(pos: PointOfSale): String = "Till checks ${pos.displayName} for each Monero payment you take, so it can show the code straight away."

    fun account(pos: PointOfSale): String = "${pos.displayName} account"

    /** What the till may do at the point of sale, and that its sign-in stays here (acceptance T2.6). */
    fun permissions(pos: PointOfSale): String = when (pos) {
        PointOfSale.Square -> "Till can read payments, orders and locations, and refund a Monero sale that's never paid."
        PointOfSale.Clover -> "Till can read payments and orders, and add a Monero payment type."
    } + " Your ${pos.displayName} sign-in stays on this device: Monokulo never sees it, or what was bought."

    /** The section over the steps at the point of sale itself. */
    fun atTheTill(pos: PointOfSale): String = when (pos) {
        PointOfSale.Square -> "In Square Point of Sale"
        PointOfSale.Clover -> "On your Clover device"
    }

    /** The title of Clover's steps card, as Square's names what its steps do. */
    const val CLOVER_STEPS_TITLE = "Take Monero in Clover Register"

    /** Clover's first step, once the till has the Monero payment type ([added]) or before it has looked. */
    fun cloverTenderStep(added: Boolean): String =
        "Till ${if (added) "added" else "adds"} a payment type called **Monero** for you"

    /** Clover's steps after the payment type (acceptance T3.3), numbered from 2. */
    val CLOVER_STEPS = listOf(
        "Open **Clover Register** on the Station, Mini, Flex or Go",
        "Ring up a sale, tap **Pay**, then choose **Monero**",
    )

    /** Clover refused to let the till add the payment type (acceptance T3.2, decision 129). */
    const val CLOVER_ADD_TENDER = "Add a payment type called **Monero** in Clover (Setup › Payments, or ask whoever manages your " +
        "Clover account), then tap **Look again**"
    const val LOOK_AGAIN = "Look again"

    /** Settings' payment type row while the Monero payment type isn't at [pos] yet (decision 129). */
    fun tenderMissing(pos: PointOfSale): String = "Not in ${pos.displayName} yet"

    /** How a sale never paid is refunded, said before the first one is (decision 130). */
    const val CLOVER_REFUND_NOTE = "A sale never paid in Monero is refunded in Clover's Orders app; Till shows you which."

    /** Step 4's test sale row while it waits: Clover's names where the refund is made, as its card's footnote does. */
    fun testSaleWaiting(pos: PointOfSale): String = "Waiting for a test sale… " + when (pos) {
        PointOfSale.Square -> "Any amount, you can refund it after."
        PointOfSale.Clover -> "Any amount; refund it in ${refundWhere(pos)} after."
    }

    fun testSaleFound(pos: PointOfSale, amount: String): String = "Test sale found: $amount. Refund it in " + when (pos) {
        PointOfSale.Square -> pos.displayName
        PointOfSale.Clover -> refundWhere(pos)
    } + " when you like."

    // Square's locations, a step of its own.

    val LOCATIONS_LEAD = "Till shows the Monero sales ${PointOfSale.Square.displayName} takes there."
    val NO_LOCATIONS = "${PointOfSale.Square.displayName} has no active locations"

    // Steps 5 and 6.

    /** The bubble floats over the point-of-sale app on this phone; beside a Clover device, over whatever the phone shows. */
    fun floatOver(pos: PointOfSale?): String = when (pos) {
        PointOfSale.Square -> "Square"
        PointOfSale.Clover, null -> "other apps"
    }

    fun floatHeading(pos: PointOfSale?): String = "Float over ${floatOver(pos)}"

    fun floatLead(pos: PointOfSale?): String = "Till puts a small bubble over ${floatOver(pos)}. It's faded while there's nothing to do " +
        "and lights up when a Monero sale comes in. Tap it to show the code."

    fun readyLead(pos: PointOfSale?, bubble: Boolean): String {
        val start = when (pos) {
            PointOfSale.Square -> "Start Till and switch to Square."
            PointOfSale.Clover -> "Start Till and ring up on your Clover device."
            null -> "Start Till."
        }
        val wait = if (bubble) {
            "Till waits in a faded bubble over ${floatOver(pos)} and lights up when a sale comes in."
        } else {
            "Till waits in its notification and alerts you when a sale comes in."
        }
        return "$start $wait"
    }

    /** The Ready screen's check row and Settings' fact: the point of sale's name. */
    fun checkName(pos: PointOfSale?): String = pos?.displayName ?: POINT_OF_SALE

    // Settings.

    /** Under the signed-in facts. */
    fun signedInFootnote(pos: PointOfSale): String = "Sales paid with this payment type at this ${place(pos)} show up in Till."

    /** Under the sign-in row, or the choice rows. */
    fun withoutFootnote(pos: PointOfSale?): String =
        "Without ${pos?.displayName ?: "a point of sale"}, Till shows the sales other tills make, without receipts."

    fun bubbleFootnote(pos: PointOfSale?): String = "A faded bubble floats over ${floatOver(pos)} and lights up when a sale comes in. Drag it to either side."

    /** The debug build's field for a stand-in on the developer's machine (acceptance T6.5). */
    fun standIn(pos: PointOfSale): String = "Stand-in ${pos.displayName} address"

    /** The Wake-ups status card's pace row. */
    fun checking(pos: PointOfSale?): String = "Checking ${name(pos)}"

    // A sale's words, by the point of sale it was rung up at.

    /**
     * The point of sale [sale] was rung up at: the one that found it
     * (`FoundSale.pointOfSale`), so it keeps that name after the till's
     * changes; another till's sale is named by the till's own, since
     * Monokulo doesn't say which point of sale took it (decision 141).
     */
    fun of(sale: TillSale, till: PointOfSale?): PointOfSale? = sale.found?.pointOfSale ?: till

    /** Whether this till rang [sale] up at a point of sale it has since changed from (decision 141). */
    fun elsewhere(sale: TillSale, till: PointOfSale?): Boolean = sale.found != null && of(sale, till) != till

    /**
     * Where [sale] was rung up, by name: the till's [location] (its Square location or Clover
     * merchant) for a sale of the point of sale it watches now, or another till's; for one rung
     * up at the point of sale it changed from, that point of sale's name, since the old location
     * or merchant went with its sign-in ("Clover · this till", never the new "Front Counter").
     */
    fun placeOf(sale: TillSale, till: PointOfSale?, location: String?): String? =
        if (elsewhere(sale, till)) of(sale, till)?.displayName else location

    fun refundedIn(pos: PointOfSale?): String = "Refunded in ${name(pos)}"

    fun refundItIn(pos: PointOfSale?): String = "refund it in ${name(pos)}"

    fun refundButton(pos: PointOfSale?, amount: String): String = "Refund $amount in ${name(pos)}"

    /** Another till's sale: its point of sale is on that till. */
    fun refundOnOtherTill(pos: PointOfSale?, till: String): String = "Refund it in ${name(pos)} on $till."

    fun hasItAsMonero(pos: PointOfSale?): String = "${nameFirst(pos)} has this sale as Monero"

    /**
     * What the till built a receipt from: Square's receipt, Clover's order (Clover prints no
     * receipt number, acceptance D7); a receipt kept after signing out of either, just that.
     */
    fun receiptLabel(pos: PointOfSale?, receiptId: String): String = when (pos) {
        PointOfSale.Square -> "Square receipt $receiptId"
        PointOfSale.Clover -> "Clover order $receiptId"
        null -> "Receipt $receiptId"
    }

    /** The live sale's button back to the point-of-sale app on this phone. */
    fun backTo(pos: PointOfSale?): String = "Back to ${pos?.displayName ?: "the till"}"

    /** The shift's own footing, and the empty shift's hint. */
    fun waitingHint(pos: PointOfSale?): String = when (pos) {
        PointOfSale.Square -> "The code appears here, and the bubble over Square lights up, when you tap Monero in Square."
        PointOfSale.Clover -> "The code appears here, and the bubble lights up, when you choose Monero on the Clover device."
        null -> "The code appears here, and the bubble lights up, when another till rings up a Monero sale."
    }

    fun startHint(pos: PointOfSale?): String = "Start watching and Till checks ${name(pos)} for Monero sales until you stop it."

    /** The refund sheet's title (acceptance T4.1), for the point of sale that took the sale. */
    fun refundSheetTitle(pos: PointOfSale): String = "Refund in ${pos.displayName}"

    /** Where at [pos] the cashier refunds a sale. */
    fun refundWhere(pos: PointOfSale): String = when (pos) {
        PointOfSale.Square -> "Square's Transactions"
        PointOfSale.Clover -> "Clover's Orders app"
    }

    /**
     * What to refund, and where: the order by its id or title, when it was rung up, the
     * amount. [asks]: the till can ask [pos] whether it's done; else (the till has changed
     * its point of sale since) the cashier's word is all it has (decision 138).
     */
    fun refundSheetBody(pos: PointOfSale, asks: Boolean, order: String?, time: String?, amount: String): String =
        "This sale wasn't paid in Monero. Refund it in ${refundWhere(pos)}: " +
            listOfNotNull(order?.let { "order $it" }, time, amount).joinToString(", ") +
            ", paid with Monero. " + if (asks) "Till will notice when it's done." else "Tap $REFUNDED_IT once it's done."

    /** "I've refunded it" asked, and the point of sale doesn't show it. */
    fun refundNotSeen(pos: PointOfSale): String =
        "${pos.displayName} doesn't show a refund yet. Finish it in ${refundWhere(pos)}, then tap again."

    const val REFUNDED_IT = "I've refunded it"
    const val NOT_YET = "Not yet"
    const val LOOKING = "Looking…"
}
