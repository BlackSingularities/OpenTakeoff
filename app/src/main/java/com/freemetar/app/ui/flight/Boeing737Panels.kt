package com.freemetar.app.ui.flight

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.SwapHoriz
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.freemetar.app.data.link.FlightState
import com.freemetar.app.data.link.LinkClient
import com.freemetar.app.data.link.Radio
import com.freemetar.app.domain.AircraftProfile
import com.freemetar.app.domain.ControlMath
import com.freemetar.app.domain.loc
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.Mono
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.cos
import kotlin.math.roundToInt
import kotlin.math.sin

// Panele Boeinga 737 w stylu kabiny NG/MAX: ciemnoszara płyta, pomarańczowe opisy, białe cyfry na czarnym tle,
// klawisze z zieloną lampką pod napisem. Tylko elementy, które mają działającą komendę w Link.

private val B_PLATE = Color(0xFF3C4246)
private val B_PLATE_EDGE = Color(0xFF23282B)
private val B_LEGEND = Color(0xFFF08A2C)      // pomarańczowe opisy MCP
private val B_LABEL = Color(0xFFE9ECEE)        // białe napisy na klawiszach
private val B_WINDOW = Color(0xFF07080A)
private val B_DIGITS = Color(0xFFF2F4FF)       // białe cyfry wyświetlaczy MCP
private val B_AMBER_DIGITS = Color(0xFFFFB347) // pomarańczowe cyfry RMP
private val B_KEY = Color(0xFF1F2427)
private val B_LAMP = Color(0xFF55E07A)
private val B_RMP_PLATE = Color(0xFF6D7479)
private val E_WHITE = Color(0xFFF2F2EA)
private val E_CYAN = Color(0xFF41C8F0)
private val E_GREEN = Color(0xFF3FE07A)
private val E_AMBER = Color(0xFFFFC21A)
private val E_RED = Color(0xFFFF3B3B)
private val E_FILL = Color(0xFF4A5876)

@Composable
private fun boeingSkin() = panelSkin(AircraftProfile.B737).copy(
    plate = B_PLATE, plateEdge = B_PLATE_EDGE, key = B_KEY, keyText = B_LABEL, lampOn = B_LAMP, lampOff = Color(0x26FFFFFF),
    legend = B_LEGEND, window = B_WINDOW, windowText = B_DIGITS, windowLabel = B_LEGEND,
    knobOuter = Color(0xFFBFB9A8), knobInner = Color(0xFFD9D3C2), knobMark = Color(0xFF2A2A2A),
)

@Composable
private fun BoeingPlate(title: String, modifier: Modifier = Modifier, plate: Color = B_PLATE, titleColor: Color = B_LEGEND, content: @Composable () -> Unit) {
    Column(modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(plate).border(1.dp, B_PLATE_EDGE, RoundedCornerShape(10.dp)).padding(10.dp)) {
        Text(title, fontWeight = FontWeight.Bold, fontSize = 11.sp, color = titleColor, modifier = Modifier.padding(bottom = 8.dp))
        content()
    }
}

/** Okienko wyświetlacza: czarne tło, cyfry w kolorze [color], opcjonalnie dotknięcie = wpisanie wartości. */
@Composable
private fun BWindow(value: String, color: Color, modifier: Modifier = Modifier, size: Int = 22, desc: String, onClick: (() -> Unit)? = null) {
    Box(
        modifier.clip(RoundedCornerShape(4.dp)).background(B_WINDOW).border(1.dp, Color(0xFF15191C), RoundedCornerShape(4.dp))
            .semantics { contentDescription = desc }
            .let { if (onClick != null) it.clickable(onClickLabel = loc("Wpisz wartość", "Type a value"), onClick = onClick) else it }
            .padding(horizontal = 8.dp, vertical = 4.dp),
        contentAlignment = Alignment.Center,
    ) { Text(value, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = size.sp, color = color, maxLines = 1, softWrap = false) }
}

/** Klawisz z powtarzaniem przy przytrzymaniu (kółko V/S, trymer): 400 ms, potem co 90 ms. */
@Composable
private fun BHoldKey(label: String, desc: String, modifier: Modifier = Modifier, action: () -> Unit) {
    val scope = rememberCoroutineScope()
    val act by rememberUpdatedState(action)
    val haptic = LocalHapticFeedback.current
    Box(
        modifier.height(40.dp).clip(RoundedCornerShape(4.dp)).background(B_KEY).border(1.dp, Color(0x40FFFFFF), RoundedCornerShape(4.dp))
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
    ) { Text(label, fontWeight = FontWeight.Bold, fontSize = 12.sp, color = B_LABEL) }
}

/** Przełącznik dźwigienkowy (A/T ARM, F/D, światła): napis nad nim, pozycja ON / OFF. */
@Composable
private fun ToggleSwitch(label: String, on: Boolean, modifier: Modifier = Modifier, legend: Color = B_LEGEND, onClick: () -> Unit) {
    val haptic = LocalHapticFeedback.current
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally) {
        Text(label, fontWeight = FontWeight.Bold, fontSize = 10.sp, color = legend, maxLines = 1)
        Box(
            Modifier.padding(top = 3.dp).size(width = 30.dp, height = 46.dp).clip(RoundedCornerShape(15.dp)).background(Color(0xFF15181A))
                .border(1.dp, Color(0x55FFFFFF), RoundedCornerShape(15.dp))
                .semantics { contentDescription = label + if (on) loc(", włączone", ", on") else loc(", wyłączone", ", off"); role = Role.Switch }
                .clickable { haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick() },
        ) {
            // Dźwigienka: na górze = ON, na dole = OFF
            Box(Modifier.align(if (on) Alignment.TopCenter else Alignment.BottomCenter).padding(4.dp).size(22.dp)
                .clip(RoundedCornerShape(11.dp)).background(Color(0xFFD8D8D0)))
        }
        Text(if (on) "ON" else "OFF", fontSize = 9.sp, color = if (on) B_LAMP else B_LABEL.copy(alpha = 0.6f))
    }
}

