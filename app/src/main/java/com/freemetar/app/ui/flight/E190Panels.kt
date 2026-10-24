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
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
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
import kotlin.math.cos
import kotlin.math.roundToInt
import kotlin.math.sin

// Panele Embraera E190 (Honeywell Primus Epic): czarnografitowa płyta, białe opisy, czarne pokrętła z żebrami,
// radio i transponder jak strona RADIO w MCDU (aktywne na zielono, zapasowe na cyjanowo). Tylko elementy z komendą w Link.

private val EJ_PLATE = Color(0xFF26292C)
private val EJ_PLATE_EDGE = Color(0xFF131517)
private val EJ_LEGEND = Color(0xFFEDEFF1)
private val EJ_KEY = Color(0xFF34383C)
private val EJ_LAMP = Color(0xFF52E07A)
private val EJ_KNOB = Color(0xFF151719)
private val D_GREEN = Color(0xFF3FE07A)
private val D_CYAN = Color(0xFF3FD3F2)
private val D_WHITE = Color(0xFFF2F2F2)
private val D_AMBER = Color(0xFFFFB020)
private val D_RED = Color(0xFFFF3B3B)
private val D_MAGENTA = Color(0xFFE85CE8)
private val D_FILL = Color(0xFF39D46A)

@Composable
private fun ejSkin() = panelSkin(AircraftProfile.E190).copy(
    plate = EJ_PLATE, plateEdge = EJ_PLATE_EDGE, key = EJ_KEY, keyPressed = Color(0xFF4A4F54), keyText = EJ_LEGEND,
    lampOn = EJ_LAMP, lampOff = Color(0x22FFFFFF), lamp = LampStyle.BAR_BELOW, keyCorner = 4.dp, keyHeight = 44.dp, legend = EJ_LEGEND,
)

@Composable
private fun EjPlate(title: String, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Column(modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(EJ_PLATE).border(2.dp, EJ_PLATE_EDGE, RoundedCornerShape(10.dp)).padding(10.dp)) {
        Text(title, fontWeight = FontWeight.Bold, fontSize = 11.sp, color = EJ_LEGEND.copy(alpha = 0.85f), modifier = Modifier.padding(bottom = 8.dp))
        content()
    }
}

/** Czarne pokrętło Primus z żebrami i białą kreską; [label] pod spodem jak na płycie (np. „PUSH SYNC”). */
@Composable
private fun EjKnob(size: Dp, title: String, under: String?, desc: String, onStep: (Int) -> Unit, onBig: (Int) -> Unit, onTap: () -> Unit) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text(title, fontWeight = FontWeight.Bold, fontSize = 10.sp, color = EJ_LEGEND)
        Spacer(Modifier.height(4.dp))
        RotaryKnob(size, desc, onStep, onBig, onTap) { angle, grip ->
            val c = center
            val r = this.size.minDimension / 2f
            drawCircle(Color(0xFF0B0C0D), r, c)
            drawCircle(Brush.radialGradient(listOf(Color(0xFF4A4F55), EJ_KNOB), Offset(c.x - r * 0.3f, c.y - r * 0.4f), r * 1.4f), r * 0.86f, c)
            drawCircle(if (grip != 0) EJ_LAMP else Color(0x33FFFFFF), r * 0.86f, c, style = Stroke(if (grip != 0) 3f else 1.2f))
            rotate(angle, c) {
                for (i in 0 until 30) rotate(i * 12f, c) {
                    drawLine(Color(0x30FFFFFF), Offset(c.x, c.y - r * 0.85f), Offset(c.x, c.y - r * 0.68f), 1.5f)
                }
                drawCircle(Color(0xFF202326), r * 0.55f, c)
                drawLine(D_WHITE, Offset(c.x, c.y - r * 0.52f), Offset(c.x, c.y - r * 0.2f), 3.5f, cap = StrokeCap.Round)
            }
        }
        if (under != null) Text(under, fontSize = 8.sp, color = EJ_LEGEND.copy(alpha = 0.8f), modifier = Modifier.padding(top = 3.dp))
    }
}

