package com.freemetar.app.ui.flight

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Layers
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.min
import androidx.compose.ui.unit.sp
import com.freemetar.app.ui.flight.avionics.gpsLeg
import com.freemetar.app.data.link.FlightState
import com.freemetar.app.data.link.LinkClient
import com.freemetar.app.data.nav.FixResult
import com.freemetar.app.domain.AircraftProfile
import com.freemetar.app.domain.Lever
import com.freemetar.app.domain.loc
import com.freemetar.app.ui.theme.Mono
import kotlin.math.roundToInt

// Kolory paneli awioniki — stałe jak w kabinie, niezależne od motywu aplikacji
private val BEZEL = Color(0xFF0B0E12)
private val KEY = Color(0xFF1E242B)
private val KEY_ON = Color(0xFF2E3A46)
private val LEGEND = Color(0xFFE6EBF0)
private val CYAN = Color(0xFF4FD1E8)
private val GREEN = Color(0xFF3FE07A)

private val RANGES = listOf(5, 10, 20, 40, 80, 160, 320)

/**
 * Widok do latania: PFD i ND jednocześnie, obok pionowe dźwignie mocy (a w tłokowych/turbośmigłowych
 * także śmigło i mieszanka). Rozmiary liczone tak, żeby całość mieściła się na ekranie bez przewijania:
 * układ (PFD nad ND albo PFD obok ND) wybierany tak, żeby wyświetlacze były jak największe w danym prostokącie.
 * Otwarty [panel] (autopilot, radio…) zajmuje miejsce ND, więc PFD i dźwignie zostają na widoku.
 */
@Composable
fun FlyDeck(
    link: LinkClient, s: FlightState, profile: AircraftProfile, nearby: List<FixResult>, availableHeight: Dp,
    modifier: Modifier = Modifier, panel: (@Composable () -> Unit)? = null, leversLeft: Boolean = false,
    /** Profil wybrany ręcznie (null = automatycznie) — decyduje też o stylu PFD i ND. */
    displayProfile: AircraftProfile? = null,
) {
    // Jedna wspólna dźwignia mocy dla wszystkich silników plus dźwignie profilu (szybowiec — żadnej)
    val levers = (if (profile.hasThrottle) 1 else 0) + profile.levers.size
    BoxWithConstraints(modifier.fillMaxWidth(), contentAlignment = Alignment.TopCenter) {
        val gap = 6.dp
        val leverW = 46.dp
        val thrW = if (levers == 0) 0.dp else leverW * levers + gap * (levers - 1)
        val instW = maxWidth - thrW - if (levers == 0) 0.dp else gap
        // Dwa układy, wybór tego z większym PFD: obok siebie (wysokość h) albo jeden nad drugim (szerokość colW)
        val rowH = min(availableHeight, instW / (1.12f + 1.05f) - 2.dp)
        val colW = min(instW, (availableHeight - gap) / (1f / 1.12f + 1f / 1.45f))
        val landscape = rowH * 1.12f > colW
        if (landscape) {
            // PFD (1,12:1) i ND (1:1) obok siebie, wysokość wg dostępnego miejsca
            val h = rowH
            Row(horizontalArrangement = Arrangement.spacedBy(gap), verticalAlignment = Alignment.Top) {
                if (leversLeft) Levers(link, s, profile, Modifier.height(h).width(thrW))
                Box(Modifier.clip(RoundedCornerShape(10.dp))) { Pfd(s, Modifier.width(h * 1.12f), displayProfile) }
                if (panel != null) PanelSlot(Modifier.width(h * 1.05f).height(h), panel)
                else NdWithControls(s, nearby, Modifier.width(h * 1.05f), aspect = 1.05f, displayProfile)
                if (!leversLeft) Levers(link, s, profile, Modifier.height(h).width(thrW))
            }
        } else {
            // PFD nad ND; ND wypełnia resztę wysokości (proporcje 1,0–1,45)
            val w = colW
            val pfdH = w / 1.12f
            val ndAspect = (w / (availableHeight - pfdH - gap).coerceAtLeast(w / 1.45f)).coerceIn(1f, 1.45f)
            val ndH = w / ndAspect
            Row(horizontalArrangement = Arrangement.spacedBy(gap), verticalAlignment = Alignment.Top) {
                if (leversLeft) Levers(link, s, profile, Modifier.height(pfdH + gap + ndH).width(thrW))
                Column(Modifier.width(w), verticalArrangement = Arrangement.spacedBy(gap)) {
                    Box(Modifier.clip(RoundedCornerShape(10.dp))) { Pfd(s, profile = displayProfile) }
                    if (panel != null) PanelSlot(Modifier.fillMaxWidth().height(ndH), panel)
                    else NdWithControls(s, nearby, Modifier.fillMaxWidth(), aspect = ndAspect, displayProfile)
                }
                if (!leversLeft) Levers(link, s, profile, Modifier.height(pfdH + gap + ndH).width(thrW))
            }
        }
    }
}