/** Pozycja dźwigni (klapy, spoilery): podświetlona, gdy dźwignia w niej stoi. */
@Composable
private fun BDetentKey(label: String, on: Boolean, modifier: Modifier = Modifier, desc: String = label, onClick: () -> Unit) {
    val haptic = LocalHapticFeedback.current
    Box(
        modifier.height(40.dp).clip(RoundedCornerShape(4.dp)).background(if (on) B_LABEL else B_KEY).border(1.dp, Color(0x40FFFFFF), RoundedCornerShape(4.dp))
            .semantics { contentDescription = desc + if (on) loc(", ustawione", ", set") else ""; role = Role.RadioButton }
            .clickable { haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick() },
        contentAlignment = Alignment.Center,
    ) { Text(label, fontWeight = FontWeight.Bold, fontSize = 11.sp, color = if (on) Color.Black else B_LABEL, maxLines = 1) }
}

// ---------------- MCP ----------------

/** Kremowoszare pokrętła MCP i ciemna tuleja wokół nich. */
private val MCP_KNOB_HI = Color(0xFFEDEBE4)
private val MCP_KNOB_MID = Color(0xFFBDB9AE)
private val MCP_KNOB_LO = Color(0xFF7D7A72)

private enum class McpKnob { IAS, HDG, ALT }

/** Rozmiar pokręteł i tryb ściśnięty: jeden rząd już od ok. 680 dp (fold, węższy tablet) — węższe klawisze i cyfry. */
private data class McpSize(val knob: Dp, val dense: Boolean = false) {
    fun key(full: Dp, dense: Dp) = if (this.dense) dense else full
}

/**
 * MCP 737 w układzie z kabiny. Tablet: jeden rząd jak na panelu — A/T ARM i F/D, IAS/MACH (N1, SPEED, LVL CHG),
 * HEADING z pierścieniem ogranicznika przechylenia (HDG SEL, obok LNAV, VOR LOC, APP), ALTITUDE (ALT HLD, V/S),
 * VERT SPEED z kółkiem przewijanym palcem w pionie, A/P ENGAGE (CMD A) i belka DISENGAGE. Fold i węższy tablet: ten sam rząd,
 * ściśnięty. Telefon: te same grupy na trzech piętrach.
 * Każde pokrętło ma swój kształt. Pominięte (bez odpowiednika w Link): COURSE, VNAV, C/O, SPD INTV, ALT INTV,
 * ogranicznik przechylenia (tylko rysunek), CMD B, CWS.
 */
