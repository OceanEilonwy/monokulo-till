package com.monokulo.till.engine

import android.util.Log
import java.util.concurrent.ConcurrentLinkedQueue
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.till_app.Action
import uniffi.till_app.Effect
import uniffi.till_app.Engine
import uniffi.till_app.EngineListener
import uniffi.till_app.Screen
import uniffi.till_app.Update
import uniffi.till_screens.OnboardingView
import uniffi.till_screens.SettingsView
import uniffi.till_screens.WakeUpsView

/** Busy with a network call (buttons wait), and what went wrong last. */
data class Work(val busy: Boolean = false, val problem: String? = null)

/**
 * What the engine last published for each slice of the screen, as Tern's
 * `Store` holds it: one [StateFlow] per [Update], fed by [Listener]. It holds
 * no state of its own; Compose draws these and sends [Action]s back.
 */
object EngineStore {
    private val mutableCore = MutableStateFlow<String?>(null)
    private val mutableScreen = MutableStateFlow<Screen?>(null)
    private val mutableWork = MutableStateFlow(Work())
    private val mutableService = MutableStateFlow(false)
    private val mutableOnboarding = MutableStateFlow<OnboardingView?>(null)
    private val mutableSettings = MutableStateFlow<SettingsView?>(null)
    private val mutableWakeUps = MutableStateFlow<WakeUpsView?>(null)
    private val mutableEffects = MutableSharedFlow<Effect>(extraBufferCapacity = 16, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    /** The core's version, once it runs. */
    val core: StateFlow<String?> = mutableCore.asStateFlow()
    val screen: StateFlow<Screen?> = mutableScreen.asStateFlow()
    val work: StateFlow<Work> = mutableWork.asStateFlow()
    /** The till runs: Android keeps its foreground service meanwhile. */
    val service: StateFlow<Boolean> = mutableService.asStateFlow()
    val onboarding: StateFlow<OnboardingView?> = mutableOnboarding.asStateFlow()
    val settings: StateFlow<SettingsView?> = mutableSettings.asStateFlow()
    val wakeUps: StateFlow<WakeUpsView?> = mutableWakeUps.asStateFlow()
    /** What only the platform can do; dropped when nothing collects them in time. */
    val effects: SharedFlow<Effect> = mutableEffects.asSharedFlow()

    @Volatile
    private var engine: Engine? = null
    private val early = ConcurrentLinkedQueue<Action>()

    /** Sends [action] to the engine, or keeps it until the engine has started. Never blocks. */
    fun send(action: Action) {
        val ready = engine
        if (ready != null) ready.send(action) else early.add(action)
    }

    /** The engine has started: what was sent before goes to it now, in order. */
    fun attach(ready: Engine) {
        engine = ready
        while (true) ready.send(early.poll() ?: break)
    }

    internal fun update(update: Update) {
        when (update) {
            is Update.Core -> mutableCore.value = update.version
            is Update.Screen -> mutableScreen.value = update.screen
            is Update.Work -> mutableWork.value = Work(update.busy, update.problem)
            is Update.Service -> mutableService.value = update.running
            is Update.Onboarding -> mutableOnboarding.value = update.view
            is Update.Settings -> mutableSettings.value = update.view
            is Update.WakeUps -> mutableWakeUps.value = update.view
        }
    }

    internal fun effect(effect: Effect) {
        mutableEffects.tryEmit(effect)
    }
}

/** Where the engine's task publishes: straight into [EngineStore]. */
object Listener : EngineListener {
    override fun onUpdate(update: Update) = EngineStore.update(update)

    override fun onEffect(effect: Effect) = EngineStore.effect(effect)

    override fun onLog(message: String) {
        Log.w("Till", message)
    }
}
