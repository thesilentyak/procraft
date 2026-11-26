package io.github.artcraftandroid.procraft.ui

import org.junit.Assert.assertEquals
import org.junit.Test

class HeldColorTest {
    @Test
    fun colours_go_to_hue_saturation_and_brightness_and_back() {
        for (rgb in listOf(0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0x00FFFF, 0xFF00FF, 0x804020, 0x123456, 0xFEDCBA, 0x000000, 0xFFFFFF, 0x7F7F7F)) {
            val (h, s, v) = rgbToHsv(rgb).toList()
            assertEquals("%06x".format(rgb), rgb, hsvToRgb(h, s, v))
        }
        assertEquals(listOf(1f / 3f, 1f, 1f), rgbToHsv(0x00FF00).toList())
        // a grey has no hue or saturation of its own
        val grey = rgbToHsv(0x808080)
        assertEquals(0f to 0f, grey[0] to grey[1])
    }

    @Test
    fun one_picker_edits_the_second_swatch_once_it_is_tapped() {
        // the onion skins' two colours, both black at first, one picker over them
        var before = 0x000000
        var after = 0x000000
        val c = HeldColor(before, greyHue = 0f) { before = it }
        // the first: a green from the ring and the disc
        c.setHsv(1f / 3f, 1f, 1f)
        assertEquals(0x00FF00, before)
        // the second tapped: the picker shows it (black, the hue kept) and its changes go there
        c.edit(after) { after = it }
        assertEquals(0f, c.brightness)
        assertEquals(1f / 3f, c.hue)
        c.setHsv(0f, 1f, 1f)
        assertEquals(0xFF0000, after)
        assertEquals("the first stays", 0x00FF00, before)
        // a turn of the ring keeps the second's own saturation and brightness
        c.setHsv(2f / 3f, c.saturation, c.brightness)
        assertEquals(0x0000FF, after)
        // and back to the first
        c.edit(before) { before = it }
        assertEquals(1f / 3f, c.hue)
        c.setHsv(c.hue, 1f, 0.5f)
        assertEquals(0x008000, before)
        assertEquals(0x0000FF, after)
    }
}