@Composable
fun BoeingMcp(link: LinkClient, s: FlightState, compact: Boolean = false) {
    val ap = s.autopilot
    val m = ap.modes
    var enter by remember { mutableStateOf<String?>(null) }
    fun cmd(n: String, v: Number? = null) { link.command(n, v?.toDouble()) }
    fun live() = link.state.value?.autopilot
    val skin = boeingSkin()

    val spdV = ap.spdKt?.let { "${it.roundToInt()}" } ?: "---"
    val hdgV = ap.hdgDeg?.let { "%03d".format(it.roundToInt() % 360) } ?: "---"
    val altV = ap.altFt?.let { "${it.roundToInt()}" } ?: "-----"
    val vsV = ap.vsFpm?.let { "%+d".format(it.roundToInt()) } ?: ""

    val switches: @Composable (Boolean, Modifier) -> Unit = { stacked, mod ->
        val both: @Composable () -> Unit = {
            ToggleSwitch("A/T ARM", ap.athr) { cmd("AUTO_THROTTLE_ARM") }
            ToggleSwitch("F/D", ap.fd) { cmd("TOGGLE_FLIGHT_DIRECTOR") }
        }
        if (stacked) Column(mod, verticalArrangement = Arrangement.spacedBy(6.dp), horizontalAlignment = Alignment.CenterHorizontally) { both() }
        else Row(mod, horizontalArrangement = Arrangement.spacedBy(10.dp)) { both() }
    }
    val ias: @Composable (McpSize, Modifier) -> Unit = { sz, mod ->
        Column(mod, horizontalAlignment = Alignment.CenterHorizontally) {
            McpWindow("IAS/MACH", "IAS\nMACH", spdV, sz.dense) { enter = "SPD" }
            Row(Modifier.padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                McpKnobView(McpKnob.IAS, sz.knob, "IAS/MACH $spdV",
                    { d -> cmd("AP_SPD_VAR_SET", ControlMath.speed(live()?.spdKt, d)) }, { d -> cmd("AP_SPD_VAR_SET", ControlMath.speed(live()?.spdKt, d * 10)) }) { enter = "SPD" }
                AfdsKey("LVL CHG", m.flc, Modifier.width(sz.key(64.dp, 56.dp)), skin) { cmd("FLIGHT_LEVEL_CHANGE") }
            }
            Row(Modifier.padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                AfdsKey("N1", false, Modifier.width(sz.key(56.dp, 44.dp)), skin) { cmd("AP_N1_HOLD") }
                AfdsKey("SPEED", false, Modifier.width(sz.key(64.dp, 52.dp)), skin) { cmd("AP_PANEL_SPEED_HOLD") }
            }
        }
    }
    val hdg: @Composable (McpSize, Modifier) -> Unit = { sz, mod ->
        Row(mod, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Column(Modifier.weight(1f), horizontalAlignment = Alignment.CenterHorizontally) {
                McpWindow("HEADING", "HDG\nTRK", hdgV, sz.dense) { enter = "HDG" }
                Box(Modifier.padding(top = 8.dp)) {
                    McpKnobView(McpKnob.HDG, sz.knob, "HEADING $hdgV",
                        { d -> cmd("HEADING_BUG_SET", ControlMath.heading(live()?.hdgDeg, d)) }, { d -> cmd("HEADING_BUG_SET", ControlMath.heading(live()?.hdgDeg, d * 10)) }) { enter = "HDG" }
                }
                AfdsKey("HDG SEL", m.hdg, Modifier.width(sz.key(72.dp, 58.dp)).padding(top = 8.dp), skin) { cmd("AP_HDG_HOLD") }
            }
            // Kolumna trybów nawigacji, jak na prawo od okienka HEADING
            Column(Modifier.width(sz.key(72.dp, 58.dp)), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                AfdsKey("LNAV", m.nav, Modifier.fillMaxWidth(), skin) { cmd("AP_NAV1_HOLD") }
                AfdsKey("VOR LOC", m.loc, Modifier.fillMaxWidth(), skin) { cmd("AP_LOC_HOLD") }
                AfdsKey("APP", m.apr, Modifier.fillMaxWidth(), skin) { cmd("AP_APR_HOLD") }
            }
        }
    }
    val alt: @Composable (McpSize, Modifier) -> Unit = { sz, mod ->
        Column(mod, horizontalAlignment = Alignment.CenterHorizontally) {
            McpWindow("ALTITUDE", null, altV, sz.dense) { enter = "ALT" }
            Box(Modifier.padding(top = 8.dp)) {
                McpKnobView(McpKnob.ALT, sz.knob, "ALTITUDE $altV",
                    { d -> cmd("AP_ALT_VAR_SET_ENGLISH", ControlMath.altitude(live()?.altFt, d * 100)) }, { d -> cmd("AP_ALT_VAR_SET_ENGLISH", ControlMath.altitude(live()?.altFt, d * 1000)) }) { enter = "ALT" }
            }
            Row(Modifier.padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                AfdsKey("ALT HLD", m.alt, Modifier.width(sz.key(68.dp, 56.dp)), skin) { cmd("AP_ALT_HOLD") }
                AfdsKey("V/S", m.vs, Modifier.width(sz.key(56.dp, 44.dp)), skin) { cmd("AP_VS_HOLD") }
            }
        }
    }
    val vs: @Composable (McpSize, Modifier) -> Unit = { sz, mod ->
        Column(mod, horizontalAlignment = Alignment.CenterHorizontally) {
            McpWindow("VERT SPEED", "V/S\nFPA", vsV, sz.dense) { enter = "VS" }
            VsWheel(sz.knob * 1.25f, Modifier.padding(top = 8.dp)) { d -> cmd("AP_VS_VAR_SET_ENGLISH", ControlMath.verticalSpeed(live()?.vsFpm, d)) }
        }
    }
    val engage: @Composable (Boolean, Modifier) -> Unit = { dense, mod ->
        Column(mod, horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("A/P ENGAGE", fontWeight = FontWeight.Bold, fontSize = 10.sp, color = B_LEGEND)
            AfdsKey("CMD", ap.master, Modifier.width(if (dense) 54.dp else 64.dp), skin) { cmd("AP_MASTER") }
            Box(
                Modifier.size(width = if (dense) 84.dp else 96.dp, height = 34.dp).clip(RoundedCornerShape(3.dp)).background(Color(0xFFE6E0CC))
                    .semantics { contentDescription = loc("Rozłącz autopilota", "Disengage autopilot"); role = Role.Button }
                    .clickable { cmd("AUTOPILOT_OFF") },
                contentAlignment = Alignment.Center,
            ) { Text("DISENGAGE", fontWeight = FontWeight.Bold, fontSize = 10.sp, color = Color(0xFF3A3A3A)) }
        }
    }

    BoxWithConstraints(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(B_PLATE).border(2.dp, B_PLATE_EDGE, RoundedCornerShape(10.dp)).padding(10.dp),
    ) {
        if (maxWidth >= 680.dp) {
            // Tablet i fold: cały panel w jednym rzędzie; poniżej 840 dp ściśnięty (przełączniki jeden pod drugim)
            val dense = maxWidth < 840.dp
            val sz = McpSize(if (dense) 52.dp else (maxWidth / 12f).coerceIn(56.dp, 76.dp), dense)
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(if (dense) 8.dp else 10.dp)) {
                switches(dense, Modifier.padding(top = if (dense) 0.dp else 18.dp))
                ias(sz, Modifier.weight(1f))
                hdg(sz, Modifier.weight(1.4f))
                alt(sz, Modifier.weight(if (dense) 0.95f else 1f))
                vs(sz, Modifier.weight(if (dense) 0.85f else 0.8f))
                engage(dense, Modifier.padding(top = 4.dp))
            }
        } else {
            // Telefon: przełączniki i A/P ENGAGE, potem IAS/MACH i ALTITUDE, HEADING, na dole VERT SPEED
            val sz = McpSize(if (compact) 60.dp else ((maxWidth - 40.dp) / 4.2f).coerceIn(60.dp, 80.dp))
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Top) { switches(false, Modifier.weight(1f)); engage(false, Modifier) }
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp)) { ias(sz, Modifier.weight(1f)); alt(sz, Modifier.weight(1f)) }
                hdg(sz, Modifier.fillMaxWidth())
                vs(sz, Modifier.fillMaxWidth())
            }
        }
    }
    if (!compact) Text(loc("Kręć pokrętłem: brzeg — duży krok, środek — mały. Kółko V/S przewijaj palcem w górę (DN) albo w dół (UP). Dotknij cyfr, aby wpisać wartość.",
        "Turn a knob: the edge for big steps, the centre for small ones. Roll the V/S wheel up (DN) or down (UP). Tap the digits to type a value."),
        style = MaterialTheme.typography.labelSmall, color = Fm.c.ink2, modifier = Modifier.padding(top = 8.dp))

    when (enter) {
        "HDG" -> EnterDialog("HEADING", loc("np. 270", "e.g. 270"), KeyboardType.Number, { it.toIntOrNull() in 0..359 }, { cmd("HEADING_BUG_SET", it.toInt()) }) { enter = null }
        "ALT" -> EnterDialog("ALTITUDE (ft)", loc("np. 12000", "e.g. 12000"), KeyboardType.Number, { it.toIntOrNull() in 0..50_000 }, { cmd("AP_ALT_VAR_SET_ENGLISH", it.toInt()) }) { enter = null }
        "SPD" -> EnterDialog("IAS (kt)", loc("np. 250", "e.g. 250"), KeyboardType.Number, { it.toIntOrNull() in 0..500 }, { cmd("AP_SPD_VAR_SET", it.toInt()) }) { enter = null }
        "VS" -> EnterDialog("VERT SPEED (ft/min)", loc("np. -1500", "e.g. -1500"), KeyboardType.Number, { it.toIntOrNull() in -8000..8000 }, { cmd("AP_VS_VAR_SET_ENGLISH", it.toInt()) }) { enter = null }
    }
}

