package com.monokulo.till.engine

import android.content.Context
import android.os.Build
import android.text.format.DateFormat
import com.monokulo.till.BuildConfig
import java.util.TimeZone
import uniffi.till_app.CloverSetup
import uniffi.till_app.EngineConfig
import uniffi.till_app.SquareSetup
import uniffi.till_screens.Device

/** What the engine is told about this app and device as it starts: the build's choices, as `Graph` read them. */
fun engineConfig(context: Context): EngineConfig = EngineConfig(
    dataDir = context.noBackupFilesDir.resolve("core").apply { mkdirs() }.path,
    device = Device.ANDROID,
    deviceName = listOf(Build.MANUFACTURER, Build.MODEL).distinct().joinToString(" ").take(40),
    appVersion = BuildConfig.VERSION_NAME,
    platform = "android",
    debug = BuildConfig.DEBUG,
    utcOffset = utcOffsetSeconds(),
    hours24 = DateFormat.is24HourFormat(context),
    square = SquareSetup(
        applicationId = BuildConfig.SQUARE_APPLICATION_ID,
        sandbox = BuildConfig.SQUARE_SANDBOX,
        redirectUri = BuildConfig.SQUARE_REDIRECT_URI,
        baseUrl = BuildConfig.SQUARE_BASE_URL.ifEmpty { null },
    ),
    clover = CloverSetup(
        applicationId = BuildConfig.CLOVER_APPLICATION_ID,
        environment = BuildConfig.CLOVER_ENVIRONMENT,
        redirectUri = BuildConfig.CLOVER_REDIRECT_URI,
        baseUrl = BuildConfig.CLOVER_BASE_URL.ifEmpty { null },
    ),
    bellUrl = BuildConfig.BELL_URL,
)

/** This device's offset from UTC now, in seconds. */
fun utcOffsetSeconds(): Int = TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 1000