/**
 * ND z nakładką sterowania jak pokrętła EFIS: tryb ARC/ROSE (lewy dolny róg), warstwy (środek dołu)
 * i zasięg −/+ (prawy dolny róg). Wszystko na wyświetlaczu — bez osobnych rzędów przycisków.
 */
@Composable
private fun NdWithControls(s: FlightState, nearby: List<FixResult>, modifier: Modifier, aspect: Float, displayProfile: AircraftProfile? = null) {
    var rangeIdx by rememberSaveable { mutableIntStateOf(3) }
    var rose by rememberSaveable { mutableStateOf(false) }
    var layersText by rememberSaveable { mutableStateOf("airport,vor,ndb") }
    var layersMenu by remember { mutableStateOf(false) }
    val layers = layersText.split(",").filter { it.isNotBlank() }.toSet()
    val range = RANGES[rangeIdx]
    BoxWithConstraints(modifier.clip(RoundedCornerShape(10.dp))) {
        val narrow = maxWidth < 230.dp
        NavDisplay(s, s.gpsLeg(), range, rose = rose, nav = nearby, layers = layers, aspect = aspect, profile = displayProfile)
        Row(Modifier.align(Alignment.BottomStart).padding(6.dp)) {
            EfisKey(if (rose) "ROSE" else "ARC", loc("fly_deck.map_mode_tap_to_switch")) { rose = !rose }
        }
        Box(Modifier.align(if (narrow) Alignment.BottomStart else Alignment.BottomCenter).padding(start = 6.dp, end = 6.dp, bottom = if (narrow) 42.dp else 6.dp)) {
            EfisKey(null, loc("common.map_layers"), icon = true, on = nearby.isNotEmpty()) { layersMenu = true }
            DropdownMenu(layersMenu, { layersMenu = false }) {
                listOf("airport" to loc("common.airports"), "vor" to "VOR", "ndb" to "NDB", "waypoint" to loc("fly_deck.waypoints")).forEach { (k, label) ->
                    DropdownMenuItem(
                        text = { Text((if (k in layers) "✓  " else "     ") + label) },
                        onClick = { layersText = (if (k in layers) layers - k else layers + k).joinToString(",") },
                    )
                }
                if (nearby.isEmpty()) DropdownMenuItem(text = { Text(loc("fly_deck.no_msfs_data_yet")) }, onClick = { layersMenu = false }, enabled = false)
            }
        }
        Row(Modifier.align(Alignment.BottomEnd).padding(6.dp), verticalAlignment = Alignment.CenterVertically) {
            EfisKey("−", loc("fly_deck.decrease_range"), enabled = rangeIdx > 0) { rangeIdx-- }
            Text("$range", fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 12.sp, color = CYAN,
                modifier = Modifier.width(if (narrow) 28.dp else 34.dp).semantics { contentDescription = loc("fly_deck.range_miles", range) }, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
            EfisKey("+", loc("fly_deck.increase_range"), enabled = rangeIdx < RANGES.lastIndex) { rangeIdx++ }
        }
    }
}

/** Miejsce na panel (zamiast ND): własne przewijanie, żeby wyświetlacz PFD i dźwignie nie uciekały z ekranu. */
@Composable
private fun PanelSlot(modifier: Modifier, content: @Composable () -> Unit) {
    Column(
        modifier.clip(RoundedCornerShape(10.dp)).background(com.freemetar.app.ui.theme.Fm.c.paper)
            .border(1.dp, com.freemetar.app.ui.theme.Fm.c.rule, RoundedCornerShape(10.dp))
            .verticalScroll(rememberScrollState()).padding(8.dp),
    ) { content() }
}

/** Mały klawisz panelu EFIS: ciemny, z białym napisem, 30 dp — na wierzchu wyświetlacza. */
@Composable
private fun EfisKey(label: String?, desc: String, icon: Boolean = false, on: Boolean = true, enabled: Boolean = true, onClick: () -> Unit) {
    val haptic = LocalHapticFeedback.current
    Box(
        Modifier.height(30.dp).width(if (label != null && label.length > 2) 52.dp else 30.dp).clip(RoundedCornerShape(6.dp))
            .background(KEY.copy(alpha = 0.92f)).border(1.dp, Color(0x40FFFFFF), RoundedCornerShape(6.dp))
            .semantics { contentDescription = desc }
            .clickable(enabled = enabled, role = Role.Button) { haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove); onClick() },
        contentAlignment = Alignment.Center,
    ) {
        if (icon) Icon(Icons.Outlined.Layers, null, tint = if (on) LEGEND else LEGEND.copy(alpha = 0.4f), modifier = Modifier.size(18.dp))
        else Text(label.orEmpty(), fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 12.sp, color = if (enabled) LEGEND else LEGEND.copy(alpha = 0.3f))
    }
}

