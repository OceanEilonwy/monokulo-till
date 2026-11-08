package com.monokulo.till

import androidx.compose.foundation.layout.Column
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertHeightIsAtLeast
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.unit.dp
import com.monokulo.till.core.PointOfSale
import com.monokulo.till.ui.LinkedMarks
import com.monokulo.till.ui.PosChoices
import uniffi.till_screens.Choice
import uniffi.till_screens.PosChoice
import uniffi.till_screens.PosMark
import org.junit.Rule
import org.junit.Test

/** The Logos board: the point of sale's marks lead its rows and link on the Connect card, and TalkBack hears only the names. */
class PosMarksTest {
    @get:Rule
    val compose = createComposeRule()

    private val described = SemanticsMatcher.keyIsDefined(SemanticsProperties.ContentDescription)

    @Test
    fun theMarksAreDecorationBesideRowsAtLeast48DpTall() {
        compose.setContent {
            Till(dark = true) {
                Column {
                    PosChoices(
                        listOf(
                            PosChoice(PosMark.SQUARE, true, Choice("Square", "Square Point of Sale", selected = true, enabled = true)),
                            PosChoice(PosMark.CLOVER, false, Choice("Clover", "Clover Register", selected = false, enabled = true)),
                        ),
                    ) {}
                    LinkedMarks(PointOfSale.Clover)
                }
            }
        }
        compose.waitForIdle()
        for (pos in PointOfSale.entries) compose.onNodeWithText(pos.displayName).assertHeightIsAtLeast(48.dp)
        compose.onAllNodes(described, useUnmergedTree = true).assertCountEquals(0)
    }
}
