package com.monokulo.till

import com.monokulo.till.ui.DarkTokens
import com.monokulo.till.ui.LightTokens
import androidx.compose.ui.graphics.toArgb
import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals

/** The window's ground before Compose's first frame is the theme's ground, light and dark, so a recreation never flashes. */
class WindowGroundTest {
    private fun ground(dir: String): Int {
        val xml = File("src/main/res/$dir/colors.xml").readText()
        val hex = Regex("""<color name="ground">#([0-9A-Fa-f]{8})</color>""").find(xml)!!.groupValues[1]
        return hex.toLong(16).toInt()
    }

    @Test
    fun theWindowsGroundIsTheThemesGround() {
        assertEquals(LightTokens.ground.toArgb(), ground("values"))
        assertEquals(DarkTokens.ground.toArgb(), ground("values-night"))
    }
}
