package com.monokulo.till

import androidx.compose.animation.core.SeekableTransitionState
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import com.monokulo.till.OneClockTest.Companion.noActions
import com.monokulo.till.ui.CodeScreen
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.SaleScreen
import org.junit.After
import org.junit.Rule
import org.junit.Test
import kotlin.test.assertTrue

/**
 * The debug build's stage trace (decision 237): off, a change logs
 * nothing; switched on with the log tag's property, every frame of a
 * running change logs one line with the move, the progress and where each
 * moving surface was drawn.
 */
class StageTraceTest {
    @get:Rule
    val compose = createComposeRule()

    private val sale = testSale("a1", expiresAt = 1_600)
    private val now = mutableLongStateOf(1_000)

    private fun shell(command: String): String {
        val fd = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(command)
        return android.os.ParcelFileDescriptor.AutoCloseInputStream(fd).bufferedReader().readText()
    }

    @After
    fun off() {
        shell("setprop log.tag.$StageTraceTag ''")
    }

    /** The trace's lines from a Sale to Code change run on the test clock, with the tag at [level]. */
    private fun enlargeLogged(level: String): List<String> {
        shell("setprop log.tag.$StageTraceTag $level")
        shell("logcat -c")
        var screen by mutableStateOf<Screen>(Screen.Sale("a1"))
        val state = SeekableTransitionState<Screen>(Screen.Sale("a1"))
        compose.setContent {
            Till {
                LaunchedEffect(screen) { state.animateTo(screen, Motion.Full.stage) }
                TillStage(state, cameUpByItself = { false }) { s ->
                    if (s is Screen.Code) CodeScreen(sale, onBack = {}) else SaleScreen(sale, "Front", refunding = false, refundProblem = null, refundSheet = null, actions = noActions, now = now)
                }
            }
        }
        compose.waitForIdle()
        compose.mainClock.autoAdvance = false
        screen = Screen.Code("a1")
        repeat(40) { compose.mainClock.advanceTimeByFrame() }
        return shell("logcat -d -s $StageTraceTag:D").lines().filter { it.contains("$StageTraceTag:") || it.contains(" $StageTraceTag ") }
    }

    @Test
    fun offTheStageLogsNothing() {
        val lines = enlargeLogged("INFO")
        assertTrue(lines.isEmpty(), "no trace with the tag below DEBUG: $lines")
    }

    @Test
    fun onEachFrameOfAChangeLogsWhereItsSurfacesAre() {
        val lines = enlargeLogged("DEBUG")
        assertTrue(lines.size >= 10, "a line per frame of the enlarge: ${lines.size}")
        assertTrue(lines.all { " p=" in it }, "each gives the progress: ${lines.take(3)}")
        assertTrue(lines.count { "CodeGrow Sale(orderId=a1) -> Code(orderId=a1)" in it } >= 10, "and the move between the two screens: ${lines.filter { "CodeGrow" !in it }}")
        assertTrue(lines.count { "code/a1 @" in it && "code-card/a1 @" in it } >= 5, "and where the code and its white were drawn: ${lines.take(3)}")
    }
}