/** Okienko MCP: pomarańczowy tytuł nad nim, w środku mały biały opis z lewej (IAS/MACH, HDG/TRK) i białe cyfry. */
@Composable
private fun McpWindow(title: String, inner: String?, value: String, dense: Boolean = false, onClick: () -> Unit) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text(title, fontWeight = FontWeight.Bold, fontSize = 10.sp, color = B_LEGEND, maxLines = 1)
        Row(
            Modifier.padding(top = 3.dp).clip(RoundedCornerShape(4.dp)).background(B_WINDOW).border(2.dp, Color(0xFF15191C), RoundedCornerShape(4.dp))
                .semantics { contentDescription = "$title $value" }
                .clickable(onClickLabel = loc("Wpisz wartość", "Type a value"), onClick = onClick)
                .padding(horizontal = if (dense) 6.dp else 8.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (inner != null) Text(inner, fontSize = 7.sp, lineHeight = 8.sp, color = B_DIGITS.copy(alpha = 0.8f), modifier = Modifier.padding(end = if (dense) 4.dp else 6.dp))
            Text(value.ifEmpty { " " }.padStart(5), fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = if (dense) 18.sp else 22.sp, color = B_DIGITS, maxLines = 1, softWrap = false)
        }
    }
}

/**
 * Pokrętło MCP — każde inne, jak na 737: IAS — mniejsze, z karbowaniem; HEADING — z zewnętrznym pierścieniem
 * ogranicznika przechylenia (10–30) i białą kreską; ALTITUDE — największe, gładkie, z głębokimi żebrami.
 */
@Composable
private fun McpKnobView(kind: McpKnob, size: Dp, desc: String, onStep: (Int) -> Unit, onBig: (Int) -> Unit, onTap: () -> Unit) {
    RotaryKnob(size, loc("Pokrętło $desc", "$desc knob"), onStep, onBig, onTap) { angle, grip ->
        val c = center
        val r = this.size.minDimension / 2f
        val body = when (kind) { McpKnob.IAS -> r * 0.72f; McpKnob.HDG -> r * 0.6f; McpKnob.ALT -> r * 0.86f }
        // Pierścień ogranicznika przechylenia wokół HEADING: podziałka 10–30 na płycie i ciemny kołnierz
        if (kind == McpKnob.HDG) {
            drawCircle(Color(0xFF2A2F33), r * 0.96f, c)
            drawCircle(B_LEGEND.copy(alpha = 0.8f), r * 0.96f, c, style = Stroke(2f))
            for (k in 0..4) {
                val a = Math.toRadians(-150.0 + k * 30.0).toFloat()
                drawLine(B_LEGEND, Offset(c.x + r * 0.8f * cos(a), c.y + r * 0.8f * sin(a)), Offset(c.x + r * 0.95f * cos(a), c.y + r * 0.95f * sin(a)), 2.5f)
            }
            drawCircle(MCP_KNOB_MID, r * 0.74f, c, style = Stroke(r * 0.1f))
        } else drawCircle(Color(0xFF1C2023), body * 1.12f, c)
        drawCircle(androidx.compose.ui.graphics.Brush.radialGradient(listOf(MCP_KNOB_HI, MCP_KNOB_MID, MCP_KNOB_LO),
            Offset(c.x - body * 0.35f, c.y - body * 0.45f), body * 1.6f), body, c)
        drawCircle(if (grip != 0) B_LAMP else Color(0x55000000), body, c, style = Stroke(if (grip != 0) 3f else 1.5f))
        rotate(angle, c) {
            val ribs = when (kind) { McpKnob.IAS -> 40; McpKnob.HDG -> 24; McpKnob.ALT -> 16 }
            val depth = when (kind) { McpKnob.ALT -> 0.7f; else -> 0.86f }
            for (i in 0 until ribs) rotate(i * 360f / ribs, c) {
                drawLine(Color(0x55000000), Offset(c.x, c.y - body * 0.98f), Offset(c.x, c.y - body * depth), if (kind == McpKnob.ALT) 3.5f else 1.5f)
            }
            if (kind == McpKnob.ALT) drawCircle(MCP_KNOB_HI.copy(alpha = 0.6f), body * 0.6f, c)
            drawLine(Color(0xFF2A2A2A), Offset(c.x, c.y - body * 0.82f), Offset(c.x, c.y - body * 0.35f), 4f, cap = StrokeCap.Round)
        }
    }
}

/**
 * Kółko VERT SPEED: pionowy bęben z żebrami między strzałkami DN (u góry) i UP (u dołu). Przewijanie palcem w górę
 * = DN (−100 ft/min co 14 dp), w dół = UP; dotknięcie strzałki = jeden krok.
 */
@Composable
private fun VsWheel(height: Dp, modifier: Modifier = Modifier, onDelta: (Int) -> Unit) {
    val haptic = LocalHapticFeedback.current
    val delta by rememberUpdatedState(onDelta)
    var offset by remember { mutableStateOf(0f) }
    Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Canvas(
            Modifier.size(width = 34.dp, height = height)
                .semantics {
                    contentDescription = loc("Kółko prędkości pionowej", "Vertical speed wheel")
                    customActions = listOf(
                        androidx.compose.ui.semantics.CustomAccessibilityAction("UP") { delta(100); true },
                        androidx.compose.ui.semantics.CustomAccessibilityAction("DN") { delta(-100); true },
                    )
                }
                .pointerInput(Unit) {
                    val stepPx = 14.dp.toPx()
                    awaitEachGesture {
                        val down = awaitFirstDown()
                        down.consume()
                        var acc = 0f
                        var lastY = down.position.y
                        while (true) {
                            val ev = awaitPointerEvent()
                            val ch = ev.changes.firstOrNull { it.id == down.id } ?: break
                            if (!ch.pressed) break
                            ch.consume()
                            val dy = ch.position.y - lastY
                            lastY = ch.position.y
                            acc += dy; offset += dy
                            while (acc >= stepPx || acc <= -stepPx) {
                                val dir = if (acc > 0) 1 else -1
                                acc -= dir * stepPx
                                // Palec w dół = UP (więcej), w górę = DN (mniej)
                                delta(dir * 100)
                                haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                            }
                        }
                    }
                },
        ) {
            val w = size.width; val h = size.height
            drawRoundRect(Color(0xFF1C2023), Offset.Zero, Size(w, h), androidx.compose.ui.geometry.CornerRadius(6f))
            val drumW = w * 0.62f; val x0 = (w - drumW) / 2
            drawRoundRect(androidx.compose.ui.graphics.Brush.horizontalGradient(listOf(MCP_KNOB_LO, MCP_KNOB_HI, MCP_KNOB_LO), x0, x0 + drumW),
                Offset(x0, h * 0.06f), Size(drumW, h * 0.88f), androidx.compose.ui.geometry.CornerRadius(drumW / 2))
            // Żebra bębna przesuwają się razem z palcem
            val gap = 10.dp.toPx()
            var y = h * 0.08f + ((offset % gap) + gap) % gap
            while (y < h * 0.92f) { drawLine(Color(0x66000000), Offset(x0 + 3f, y), Offset(x0 + drumW - 3f, y), 2f); y += gap }
        }
        Column(verticalArrangement = Arrangement.SpaceBetween, modifier = Modifier.height(height)) {
            Text("DN ▲", fontWeight = FontWeight.Bold, fontSize = 10.sp, color = B_LEGEND,
                modifier = Modifier.clickable(onClickLabel = "DN") { delta(-100) }.padding(4.dp))
            Text("UP ▼", fontWeight = FontWeight.Bold, fontSize = 10.sp, color = B_LEGEND,
                modifier = Modifier.clickable(onClickLabel = "UP") { delta(100) }.padding(4.dp))
        }
    }
}
// ---------------- Radio i transponder ----------------

