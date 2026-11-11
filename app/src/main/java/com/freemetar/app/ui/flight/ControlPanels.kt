package com.freemetar.app.ui.flight

import com.freemetar.app.domain.grouped
import com.freemetar.app.domain.loc

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.SwapHoriz
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.freemetar.app.data.link.FlightState
import com.freemetar.app.data.link.LinkClient
import com.freemetar.app.data.link.Radio
import com.freemetar.app.domain.AircraftProfile
import com.freemetar.app.domain.ControlMath
import com.freemetar.app.domain.Light
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.Mono
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

/**
 * Przycisk trybu w stylu panelu FCU: zielona obwódka i tekst, gdy tryb jest włączony.
 * Duży cel dotyku (min. 52 dp wysokości) — używany w trakcie lotu.
 */
@Composable
private fun ModeKey(label: String, on: Boolean, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val c = Fm.c
    val haptic = LocalHapticFeedback.current
    Box(
        modifier.height(52.dp).clip(RoundedCornerShape(8.dp))
            .background(if (on) c.green.copy(alpha = 0.14f) else c.terrain)
            .border(1.5.dp, if (on) c.green else c.rule, RoundedCornerShape(8.dp))
            .clickable(role = Role.Switch) { haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick() },
        contentAlignment = Alignment.Center,
    ) {
        Text(label, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 14.sp, color = if (on) c.green else c.ink2)
    }
}

/** Przycisk −/+ powtarzający akcję, gdy jest przytrzymany (najpierw po 400 ms, potem co 90 ms). */
@Composable
private fun RepeatKey(label: String, desc: String, modifier: Modifier = Modifier, action: () -> Unit) {
    val c = Fm.c
    val scope = rememberCoroutineScope()
    val act by rememberUpdatedState(action)
    val haptic = LocalHapticFeedback.current
    Box(
        modifier.size(52.dp).clip(RoundedCornerShape(8.dp)).background(c.terrain)
            .semantics { contentDescription = desc; role = Role.Button; onClick { act(); true } }
            .pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown()
                    haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                    act()
                    val job = scope.launch { delay(400); while (true) { act(); delay(90) } }
                    waitForUpOrCancellation()
                    job.cancel()
                }
            },
        contentAlignment = Alignment.Center,
    ) { Text(label, fontSize = 22.sp, fontWeight = FontWeight.Bold, color = c.accent) }
}

/**
 * Pokrętło nastawy: wartość w cyjanie (wartość wybrana), −/+ z powtarzaniem, małe i duże kroki,
 * dotknięcie wartości otwiera wpisanie liczby.
 */
@Composable
private fun SelectorRow(
    label: String, value: String,
    small: Pair<String, () -> Unit>, big: Pair<String, () -> Unit>?,
    smallDown: () -> Unit, bigDown: (() -> Unit)?,
    onEnter: (() -> Unit)?,
) {
    val c = Fm.c
    // Poniżej 360 dp szerokości ekranu cztery przyciski i wartość się nie mieszczą
    val roomy = androidx.compose.ui.platform.LocalConfiguration.current.screenWidthDp >= 360
    Row(Modifier.fillMaxWidth().padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        if (roomy && bigDown != null && big != null) RepeatKey("«", loc("control.down_by", label, big.first), action = bigDown)
        RepeatKey("−", loc("control.down_by", label, small.first), action = smallDown)
        Column(
            Modifier.weight(1f).height(52.dp).clip(RoundedCornerShape(8.dp)).background(Color.Black.copy(alpha = 0.35f))
                .let { if (onEnter != null) it.clickable(onClick = onEnter) else it },
            horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center,
        ) {
            Text(label, fontSize = 11.sp, color = c.ink2)
            Text(value, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 20.sp, color = c.accent)
        }
        RepeatKey("+", loc("control.up_by", label, small.first), action = small.second)
        if (roomy && big != null) RepeatKey("»", loc("control.up_by", label, big.first), action = big.second)
    }
}

