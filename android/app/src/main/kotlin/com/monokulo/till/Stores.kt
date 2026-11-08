package com.monokulo.till

import com.monokulo.till.core.Connection
import com.monokulo.till.core.PointOfSale
import com.monokulo.till.push.BellAddress
import com.monokulo.till.push.BellRegistration
import com.monokulo.till.push.WakeUpChoice
import com.monokulo.till.push.WakeUpStore
import com.monokulo.till.sales.CloverMerchant
import com.monokulo.till.sales.CursorStore
import com.monokulo.till.sales.Pkce
import com.monokulo.till.sales.ShiftStore
import com.monokulo.till.sales.SquareLocation
import com.monokulo.till.sales.PosTokens
import com.monokulo.till.sales.TenderStore
import com.monokulo.till.sales.TillSale
import com.monokulo.till.sales.TokenStore
import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.builtins.SetSerializer
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.json.Json

/**
 * A point-of-sale sign-in under way: which point of sale, its PKCE, and the
 * application it signs in through when that is a Bell's.
 */
data class PendingSignIn(val pointOfSale: PointOfSale, val pkce: Pkce, val applicationId: String? = null)

/** Everything the app keeps, by name, in its [SecureStore]. */
class Stores(private val secure: SecureStore) {
    private val json = Json { ignoreUnknownKeys = true; explicitNulls = false }

    private fun <T> read(name: String, serializer: KSerializer<T>): T? =
        secure.get(name)?.let { runCatching { json.decodeFromString(serializer, it) }.getOrNull() }

    private fun <T> write(name: String, serializer: KSerializer<T>, value: T?) =
        secure.put(name, value?.let { json.encodeToString(serializer, it) })

    /** The store's key and API, nothing else about it (decision 50). */
    var connection: Connection?
        get() = secure.get("connection")?.let(Connection::restore)
        set(value) = secure.put("connection", value?.let(Connection::save))

    /** The Square location this till watches. */
    var location: SquareLocation?
        get() = read("square.location", SquareLocation.serializer())
        set(value) = write("square.location", SquareLocation.serializer(), value)

    /**
     * The point of sale the till watches, chosen in setup or Settings (decision 126). A till
     * from before there was a choice, signed in to Square, is a Square till.
     */
    var pointOfSale: PointOfSale?
        get() = secure.get("pos.kind")?.let(PointOfSale::fromWire) ?: PointOfSale.Square.takeIf { tokens.load() != null }
        set(value) = secure.put("pos.kind", value?.wire)

    /** Where the till watches, by name: its Square location, or its Clover merchant. */
    val placeName: String?
        get() = when (pointOfSale) {
            PointOfSale.Square -> location?.name
            PointOfSale.Clover -> cloverMerchant?.name
            null -> null
        }

    /** The cashier chose to run without a point of sale (sales from other tills only). */
    var withoutPos: Boolean
        get() = secure.get("pos.skipped") == "true" || secure.get("square.skipped") == "true"
        set(value) {
            secure.put("pos.skipped", value.toString().takeIf { value })
            // The key a till from before there was a choice kept it under.
            secure.put("square.skipped", null)
        }

    /** Whether the till watches for sales: on until the cashier stops it. */
    var shiftOn: Boolean
        get() = secure.get("shift.on") != "false"
        set(value) = secure.put("shift.on", value.toString())

    /** When this shift began, Unix seconds: the cashier turned watching on (the shift's "since 9:02 am"). */
    var shiftStartedAt: Long?
        get() = secure.get("shift.started")?.toLongOrNull()
        set(value) = secure.put("shift.started", value?.toString())

    /** The cashier went through setup to the end (OnbReady's "Start Till"). */
    var setupDone: Boolean
        get() = secure.get("setup.done") == "true"
        set(value) = secure.put("setup.done", value.toString().takeIf { value })

    /** Show the bubble over other apps when allowed; false when the cashier chose notifications only. */
    var bubbleOn: Boolean
        get() = secure.get("bubble.on") != "false"
        set(value) = secure.put("bubble.on", value.toString())

    /** Where the cashier left the bubble: which side, and how far down (px). */
    var bubbleOnLeft: Boolean
        get() = secure.get("bubble.left") == "true"
        set(value) = secure.put("bubble.left", value.toString())

    var bubbleY: Int?
        get() = secure.get("bubble.y")?.toIntOrNull()
        set(value) = secure.put("bubble.y", value?.toString())

    /**
     * A point-of-sale sign-in under way, kept in case Android ends the app
     * meanwhile, and the application it goes through when that is a Bell's
     * rather than the one the till signs in through now (decision 96).
     */
    var pendingSignIn: PendingSignIn?
        get() = read("square.pending", PkceState.serializer())?.let { state ->
            // One kept before there was a choice is Square's.
            val pos = state.pointOfSale?.let(PointOfSale::fromWire) ?: PointOfSale.Square.takeIf { state.pointOfSale == null }
            pos?.let { PendingSignIn(it, Pkce(state.verifier, state.state), state.applicationId) }
        }
        set(value) = write(
            "square.pending", PkceState.serializer(),
            value?.let { PkceState(it.pkce.verifier, it.pkce.state, it.applicationId, it.pointOfSale.wire) },
        )