private enum class BoeingRadio(val label: String, val com: Boolean) { VHF1("VHF 1", true), VHF2("VHF 2", true), NAV1("NAV 1", false), NAV2("NAV 2", false) }

/**
 * Panel radiowy 737 (szary, pomarańczowe cyfry): ACTIVE i STANDBY, przycisk zamiany, wybór VHF 1 / VHF 2 z lampką nad klawiszem
 * oraz NAV 1 / NAV 2 (na 737 osobne panele NAV — tu w tym samym panelu). Pokrętło: pierścień 1 MHz, środek 25 kHz (VHF) albo 50 kHz (NAV).
 */
@Composable
fun BoeingRadioPanel(link: LinkClient, s: FlightState) {
    var selName by rememberSaveable { mutableStateOf(BoeingRadio.VHF1.name) }
    val sel = BoeingRadio.valueOf(selName)
    var enter by remember { mutableStateOf(false) }
    fun cmd(n: String, v: Number? = null) { link.command(n, v?.toDouble()) }
    fun pick(r: com.freemetar.app.data.link.Radios?): Radio? = when (sel) { BoeingRadio.VHF1 -> r?.com1; BoeingRadio.VHF2 -> r?.com2; BoeingRadio.NAV1 -> r?.nav1; BoeingRadio.NAV2 -> r?.nav2 }
    val radio = pick(s.radios)
    val fmt = if (sel.com) "%.3f" else "%.2f"
    val stbyEvent = when (sel) { BoeingRadio.VHF1 -> "COM_STBY_RADIO_SET_HZ"; BoeingRadio.VHF2 -> "COM2_STBY_RADIO_SET_HZ"; BoeingRadio.NAV1 -> "NAV1_STBY_SET_HZ"; BoeingRadio.NAV2 -> "NAV2_STBY_SET_HZ" }
    val swap = when (sel) { BoeingRadio.VHF1 -> "COM_STBY_RADIO_SWAP"; BoeingRadio.VHF2 -> "COM2_RADIO_SWAP"; BoeingRadio.NAV1 -> "NAV1_RADIO_SWAP"; BoeingRadio.NAV2 -> "NAV2_RADIO_SWAP" }
    val tuner = remember { StbyTuner() }
    fun setStby(stepKhz: Int) {
        val khz = tuner.step(sel.name, pick(link.state.value?.radios)?.standbyMhz, stepKhz, sel.com) ?: return
        cmd(stbyEvent, khz.toLong() * 1000)
    }
    val active = radio?.activeMhz?.let { fmt.format(java.util.Locale.US, it) } ?: "---.---"
    val stby = tuner.shown(sel.name, radio?.standbyMhz)?.let { fmt.format(java.util.Locale.US, it) } ?: "---.---"

    BoeingPlate(if (sel.com) "VHF COMM" else "NAV", plate = B_RMP_PLATE, titleColor = B_LABEL) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Column(Modifier.weight(1f), horizontalAlignment = Alignment.CenterHorizontally) {
                Text("ACTIVE", fontSize = 12.sp, color = B_LABEL)
                BWindow(active, B_AMBER_DIGITS, Modifier.fillMaxWidth().padding(top = 3.dp), desc = "${sel.label} ACTIVE $active")
            }
            Box(
                Modifier.size(width = 46.dp, height = 38.dp).clip(RoundedCornerShape(6.dp)).background(B_KEY)
                    .semantics { contentDescription = loc("Zamień aktywną i zapasową ${sel.label}", "Swap active and standby ${sel.label}") }
                    .clickable(role = Role.Button) { cmd(swap) },
                contentAlignment = Alignment.Center,
            ) { Icon(Icons.Outlined.SwapHoriz, null, tint = B_LABEL, modifier = Modifier.size(26.dp)) }
            Column(Modifier.weight(1f), horizontalAlignment = Alignment.CenterHorizontally) {
                Text("STANDBY", fontSize = 12.sp, color = B_LABEL)
                BWindow(stby, B_AMBER_DIGITS, Modifier.fillMaxWidth().padding(top = 3.dp), desc = "${sel.label} STANDBY $stby") { enter = true }
            }
        }
        Spacer(Modifier.height(12.dp))
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                listOf(BoeingRadio.VHF1 to BoeingRadio.VHF2, BoeingRadio.NAV1 to BoeingRadio.NAV2).forEach { (a, b) ->
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        listOf(a, b).forEach { r -> RadioSelectKey(r.label, sel == r, Modifier.weight(1f)) { selName = r.name } }
                    }
                }
            }
            DualKnob(80.dp, loc("Pokrętło częstotliwości ${sel.label}", "${sel.label} frequency knob"),
                skin = boeingSkin().copy(knobOuter = Color(0xFFC8CBCD), knobInner = Color(0xFFE2E4E5)),
                onStep = { setStby(it * if (sel.com) 25 else 50) }, onBigStep = { setStby(it * 1000) }, onPush = { enter = true })
        }
    }
    if (enter) {
        val valid: (String) -> Boolean = { t -> ControlMath.parseMhz(t)?.let { if (sel.com) ControlMath.comHz(it) else ControlMath.navHz(it) } != null }
        EnterDialog(loc("${sel.label} — częstotliwość zapasowa (MHz)", "${sel.label} — standby frequency (MHz)"), if (sel.com) "np. 121.500" else "np. 110.30", KeyboardType.Decimal, valid,
            { t -> ControlMath.parseMhz(t)?.let { if (sel.com) ControlMath.comHz(it) else ControlMath.navHz(it) }?.let { hz -> cmd(stbyEvent, hz) } }) { enter = false }
    }
}