/** Kółko V/S (Primus): pionowy bęben między UP (u góry) i DN (u dołu); palec w górę = UP (+100), w dół = DN. */
@Composable
private fun EjVsWheel(height: Dp, onDelta: (Int) -> Unit) {
    val haptic = LocalHapticFeedback.current
    val delta by rememberUpdatedState(onDelta)
    var offset by remember { mutableStateOf(0f) }
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text("UP", fontWeight = FontWeight.Bold, fontSize = 9.sp, color = EJ_LEGEND, modifier = Modifier.clickable(onClickLabel = "UP") { delta(100) }.padding(2.dp))
        Canvas(
            Modifier.size(width = 28.dp, height = height)
                .semantics {
                    contentDescription = loc("Kółko prędkości pionowej", "Vertical speed wheel")
                    customActions = listOf(CustomAccessibilityAction("UP") { delta(100); true }, CustomAccessibilityAction("DN") { delta(-100); true })
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
                                // Palec w górę = UP (więcej), w dół = DN (mniej)
                                delta(-dir * 100)
                                haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                            }
                        }
                    }
                },
        ) {
            val w = size.width; val h = size.height
            drawRoundRect(Color(0xFF0B0C0D), Offset.Zero, Size(w, h), androidx.compose.ui.geometry.CornerRadius(5f))
            val dw = w * 0.6f; val x0 = (w - dw) / 2
            drawRoundRect(Brush.horizontalGradient(listOf(EJ_KNOB, Color(0xFF4A4F55), EJ_KNOB), x0, x0 + dw), Offset(x0, h * 0.05f), Size(dw, h * 0.9f),
                androidx.compose.ui.geometry.CornerRadius(dw / 2))
            val gap = 9.dp.toPx()
            var y = h * 0.07f + ((offset % gap) + gap) % gap
            while (y < h * 0.93f) { drawLine(Color(0x44FFFFFF), Offset(x0 + 3f, y), Offset(x0 + dw - 3f, y), 1.5f); y += gap }
        }
        Text("DN", fontWeight = FontWeight.Bold, fontSize = 9.sp, color = EJ_LEGEND, modifier = Modifier.clickable(onClickLabel = "DN") { delta(-100) }.padding(2.dp))
    }
}

// ---------------- Guidance Panel ----------------

/**
 * Guidance Panel E190 (Primus Epic): FD, NAV, APP, HDG (pokrętło HDG SEL — dotknięcie = PUSH SYNC do bieżącego kursu),
 * AP, YD, A/T, SPEED (pokrętło) z FLCH, ALT SEL z ALT, V/S z kółkiem. Cyfry nastaw w okienkach nad pokrętłami
 * (w kabinie są na PFD). Pominięte (bez odpowiednika w Link): CRS, BANK, SRC, FMS/MAN, FPA, BARO, MINIMUMS.
 */