    /**
     * The Square application the till signs in through when it isn't the
     * build's own: a Bell's (`docs/design.md` §3, decision 95). Set once a
     * sign-in through it has finished, never before.
     */
    var squareApplicationId: String?
        get() = secure.get("square.application")
        set(value) = secure.put("square.application", value?.takeIf { it != BuildConfig.SQUARE_APPLICATION_ID })

    /** Settings › Wake-ups: the choice, and the registration the Bell holds while there is one. */
    val wakeUps = object : WakeUpStore {
        override var choice: WakeUpChoice
            get() = read("wakeups.choice", WakeUpChoice.serializer()) ?: WakeUpChoice.TimerOnly
            set(value) = write("wakeups.choice", WakeUpChoice.serializer(), value.takeIf { it != WakeUpChoice.TimerOnly })

        override var ownBell: BellAddress?
            get() = read("wakeups.own", BellAddress.serializer())
            set(value) = write("wakeups.own", BellAddress.serializer(), value)

        override var registration: BellRegistration?
            get() = read("wakeups.registration", BellRegistration.serializer())
            set(value) = write("wakeups.registration", BellRegistration.serializer(), value)
    }

    /** Debug builds: a stand-in Square on the developer's machine. */
    var squareBaseUrl: String?
        get() = secure.get("debug.square.base")
        set(value) = secure.put("debug.square.base", value?.takeIf { it.isNotBlank() })

    /** Debug builds: a stand-in Clover (`tools/fake-clover`), both its web and API hosts. */
    var cloverBaseUrl: String?
        get() = secure.get("debug.clover.base")
        set(value) = secure.put("debug.clover.base", value?.takeIf { it.isNotBlank() })

    /** The Clover merchant this till is at: the redirect's `merchant_id`, read from Clover at sign-in (decision 127). */
    var cloverMerchant: CloverMerchant?
        get() = read("clover.merchant", CloverMerchant.serializer())
        set(value) = write("clover.merchant", CloverMerchant.serializer(), value)

    /** The Clover application the till signs in through when it isn't the build's own: a Bell's (decision 132). */
    var cloverApplicationId: String?
        get() = secure.get("clover.application")
        set(value) = secure.put("clover.application", value?.takeIf { it != BuildConfig.CLOVER_APPLICATION_ID })

    @Serializable
    private data class PkceState(val verifier: String, val state: String, val applicationId: String? = null, val pointOfSale: String? = null)

    val tokens = object : TokenStore {
        override fun load() = read("square.tokens", PosTokens.serializer())
        override fun save(tokens: PosTokens?) = write("square.tokens", PosTokens.serializer(), tokens)
    }

    /** Clover's sign-in, beside Square's: one store per point of sale (acceptance T2.3). */
    val cloverTokens = object : TokenStore {
        override fun load() = read("clover.tokens", PosTokens.serializer())
        override fun save(tokens: PosTokens?) = write("clover.tokens", PosTokens.serializer(), tokens)
    }

    /** Where Square's search for sales has got to. */
    val cursor = cursor("square")

    /** Where Clover's has. */
    val cloverCursor = cursor("clover")

    private fun cursor(pos: String) = object : CursorStore {
        override fun since() = secure.get("$pos.since")
        override fun handled(): Set<String> = read("$pos.handled", SetSerializer(String.serializer())) ?: emptySet()
        override fun save(since: String, handled: Set<String>) {
            secure.put("$pos.since", since)
            write("$pos.handled", SetSerializer(String.serializer()), handled)
        }
    }

    /** Forgets where [pos]'s search for sales has got to: a new sign-in starts from now. */
    fun forgetCursor(pos: PointOfSale) {
        secure.put("${pos.wire}.since", null)
        secure.put("${pos.wire}.handled", null)
    }

    /** The Monero payment type's id at each Clover merchant (decision 129). */
    val tenders = object : TenderStore {
        override fun tenderId(merchantId: String) = secure.get("clover.tender.$merchantId")
        override fun save(merchantId: String, tenderId: String?) = secure.put("clover.tender.$merchantId", tenderId)
    }

    /** This shift's sales, receipts and all, encrypted like everything else here (decision 64). */
    val shift = object : ShiftStore {
        override fun load() = read("shift.sales", ListSerializer(TillSale.serializer())).orEmpty()
        override fun save(sales: List<TillSale>) {
            write("shift.sales", ListSerializer(TillSale.serializer()), sales.ifEmpty { null })
        }
    }

    /** [sale]'s point of sale, for its words ([PosText.of]). */
    fun pointOfSale(sale: TillSale): PointOfSale? = PosText.of(sale, pointOfSale)

    /** Where [sale] was rung up, by name ([PosText.placeOf]): [placeName], or the point of sale the till changed from. */
    fun placeName(sale: TillSale): String? = PosText.placeOf(sale, pointOfSale, placeName)

    /** Forgets the store and everything about this till's point of sale. */
    fun forgetAll() {
        connection = null
        setupDone = false
        location = null
        pointOfSale = null
        withoutPos = false
        pendingSignIn = null
        tokens.save(null)
        cloverTokens.save(null)
        cloverMerchant?.let { tenders.save(it.id, null) }
        cloverMerchant = null
        PointOfSale.entries.forEach(::forgetCursor)
        shift.save(emptyList())
        shiftStartedAt = null
        squareApplicationId = null
        cloverApplicationId = null
        wakeUps.choice = WakeUpChoice.TimerOnly
        wakeUps.ownBell = null
        wakeUps.registration = null
    }
}
