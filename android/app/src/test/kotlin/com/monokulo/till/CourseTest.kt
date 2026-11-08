package com.monokulo.till

import org.junit.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * A shared surface's course over a move (decision 232): where it is at a
 * point of the move, and the parts of the move over which it holds or has
 * cleared a text run, which is when words on it or under it may show
 * (decision 233).
 */
class CourseTest {
    private val row = floatArrayOf(0f, 100f, 100f, 120f)
    private val screen = floatArrayOf(0f, 0f, 100f, 200f)

    private fun course(from: FloatArray, to: FloatArray, p: Float = 0f) =
        Course("sale/a1").apply { line(from[0], from[1], from[2], from[3], to[0], to[1], to[2], to[3], p) }

    @Test
    fun itIsTheStraightLineBetweenItsEndsAtTheStagesProgress() {
        val c = course(row, screen, 0.25f)
        assertEquals(0f, c.left)
        assertEquals(75f, c.top)
        assertEquals(100f, c.right)
        assertEquals(140f, c.bottom)
    }

    @Test
    fun aGrowingGroundHoldsAWordOnlyOnceItsEdgeHasPassedIt() {
        val c = course(row, screen)
        // A word under the row, on the sale: the ground's bottom reaches 169.5 (170 less half a pixel) at 0.61875 of the move.
        val under = c.holding(10f, 150f, 90f, 170f)
        assertEquals(0.61875f, Course.heldFrom(under), 1e-4f)
        assertEquals(1f, Course.heldUntil(under))
        // The row's own words: held from the first frame to the last.
        val own = c.holding(10f, 102f, 90f, 118f)
        assertEquals(0f, Course.heldFrom(own))
        assertEquals(1f, Course.heldUntil(own))
    }

    @Test
    fun aShrinkingGroundLetsGoOfAWordAsItsEdgePassesIt() {
        val held = course(screen, row).holding(10f, 150f, 90f, 170f)
        assertEquals(0f, Course.heldFrom(held))
        assertEquals(0.38125f, Course.heldUntil(held), 1e-4f)
    }

    @Test
    fun aWordAShrinkingGroundNeverHeldIsNeverHeld() {
        val held = course(row, row).holding(10f, 150f, 90f, 170f)
        assertTrue(Course.heldFrom(held) > Course.heldUntil(held), "never held: ${Course.heldFrom(held)}..${Course.heldUntil(held)}")
    }

    @Test
    fun wordsUnderAShrinkingCardShowOnceItHasPassedThem() {
        // The enlarged code's white shrinking back into its card, a word below the card.
        val c = course(screen, floatArrayOf(20f, 50f, 80f, 110f))
        assertEquals(50f / 90f, c.clearOf(10f, 150f, 90f, 170f), 1e-4f)
        // A word the card still covers at rest is never clear of it.
        assertEquals(2f, c.clearOf(30f, 60f, 70f, 100f))
        // One it never met is clear from the start.
        assertEquals(0f, course(row, row).clearOf(10f, 150f, 90f, 170f))
    }

    @Test
    fun itHoldsABoxWithinHalfAPixel() {
        val c = course(row, screen, 1f)
        assertTrue(c.holds(0.4f, 0f, 100f, 200.4f))
        assertTrue(!c.holds(-1f, 0f, 100f, 200f))
    }

    /**
     * A course (the code's bitmap among what it drew between) lasts only
     * while one of its ends is on a screen: a key is a sale's, so the stage
     * would otherwise hold a course and a code for every sale ever opened,
     * and walk them all on every frame of a move.
     */
    @Test
    fun theStageKeepsACourseOnlyWhileOneOfItsEndsIsOnAScreen() {
        // Three sales' screens opened and left, each with its row, its code and its order card: ends come and go.
        val courses = Courses()
        val screens = listOf(Screen.Till, Screen.Sale("a1"))
        for (sale in listOf("a1", "a2", "a3")) {
            val ends = listOf("sale/$sale", "code/$sale", "order/$sale").flatMap { key -> screens.map { End(key, it, Move.RowGrow) } }
            ends.forEach(courses::add)
            assertEquals(3, courses.size(), "one course per surface on screen")
            ends.forEach(courses::remove)
            assertEquals(0, courses.size(), "nothing left of $sale's surfaces")
            assertEquals(null, courses.find("code/$sale"))
        }
        // One end left of a surface keeps its course, and the course is found by key.
        val left = End("sale/a4", Screen.Till, Move.RowGrow)
        courses.add(left)
        courses.add(End("sale/a4", Screen.Sale("a4"), Move.RowGrow))
        courses.remove(left)
        assertEquals(1, courses.size())
        assertEquals(Move.RowGrow, courses.find("sale/a4")?.during)
    }
}