@Composable
fun EmbraerGuidancePanel(link: LinkClient, s: FlightState, compact: Boolean = false) {
    val ap = s.autopilot
    val m = ap.modes
    var enter by remember { mutableStateOf<String?>(null) }
    fun cmd(n: String, v: Number? = null) { link.command(n, v?.toDouble()) }
    fun live() = link.state.value?.autopilot
    val skin = ejSkin()
    val trackNow = (s.attitude.hdgMagDeg ?: s.attitude.hdgTrueDeg)

    val spdV = ap.spdKt?.let { "${it.roundToInt()}" } ?: "---"
    val hdgV = ap.hdgDeg?.let { "%03d°".format(it.roundToInt() % 360) } ?: "---"
    val altV = ap.altFt?.let { "${it.roundToInt()}" } ?: "-----"
    val vsV = ap.vsFpm?.let { "%+d".format(it.roundToInt()) } ?: "----"

    val readout: @Composable (String, String, Color, String) -> Unit = { label, value, col, key ->
        Column(
            Modifier.clip(RoundedCornerShape(4.dp)).background(Color.Black).clickable(onClickLabel = loc("Wpisz wartość", "Type a value")) { enter = key }
                .semantics { contentDescription = "$label $value" }.padding(horizontal = 8.dp, vertical = 3.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(label, fontSize = 8.sp, color = D_WHITE.copy(alpha = 0.7f))
            Text(value, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 18.sp, color = col, maxLines = 1, softWrap = false)
        }
    }

    BoxWithConstraints(Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(EJ_PLATE).border(2.dp, EJ_PLATE_EDGE, RoundedCornerShape(10.dp)).padding(10.dp)) {
        // Jeden rząd mieści się już od ok. 520 dp — fold i każdy tablet; telefon w pionie zostaje przy dwóch piętrach
        val wide = maxWidth >= 520.dp
        val kn = if (compact) 56.dp else if (wide) (maxWidth / 12f).coerceIn(56.dp, 72.dp) else ((maxWidth - 40.dp) / 4.6f).coerceIn(56.dp, 72.dp)
        val keyW = if (wide) 58.dp else 62.dp

        // Grupy jak na panelu: [FD NAV APP | HDG] [AP YD A/T] [SPEED FLCH] [ALT SEL ALT] [V/S]
        val lateral: @Composable (Modifier) -> Unit = { mod ->
            Row(mod, horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.Top) {
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    AfdsKey("FD", ap.fd, Modifier.width(keyW), skin) { cmd("TOGGLE_FLIGHT_DIRECTOR") }
                    AfdsKey("NAV", m.nav, Modifier.width(keyW), skin) { cmd("AP_NAV1_HOLD") }
                    AfdsKey("APP", m.apr, Modifier.width(keyW), skin) { cmd("AP_APR_HOLD") }
                }
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    readout("HDG", hdgV, D_CYAN, "HDG")
                    Spacer(Modifier.height(6.dp))
                    EjKnob(kn, "HDG SEL", "PUSH SYNC", loc("Pokrętło kursu $hdgV — dotknięcie ustawia bieżący kurs", "Heading knob $hdgV — tap syncs to the current heading"),
                        { d -> cmd("HEADING_BUG_SET", ControlMath.heading(live()?.hdgDeg, d)) }, { d -> cmd("HEADING_BUG_SET", ControlMath.heading(live()?.hdgDeg, d * 10)) },
                        // PUSH SYNC: kurs zadany = bieżący kurs samolotu
                        { trackNow?.let { cmd("HEADING_BUG_SET", ((it.roundToInt() % 360) + 360) % 360) } })
                    AfdsKey("HDG", m.hdg, Modifier.width(keyW).padding(top = 6.dp), skin) { cmd("AP_HDG_HOLD") }
                }
            }
        }
        val engage: @Composable (Modifier) -> Unit = { mod ->
            Column(mod, verticalArrangement = Arrangement.spacedBy(6.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                AfdsKey("AP", ap.master, Modifier.width(keyW), skin) { cmd("AP_MASTER") }
                AfdsKey("YD", false, Modifier.width(keyW), skin) { cmd("YAW_DAMPER_TOGGLE") }
                AfdsKey("A/T", ap.athr, Modifier.width(keyW), skin) { cmd("AUTO_THROTTLE_ARM") }
            }
        }
        val speed: @Composable (Modifier) -> Unit = { mod ->
            Column(mod, horizontalAlignment = Alignment.CenterHorizontally) {
                readout("SPD", spdV, D_CYAN, "SPD")
                Spacer(Modifier.height(6.dp))
                EjKnob(kn, "SPEED", null, loc("Pokrętło prędkości $spdV", "Speed knob $spdV"),
                    { d -> cmd("AP_SPD_VAR_SET", ControlMath.speed(live()?.spdKt, d)) }, { d -> cmd("AP_SPD_VAR_SET", ControlMath.speed(live()?.spdKt, d * 10)) }) { enter = "SPD" }
                AfdsKey("FLCH", m.flc, Modifier.width(keyW).padding(top = 6.dp), skin) { cmd("FLIGHT_LEVEL_CHANGE") }
            }
        }
        val altitude: @Composable (Modifier) -> Unit = { mod ->
            Column(mod, horizontalAlignment = Alignment.CenterHorizontally) {
                readout("ALT SEL", altV, D_CYAN, "ALT")
                Spacer(Modifier.height(6.dp))
                EjKnob(kn, "ALT SEL", null, loc("Pokrętło wysokości $altV", "Altitude knob $altV"),
                    { d -> cmd("AP_ALT_VAR_SET_ENGLISH", ControlMath.altitude(live()?.altFt, d * 100)) }, { d -> cmd("AP_ALT_VAR_SET_ENGLISH", ControlMath.altitude(live()?.altFt, d * 1000)) }) { enter = "ALT" }
                AfdsKey("ALT", m.alt, Modifier.width(keyW).padding(top = 6.dp), skin) { cmd("AP_ALT_HOLD") }
            }
        }
        val vertical: @Composable (Modifier) -> Unit = { mod ->
            Column(mod, horizontalAlignment = Alignment.CenterHorizontally) {
                readout("V/S", vsV, D_CYAN, "VS")
                Spacer(Modifier.height(6.dp))
                EjVsWheel(kn) { d -> cmd("AP_VS_VAR_SET_ENGLISH", ControlMath.verticalSpeed(live()?.vsFpm, d)) }
                AfdsKey("VS", m.vs, Modifier.width(keyW).padding(top = 6.dp), skin) { cmd("AP_VS_HOLD") }
            }
        }

        if (wide) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.Top) {
                lateral(Modifier); engage(Modifier.padding(top = 4.dp)); speed(Modifier); altitude(Modifier); vertical(Modifier)
            }
        } else {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.Top) { lateral(Modifier); engage(Modifier) }
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly, verticalAlignment = Alignment.Top) { speed(Modifier); altitude(Modifier); vertical(Modifier) }
            }
        }
    }
    if (!compact) Text(loc("Kręć pokrętłem: brzeg — duży krok, środek — mały. HDG SEL: dotknięcie ustawia bieżący kurs. Dotknij cyfr, aby wpisać wartość.",
        "Turn a knob: the edge for big steps, the centre for small ones. HDG SEL: a tap syncs to the current heading. Tap the digits to type a value."),
        style = MaterialTheme.typography.labelSmall, color = Fm.c.ink2, modifier = Modifier.padding(top = 8.dp))

    when (enter) {
        "HDG" -> EnterDialog("HDG", loc("np. 270", "e.g. 270"), KeyboardType.Number, { it.toIntOrNull() in 0..359 }, { cmd("HEADING_BUG_SET", it.toInt()) }) { enter = null }
        "ALT" -> EnterDialog("ALT SEL (ft)", loc("np. 12000", "e.g. 12000"), KeyboardType.Number, { it.toIntOrNull() in 0..50_000 }, { cmd("AP_ALT_VAR_SET_ENGLISH", it.toInt()) }) { enter = null }
        "SPD" -> EnterDialog("SPEED (kt)", loc("np. 250", "e.g. 250"), KeyboardType.Number, { it.toIntOrNull() in 0..500 }, { cmd("AP_SPD_VAR_SET", it.toInt()) }) { enter = null }
        "VS" -> EnterDialog("V/S (ft/min)", loc("np. -1500", "e.g. -1500"), KeyboardType.Number, { it.toIntOrNull() in -8000..8000 }, { cmd("AP_VS_VAR_SET_ENGLISH", it.toInt()) }) { enter = null }
    }
}

