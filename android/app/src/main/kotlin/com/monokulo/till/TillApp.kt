package com.monokulo.till

import android.app.Application
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Build
import android.provider.Settings
import android.text.format.DateFormat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import com.monokulo.till.engine.EngineStore
import com.monokulo.till.engine.KeystorePlatform
import com.monokulo.till.engine.Listener
import com.monokulo.till.engine.UnifiedPushPlatform
import com.monokulo.till.engine.engineConfig
import com.monokulo.till.engine.utcOffsetSeconds
import uniffi.till_app.Action
import uniffi.till_app.Engine
import com.monokulo.till.core.AppIdentity
import com.monokulo.till.core.PointOfSale
import com.monokulo.till.core.MonokuloApi
import com.monokulo.till.core.Tor
import com.monokulo.till.ffi.Tls
import com.monokulo.till.push.BellAddress
import com.monokulo.till.push.BellApi
import com.monokulo.till.push.BellWakeUps
import com.monokulo.till.push.PosSide
import com.monokulo.till.push.PosSignIn
import com.monokulo.till.push.UnifiedPushSubscriptions
import com.monokulo.till.push.WakeUps
import com.monokulo.till.sales.CloverApi
import com.monokulo.till.sales.CloverAuth
import com.monokulo.till.sales.CloverConfig
import com.monokulo.till.sales.CloverEnvironment
import com.monokulo.till.sales.CloverMerchant
import com.monokulo.till.sales.CloverSource
import com.monokulo.till.sales.PosException
import com.monokulo.till.sales.SquareApi
import com.monokulo.till.sales.SquareAuth
import com.monokulo.till.sales.SquareConfig
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import okhttp3.OkHttpClient
import java.io.IOException
import java.util.concurrent.TimeUnit

/**
 * The app's process: the Rust core's [engine] (docs/ios-plan.md §2), which
 * holds everything the app does, and the [engineStore] of what it last
 * published, which the activity, the service and the overlay draw.
 */
class TillApp : Application() {
    /** The Kotlin graph the service, the overlay and the ding path still run on, until they move to the engine. */
    val graph by lazy { Graph(this) }

    lateinit var engine: Engine
        private set

    /** What the engine last published, slice by slice. */
    val engineStore: EngineStore get() = EngineStore

    override fun onCreate() {
        super.onCreate()
        // Before the Rust core's first request (docs/decisions/core.md 36).
        Tls.init(this)
        engine = Engine(engineConfig(this), KeystorePlatform(this), UnifiedPushPlatform(this, EngineStore::send), Listener)
        EngineStore.attach(engine)
        // The engine's Android side: the till's service, its notices and the bubble.
        TillService.follow(this)
        permissionsMayHaveChanged()
        // The time zone or the clock's style changing: every time the screens write is written again.
        val clock = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) = clockMayHaveChanged()
        }
        val changes = IntentFilter().apply {
            addAction(Intent.ACTION_TIMEZONE_CHANGED)
            addAction(Intent.ACTION_TIME_CHANGED)
        }
        ContextCompat.registerReceiver(this, clock, changes, ContextCompat.RECEIVER_NOT_EXPORTED)
    }

    /** Tells the engine what Android allows now: alerts, and drawing over other apps. */
    fun permissionsMayHaveChanged() {
        EngineStore.send(Action.Permissions(NotificationManagerCompat.from(this).areNotificationsEnabled(), Settings.canDrawOverlays(this)))
    }

    /** Tells the engine the device's offset from UTC and whether it keeps a 24-hour clock. */
    fun clockMayHaveChanged() {
        EngineStore.send(Action.ClockChanged(utcOffsetSeconds(), DateFormat.is24HourFormat(this)))
    }
}

/** The app's long-lived parts. */
class Graph(app: Application) {
    val stores = Stores(SecureStore(app))

    val identity = AppIdentity(
        slug = "till",
        name = "Monokulo Till",
        version = BuildConfig.VERSION_NAME,
        platform = "android",
        deviceName = listOf(Build.MANUFACTURER, Build.MODEL).distinct().joinToString(" ").take(40),
    )

    val http: OkHttpClient = OkHttpClient.Builder()
        // Onion addresses through Orbot's SOCKS proxy (decision 55); the rest straight out.
        .proxySelector(Tor.Proxies())
        // Tor takes longer to build a circuit.
        .connectTimeout(30, TimeUnit.SECONDS)
        // The event stream sends a comment every 25 s; longer silence is a dead connection.
        .readTimeout(60, TimeUnit.SECONDS)
        .build()

    val monokulo = MonokuloApi(http, identity)

    fun squareConfig() = SquareConfig(
        // A Bell's application once the till has signed in through it (decision 95).
        applicationId = stores.squareApplicationId ?: BuildConfig.SQUARE_APPLICATION_ID,
        sandbox = BuildConfig.SQUARE_SANDBOX,
        redirectUri = BuildConfig.SQUARE_REDIRECT_URI,
        baseUrl = if (BuildConfig.DEBUG) stores.squareBaseUrl ?: BuildConfig.SQUARE_BASE_URL.ifEmpty { null } else null,
    )

    fun squareAuth() = SquareAuth(http, squareConfig(), stores.tokens)