/** Klawisz wyboru radia z podłużną lampką nad nim (świeci na bursztynowo, gdy radio wybrane). */
@Composable
private fun RadioSelectKey(label: String, on: Boolean, modifier: Modifier, onClick: () -> Unit) {
    val haptic = LocalHapticFeedback.current
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally) {
        Box(Modifier.width(34.dp).height(5.dp).clip(RoundedCornerShape(3.dp)).background(if (on) B_AMBER_DIGITS else Color(0xFF4A4F53)))
        Box(
            Modifier.padding(top = 4.dp).fillMaxWidth().height(36.dp).clip(RoundedCornerShape(5.dp)).background(B_KEY)
                .semantics { contentDescription = label + if (on) loc(", wybrane", ", selected") else ""; role = Role.RadioButton }
                .clickable { haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove); onClick() },
            contentAlignment = Alignment.Center,
        ) { Text(label, fontWeight = FontWeight.Bold, fontSize = 12.sp, color = Color(0xFFF2D7B0)) }
    }
}

/**
 * Panel ATC 737: cztery cyfry kodu przestawiane osobnymi klawiszami (jak dwa podwójne pokrętła — każde dotknięcie +1, 0–7),
 * przełącznik trybu STBY / ALT RPTG OFF / XPNDR i klawisz IDENT.
 */
@Composable
fun BoeingAtc(link: LinkClient, s: FlightState) {
    val x = s.radios.transponder
    fun cmd(n: String, v: Number? = null) { link.command(n, v?.toDouble()) }
    val code = x?.code?.takeIf { it.length == 4 } ?: "0000"
    val emergency = code in setOf("7500", "7600", "7700")

    BoeingPlate("ATC") {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            code.forEachIndexed { i, ch ->
                Column(Modifier.weight(1f), horizontalAlignment = Alignment.CenterHorizontally) {
                    BWindow("$ch", if (emergency) E_RED else B_DIGITS, Modifier.fillMaxWidth(), size = 24, desc = loc("Cyfra ${i + 1}: $ch", "Digit ${i + 1}: $ch"))
                    Row(Modifier.padding(top = 4.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        // −/+ dla cyfry; zawija w zakresie 0–7 i wysyła cały kod
                        listOf(-1 to "−", 1 to "+").forEach { (d, sign) ->
                            Box(
                                Modifier.size(30.dp).clip(RoundedCornerShape(15.dp)).background(B_KEY)
                                    .semantics { contentDescription = loc("Cyfra ${i + 1} $sign", "Digit ${i + 1} $sign") }
                                    .clickable(role = Role.Button) {
                                        val digits = code.map { it - '0' }.toMutableList()
                                        digits[i] = (digits[i] + d + 8) % 8
                                        cmd("XPNDR_SET", digits.joinToString("").toInt())
                                    },
                                contentAlignment = Alignment.Center,
                            ) { Text(sign, fontWeight = FontWeight.Bold, fontSize = 16.sp, color = B_LABEL) }
                        }
                    }
                }
            }
        }
        Spacer(Modifier.height(10.dp))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            listOf("STBY" to ("standby" to 1), "ALT RPTG OFF" to ("on" to 3), "XPNDR" to ("alt" to 4)).forEach { (label, mv) ->
                BDetentKey(label, x?.mode == mv.first, Modifier.weight(1f), loc("Tryb transpondera $label", "Transponder mode $label")) { cmd("XPNDR_MODE", mv.second) }
            }
            Box(
                Modifier.weight(0.8f).height(40.dp).clip(RoundedCornerShape(4.dp)).background(B_KEY)
                    .border(1.dp, if (x?.ident == true) B_LAMP else Color(0x40FFFFFF), RoundedCornerShape(4.dp))
                    .clickable(role = Role.Button, onClickLabel = "IDENT") { cmd("XPNDR_IDENT_ON") },
                contentAlignment = Alignment.Center,
            ) { Text("IDENT", fontWeight = FontWeight.Bold, fontSize = 11.sp, color = if (x?.ident == true) B_LAMP else B_LABEL) }
        }
    }
}

// ---------------- Systemy ----------------

/**
 * Systemy 737: oświetlenie z nadgłowia (LANDING, TAXI, LOGO, POSITION, STROBE, ANTI COLLISION, WING, WHEEL WELL — tu bez niego),
 * dźwignia podwozia z lampkami, klapy 0–40, dźwignia hamulca aerodynamicznego (DOWN / ARMED / FLT DET / UP), hamulec postojowy i trymer stabilizatora.
 */