// ---------------- Radio: strona RADIO w MCDU ----------------

/**
 * Radio i transponder jak strona RADIO w MCDU E-Jeta: czarny ekran, COM1/COM2 z lewej, NAV1/NAV2 z prawej,
 * częstotliwość aktywna na zielono, zapasowa na cyjanowo; klawisze wyboru linii po bokach ekranu (dotknięcie = zamiana),
 * dotknięcie zapasowej — wpisanie. Na dole XPDR: kod i tryb (STBY / ALT / ON), IDENT; pod ekranem klawiatura 0–7 do kodu.
 */
@Composable
fun EmbraerRadio(link: LinkClient, s: FlightState) {
    val r = s.radios
    val x = r.transponder
    var enter by remember { mutableStateOf<String?>(null) }
    var typed by rememberSaveable { mutableStateOf("") }
    fun cmd(n: String, v: Number? = null) { link.command(n, v?.toDouble()) }
    fun f(radio: Radio?, nav: Boolean, stby: Boolean) = (if (stby) radio?.standbyMhz else radio?.activeMhz)?.let { (if (nav) "%.2f" else "%.3f").format(java.util.Locale.US, it) } ?: "---.---"

    EjPlate("MCDU — RADIO") {
        Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(6.dp)).background(Color.Black).border(3.dp, Color(0xFF3A3E42), RoundedCornerShape(6.dp)).padding(10.dp)) {
            Text("RADIO", fontFamily = Mono, fontSize = 14.sp, color = D_WHITE, modifier = Modifier.fillMaxWidth(), textAlign = TextAlign.Center)
            Spacer(Modifier.height(6.dp))
            listOf(
                Triple("COM1", r.com1 to false, "NAV1" to r.nav1),
                Triple("COM2", r.com2 to false, "NAV2" to r.nav2),
            ).forEach { (comName, com, nav) ->
                Row(Modifier.fillMaxWidth().padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                    McduLsk(loc("Zamień $comName", "Swap $comName")) { cmd(if (comName == "COM1") "COM_STBY_RADIO_SWAP" else "COM2_RADIO_SWAP") }
                    Column(Modifier.weight(1f).padding(start = 6.dp)) {
                        Text(comName, fontFamily = Mono, fontSize = 10.sp, color = D_WHITE.copy(alpha = 0.75f))
                        Text(f(com.first, false, false), fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 16.sp, color = D_GREEN)
                        Text(f(com.first, false, true), fontFamily = Mono, fontSize = 14.sp, color = D_CYAN, modifier = Modifier.clickable(onClickLabel = loc("Wpisz", "Type")) { enter = comName })
                    }
                    Column(Modifier.weight(1f).padding(end = 6.dp), horizontalAlignment = Alignment.End) {
                        Text(nav.first, fontFamily = Mono, fontSize = 10.sp, color = D_WHITE.copy(alpha = 0.75f))
                        Text(f(nav.second, true, false), fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 16.sp, color = D_GREEN)
                        Text(f(nav.second, true, true), fontFamily = Mono, fontSize = 14.sp, color = D_CYAN, modifier = Modifier.clickable(onClickLabel = loc("Wpisz", "Type")) { enter = nav.first })
                    }
                    McduLsk(loc("Zamień ${nav.first}", "Swap ${nav.first}")) { cmd(if (nav.first == "NAV1") "NAV1_RADIO_SWAP" else "NAV2_RADIO_SWAP") }
                }
            }
            Spacer(Modifier.height(6.dp))
            Box(Modifier.fillMaxWidth().height(1.dp).background(D_WHITE.copy(alpha = 0.25f)))
            Spacer(Modifier.height(6.dp))
            val code = if (typed.isNotEmpty()) typed.padEnd(4, '_') else x?.code ?: "----"
            val emergency = typed.isEmpty() && x?.code in setOf("7500", "7600", "7700")
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                McduLsk("IDENT") { cmd("XPNDR_IDENT_ON") }
                Column(Modifier.weight(1f).padding(start = 6.dp)) {
                    Text("XPDR", fontFamily = Mono, fontSize = 10.sp, color = D_WHITE.copy(alpha = 0.75f))
                    Text(code, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 18.sp, color = if (emergency) D_RED else if (typed.isNotEmpty()) D_CYAN else D_GREEN)
                    Text(if (x?.ident == true) "IDENT" else "<IDENT", fontFamily = Mono, fontSize = 11.sp, color = if (x?.ident == true) D_GREEN else D_CYAN)
                }
                Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    listOf("STBY" to ("standby" to 1), "ALT" to ("alt" to 4), "ON" to ("on" to 3)).forEach { (label, mv) ->
                        val on = x?.mode == mv.first
                        Text(label + if (on) " ◄" else " >", fontFamily = Mono, fontSize = 13.sp, color = if (on) D_GREEN else D_CYAN,
                            modifier = Modifier.clickable(onClickLabel = loc("Tryb $label", "Mode $label")) { cmd("XPNDR_MODE", mv.second) }.padding(horizontal = 4.dp, vertical = 2.dp))
                    }
                }
            }
        }
        Spacer(Modifier.height(10.dp))
        // Klawiatura MCDU do kodu transpondera: 0–7, CLR; czwarta cyfra wysyła kod
        listOf(listOf("1", "2", "3", "4", "CLR"), listOf("5", "6", "7", "0", "")).forEach { row ->
            Row(Modifier.fillMaxWidth().padding(bottom = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                row.forEach { k ->
                    if (k.isEmpty()) Spacer(Modifier.weight(1f))
                    else Box(
                        Modifier.weight(1f).height(38.dp).clip(RoundedCornerShape(19.dp)).background(EJ_KEY)
                            .clickable(role = Role.Button, onClickLabel = if (k == "CLR") loc("Wyczyść kod", "Clear code") else k) {
                                if (k == "CLR") typed = ""
                                else {
                                    val next = typed + k
                                    if (next.length == 4) { ControlMath.squawk(next)?.let { cmd("XPNDR_SET", it) }; typed = "" } else typed = next
                                }
                            },
                        contentAlignment = Alignment.Center,
                    ) { Text(k, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 14.sp, color = EJ_LEGEND) }
                }
            }
        }
    }
    enter?.let { name ->
        val nav = name.startsWith("NAV")
        val ev = when (name) { "COM1" -> "COM_STBY_RADIO_SET_HZ"; "COM2" -> "COM2_STBY_RADIO_SET_HZ"; "NAV1" -> "NAV1_STBY_SET_HZ"; else -> "NAV2_STBY_SET_HZ" }
        fun hz(t: String) = ControlMath.parseMhz(t)?.let { if (nav) ControlMath.navHz(it) else ControlMath.comHz(it) }
        EnterDialog(loc("$name — częstotliwość zapasowa (MHz)", "$name — standby frequency (MHz)"), if (nav) "np. 110.30" else "np. 121.500", KeyboardType.Decimal,
            { hz(it) != null }, { t -> hz(t)?.let { cmd(ev, it) } }) { enter = null }
    }
}

