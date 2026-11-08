package com.monokulo.till

import androidx.compose.foundation.layout.Row
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.Density
import com.monokulo.till.ui.BadgeTag
import com.monokulo.till.ui.ChipTag
import com.monokulo.till.ui.FilterChip
import org.junit.Rule
import org.junit.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/** The owner's report: the Needs action count follows its chip's curve, inset from the end as from the top and bottom. */
class FilterChipTest {
    @get:Rule
    val compose = createComposeRule()

    private fun bounds(tag: String): Rect = compose.onNodeWithTag(tag, useUnmergedTree = true).fetchSemanticsNode().boundsInRoot

    private data class Look(val count: Int, val fontScale: Float = 1f, val selected: Boolean = false, val dark: Boolean = false)

    private var look by mutableStateOf(Look(1))
    private var shown = false

    /** Shows the chip as [next] looks: set once, then changed in place, as a rule takes one content. */
    private fun show(next: Look) {
        look = next
        if (!shown) {
            shown = true
            compose.setContent {
                Till(dark = look.dark) {
                    val density = LocalDensity.current
                    CompositionLocalProvider(LocalDensity provides Density(density.density, look.fontScale)) {
                        Row { FilterChip("Needs action", look.count, look.selected) {} }
                    }
                }
            }
        }
        compose.waitForIdle()
    }

    private fun assertConcentric(what: String) {
        val chip = bounds(ChipTag)
        val badge = bounds(BadgeTag)
        val top = badge.top - chip.top
        val bottom = chip.bottom - badge.bottom
        val end = chip.right - badge.right
        assertTrue(top > 0f, "$what: the badge sits inside the chip ($top)")
        assertEquals(top, bottom, 1f, "$what: centred top to bottom")
        assertEquals(top, end, 1f, "$what: as far from the chip's end as from its top")
        assertTrue(badge.width >= badge.height - 1f, "$what: never narrower than a circle (${badge.width} x ${badge.height})")
    }

    @Test
    fun theBadgeIsConcentricWithTheChipsEnd() {
        for (count in listOf(1, 9, 12, 128)) {
            show(Look(count))
            assertConcentric("count $count")
        }
    }

    @Test
    fun itStaysConcentricSelectedAndInTheDark() {
        show(Look(12, selected = true, dark = true))
        assertConcentric("selected, dark")
    }

    @Test
    fun itStaysConcentricAtTheLargestFontScale() {
        for (count in listOf(3, 128)) {
            show(Look(count, fontScale = 2f))
            assertConcentric("count $count at 2x")
        }
    }
}