@Composable
fun BoeingSystems(link: LinkClient, s: FlightState) {
    fun cmd(n: String, v: Number? = null) { link.command(n, v?.toDouble()) }
    val ctl = s.controls
    val l = s.lights
    var confirmGear by remember { mutableStateOf(false) }

    BoeingPlate("LIGHTS") {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
            ToggleSwitch("LANDING", l.landing == true, legend = B_LABEL) { cmd("LANDING_LIGHTS_TOGGLE") }
            ToggleSwitch("TAXI", l.taxi == true, legend = B_LABEL) { cmd("TOGGLE_TAXI_LIGHTS") }
            ToggleSwitch("LOGO", l.logo == true, legend = B_LABEL) { cmd("TOGGLE_LOGO_LIGHTS") }
            ToggleSwitch("WING", l.wing == true, legend = B_LABEL) { cmd("TOGGLE_WING_LIGHTS") }
        }
        Spacer(Modifier.height(8.dp))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
            ToggleSwitch("POSITION", l.nav == true, legend = B_LABEL) { cmd("TOGGLE_NAV_LIGHTS") }
            ToggleSwitch("STROBE", l.strobe == true, legend = B_LABEL) { cmd("STROBES_TOGGLE") }
            ToggleSwitch("ANTI COLL", l.beacon == true, legend = B_LABEL) { cmd("TOGGLE_BEACON_LIGHTS") }
            ToggleSwitch("CABIN", l.cabin == true, legend = B_LABEL) { cmd("TOGGLE_CABIN_LIGHTS") }
        }
    }
    Spacer(Modifier.height(10.dp))

    // Podwozie: trzy zielone lampki (zablokowane w dole), czerwone w trakcie ruchu; dźwignia UP / DN
    val gearDown = ctl.gearDown == true
    val moving = ctl.gearPct?.let { it > 1 && it < 99 } == true
    BoeingPlate("LANDING GEAR") {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(Modifier.weight(1f), horizontalArrangement = Arrangement.SpaceEvenly) {
                listOf("NOSE", "LEFT", "RIGHT").forEach { g ->
                    Text(g, fontWeight = FontWeight.Bold, fontSize = 10.sp, textAlign = TextAlign.Center,
                        color = if (moving || gearDown) Color.Black else B_LABEL.copy(alpha = 0.4f),
                        modifier = Modifier.width(54.dp).clip(RoundedCornerShape(3.dp))
                            .background(if (moving) E_RED else if (gearDown) E_GREEN else Color(0xFF1A1D1F)).padding(vertical = 8.dp))
                }
            }
            BDetentKey(if (gearDown) "DN" else "UP", true, Modifier.width(80.dp), loc("Dźwignia podwozia", "Gear lever")) {
                if (gearDown && s.aircraft.onGround == true) confirmGear = true else cmd("GEAR_TOGGLE")
            }
        }
    }
    Spacer(Modifier.height(10.dp))

    // Klapy 737: 0, 1, 2, 5, 10, 15, 25, 30, 40 (9 pozycji); przy innej liczbie — numery pozycji
    val positions = ctl.flapsPositions?.takeIf { it > 1 }
    BoeingPlate("FLAPS") {
        if (positions == null) Text("—", color = B_LABEL)
        else {
            val names = if (positions == 9) listOf("UP", "1", "2", "5", "10", "15", "25", "30", "40") else (0 until positions).map { "$it" }
            names.chunked(5).forEachIndexed { row, chunk ->
                Row(Modifier.fillMaxWidth().padding(bottom = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    chunk.forEachIndexed { j, name ->
                        val i = row * 5 + j
                        BDetentKey(name, ctl.flapsIndex == i, Modifier.weight(1f), loc("Klapy $name", "Flaps $name")) { cmd("FLAPS_SET", (16383.0 * i / (positions - 1)).roundToInt()) }
                    }
                    repeat(5 - chunk.size) { Spacer(Modifier.weight(1f)) }
                }
            }
        }
    }
    Spacer(Modifier.height(10.dp))

    // Hamulec aerodynamiczny: ARMED (uzbrojenie do lądowania) i obok oś dźwigni
    BoeingPlate("SPEED BRAKE") {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            BDetentKey("ARMED", ctl.spoilersArmed == true, Modifier.width(78.dp), loc("Spoilery uzbrojone", "Speed brake armed")) { cmd("SPOILERS_ARM_TOGGLE") }
            SpeedBrakeAxis(link, s, Modifier.weight(1f), label = "SPEED BRAKE", labelColor = B_LEGEND)
        }
    }
    Spacer(Modifier.height(10.dp))

    // Hamulec postojowy (mały klawisz z czerwoną lampką) i obok oś hamulców kół
    BoeingPlate("PARKING BRAKE / BRAKES") {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Box(Modifier.size(14.dp).clip(RoundedCornerShape(7.dp)).background(if (ctl.parkingBrake == true) E_RED else Color(0xFF3A1A1A)))
            BDetentKey(if (ctl.parkingBrake == true) "SET" else "OFF", ctl.parkingBrake == true, Modifier.width(64.dp), loc("Hamulec postojowy", "Parking brake")) { cmd("PARKING_BRAKES") }
            WheelBrakeAxis(link, s, Modifier.weight(1f), labelColor = B_LEGEND)
        }
    }
    Spacer(Modifier.height(10.dp))

    BoeingPlate("STAB TRIM") {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            BHoldKey("NOSE DN", loc("Trym nosem w dół", "Trim nose down"), Modifier.weight(1f)) { cmd("ELEV_TRIM_DN") }
            BWindow(ctl.elevatorTrimPct?.let { "%+.1f".format(it) } ?: "--", B_DIGITS, Modifier.weight(1f), size = 15, desc = loc("Trym", "Trim"))
            BHoldKey("NOSE UP", loc("Trym nosem w górę", "Trim nose up"), Modifier.weight(1f)) { cmd("ELEV_TRIM_UP") }
        }
    }

    if (confirmGear) AlertDialog(
        onDismissRequest = { confirmGear = false },
        title = { Text(loc("Schować podwozie na ziemi?", "Retract the gear on the ground?")) },
        text = { Text(loc("Samolot stoi na ziemi. Schowanie podwozia może go uszkodzić w symulatorze.", "The aircraft is on the ground. Retracting the gear may damage it in the simulator.")) },
        confirmButton = { TextButton({ cmd("GEAR_UP"); confirmGear = false }) { Text(loc("Schowaj", "Retract"), color = Fm.c.danger) } },
        dismissButton = { TextButton({ confirmGear = false }) { Text(loc("Anuluj", "Cancel")) } },
    )
}

// ---------------- Silniki: EICAS i dźwignie ----------------

@Composable
fun BoeingEngines(link: LinkClient, s: FlightState) {
    EngineDisplayBox { BoeingEicas(s) }
    Spacer(Modifier.height(12.dp))
    Quadrant(link, s, AircraftProfile.B737)
    if (s.autopilot.athr) Text(loc("A/T włączony — symulator może sam przestawiać dźwignie.", "A/T is on — the simulator may move the thrust levers itself."),
        style = MaterialTheme.typography.labelSmall, color = Fm.c.caution, modifier = Modifier.padding(top = 8.dp))
}

/**
 * Górny EICAS 737: na silnik zegar N1 (podziałka 0–10 ×10 %, wypełniony sektor do bieżącej wartości, odczyt w ramce,
 * czerwona kreska limitu i niebieski znacznik położenia dźwigni), pod nim EGT, N2 i FF; na dole FUEL QTY (L / CTR / R) i TOTAL.
 * Brak danych o oleju, drganiach i hydraulice w Link — tych zegarów nie ma.
 */
