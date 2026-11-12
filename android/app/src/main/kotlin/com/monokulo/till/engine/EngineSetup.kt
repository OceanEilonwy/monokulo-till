package com.monokulo.till.engine

import android.content.Context
import android.os.Build
import android.text.format.DateFormat
import com.monokulo.till.BuildConfig
import java.util.TimeZone
import uniffi.till_app.CloverEnvironment
import uniffi.till_app.CloverSetup
import uniffi.till_app.EngineConfig
import uniffi.till_app.SquareEnvironment
import uniffi.till_app.SquareSetup
import uniffi.till_screens.Device

/** What the engine is told about this app and device as it starts: the build's choices, as `Graph` read them. */
fun engineConfig(context: Context): EngineConfig = EngineConfig(
    dataDir = context.noBackupFilesDir.resolve("core").apply { mkdirs() }.path,
    device = Device.ANDROID,
    deviceName = listOf(Build.MANUFACTURER, Build.MODEL).distinct().joinToString(" ").take(40),
    appVersion = BuildConfig.VERSION_NAME,
    debug = BuildConfig.DEBUG,
    utcOffset = utcOffsetSeconds(),
    hours24 = DateFormat.is24HourFormat(context),
    square = SquareSetup(
        applicationId = BuildConfig.SQUARE_APPLICATION_ID,
        environment = if (BuildConfig.SQUARE_SANDBOX) SquareEnvironment.SANDBOX else SquareEnvironment.PRODUCTION,
        redirectUri = BuildConfig.SQUARE_REDIRECT_URI,
        baseUrl = BuildConfig.SQUARE_BASE_URL.ifEmpty { null },
        // A debug build's sign-in comes back on its own scheme; a release build's is the redirect itself.
        returnUri = BuildConfig.SQUARE_RETURN_URI.takeIf { it != BuildConfig.SQUARE_REDIRECT_URI },
    ),
    clover = CloverSetup(
        applicationId = BuildConfig.CLOVER_APPLICATION_ID,
        environment = cloverEnvironment(BuildConfig.CLOVER_ENVIRONMENT),
        redirectUri = BuildConfig.CLOVER_REDIRECT_URI,
        baseUrl = BuildConfig.CLOVER_BASE_URL.ifEmpty { null },
        returnUri = BuildConfig.CLOVER_RETURN_URI.takeIf { it != BuildConfig.CLOVER_REDIRECT_URI },
    ),
    bellUrl = BuildConfig.BELL_URL,
    // A tablet draws the iPad's pages (core.md decision 239).
    largeScreen = context.resources.configuration.smallestScreenWidthDp >= 600,
)

/**
 * The Clover environment [name] is, as builds spell them (`till.clover.environment`: `north_america`, `europe`,
 * `latin_america` or `sandbox`). A build naming none of them can't sign in to Clover at all, so it stops as it starts.
 */
fun cloverEnvironment(name: String): CloverEnvironment = CloverEnvironment.valueOf(name.uppercase())

/** This device's offset from UTC now, in seconds. */
fun utcOffsetSeconds(): Int = TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 1000
