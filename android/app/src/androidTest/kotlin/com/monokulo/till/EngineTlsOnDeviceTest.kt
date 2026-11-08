package com.monokulo.till

import androidx.test.platform.app.InstrumentationRegistry
import com.monokulo.till.engine.engineConfig
import com.monokulo.till.ffi.Tls
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import org.junit.Test
import uniffi.till_app.Action
import uniffi.till_app.Effect
import uniffi.till_app.Engine
import uniffi.till_app.EngineListener
import uniffi.till_app.Platform
import uniffi.till_app.PushPlatform
import uniffi.till_app.Distributors
import uniffi.till_app.Update
import kotlin.test.assertFalse
import kotlin.test.assertNotNull

/**
 * The Rust core makes an HTTPS request on the device (docs/decisions/core.md
 * 36): with [Tls.init] done, rustls-platform-verifier checks the server's
 * certificate through Android's trust manager. Connecting to a real https
 * host that isn't a Monokulo gets that host's answer back as the problem;
 * without the verifier's context the request panics and no answer comes.
 * Needs the network.
 */
class EngineTlsOnDeviceTest {
    private class Secrets : Platform {
        val kept = ConcurrentHashMap<String, ByteArray>()
        override fun secretGet(key: String): ByteArray? = kept[key]
        override fun secretPut(key: String, value: ByteArray) {
            kept[key] = value
        }
        override fun secretRemove(key: String) {
            kept.remove(key)
        }
        override fun formatMoney(minor: Long, currency: String) = "$minor $currency"
    }

    private object NoPush : PushPlatform {
        override fun subscribe(vapidPublicKey: String) = Unit
        override fun unsubscribe() = Unit
        override fun distributors(): Distributors = Distributors.None
        override fun saveDistributor(name: String) = Unit
    }

    private class Heard : EngineListener {
        val problems = LinkedBlockingQueue<String>()
        override fun onUpdate(update: Update) {
            if (update is Update.Work) update.problem?.let(problems::add)
        }
        override fun onEffect(effect: Effect) = Unit
        override fun onLog(message: String) = Unit
    }

    @Test
    fun anHttpsRequestFromTheCoreGetsItsAnswer() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        Tls.init(context)
        val heard = Heard()
        val engine = Engine(engineConfig(context), Secrets(), NoPush, heard)
        try {
            engine.send(Action.ConnectLink("https://example.com/apps/connect#token=ct_" + "0123456789abcdef".repeat(4)))
            engine.send(Action.Connect)
            val problem = heard.problems.poll(60, TimeUnit.SECONDS)
            assertNotNull(problem, "no answer within a minute: the request never finished")
            assertFalse(problem.contains("error sending request"), "the request didn't reach the host: $problem")
        } finally {
            engine.close()
        }
    }
}