/** Okno wpisania liczby (kurs, wysokość, częstotliwość, kod transpondera). */
@Composable
internal fun EnterDialog(title: String, hint: String, keyboard: KeyboardType, validate: (String) -> Boolean, onDone: (String) -> Unit, onDismiss: () -> Unit) {
    var text by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            OutlinedTextField(text, { text = it }, singleLine = true, placeholder = { Text(hint) },
                textStyle = MaterialTheme.typography.headlineSmall.copy(fontFamily = Mono),
                keyboardOptions = KeyboardOptions(keyboardType = keyboard))
        },
        confirmButton = { TextButton({ onDone(text); onDismiss() }, enabled = validate(text)) { Text(loc("control.set")) } },
        dismissButton = { TextButton(onDismiss) { Text(loc("common.cancel")) } },
    )
}

// ---------------- AUTOPILOT ----------------

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun AutopilotPanel(link: LinkClient, s: FlightState) {
    val ap = s.autopilot
    var enter by remember { mutableStateOf<String?>(null) }
    fun cmd(n: String, v: Number? = null) { link.command(n, v?.toDouble()) }

    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        ModeKey("AP", ap.master, Modifier.weight(1f)) { cmd("AP_MASTER") }
        ModeKey("FD", ap.fd, Modifier.weight(1f)) { cmd("TOGGLE_FLIGHT_DIRECTOR") }
        ModeKey("A/THR", ap.athr, Modifier.weight(1f)) { cmd("AUTO_THROTTLE_ARM") }
    }
    Spacer(Modifier.size(6.dp))
    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        ModeKey("HDG", ap.modes.hdg, Modifier.weight(1f)) { cmd("AP_HDG_HOLD") }
        ModeKey("NAV", ap.modes.nav, Modifier.weight(1f)) { cmd("AP_NAV1_HOLD") }
        ModeKey("LOC", ap.modes.loc, Modifier.weight(1f)) { cmd("AP_LOC_HOLD") }
        ModeKey("APPR", ap.modes.apr, Modifier.weight(1f)) { cmd("AP_APR_HOLD") }
    }
    Spacer(Modifier.size(6.dp))
    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        ModeKey("ALT", ap.modes.alt, Modifier.weight(1f)) { cmd("AP_ALT_HOLD") }
        ModeKey("V/S", ap.modes.vs, Modifier.weight(1f)) { cmd("AP_VS_HOLD") }
        ModeKey("FLC", ap.modes.flc, Modifier.weight(1f)) { cmd("FLIGHT_LEVEL_CHANGE") }
        ModeKey("SPD", ap.athr && ap.spdKt != null, Modifier.weight(1f)) { cmd("AP_PANEL_SPEED_HOLD") }
    }
    Spacer(Modifier.size(10.dp))
    // Nastawy wysyłane jako wartości bezwzględne (*_SET) — zgubiony pakiet nie kumuluje błędu
    SelectorRow("HDG", ap.hdgDeg?.let { "%03d°".format(it.roundToInt() % 360) } ?: "—",
        "1°" to { cmd("HEADING_BUG_SET", ControlMath.heading(link.state.value?.autopilot?.hdgDeg, 1)) },
        "10°" to { cmd("HEADING_BUG_SET", ControlMath.heading(link.state.value?.autopilot?.hdgDeg, 10)) },
        { cmd("HEADING_BUG_SET", ControlMath.heading(link.state.value?.autopilot?.hdgDeg, -1)) },
        { cmd("HEADING_BUG_SET", ControlMath.heading(link.state.value?.autopilot?.hdgDeg, -10)) },
    ) { enter = "HDG" }
    SelectorRow("ALT", ap.altFt?.let { "${it.roundToInt().grouped()} ft" } ?: "—",
        "100 ft" to { cmd("AP_ALT_VAR_SET_ENGLISH", ControlMath.altitude(link.state.value?.autopilot?.altFt, 100)) },
        "1000 ft" to { cmd("AP_ALT_VAR_SET_ENGLISH", ControlMath.altitude(link.state.value?.autopilot?.altFt, 1000)) },
        { cmd("AP_ALT_VAR_SET_ENGLISH", ControlMath.altitude(link.state.value?.autopilot?.altFt, -100)) },
        { cmd("AP_ALT_VAR_SET_ENGLISH", ControlMath.altitude(link.state.value?.autopilot?.altFt, -1000)) },
    ) { enter = "ALT" }
    SelectorRow("V/S", ap.vsFpm?.let { "%+d".format(it.roundToInt()) } ?: "—",
        "100" to { cmd("AP_VS_VAR_SET_ENGLISH", ControlMath.verticalSpeed(link.state.value?.autopilot?.vsFpm, 100)) }, null,
        { cmd("AP_VS_VAR_SET_ENGLISH", ControlMath.verticalSpeed(link.state.value?.autopilot?.vsFpm, -100)) }, null, null,
    )
    SelectorRow("SPD", ap.spdKt?.let { "${it.roundToInt()} kt" } ?: "—",
        "1 kt" to { cmd("AP_SPD_VAR_SET", ControlMath.speed(link.state.value?.autopilot?.spdKt, 1)) },
        "10 kt" to { cmd("AP_SPD_VAR_SET", ControlMath.speed(link.state.value?.autopilot?.spdKt, 10)) },
        { cmd("AP_SPD_VAR_SET", ControlMath.speed(link.state.value?.autopilot?.spdKt, -1)) },
        { cmd("AP_SPD_VAR_SET", ControlMath.speed(link.state.value?.autopilot?.spdKt, -10)) },
    ) { enter = "SPD" }

    when (enter) {
        "HDG" -> EnterDialog(loc("common.autopilot_heading"), loc("common.e_g_270"), KeyboardType.Number, { it.toIntOrNull() in 0..359 },
            { cmd("HEADING_BUG_SET", it.toInt()) }) { enter = null }
        "ALT" -> EnterDialog(loc("control.autopilot_altitude_ft"), loc("common.e_g_12000"), KeyboardType.Number, { it.toIntOrNull() in 0..50_000 },
            { cmd("AP_ALT_VAR_SET_ENGLISH", it.toInt()) }) { enter = null }
        "SPD" -> EnterDialog(loc("control.autopilot_speed_kt"), loc("common.e_g_250"), KeyboardType.Number, { it.toIntOrNull() in 0..500 },
            { cmd("AP_SPD_VAR_SET", it.toInt()) }) { enter = null }
    }
}

