package com.monokulo.till

import android.app.Application
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
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
import com.monokulo.till.ffi.Tls

/**
 * The app's process: the Rust core's [engine] (docs/ios-plan.md §2), which
 * holds everything the app does, and the [engineStore] of what it last
 * published, which the activity, the service and the overlay draw.
 */
class TillApp : Application() {
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