    fun squareApi() = SquareApi(http, squareConfig(), squareAuth())

    /** Clover's region, `-Ptill.clover.environment`: a build whose property isn't one fails here, as BELL_URL's does. */
    private val cloverEnvironment: CloverEnvironment = requireNotNull(CloverEnvironment.fromWire(BuildConfig.CLOVER_ENVIRONMENT)) {
        "-Ptill.clover.environment isn't sandbox, north_america, europe or latin_america: ${BuildConfig.CLOVER_ENVIRONMENT}"
    }

    fun cloverConfig() = CloverConfig(
        // A Bell's application once the till has signed in through it (decision 132).
        applicationId = stores.cloverApplicationId ?: BuildConfig.CLOVER_APPLICATION_ID,
        environment = cloverEnvironment,
        redirectUri = BuildConfig.CLOVER_REDIRECT_URI,
        userAgent = "Till/${BuildConfig.VERSION_NAME}",
        baseUrl = if (BuildConfig.DEBUG) stores.cloverBaseUrl ?: BuildConfig.CLOVER_BASE_URL.ifEmpty { null } else null,
    )

    fun cloverAuth(config: CloverConfig = cloverConfig()) = CloverAuth(http, config, stores.cloverTokens)

    fun cloverApi() = CloverApi(http, cloverConfig(), cloverAuth())

    /** Clover's sales at [merchant], for the till and for finding the Monero payment type. */
    fun cloverSource(merchant: CloverMerchant) = CloverSource(cloverApi(), cloverConfig(), merchant, stores.cloverCursor, stores.tenders)

    /** The project's Bell, `-Ptill.bell.url` (decision 98): what "Monokulo Bell" registers with and Settings shows. */
    val monokuloBell: BellAddress = requireNotNull(BellAddress.parse(BuildConfig.BELL_URL, allowHttp = BuildConfig.DEBUG)) {
        "-Ptill.bell.url isn't a Bell address this build takes: ${BuildConfig.BELL_URL}"
    }

    /** The till's push subscription for its Bell (decision 90); a ding goes from here to the running shift. */
    val push = UnifiedPushSubscriptions(app) { stores.pointOfSale }

    /**
     * Settings › Wake-ups (`docs/design.md` §3): made on first use, which
     * registers with the chosen Bell again (decision 95's app start). A
     * process the connector started for a ding alone never makes it, so a
     * ding outside a shift costs the Bell nothing (decision 93).
     */
    val wakeUps: WakeUps by lazy {
        BellWakeUps(
            push = push,
            distributors = push,
            bells = BellApi(http),
            store = stores.wakeUps,
            monokulo = monokuloBell,
            pos = posSide,
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Default),
            now = { System.currentTimeMillis() / 1000 },
        )
    }

    /** The till's point-of-sale sign-in, as a Bell registration needs it (decision 132). */
    private val posSide = object : PosSide {
        override val signedIn: PosSignIn?
            get() = if (stores.withoutPos) null else when (stores.pointOfSale) {
                PointOfSale.Square -> {
                    val location = stores.location
                    if (location == null || stores.tokens.load() == null) {
                        null
                    } else {
                        // The environment is the application's, the one the till signed in through (decision 305).
                        val config = squareConfig()
                        PosSignIn(PointOfSale.Square, config.applicationId, config.environment, location.id)
                    }
                }
                // A Clover merchant is its own location (acceptance D1).
                PointOfSale.Clover -> {
                    val merchant = stores.cloverMerchant
                    if (merchant == null || stores.cloverTokens.load() == null) {
                        null
                    } else {
                        PosSignIn(PointOfSale.Clover, cloverConfig().applicationId, cloverEnvironment.wire, merchant.id)
                    }
                }
                null -> null
            }

        override suspend fun accessToken(): String? = try {
            when (stores.pointOfSale) {
                PointOfSale.Square -> squareAuth().accessToken()
                PointOfSale.Clover -> cloverAuth().accessToken()
                null -> null
            }
        } catch (e: PosException) {
            if (e.signInAgain) null else throw IOException(e.message, e)
        }
    }

    val runner = TillRunner(this)

    /** The store as last read, this session only. */
    val store = LiveStore(monokulo, onDisconnected = runner::disconnected, onOnionApi = ::onionApi)

    /**
     * Keeps where the store's onion entry is with the connection: a till
     * that connected before Monokulo gave it learns it here. One the store
     * no longer has takes the till off Tor.
     */
    private fun onionApi(url: String?) {
        val connection = stores.connection ?: return
        if (connection.onionApi == url) return
        stores.connection = connection.copy(onionApi = url, tor = connection.tor && url != null)
    }

    /** Till's own screen is showing: the bubble steps aside. */
    val appVisible = MutableStateFlow(false)

    /** The cashier wants the bubble (it shows only once Android allows it, too). */
    val bubbleWanted = MutableStateFlow(stores.bubbleOn)

    /** Bumped when the app comes back to the front, so the service sees permissions granted meanwhile. */
    val permissions = MutableStateFlow(0)

    fun setBubble(on: Boolean) {
        stores.bubbleOn = on
        bubbleWanted.value = on
    }
}