// ---------------- RADIO ----------------

@Composable
fun RadioPanel(link: LinkClient, s: FlightState, profile: AircraftProfile) {
    var enter by remember { mutableStateOf<String?>(null) }
    fun cmd(n: String, v: Number? = null) { link.command(n, v?.toDouble()) }
    val r = s.radios
    // Tylko radia, które ma dany samolot (szybowiec — jedno COM, śmigłowiec — COM i NAV)
    RadioRow("COM1", r.com1, "%.3f", onSwap = { cmd("COM_STBY_RADIO_SWAP") }) { enter = "COM1" }
    if (profile.com2) RadioRow("COM2", r.com2, "%.3f", onSwap = { cmd("COM2_RADIO_SWAP") }) { enter = "COM2" }
    if (profile.nav) RadioRow("NAV1", r.nav1, "%.2f", onSwap = { cmd("NAV1_RADIO_SWAP") }) { enter = "NAV1" }
    if (profile.nav2) RadioRow("NAV2", r.nav2, "%.2f", onSwap = { cmd("NAV2_RADIO_SWAP") }) { enter = "NAV2" }

    val c = Fm.c
    val x = r.transponder
    Spacer(Modifier.size(10.dp))
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Column(
            Modifier.weight(1f).height(56.dp).clip(RoundedCornerShape(8.dp)).background(Color.Black.copy(alpha = 0.35f)).clickable { enter = "XPDR" },
            horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center,
        ) {
            Text("SQUAWK", fontSize = 11.sp, color = c.ink2)
            Text(x?.code ?: "—", fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 22.sp, color = if (x?.code in setOf("7500", "7600", "7700")) c.danger else c.accent)
        }
        ModeKey("IDENT", x?.ident == true, Modifier.width(88.dp)) { cmd("XPNDR_IDENT_ON") }
    }
    Spacer(Modifier.size(6.dp))
    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        listOf("standby" to 1 to "STBY", "on" to 3 to "ON", "alt" to 4 to "ALT").forEach { (pair, label) ->
            val (mode, value) = pair
            ModeKey(label, x?.mode == mode, Modifier.weight(1f)) { cmd("XPNDR_MODE", value) }
        }
    }
    Row(horizontalArrangement = Arrangement.spacedBy(6.dp), modifier = Modifier.padding(top = 6.dp)) {
        listOf("7000", "2000", "1200").forEach { code -> ModeKey(code, x?.code == code, Modifier.weight(1f)) { cmd("XPNDR_SET", code.toInt()) } }
    }

    when (enter) {
        "COM1", "COM2" -> EnterDialog(loc("common.standby_frequency_mhz", enter), loc("common.e_g_121_500"), KeyboardType.Decimal,
            { ControlMath.parseMhz(it)?.let(ControlMath::comHz) != null },
            { t -> ControlMath.parseMhz(t)?.let(ControlMath::comHz)?.let { hz -> cmd(if (enter == "COM1") "COM_STBY_RADIO_SET_HZ" else "COM2_STBY_RADIO_SET_HZ", hz) } }) { enter = null }
        "NAV1", "NAV2" -> EnterDialog(loc("common.standby_frequency_mhz", enter), loc("control.e_g_113_45"), KeyboardType.Decimal,
            { ControlMath.parseMhz(it)?.let(ControlMath::navHz) != null },
            { t -> ControlMath.parseMhz(t)?.let(ControlMath::navHz)?.let { hz -> cmd(if (enter == "NAV1") "NAV1_STBY_SET_HZ" else "NAV2_STBY_SET_HZ", hz) } }) { enter = null }
        "XPDR" -> EnterDialog(loc("control.transponder_code"), loc("control.4_digits_0_7"), KeyboardType.Number, { ControlMath.squawk(it) != null },
            { t -> ControlMath.squawk(t)?.let { cmd("XPNDR_SET", it) } }) { enter = null }
    }
}

