package io.github.artcraftandroid.procraft.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.PopupProperties
import io.github.artcraftandroid.procraft.core.PluginLayerFilter

private val FiltersVeil = Color(0x80000000)
private val FiltersSheet get() = if (Pc.light) Color(0xFFF2F2F7) else Color(0xFF1C1C1E)
private val FiltersTitle get() = if (Pc.light) Color(0xFF3A3A3A) else Color(0xFFC1C1C1)
private val FiltersDim = Color(0xFF868686)
private val FiltersInk get() = if (Pc.light) Color.Black else Color.White
private val FiltersRule get() = if (Pc.light) Color(0xFFD1D1D6) else Color(0xFF3A3A3C)

/** [filters] under their headings, each heading once, where its first filter comes. */
internal fun groupedFilters(filters: List<PluginLayerFilter>): List<Pair<String, List<PluginLayerFilter>>> {
    val groups = LinkedHashMap<String, MutableList<PluginLayerFilter>>()
    for (f in filters) groups.getOrPut(f.group) { mutableListOf() }.add(f)
    return groups.map { (group, list) -> group to list.toList() }
}

/**
 * The list the Layers panel's + opens for a filter layer (interface 17): a sheet over everything, as the Textures
 * sheet is, with the filters the plugins on offer under their headings. A tap picks one ([onPick] with its id);
 * Cancel, or a tap off the sheet, closes it.
 */
@Composable
internal fun FilterLayerList(filters: List<PluginLayerFilter>, onPick: (String) -> Unit, onClose: () -> Unit) {
    val atOrigin = remember {
        object : androidx.compose.ui.window.PopupPositionProvider {
            override fun calculatePosition(
                anchorBounds: androidx.compose.ui.unit.IntRect,
                windowSize: androidx.compose.ui.unit.IntSize,
                layoutDirection: androidx.compose.ui.unit.LayoutDirection,
                popupContentSize: androidx.compose.ui.unit.IntSize,
            ) = androidx.compose.ui.unit.IntOffset.Zero
        }
    }
    val groups = remember(filters) { groupedFilters(filters) }
    PcPopup(atOrigin, onDismissRequest = onClose, properties = PopupProperties(focusable = true, clippingEnabled = false)) {
        Box(Modifier.fillMaxSize().background(FiltersVeil).pointerInput(Unit) { awaitEachGesture { awaitFirstDown().consume(); onClose() } }) {
            Column(
                Modifier.align(Alignment.TopCenter).offset(y = 74.dp).width(440.dp).heightIn(max = 640.dp).clip(RoundedCornerShape(26.dp)).background(FiltersSheet)
                    .pointerInput(Unit) { awaitEachGesture { awaitFirstDown(requireUnconsumed = false).consume() } },
            ) {
                Row(Modifier.fillMaxWidth().padding(start = 19.dp, end = 19.dp, top = 30.dp).height(39.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text("Filter Layer", color = FiltersTitle, fontSize = 28.sp, fontWeight = FontWeight.Bold, maxLines = 1, modifier = Modifier.weight(1f))
                    Text("Cancel", color = FiltersDim, fontSize = 17.sp, maxLines = 1, modifier = Modifier.clickable(onClick = onClose))
                }
                LazyColumn(Modifier.fillMaxWidth().padding(start = 19.dp, end = 19.dp, top = 14.dp, bottom = 19.dp)) {
                    for ((group, list) in groups) {
                        item(key = "group $group") {
                            Box(Modifier.fillMaxWidth().height(38.dp), contentAlignment = Alignment.BottomStart) {
                                Text(group, color = FiltersDim, fontSize = 15.sp, fontWeight = FontWeight.Bold, maxLines = 1, modifier = Modifier.padding(bottom = 10.dp))
                            }
                        }
                        for (f in list) {
                            item(key = f.id) {
                                Column(Modifier.fillMaxWidth().height(44.dp).clickable { onPick(f.id) }) {
                                    Box(Modifier.weight(1f).fillMaxWidth().padding(start = 12.dp), contentAlignment = Alignment.CenterStart) {
                                        Text(f.label, color = FiltersInk, fontSize = 17.sp, maxLines = 1)
                                    }
                                    Box(Modifier.fillMaxWidth().padding(start = 12.dp).height(1.dp).background(FiltersRule))
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