/** Klawisz wyboru linii MCDU (LSK): krótka pozioma belka przy krawędzi ekranu. */
@Composable
private fun McduLsk(desc: String, onClick: () -> Unit) {
    Box(
        Modifier.size(width = 26.dp, height = 30.dp).clip(RoundedCornerShape(3.dp)).background(EJ_KEY)
            .semantics { contentDescription = desc; role = Role.Button }.clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { Box(Modifier.size(width = 14.dp, height = 3.dp).background(EJ_LEGEND)) }
}

// ---------------- Systemy ----------------

/** Przełącznik dźwigienkowy z nadgłowia E-Jeta: napis nad nim, dźwigienka u góry = ON. */
@Composable
private fun EjToggle(label: String, on: Boolean, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val haptic = LocalHapticFeedback.current
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally) {
        Text(label, fontWeight = FontWeight.Bold, fontSize = 9.sp, color = EJ_LEGEND, maxLines = 1)
        Box(
            Modifier.padding(top = 3.dp).size(width = 28.dp, height = 44.dp).clip(RoundedCornerShape(14.dp)).background(Color(0xFF111315))
                .border(1.dp, Color(0x44FFFFFF), RoundedCornerShape(14.dp))
                .semantics { contentDescription = label + if (on) loc(", włączone", ", on") else loc(", wyłączone", ", off"); role = Role.Switch }
                .clickable { haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick() },
        ) {
            Box(Modifier.align(if (on) Alignment.TopCenter else Alignment.BottomCenter).padding(4.dp).size(20.dp).clip(RoundedCornerShape(10.dp)).background(Color(0xFFCFD3D6)))
        }
        Text(if (on) "ON" else "OFF", fontSize = 8.sp, color = if (on) EJ_LAMP else EJ_LEGEND.copy(alpha = 0.55f))
    }
}

@Composable
private fun EjDetent(label: String, on: Boolean, modifier: Modifier = Modifier, desc: String = label, onClick: () -> Unit) {
    val haptic = LocalHapticFeedback.current
    Box(
        modifier.height(40.dp).clip(RoundedCornerShape(4.dp)).background(if (on) EJ_LEGEND else EJ_KEY).border(1.dp, Color(0x33FFFFFF), RoundedCornerShape(4.dp))
            .semantics { contentDescription = desc + if (on) loc(", ustawione", ", set") else ""; role = Role.RadioButton }
            .clickable { haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick() },
        contentAlignment = Alignment.Center,
    ) { Text(label, fontWeight = FontWeight.Bold, fontSize = 11.sp, color = if (on) Color.Black else EJ_LEGEND, maxLines = 1) }
}