@Composable
private fun RadioRow(name: String, r: Radio?, fmt: String, onSwap: () -> Unit, onEdit: () -> Unit) {
    val c = Fm.c
    Row(Modifier.fillMaxWidth().padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(name, fontFamily = Mono, fontSize = 11.sp, color = c.ink2, modifier = Modifier.width(38.dp))
        Text(r?.activeMhz?.let { fmt.format(java.util.Locale.US, it) } ?: "—", fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 16.sp, color = c.green,
            modifier = Modifier.weight(1f), textAlign = TextAlign.Center, maxLines = 1, softWrap = false)
        Box(Modifier.size(40.dp).clip(RoundedCornerShape(8.dp)).background(c.terrain).clickable(role = Role.Button, onClick = onSwap), contentAlignment = Alignment.Center) {
            Icon(Icons.Outlined.SwapHoriz, loc("common.swap_active_and_standby", name), tint = c.accent)
        }
        Text(r?.standbyMhz?.let { fmt.format(java.util.Locale.US, it) } ?: "—", fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 16.sp, color = c.accent,
            modifier = Modifier.weight(1f).clip(RoundedCornerShape(8.dp)).clickable(onClick = onEdit).padding(vertical = 10.dp), textAlign = TextAlign.Center, maxLines = 1, softWrap = false)
    }
}

