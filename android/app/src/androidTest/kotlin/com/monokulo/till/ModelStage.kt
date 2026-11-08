package com.monokulo.till

import androidx.compose.animation.core.SeekableTransitionState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import com.monokulo.till.sales.TillSale
import com.monokulo.till.sales.live

/**
 * The view model as the stage sees it (`AppViewModel.back`, `backTarget`,
 * `back(from)`): a screen, where Back goes from it, and a Back that acts
 * only from the screen still up. [commits] records each Back the stage
 * delivered, with the screen it said it left.
 */
class TestModel(start: Screen) {
    val screen = mutableStateOf(start)
    val commits = mutableListOf<Screen>()
    var cameUpByItself = false

    /** The screen the stage rests on, as the stage reports it (`AppViewModel.landed`); null while a change runs. */
    var restingOn: Screen? = start
        private set

    /** The sales the cashier left, and those live at the last look: what `AppViewModel.follow` reads. */
    val left = mutableSetOf<String>()
    private var liveBefore = emptySet<String>()

    /** Where Back goes from here, as `AppViewModel.backTarget` says. */
    var backTarget: () -> Screen? = {
        when (val s = screen.value) {
            is Screen.Sale -> Screen.Till
            is Screen.Code -> Screen.Sale(s.orderId)
            is Screen.Order -> Screen.Sale(s.orderId)
            else -> null
        }
    }

    fun go(to: Screen, byItself: Boolean = false) {
        cameUpByItself = byItself
        if (to != screen.value) restingOn = null
        screen.value = to
    }

    /** The stage landed on [at] (`FollowScreen`'s report), as `AppViewModel.landed` takes it. */
    fun landed(at: Screen) {
        if (at == screen.value) restingOn = at
    }

    /** The sales changed, as `AppViewModel.follow` takes them: a sale new since the last look rises only off the resting shift. */
    fun salesChanged(sales: List<TillSale>) {
        val next = TillNav.follow(screen.value, sales, left, liveBefore, restingOn == Screen.Till)
        if (next != screen.value) go(next, byItself = true)
        liveBefore = sales.filter { it.live }.map { it.orderId }.toSet()
    }

    /** Back from [from] only if it's still up (decision 77). */
    fun back(from: Screen) {
        commits += from
        if (screen.value == from) {
            (from as? Screen.Sale)?.let { left += it.orderId }
            backTarget()?.let { go(it) }
        }
    }

    /** The stage, once [ModelStage] has made it. */
    lateinit var stage: SeekableTransitionState<Screen>
}

/** The activity's stage over [model]: the one driver, Android's back, and the screens. */
@Composable
fun ModelStage(model: TestModel, content: @Composable (Screen) -> Unit) {
    val state = remember { SeekableTransitionState(model.screen.value).also { model.stage = it } }
    val scrub = remember { BackScrub() }
    FollowScreen(state, scrub, model.screen, onLanded = model::landed)
    // As the activity's: on the shift, Back is the stage's only while it is still arriving there (decision 260 c).
    val arriving by remember { derivedStateOf { state.currentState != state.targetState } }
    StageBack(
        enabled = model.screen.value != Screen.Till || arriving, state = state, scrub = scrub,
        current = { model.screen.value }, target = { model.backTarget() }, onCommit = model::back,
    )
    TillStage(state, cameUpByItself = { model.cameUpByItself }, scrub, content)
}