/**
 * Systemy E190: EXTERNAL LIGHTS z nadgłowia (NAV, STROBE, RED BCN, LOGO, LANDING, TAXI, INSP → skrzydło),
 * dźwignia podwozia z trzema wskaźnikami DN, klapy 0–5 i FULL, dźwignia SPEED BRAKE (oś, CLOSE–OPEN — E-Jet nie ma uzbrajania),
 * EMER/PRKG BRAKE z osią hamulców kół i trymer wysokości (PITCH TRIM).
 */
@Composable
fun EmbraerSystems(link: LinkClient, s: FlightState) {
    fun cmd(n: String, v: Number? = null) { link.command(n, v?.toDouble()) }
    val ctl = s.controls
    val l = s.lights
    var confirmGear by remember { mutableStateOf(false) }

    EjPlate("EXTERNAL LIGHTS") {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
            EjToggle("NAV", l.nav == true) { cmd("TOGGLE_NAV_LIGHTS") }
            EjToggle("STROBE", l.strobe == true) { cmd("STROBES_TOGGLE") }
            EjToggle("RED BCN", l.beacon == true) { cmd("TOGGLE_BEACON_LIGHTS") }
            EjToggle("LOGO", l.logo == true) { cmd("TOGGLE_LOGO_LIGHTS") }
        }
        Spacer(Modifier.height(8.dp))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
            EjToggle("LANDING", l.landing == true) { cmd("LANDING_LIGHTS_TOGGLE") }
            EjToggle("TAXI", l.taxi == true) { cmd("TOGGLE_TAXI_LIGHTS") }
            EjToggle("INSP", l.wing == true) { cmd("TOGGLE_WING_LIGHTS") }
            EjToggle("CABIN", l.cabin == true) { cmd("TOGGLE_CABIN_LIGHTS") }
        }
    }
    Spacer(Modifier.height(10.dp))

    // Podwozie: trzy zielone „DN” (zablokowane w dole), czerwone w trakcie ruchu
    val gearDown = ctl.gearDown == true
    val moving = ctl.gearPct?.let { it > 1 && it < 99 } == true
    EjPlate("LANDING GEAR") {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(Modifier.weight(1f), horizontalArrangement = Arrangement.SpaceEvenly) {
                repeat(3) {
                    Box(Modifier.size(40.dp).clip(RoundedCornerShape(20.dp)).background(Color.Black)
                        .border(2.dp, if (moving) D_RED else if (gearDown) D_GREEN else Color(0xFF3A3E42), RoundedCornerShape(20.dp)),
                        contentAlignment = Alignment.Center) {
                        Text(if (moving) "▒" else if (gearDown) "DN" else "UP", fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 11.sp,
                            color = if (moving) D_RED else if (gearDown) D_GREEN else D_WHITE.copy(alpha = 0.6f))
                    }
                }
            }
            EjDetent(if (gearDown) "DN" else "UP", true, Modifier.width(80.dp), loc("Dźwignia podwozia", "Gear lever")) {
                if (gearDown && s.aircraft.onGround == true) confirmGear = true else cmd("GEAR_TOGGLE")
            }
        }
    }
    Spacer(Modifier.height(10.dp))

    // Klapy E190: 0, 1, 2, 3, 4, 5, FULL (7 pozycji); przy innej liczbie — numery
    val positions = ctl.flapsPositions?.takeIf { it > 1 }
    EjPlate("FLAP") {
        if (positions == null) Text("—", color = EJ_LEGEND)
        else Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(5.dp)) {
            repeat(positions) { i ->
                val name = if (positions == 7) listOf("0", "1", "2", "3", "4", "5", "FULL")[i] else "$i"
                EjDetent(name, ctl.flapsIndex == i, Modifier.weight(if (name == "FULL") 1.6f else 1f), loc("Klapy $name", "Flaps $name")) {
                    cmd("FLAPS_SET", (16383.0 * i / (positions - 1)).roundToInt())
                }
            }
        }
    }
    Spacer(Modifier.height(10.dp))

    EjPlate("SPEED BRAKE") { SpeedBrakeAxis(link, s, label = "CLOSE — OPEN", labelColor = EJ_LEGEND) }
    Spacer(Modifier.height(10.dp))

    EjPlate("EMER / PRKG BRAKE") {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            // Rączka hamulca w żółto-czarne pasy, jak na pedestale
            Box(
                Modifier.size(width = 64.dp, height = 40.dp).clip(RoundedCornerShape(4.dp))
                    .background(if (ctl.parkingBrake == true) Brush.linearGradient(listOf(Color(0xFFF2C21B), Color(0xFF1A1A1A), Color(0xFFF2C21B), Color(0xFF1A1A1A)), Offset.Zero, Offset(40f, 40f), androidx.compose.ui.graphics.TileMode.Repeated)
                    else Brush.linearGradient(listOf(EJ_KEY, EJ_KEY)))
                    .semantics { contentDescription = loc("Hamulec postojowy", "Parking brake") + if (ctl.parkingBrake == true) loc(", zaciągnięty", ", set") else ""; role = Role.Switch }
                    .clickable { cmd("PARKING_BRAKES") },
                contentAlignment = Alignment.Center,
            ) { Text(if (ctl.parkingBrake == true) "SET" else "OFF", fontWeight = FontWeight.Bold, fontSize = 11.sp, color = if (ctl.parkingBrake == true) Color.Black else EJ_LEGEND,
                modifier = Modifier.clip(RoundedCornerShape(3.dp)).background(if (ctl.parkingBrake == true) Color(0xCCF2C21B) else Color.Transparent).padding(horizontal = 4.dp)) }
            WheelBrakeAxis(link, s, Modifier.weight(1f), labelColor = EJ_LEGEND)
        }
    }
    Spacer(Modifier.height(10.dp))

    EjPlate("PITCH TRIM") {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            EjDetent("DN", false, Modifier.weight(1f), loc("Trym nosem w dół", "Trim nose down")) { cmd("ELEV_TRIM_DN") }
            Text(ctl.elevatorTrimPct?.let { "%+.1f".format(it) } ?: "--", fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 16.sp, color = D_GREEN,
                textAlign = TextAlign.Center, modifier = Modifier.weight(1f).clip(RoundedCornerShape(4.dp)).background(Color.Black).padding(vertical = 8.dp))
            EjDetent("UP", false, Modifier.weight(1f), loc("Trym nosem w górę", "Trim nose up")) { cmd("ELEV_TRIM_UP") }
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

// ---------------- Silniki: EICAS E-Jeta i dźwignie ----------------

@Composable
fun EmbraerEngines(link: LinkClient, s: FlightState) {
    EngineDisplayBox { EmbraerEicas(s) }
    Spacer(Modifier.height(12.dp))
    Quadrant(link, s, AircraftProfile.E190)
    if (s.autopilot.athr) Text(loc("A/T włączony — symulator może sam przestawiać dźwignie.", "A/T is on — the simulator may move the thrust levers itself."),
        style = MaterialTheme.typography.labelSmall, color = Fm.c.caution, modifier = Modifier.padding(top = 8.dp))
}

/**
 * EICAS E190: z lewej na silnik zegar N1 i ITT (łuk z zielonym wypełnieniem, odczyt w ramce pod spodem),
 * N2 i FF pod zegarami, FUEL QTY (L, R) i FQ TOTAL; z prawej podwozie (DN w kółkach), klapy S/F i trym wysokości.
 * ITT pokazuje temperaturę gazów wylotowych z Link (EGT). Brak danych o oleju, wibracjach i kabinie — tych pól nie ma.
 */
@Composable
fun EmbraerEicas(s: FlightState) {
    val m = rememberTextMeasurer()
    val engines = s.engines.take(2).ifEmpty { return }
    val desc = engines.mapIndexed { i, e -> "ENG ${i + 1} N1 ${"%.1f".format(e.n1Pct ?: 0.0)}%" }.joinToString(", ")
    Canvas(Modifier.fillMaxWidth().aspectRatio(0.92f).clip(RoundedCornerShape(10.dp)).semantics { contentDescription = "EICAS: $desc" }) {
        val w = size.width; val h = size.height
        drawRect(Color(0xFF02040A))
        // Pionowa linia oddziela silniki od podwozia/klap, jak na EICAS
        drawLine(D_WHITE.copy(alpha = 0.8f), Offset(w * 0.6f, h * 0.05f), Offset(w * 0.6f, h * 0.95f), 2f)
        val cxs = listOf(w * 0.15f, w * 0.43f)
        val r = w * 0.1f
        engines.forEachIndexed { i, e ->
            val cx = cxs[i]
            val thr = e.throttlePct ?: s.controls.throttlePct
            ejDial(m, cx, h * 0.15f, r, (e.n1Pct ?: 0.0) / 110.0, 100.0 / 110.0, "%.1f".format(e.n1Pct ?: 0.0), thr?.let { it / 100.0 })
            ejDial(m, cx, h * 0.42f, r, (e.egtC ?: 0.0) / 1000.0, 0.95, "${(e.egtC ?: 0.0).roundToInt()}", null)
            ejText(m, e.n2Pct?.let { "%.1f".format(it) } ?: "--", cx, h * 0.62f, D_GREEN, 15f)
            ejText(m, e.fuelFlowKgH?.let { "${it.roundToInt()}" } ?: "--", cx, h * 0.69f, D_GREEN, 15f)
        }
        val lx = w * 0.29f
        ejText(m, "N1", lx, h * 0.23f, D_WHITE, 12f)
        ejText(m, "ITT", lx, h * 0.5f, D_WHITE, 12f)
        ejText(m, "N2", lx, h * 0.62f, D_WHITE, 12f)
        ejText(m, "FF KPH", lx, h * 0.69f, D_WHITE, 11f)
        // Paliwo: L, R i suma w ramce
        drawLine(D_WHITE.copy(alpha = 0.6f), Offset(w * 0.03f, h * 0.76f), Offset(w * 0.57f, h * 0.76f), 1.5f)
        val t = s.fuel.tanks
        ejText(m, "FUEL QTY", lx, h * 0.8f, D_WHITE, 11f)
        ejText(m, t["leftKg"]?.let { "${it.roundToInt()}" } ?: "--", cxs[0], h * 0.85f, D_GREEN, 15f)
        ejText(m, t["rightKg"]?.let { "${it.roundToInt()}" } ?: "--", cxs[1], h * 0.85f, D_GREEN, 15f)
        ejText(m, "FQ KG", lx, h * 0.9f, D_WHITE, 11f)
        ejText(m, s.fuel.totalKg?.let { "${it.roundToInt()}" } ?: "--", lx, h * 0.95f, D_GREEN, 16f, box = true)

        // Prawa kolumna: podwozie, klapy (S/F), trym
        val rx = w * 0.8f
        ejText(m, "LG / AUTOBRAKE", rx, h * 0.08f, D_WHITE, 10f)
        val down = s.controls.gearDown == true
        val moving = s.controls.gearPct?.let { it > 1 && it < 99 } == true
        listOf(Offset(rx, h * 0.15f), Offset(rx - w * 0.08f, h * 0.22f), Offset(rx + w * 0.08f, h * 0.22f)).forEach { p ->
            val col = if (moving) D_AMBER else if (down) D_GREEN else D_WHITE.copy(alpha = 0.6f)
            drawCircle(col, w * 0.032f, p, style = Stroke(2.5f))
            ejText(m, if (moving) "" else if (down) "DN" else "UP", p.x, p.y, col, 10f)
        }
        ejText(m, "SLAT/FLAP", rx, h * 0.4f, D_WHITE, 10f)
        val fi = s.controls.flapsIndex; val fp = s.controls.flapsPositions
        ejText(m, if (fi != null && fp == 7) listOf("0", "1", "2", "3", "4", "5", "FULL")[fi.coerceIn(0, 6)] else fi?.toString() ?: "--", rx, h * 0.46f, D_GREEN, 18f, box = true)
        ejText(m, "TRIMS", rx, h * 0.62f, D_WHITE, 10f)
        ejText(m, "PITCH", rx, h * 0.68f, D_WHITE, 10f)
        ejText(m, s.controls.elevatorTrimPct?.let { "%+.1f".format(it) } ?: "--", rx, h * 0.74f, D_GREEN, 15f)
        if (s.controls.parkingBrake == true) ejText(m, "PRKG BRAKE", rx, h * 0.88f, D_AMBER, 11f)
        if (s.autopilot.athr) ejText(m, "A/T", rx, h * 0.94f, D_MAGENTA, 12f)
    }
}

private fun DrawScope.ejText(m: TextMeasurer, t: String, x: Float, y: Float, col: Color, sz: Float, box: Boolean = false) {
    if (t.isEmpty()) return
    val l = m.measure(t, TextStyle(color = col, fontSize = sz.sp, fontFamily = Mono, fontWeight = FontWeight.Bold))
    val tl = Offset(x - l.size.width / 2f, y - l.size.height / 2f)
    if (box) drawRect(D_WHITE.copy(alpha = 0.8f), Offset(tl.x - 8f, tl.y - 2f), Size(l.size.width + 16f, l.size.height + 4f), style = Stroke(2f))
    drawText(l, topLeft = tl)
}

/** Zegar E-Jeta: łuk 210° z zielonym wypełnieniem do wartości, czerwona kreska limitu, cyjanowy znacznik dźwigni, odczyt w ramce pod spodem. */
private fun DrawScope.ejDial(m: TextMeasurer, cx: Float, cy: Float, r: Float, frac: Double, redFrac: Double, value: String, cmdFrac: Double?) {
    val start = 150f; val sweep = 240f
    val f = frac.coerceIn(0.0, 1.0).toFloat()
    val tl = Offset(cx - r, cy - r); val sz = Size(r * 2, r * 2)
    drawArc(D_FILL.copy(alpha = 0.85f), start, sweep * f, true, tl, sz)
    drawArc(D_WHITE, start, sweep, false, tl, sz, style = Stroke(2.5f))
    val ra = Math.toRadians(start + sweep * redFrac).toFloat()
    drawLine(D_RED, Offset(cx + r * 0.9f * cos(ra), cy + r * 0.9f * sin(ra)), Offset(cx + r * 1.15f * cos(ra), cy + r * 1.15f * sin(ra)), 4f)
    cmdFrac?.let { c ->
        val ca = Math.toRadians(start + sweep * c.coerceIn(0.0, 1.0)).toFloat()
        drawCircle(D_CYAN, 4f, Offset(cx + r * 1.1f * cos(ca), cy + r * 1.1f * sin(ca)))
    }
    val na = Math.toRadians(start + sweep * f.toDouble()).toFloat()
    drawLine(D_WHITE, Offset(cx, cy), Offset(cx + r * cos(na), cy + r * sin(na)), 3f, cap = StrokeCap.Round)
    val over = f >= redFrac
    ejText(m, value, cx, cy + r * 0.85f, if (over) D_RED else D_GREEN, 16f, box = true)
}