@Composable
fun BoeingEicas(s: FlightState) {
    val m = rememberTextMeasurer()
    val engines = s.engines.take(2).ifEmpty { return }
    val desc = engines.mapIndexed { i, e -> "ENG ${i + 1} N1 ${"%.1f".format(e.n1Pct ?: 0.0)}%" }.joinToString(", ")
    Canvas(Modifier.fillMaxWidth().aspectRatio(0.95f).clip(RoundedCornerShape(10.dp)).semantics { contentDescription = "EICAS: $desc" }) {
        val w = size.width; val h = size.height
        drawRect(Color.Black)
        val cxs = listOf(w * 0.27f, w * 0.71f)
        val r = w * 0.15f
        // Nagłówek: SAT (z Link przychodzi temperatura zewnętrzna)
        s.environment.oatC?.let { eText(m, "SAT %+d°C".format(it.roundToInt()), w * 0.5f, h * 0.035f, E_CYAN, 13f) }
        engines.forEachIndexed { i, e ->
            val cx = cxs[i]
            val thr = e.throttlePct ?: s.controls.throttlePct
            eDial(m, cx, h * 0.17f, r, (e.n1Pct ?: 0.0) / 110.0, 104.0 / 110.0, "%.1f".format(e.n1Pct ?: 0.0), ticks = true, cmdFrac = thr?.let { it * 1.04 / 110.0 })
            eDial(m, cx, h * 0.44f, r * 0.9f, (e.egtC ?: 0.0) / 1000.0, 0.95, "${(e.egtC ?: 0.0).roundToInt()}")
            eDial(m, cx, h * 0.66f, r * 0.85f, (e.n2Pct ?: 0.0) / 110.0, 105.0 / 110.0, "%.1f".format(e.n2Pct ?: 0.0))
            // FF w tysiącach kg/h, jak na EICAS
            eText(m, e.fuelFlowKgH?.let { "%.2f".format(it / 1000) } ?: "--", cx + r * 0.45f, h * 0.84f, E_WHITE, 16f, box = true)
        }
        eText(m, "N1", w * 0.49f, h * 0.24f, E_CYAN, 14f)
        eText(m, "EGT", w * 0.49f, h * 0.5f, E_CYAN, 14f)
        eText(m, "N2", w * 0.49f, h * 0.72f, E_CYAN, 14f)
        eText(m, "FF", w * 0.49f, h * 0.82f, E_CYAN, 13f)
        eText(m, "KG × 1000", w * 0.49f, h * 0.86f, E_CYAN, 11f)
        // FUEL QTY: lewy, środkowy, prawy zbiornik i suma
        drawLine(E_CYAN, Offset(w * 0.04f, h * 0.9f), Offset(w * 0.96f, h * 0.9f), 2f)
        val t = s.fuel.tanks
        fun kg(k: String) = t[k]?.let { "%.2f".format(it / 1000) } ?: "--"
        eText(m, "FUEL QTY — KG × 1000", w * 0.36f, h * 0.925f, E_CYAN, 11f)
        eText(m, kg("leftKg"), w * 0.14f, h * 0.965f, E_WHITE, 15f)
        eText(m, kg("centerKg"), w * 0.36f, h * 0.965f, E_WHITE, 15f)
        eText(m, kg("rightKg"), w * 0.58f, h * 0.965f, E_WHITE, 15f)
        eText(m, s.fuel.totalKg?.let { "%.1f".format(it / 1000) } ?: "--", w * 0.84f, h * 0.935f, E_WHITE, 19f)
        eText(m, "TOTAL", w * 0.84f, h * 0.975f, E_CYAN, 11f)
    }
}

private fun DrawScope.eText(m: TextMeasurer, t: String, x: Float, y: Float, col: Color, sz: Float, box: Boolean = false) {
    val l = m.measure(t, TextStyle(color = col, fontSize = sz.sp, fontFamily = Mono, fontWeight = FontWeight.Bold))
    val tl = Offset(x - l.size.width / 2f, y - l.size.height / 2f)
    if (box) drawRect(E_WHITE, Offset(tl.x - 8f, tl.y - 2f), Size(l.size.width + 16f, l.size.height + 4f), style = Stroke(2f))
    drawText(l, topLeft = tl)
}

/**
 * Zegar EICAS: łuk 210° od godziny 9 zgodnie z ruchem wskazówek, wypełniony sektor do wartości (szaroniebieski),
 * biała wskazówka, czerwona kreska limitu, odczyt w ramce w prawym górnym rogu. [cmdFrac] — znacznik położenia dźwigni.
 */
private fun DrawScope.eDial(m: TextMeasurer, cx: Float, cy: Float, r: Float, frac: Double, redFrac: Double, value: String, ticks: Boolean = false, cmdFrac: Double? = null) {
    val start = 180f; val sweep = 210f
    val f = frac.coerceIn(0.0, 1.0).toFloat()
    val tl = Offset(cx - r, cy - r); val sz = Size(r * 2, r * 2)
    // Sektor wartości
    val path = Path().apply {
        moveTo(cx, cy)
        arcTo(androidx.compose.ui.geometry.Rect(tl, sz), start, sweep * f, false)
        close()
    }
    drawPath(path, E_FILL)
    drawArc(E_WHITE, start, sweep, false, tl, sz, style = Stroke(2.5f))
    if (ticks) for (k in 0..10 step 2) {
        val a = Math.toRadians(start + sweep * (k * 10 / 110.0)).toFloat()
        drawLine(E_WHITE, Offset(cx + r * 0.86f * cos(a), cy + r * 0.86f * sin(a)), Offset(cx + r * cos(a), cy + r * sin(a)), 2f)
        eText(m, "$k", cx + r * 0.7f * cos(a), cy + r * 0.7f * sin(a), E_WHITE, 10f)
    }
    val ra = Math.toRadians(start + sweep * redFrac).toFloat()
    drawLine(E_RED, Offset(cx + r * 0.95f * cos(ra), cy + r * 0.95f * sin(ra)), Offset(cx + r * 1.15f * cos(ra), cy + r * 1.15f * sin(ra)), 5f)
    cmdFrac?.let { c ->
        val ca = Math.toRadians(start + sweep * c.coerceIn(0.0, 1.0)).toFloat()
        drawLine(E_GREEN, Offset(cx + r * 1.02f * cos(ca), cy + r * 1.02f * sin(ca)), Offset(cx + r * 1.2f * cos(ca), cy + r * 1.2f * sin(ca)), 4f, cap = StrokeCap.Round)
    }
    val na = Math.toRadians(start + sweep * f.toDouble()).toFloat()
    drawLine(E_WHITE, Offset(cx, cy), Offset(cx + r * cos(na), cy + r * sin(na)), 3.5f, cap = StrokeCap.Round)
    val over = f >= redFrac
    eText(m, value, cx + r * 0.62f, cy - r * 0.55f, if (over) E_RED else if (f >= redFrac * 0.97) E_AMBER else E_WHITE, 17f, box = true)
}