// ---------------- SYSTEMY ----------------

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun SystemsPanel(link: LinkClient, s: FlightState, profile: AircraftProfile) {
    val c = Fm.c
    fun cmd(n: String, v: Number? = null) { link.command(n, v?.toDouble()) }
    val ctl = s.controls
    val gearDown = ctl.gearDown == true
    var confirmGear by remember { mutableStateOf(false) }

    // Podwozie tylko chowane; hamulec postojowy — wszystko poza szybowcem
    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        if (profile.retractableGear) ModeKey(if (gearDown) loc("control.gear_2") else loc("control.gear"), gearDown, Modifier.weight(1f)) {
            // Chowanie podwozia na ziemi wymaga potwierdzenia
            if (gearDown && s.aircraft.onGround == true) confirmGear = true else cmd("GEAR_TOGGLE")
        }
    }
    // Hamulec postojowy (mały klawisz) i obok oś hamulców kół; szybowiec bez hamulca postojowego
    if (profile != AircraftProfile.GLIDER) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.padding(top = 6.dp)) {
        ModeKey(if (ctl.parkingBrake == true) "PARK ●" else "PARK", ctl.parkingBrake == true, Modifier.width(92.dp)) { cmd("PARKING_BRAKES") }
        WheelBrakeAxis(link, s, Modifier.weight(1f))
    }
    Spacer(Modifier.size(8.dp))
    val flaps = ctl.flapsIndex
    val positions = ctl.flapsPositions
    if (profile.hasFlaps) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        RepeatKey("▲", loc("control.flaps_up"), action = { cmd("FLAPS_DECR") })
        Column(Modifier.weight(1f).height(52.dp).clip(RoundedCornerShape(8.dp)).background(Color.Black.copy(alpha = 0.35f)),
            horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
            Text(loc("control.flaps"), fontSize = 11.sp, color = c.ink2)
            Text(if (flaps != null && positions != null) "$flaps / ${positions - 1}" else "—", fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 19.sp, color = c.accent)
        }
        RepeatKey("▼", loc("control.flaps_down"), action = { cmd("FLAPS_INCR") })
    }
    Spacer(Modifier.size(8.dp))
    // Spoilery tylko w samolotach, które je mają; w szybowcu to hamulce aerodynamiczne (bez uzbrajania)
    // Uzbrojenie (mały klawisz) i obok oś hamulca aerodynamicznego
    if (profile == AircraftProfile.GLIDER) SpeedBrakeAxis(link, s, label = loc("control.airbrakes"))
    else if (profile.hasSpoilers) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        ModeKey("ARM", ctl.spoilersArmed == true, Modifier.width(92.dp)) { cmd("SPOILERS_ARM_TOGGLE") }
        SpeedBrakeAxis(link, s, Modifier.weight(1f))
    }
    Spacer(Modifier.size(8.dp))
    if (profile.hasTrim) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        RepeatKey("▼", loc("common.trim_nose_down"), action = { cmd("ELEV_TRIM_DN") })
        Column(Modifier.weight(1f).height(52.dp).clip(RoundedCornerShape(8.dp)).background(Color.Black.copy(alpha = 0.35f)),
            horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
            Text(loc("control.trim"), fontSize = 11.sp, color = c.ink2)
            Text(ctl.elevatorTrimPct?.let { "%+.1f%%".format(it) } ?: "—", fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 18.sp, color = c.accent)
        }
        RepeatKey("▲", loc("common.trim_nose_up"), action = { cmd("ELEV_TRIM_UP") })
    }
    Spacer(Modifier.size(10.dp))
    Text(loc("control.lights"), style = MaterialTheme.typography.labelMedium, color = c.ink2)
    Spacer(Modifier.size(4.dp))
    val l = s.lights
    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp), maxItemsInEachRow = 3) {
        profile.lights.map { light ->
            when (light) {
                Light.BEACON -> Triple("BEACON", l.beacon, "TOGGLE_BEACON_LIGHTS")
                Light.STROBE -> Triple("STROBE", l.strobe, "STROBES_TOGGLE")
                Light.NAV -> Triple("NAV", l.nav, "TOGGLE_NAV_LIGHTS")
                Light.LANDING -> Triple("LANDING", l.landing, "LANDING_LIGHTS_TOGGLE")
                Light.TAXI -> Triple("TAXI", l.taxi, "TOGGLE_TAXI_LIGHTS")
                Light.LOGO -> Triple("LOGO", l.logo, "TOGGLE_LOGO_LIGHTS")
                Light.WING -> Triple("WING", l.wing, "TOGGLE_WING_LIGHTS")
                Light.PANEL -> Triple("PANEL", l.panel, "PANEL_LIGHTS_TOGGLE")
                Light.CABIN -> Triple("CABIN", l.cabin, "TOGGLE_CABIN_LIGHTS")
            }
        }.forEach { (label, on, event) -> ModeKey(label, on == true, Modifier.weight(1f)) { cmd(event) } }
        // Niepełny ostatni rząd: puste miejsca, żeby klawisze miały tę samą szerokość
        repeat((3 - profile.lights.size % 3) % 3) { Spacer(Modifier.weight(1f)) }
    }

    if (confirmGear) {
        AlertDialog(
            onDismissRequest = { confirmGear = false },
            title = { Text(loc("common.retract_the_gear_on_the_ground")) },
            text = { Text(loc("common.the_aircraft_is_on_the_ground")) },
            confirmButton = { TextButton({ cmd("GEAR_UP"); confirmGear = false }) { Text(loc("common.retract"), color = c.danger) } },
            dismissButton = { TextButton({ confirmGear = false }) { Text(loc("common.cancel")) } },
        )
    }
}