/** Kolumny dźwigni obok wyświetlaczy: wspólna moc wszystkich silników i dźwignie profilu (śmigło, mieszanka, CL). */
@Composable
private fun Levers(link: LinkClient, s: FlightState, p: AircraftProfile, modifier: Modifier) {
    if (!p.hasThrottle && p.levers.isEmpty()) return
    fun send(name: String, pct: Float) = link.command(name, (pct / 100f * 16383f).roundToInt().coerceIn(0, 16383).toDouble())
    // Wąska dźwignia: zapadki bez długich podpisów, tylko pierwsza litera (Airbus: I, C, F, T)
    val thrDetents = if (p == AircraftProfile.A320) p.detents.map { (pct, l) -> pct to l.take(1) } else p.detents.map { (pct, _) -> pct to "" }
    Row(modifier.clip(RoundedCornerShape(10.dp)).background(BEZEL).padding(vertical = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        if (p.hasThrottle) Column(Modifier.weight(1f).fillMaxHeight(), horizontalAlignment = Alignment.CenterHorizontally) {
            CompactLever("THR", s.controls.throttlePct, Color(0xFF2A2D31), thrDetents, Modifier.weight(1f)) { send("THROTTLE_SET", it) }
            // Szybkie przyciski pod dźwignią: jałowy, odwracacz ciągu i (odrzutowce) TOGA
            MiniKey("IDLE") { link.command("THROTTLE_CUT") }
            if (p.hasReverser) MiniKey("REV", enabled = s.aircraft.onGround != false) { link.command("THROTTLE_REVERSE_THRUST_TOGGLE") }
            if (p.jet) MiniKey("TOGA") { toga(link) }
        }
        p.levers.forEach { lever ->
            Column(Modifier.weight(1f).fillMaxHeight(), horizontalAlignment = Alignment.CenterHorizontally) {
                when (lever) {
                    Lever.PROP -> CompactLever("PROP", s.controls.propPct, Color(0xFF1F5FBF), listOf(0 to "", 100 to ""), Modifier.weight(1f)) { send("PROPELLER_SET", it) }
                    Lever.CONDITION -> CompactLever("CL", s.controls.propPct, Color(0xFF1F5FBF), listOf(0 to "", 70 to "", 100 to ""), Modifier.weight(1f)) { send("PROPELLER_SET", it) }
                    Lever.MIXTURE -> CompactLever("MIX", s.controls.mixturePct, Color(0xFFC62828), listOf(0 to "", 100 to ""), Modifier.weight(1f)) { send("MIXTURE_SET", it) }
                }
            }
        }
    }
}

@Composable
private fun MiniKey(label: String, enabled: Boolean = true, onClick: () -> Unit) {
    val haptic = LocalHapticFeedback.current
    Box(
        Modifier.padding(top = 4.dp).width(42.dp).height(26.dp).clip(RoundedCornerShape(5.dp)).background(KEY)
            .clickable(enabled = enabled, role = Role.Button, onClickLabel = label) { haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick() },
        contentAlignment = Alignment.Center,
    ) { Text(label, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 9.sp, color = if (enabled) LEGEND else LEGEND.copy(alpha = 0.3f)) }
}

/**
 * Wąska dźwignia (46 dp): odczyt % u góry, szczelina z zapadkami, rączka przeciągana palcem.
 * Wartość idzie do symulatora co 120 ms w trakcie przeciągania i raz po puszczeniu; zapadki „łapią” ±3 %.
 */
@Composable
private fun CompactLever(label: String, actual: Double?, handle: Color, detents: List<Pair<Int, String>>, modifier: Modifier, onSet: (Float) -> Unit) {
    val haptic = LocalHapticFeedback.current
    var dragging by remember { mutableStateOf(false) }
    var local by remember { mutableStateOf(0f) }
    var lastSent by remember { mutableLongStateOf(0L) }
    val set by rememberUpdatedState(onSet)
    val shown = if (dragging) local else (actual ?: 0.0).toFloat()
    Text(label, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 9.sp, color = LEGEND.copy(alpha = 0.7f))
    Text("${shown.roundToInt()}", fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 13.sp, color = if (dragging) CYAN else GREEN)
    Canvas(
        modifier.fillMaxWidth().padding(top = 4.dp)
            .semantics {
                contentDescription = "$label ${shown.roundToInt()}%"
                customActions = listOf(
                    CustomAccessibilityAction(loc("common.more")) { set(((actual ?: 0.0) + 10).toFloat().coerceAtMost(100f)); true },
                    CustomAccessibilityAction(loc("common.less")) { set(((actual ?: 0.0) - 10).toFloat().coerceAtLeast(0f)); true },
                )
            }
            .pointerInput(detents) {
                val pad = 14.dp.toPx()
                fun pctAt(y: Float): Float {
                    val raw = ((size.height - pad - y) / (size.height - 2 * pad) * 100f).coerceIn(0f, 100f)
                    return detents.firstOrNull { kotlin.math.abs(it.first - raw) <= 3f }?.first?.toFloat() ?: raw
                }
                awaitEachGesture {
                    val down = awaitFirstDown()
                    down.consume()
                    dragging = true
                    local = pctAt(down.position.y)
                    var lastDetent: Int? = null
                    while (true) {
                        val ev = awaitPointerEvent()
                        val ch = ev.changes.firstOrNull { it.id == down.id } ?: break
                        if (!ch.pressed) break
                        ch.consume()
                        local = pctAt(ch.position.y)
                        val d = detents.firstOrNull { it.first.toFloat() == local }?.first
                        if (d != null && d != lastDetent) haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                        lastDetent = d
                        val now = System.currentTimeMillis()
                        if (now - lastSent >= 120) { lastSent = now; set(local) }
                    }
                    set(local)
                    dragging = false
                }
            },
    ) {
        val pad = 14.dp.toPx()
        val cx = size.width / 2
        val top = pad; val bottom = size.height - pad
        drawRoundRect(Color.Black, Offset(cx - 4.dp.toPx(), top - 4), Size(8.dp.toPx(), bottom - top + 8), CornerRadius(8f))
        for (p in 0..100 step 10) {
            val y = bottom - (bottom - top) * p / 100f
            drawLine(LEGEND.copy(alpha = 0.35f), Offset(cx - 13.dp.toPx(), y), Offset(cx - 8.dp.toPx(), y), 1.5f)
        }
        detents.forEach { (p, _) ->
            val y = bottom - (bottom - top) * p / 100f
            drawLine(LEGEND, Offset(cx + 8.dp.toPx(), y), Offset(cx + 15.dp.toPx(), y), 3f)
        }
        val y = bottom - (bottom - top) * shown / 100f
        val hw = 34.dp.toPx(); val hh = 18.dp.toPx()
        drawRoundRect(Brush.verticalGradient(listOf(handle.lighten(0.25f), handle.darken(0.25f)), y - hh / 2, y + hh / 2),
            Offset(cx - hw / 2, y - hh / 2), Size(hw, hh), CornerRadius(7f))
        drawLine(Color.White.copy(alpha = 0.6f), Offset(cx - hw / 2 + 5, y), Offset(cx + hw / 2 - 5, y), 2f)
        if (dragging) drawRoundRect(CYAN, Offset(cx - hw / 2 - 3, y - hh / 2 - 3), Size(hw + 6, hh + 6), CornerRadius(9f), style = Stroke(3f))
    }
}

/** TOGA: pełna moc na wszystkich silnikach, a przy autociągu także tryb odejścia (TO/GA). */
internal fun toga(link: LinkClient) {
    link.command("THROTTLE_SET", 16383.0)
    link.command("AUTO_THROTTLE_TO_GA")
}
