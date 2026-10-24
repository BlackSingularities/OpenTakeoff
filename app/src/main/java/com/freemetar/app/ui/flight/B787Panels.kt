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
import androidx.compose.material3.AlertDialog
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
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
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

// Panele Boeinga 787: grafitowo-niebieskawa płyta, białe opisy, czarne okienka z białymi cyframi, srebrne pokrętła,
// klawisze z podłużną lampką pod napisem. Radio i transponder jak TCP (ekran dotykowy z klawiaturą na pedestale).
// Tylko elementy, które mają działającą komendę w Link — VNAV, BANK LIMIT, MINS czy AUTOBRAKE nie są pokazane jako działające.

private val D7_PLATE = Color(0xFF353C43)
private val D7_PLATE_EDGE = Color(0xFF1C2126)
private val D7_LEGEND = Color(0xFFF1F3F5)
private val D7_KEY = Color(0xFF23292E)
private val D7_LAMP = Color(0xFFFFFFFF)
private val D7_LAMP_OFF = Color(0x22FFFFFF)
private val D7_WINDOW = Color(0xFF07090B)
private val D7_KNOB_HI = Color(0xFFE4E7EA)
private val D7_KNOB_MID = Color(0xFFA9AFB4)
private val D7_KNOB_LO = Color(0xFF5E656B)
private val D7_SCREEN = Color(0xFF05070A)
private val X_WHITE = Color(0xFFF2F4F6)
private val X_CYAN = Color(0xFF4FD8F0)
private val X_GREEN = Color(0xFF40E07C)
private val X_MAGENTA = Color(0xFFEE55DD)
private val X_AMBER = Color(0xFFFFB020)
private val X_RED = Color(0xFFFF3B3B)
private val X_FILL = Color(0xFF5B6168)

@Composable
private fun d7Skin() = panelSkin(AircraftProfile.B787).copy(
    plate = D7_PLATE, plateEdge = D7_PLATE_EDGE, key = D7_KEY, keyPressed = Color(0xFF384047), keyText = D7_LEGEND,
    lampOn = D7_LAMP, lampOff = D7_LAMP_OFF, lamp = LampStyle.BAR_BELOW, keyCorner = 3.dp, keyHeight = 42.dp, legend = D7_LEGEND,
    window = D7_WINDOW, windowText = X_WHITE, windowLabel = D7_LEGEND,
)

@Composable
private fun D7Plate(title: String, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Column(modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(D7_PLATE).border(2.dp, D7_PLATE_EDGE, RoundedCornerShape(10.dp)).padding(10.dp)) {
        Text(title, fontWeight = FontWeight.Bold, fontSize = 11.sp, color = D7_LEGEND.copy(alpha = 0.85f), modifier = Modifier.padding(bottom = 8.dp))
        content()
    }
}

/** Okienko MCP 787: tytuł nad nim (np. IAS ( ) MACH), w środku małe oznaczenie trybu z lewej i białe cyfry. */
@Composable
private fun D7Window(title: String, tag: String?, value: String, dense: Boolean, onClick: () -> Unit) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text(title, fontWeight = FontWeight.Bold, fontSize = 10.sp, color = D7_LEGEND, maxLines = 1)
        Row(
            Modifier.padding(top = 3.dp).clip(RoundedCornerShape(3.dp)).background(D7_WINDOW).border(2.dp, Color(0xFF14181B), RoundedCornerShape(3.dp))
                .semantics { contentDescription = "$title $value" }
                .clickable(onClickLabel = loc("Wpisz wartość", "Type a value"), onClick = onClick)
                .padding(horizontal = if (dense) 6.dp else 8.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (tag != null) Text(tag, fontFamily = Mono, fontSize = 9.sp, color = X_WHITE.copy(alpha = 0.8f), modifier = Modifier.padding(end = 5.dp))
            Text(value.ifEmpty { " " }.padStart(5), fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = if (dense) 18.sp else 21.sp, color = X_WHITE, maxLines = 1, softWrap = false)
        }
    }
}

/** Srebrne pokrętło MCP 787 z ząbkowanym brzegiem i czarną kreską; [ring] — rysuje pierścień BANK LIMIT wokół HDG. */
@Composable
private fun D7Knob(size: Dp, desc: String, ring: Boolean = false, onStep: (Int) -> Unit, onBig: (Int) -> Unit, onTap: () -> Unit) {
    RotaryKnob(size, desc, onStep, onBig, onTap) { angle, grip ->
        val c = center
        val r = this.size.minDimension / 2f
        val body = if (ring) r * 0.62f else r * 0.8f
        if (ring) {
            drawCircle(Color(0xFF1E2327), r * 0.95f, c)
            drawCircle(D7_KNOB_MID, r * 0.95f, c, style = Stroke(r * 0.12f))
            for (k in 0..4) {
                val a = Math.toRadians(-150.0 + k * 30.0).toFloat()
                drawLine(D7_LEGEND, Offset(c.x + r * 0.78f * cos(a), c.y + r * 0.78f * sin(a)), Offset(c.x + r * 0.9f * cos(a), c.y + r * 0.9f * sin(a)), 2f)
            }
        } else drawCircle(Color(0xFF15191C), body * 1.12f, c)
        drawCircle(Brush.radialGradient(listOf(D7_KNOB_HI, D7_KNOB_MID, D7_KNOB_LO), Offset(c.x - body * 0.35f, c.y - body * 0.45f), body * 1.6f), body, c)
        drawCircle(if (grip != 0) X_CYAN else Color(0x55000000), body, c, style = Stroke(if (grip != 0) 3f else 1.5f))
        rotate(angle, c) {
            for (i in 0 until 36) rotate(i * 10f, c) {
                drawLine(Color(0x44000000), Offset(c.x, c.y - body * 0.98f), Offset(c.x, c.y - body * 0.84f), 1.5f)
            }
            drawLine(Color(0xFF1E1E1E), Offset(c.x, c.y - body * 0.8f), Offset(c.x, c.y - body * 0.3f), 4f, cap = StrokeCap.Round)
        }
    }
}

/** Kółko V/S 787: pionowy bęben między DOWN (u góry) i UP (u dołu), jak na MCP; palec w dół = UP (+100), w górę = DOWN. */
@Composable
private fun D7VsWheel(height: Dp, onDelta: (Int) -> Unit) {
    val haptic = LocalHapticFeedback.current
    val delta by rememberUpdatedState(onDelta)
    var offset by remember { mutableStateOf(0f) }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Column(Modifier.height(height), verticalArrangement = Arrangement.SpaceBetween) {
            Text("DOWN", fontWeight = FontWeight.Bold, fontSize = 9.sp, color = D7_LEGEND, modifier = Modifier.clickable(onClickLabel = "DOWN") { delta(-100) }.padding(3.dp))
            Text("UP", fontWeight = FontWeight.Bold, fontSize = 9.sp, color = D7_LEGEND, modifier = Modifier.clickable(onClickLabel = "UP") { delta(100) }.padding(3.dp))
        }
        Canvas(
            Modifier.size(width = 30.dp, height = height)
                .semantics {
                    contentDescription = loc("Kółko prędkości pionowej", "Vertical speed wheel")
                    customActions = listOf(CustomAccessibilityAction("UP") { delta(100); true }, CustomAccessibilityAction("DOWN") { delta(-100); true })
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
                                delta(dir * 100)
                                haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                            }
                        }
                    }
                },
        ) {
            val w = size.width; val h = size.height
            drawRoundRect(Color(0xFF15191C), Offset.Zero, Size(w, h), CornerRadius(6f))
            val dw = w * 0.62f; val x0 = (w - dw) / 2
            drawRoundRect(Brush.horizontalGradient(listOf(D7_KNOB_LO, D7_KNOB_HI, D7_KNOB_LO), x0, x0 + dw), Offset(x0, h * 0.05f), Size(dw, h * 0.9f), CornerRadius(dw / 2))
            val gap = 9.dp.toPx()
            var y = h * 0.07f + ((offset % gap) + gap) % gap
            while (y < h * 0.93f) { drawLine(Color(0x66000000), Offset(x0 + 3f, y), Offset(x0 + dw - 3f, y), 2f); y += gap }
        }
    }
}

/** Przełącznik dźwigienkowy 787 (F/D ON-OFF, A/T ARM, światła): napis nad nim, dźwigienka u góry = ON. */
@Composable
private fun D7Toggle(label: String, on: Boolean, modifier: Modifier = Modifier, onText: String = "ON", onClick: () -> Unit) {
    val haptic = LocalHapticFeedback.current
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally) {
        Text(label, fontWeight = FontWeight.Bold, fontSize = 9.sp, color = D7_LEGEND, maxLines = 1)
        Box(
            Modifier.padding(top = 3.dp).size(width = 28.dp, height = 44.dp).clip(RoundedCornerShape(14.dp)).background(Color(0xFF14181B))
                .border(1.dp, Color(0x44FFFFFF), RoundedCornerShape(14.dp))
                .semantics { contentDescription = label + if (on) loc(", włączone", ", on") else loc(", wyłączone", ", off"); role = Role.Switch }
                .clickable { haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick() },
        ) {
            Box(Modifier.align(if (on) Alignment.TopCenter else Alignment.BottomCenter).padding(4.dp).size(20.dp).clip(RoundedCornerShape(10.dp))
                .background(Brush.radialGradient(listOf(D7_KNOB_HI, D7_KNOB_MID))))
        }
        Text(if (on) onText else "OFF", fontSize = 8.sp, color = if (on) D7_LEGEND else D7_LEGEND.copy(alpha = 0.5f))
    }
}

/** Klawisz zapadki/wyboru (klapy, podwozie, tryby XPDR): zaznaczony = jasne lico. */
@Composable
private fun D7Detent(label: String, on: Boolean, modifier: Modifier = Modifier, desc: String = label, onClick: () -> Unit) {
    val haptic = LocalHapticFeedback.current
    Box(
        modifier.height(40.dp).clip(RoundedCornerShape(4.dp)).background(if (on) D7_LEGEND else D7_KEY).border(1.dp, Color(0x33FFFFFF), RoundedCornerShape(4.dp))
            .semantics { contentDescription = desc + if (on) loc(", ustawione", ", set") else ""; role = Role.RadioButton }
            .clickable { haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick() },
        contentAlignment = Alignment.Center,
    ) { Text(label, fontWeight = FontWeight.Bold, fontSize = 11.sp, color = if (on) Color.Black else D7_LEGEND, maxLines = 1) }
}

/** Klawisz z powtarzaniem przy przytrzymaniu (trymer): 400 ms, potem co 90 ms. */
@Composable
private fun D7HoldKey(label: String, desc: String, modifier: Modifier = Modifier, action: () -> Unit) {
    val scope = rememberCoroutineScope()
    val act by rememberUpdatedState(action)
    val haptic = LocalHapticFeedback.current
    Box(
        modifier.height(40.dp).clip(RoundedCornerShape(4.dp)).background(D7_KEY).border(1.dp, Color(0x40FFFFFF), RoundedCornerShape(4.dp))
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
    ) { Text(label, fontWeight = FontWeight.Bold, fontSize = 12.sp, color = D7_LEGEND) }
}

// ---------------- MCP ----------------

private data class D7Size(val knob: Dp, val dense: Boolean = false) {
    fun key(full: Dp, dense: Dp) = if (this.dense) dense else full
}

/**
 * MCP 787 w układzie z kabiny: F/D, A/P, A/T ARM, CLB/CON i A/T; IAS ( ) MACH z przełącznikiem IAS/MACH; LNAV i FLCH;
 * HDG ( ) TRK z pierścieniem BANK LIMIT (dotknięcie pokrętła = HDG SEL) i HOLD; V/S ( ) FPA z kółkiem i klawiszem V/S;
 * ALTITUDE z HOLD, LOC i APP; belka A/P DISENGAGE. Tablet i fold: jeden rząd (ściśnięty poniżej 900 dp), telefon: piętra.
 * Pominięte (bez komendy w Link): VNAV, TRK/FPA, XFR, BANK LIMIT (tylko rysunek), AUTO/1000, prawe F/D i A/P jako osobne.
 */
@Composable
fun B787Mcp(link: LinkClient, s: FlightState, compact: Boolean = false) {
    val ap = s.autopilot
    val m = ap.modes
    val mach = s.avionics?.ap?.machHold == true
    var enter by remember { mutableStateOf<String?>(null) }
    fun cmd(n: String, v: Number? = null) { link.command(n, v?.toDouble()) }
    fun live() = link.state.value?.autopilot
    val skin = d7Skin()

    val spdV = if (mach) s.avionics?.ap?.machVar?.let { ".%03d".format((it * 1000).roundToInt()) } ?: "---" else ap.spdKt?.let { "${it.roundToInt()}" } ?: "---"
    val hdgV = ap.hdgDeg?.let { "%03d".format(it.roundToInt() % 360) } ?: "---"
    val altV = ap.altFt?.let { "${it.roundToInt()}" } ?: "-----"
    val vsV = ap.vsFpm?.let { "%+d".format(it.roundToInt()) } ?: ""

    val left: @Composable (D7Size, Modifier) -> Unit = { sz, mod ->
        Row(mod, horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.Top) {
            D7Toggle("F/D", ap.fd) { cmd("TOGGLE_FLIGHT_DIRECTOR") }
            Column(verticalArrangement = Arrangement.spacedBy(6.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                AfdsKey("A/P", ap.master, Modifier.width(sz.key(56.dp, 48.dp)), skin) { cmd("AP_MASTER") }
                D7Toggle("A/T ARM", ap.athr) { cmd("AUTO_THROTTLE_ARM") }
            }
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                AfdsKey("CLB/CON", false, Modifier.width(sz.key(76.dp, 66.dp)), skin) { cmd("AP_N1_HOLD") }
                AfdsKey("A/T", false, Modifier.width(sz.key(76.dp, 66.dp)), skin) { cmd("AP_PANEL_SPEED_HOLD") }
            }
        }
    }
    val speed: @Composable (D7Size, Modifier) -> Unit = { sz, mod ->
        Column(mod, horizontalAlignment = Alignment.CenterHorizontally) {
            D7Window("IAS ( ) MACH", if (mach) "M" else null, spdV, sz.dense) { enter = "SPD" }
            Row(Modifier.padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                D7Knob(sz.knob, loc("Pokrętło IAS/MACH $spdV", "IAS/MACH knob $spdV"),
                    onStep = { d -> cmd("AP_SPD_VAR_SET", ControlMath.speed(live()?.spdKt, d)) }, onBig = { d -> cmd("AP_SPD_VAR_SET", ControlMath.speed(live()?.spdKt, d * 10)) }) { enter = "SPD" }
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    AfdsKey("LNAV", m.nav, Modifier.width(sz.key(62.dp, 54.dp)), skin) { cmd("AP_NAV1_HOLD") }
                    AfdsKey("FLCH", m.flc, Modifier.width(sz.key(62.dp, 54.dp)), skin) { cmd("FLIGHT_LEVEL_CHANGE") }
                }
            }
            AfdsKey("IAS/MACH", mach, Modifier.width(sz.key(84.dp, 72.dp)).padding(top = 6.dp), skin) { cmd("AP_PANEL_MACH_HOLD") }
        }
    }
    val heading: @Composable (D7Size, Modifier) -> Unit = { sz, mod ->
        Column(mod, horizontalAlignment = Alignment.CenterHorizontally) {
            D7Window("HDG ( ) TRK", "HDG", hdgV, sz.dense) { enter = "HDG" }
            Box(Modifier.padding(top = 8.dp)) {
                D7Knob(sz.knob * 1.1f, loc("Pokrętło kursu $hdgV — dotknięcie: HDG SEL", "Heading knob $hdgV — tap: HDG SEL"), ring = true,
                    onStep = { d -> cmd("HEADING_BUG_SET", ControlMath.heading(live()?.hdgDeg, d)) }, onBig = { d -> cmd("HEADING_BUG_SET", ControlMath.heading(live()?.hdgDeg, d * 10)) }) { cmd("AP_HDG_HOLD") }
            }
            Text("BANK LIMIT · SEL", fontSize = 8.sp, color = D7_LEGEND.copy(alpha = 0.75f), modifier = Modifier.padding(top = 3.dp))
            AfdsKey("HOLD", m.hdg, Modifier.width(sz.key(62.dp, 54.dp)).padding(top = 6.dp), skin) { cmd("AP_HDG_HOLD") }
        }
    }
    val vertical: @Composable (D7Size, Modifier) -> Unit = { sz, mod ->
        Column(mod, horizontalAlignment = Alignment.CenterHorizontally) {
            D7Window("V/S ( ) FPA", "V/S", vsV, sz.dense) { enter = "VS" }
            Box(Modifier.padding(top = 8.dp)) { D7VsWheel(sz.knob * 1.25f) { d -> cmd("AP_VS_VAR_SET_ENGLISH", ControlMath.verticalSpeed(live()?.vsFpm, d)) } }
            AfdsKey("V/S", m.vs, Modifier.width(sz.key(62.dp, 54.dp)).padding(top = 6.dp), skin) { cmd("AP_VS_HOLD") }
        }
    }
    val altitude: @Composable (D7Size, Modifier) -> Unit = { sz, mod ->
        Column(mod, horizontalAlignment = Alignment.CenterHorizontally) {
            D7Window("ALTITUDE", null, altV, sz.dense) { enter = "ALT" }
            Row(Modifier.padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                D7Knob(sz.knob, loc("Pokrętło wysokości $altV", "Altitude knob $altV"),
                    onStep = { d -> cmd("AP_ALT_VAR_SET_ENGLISH", ControlMath.altitude(live()?.altFt, d * 100)) }, onBig = { d -> cmd("AP_ALT_VAR_SET_ENGLISH", ControlMath.altitude(live()?.altFt, d * 1000)) }) { enter = "ALT" }
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    AfdsKey("LOC", m.loc, Modifier.width(sz.key(62.dp, 54.dp)), skin) { cmd("AP_LOC_HOLD") }
                    AfdsKey("APP", m.apr, Modifier.width(sz.key(62.dp, 54.dp)), skin) { cmd("AP_APR_HOLD") }
                }
            }
            AfdsKey("HOLD", m.alt, Modifier.width(sz.key(62.dp, 54.dp)).padding(top = 6.dp), skin) { cmd("AP_ALT_HOLD") }
        }
    }
    val disengage: @Composable (Modifier) -> Unit = { mod ->
        Column(mod, horizontalAlignment = Alignment.CenterHorizontally) {
            Text("A/P DISENGAGE", fontWeight = FontWeight.Bold, fontSize = 9.sp, color = D7_LEGEND)
            Box(
                Modifier.padding(top = 3.dp).size(width = 110.dp, height = 30.dp).clip(RoundedCornerShape(3.dp)).background(Color(0xFFC9CDD0))
                    .semantics { contentDescription = loc("Rozłącz autopilota", "Disengage autopilot"); role = Role.Button }
                    .clickable { cmd("AUTOPILOT_OFF") },
            )
        }
    }

    BoxWithConstraints(Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(D7_PLATE).border(2.dp, D7_PLATE_EDGE, RoundedCornerShape(10.dp)).padding(10.dp)) {
        if (maxWidth >= 720.dp) {
            // Tablet i fold: cały MCP w jednym rzędzie jak w kabinie, belka DISENGAGE pod środkiem
            val dense = maxWidth < 900.dp
            val sz = D7Size(if (dense) 50.dp else (maxWidth / 15f).coerceIn(56.dp, 72.dp), dense)
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.Top) {
                    left(sz, Modifier); speed(sz, Modifier); heading(sz, Modifier); vertical(sz, Modifier); altitude(sz, Modifier)
                }
                disengage(Modifier.padding(top = 8.dp))
            }
        } else {
            val sz = D7Size(if (compact) 56.dp else ((maxWidth - 40.dp) / 4.6f).coerceIn(56.dp, 72.dp))
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.Top) { left(sz, Modifier); disengage(Modifier) }
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly, verticalAlignment = Alignment.Top) { speed(sz, Modifier); heading(sz, Modifier) }
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly, verticalAlignment = Alignment.Top) { vertical(sz, Modifier); altitude(sz, Modifier) }
            }
        }
    }
    if (!compact) Text(loc("Kręć pokrętłem: brzeg — duży krok, środek — mały. HDG: dotknięcie pokrętła = HDG SEL. Kółko V/S przewijaj palcem. Dotknij cyfr, aby wpisać wartość.",
        "Turn a knob: the edge for big steps, the centre for small ones. HDG: tap the knob for HDG SEL. Roll the V/S wheel with a finger. Tap the digits to type a value."),
        style = MaterialTheme.typography.labelSmall, color = Fm.c.ink2, modifier = Modifier.padding(top = 8.dp))

    when (enter) {
        "HDG" -> EnterDialog("HDG", loc("np. 270", "e.g. 270"), KeyboardType.Number, { it.toIntOrNull() in 0..359 }, { cmd("HEADING_BUG_SET", it.toInt()) }) { enter = null }
        "ALT" -> EnterDialog("ALTITUDE (ft)", loc("np. 12000", "e.g. 12000"), KeyboardType.Number, { it.toIntOrNull() in 0..50_000 }, { cmd("AP_ALT_VAR_SET_ENGLISH", it.toInt()) }) { enter = null }
        "SPD" -> EnterDialog("IAS (kt)", loc("np. 250", "e.g. 250"), KeyboardType.Number, { it.toIntOrNull() in 0..500 }, { cmd("AP_SPD_VAR_SET", it.toInt()) }) { enter = null }
        "VS" -> EnterDialog("V/S (ft/min)", loc("np. -1500", "e.g. -1500"), KeyboardType.Number, { it.toIntOrNull() in -8000..8000 }, { cmd("AP_VS_VAR_SET_ENGLISH", it.toInt()) }) { enter = null }
    }
}

// ---------------- Radio: TCP ----------------

private enum class TcpPage { VHF, NAV, XPDR }

/**
 * TCP 787 (Tuning and Control Panel): czarny ekran ze stroną VHF (L i R), NAV (L i R) albo XPDR; częstotliwość aktywna
 * na biało, zapasowa na cyjanowo. Wartość wpisuje się na klawiaturze do linii notatnika i wstawia klawiszem przy linii
 * zapasowej (jak na TCP); klawisz z lewej zamienia aktywną z zapasową. XPDR: kod z notatnika, tryby STBY / ALT OFF / XPDR, IDENT.
 */
@Composable
fun B787Tcp(link: LinkClient, s: FlightState) {
    val r = s.radios
    val x = r.transponder
    var pageName by rememberSaveable { mutableStateOf(TcpPage.VHF.name) }
    val page = TcpPage.valueOf(pageName)
    var scratch by rememberSaveable { mutableStateOf("") }
    var msg by remember { mutableStateOf<String?>(null) }
    fun cmd(n: String, v: Number? = null) { link.command(n, v?.toDouble()) }
    fun f(radio: Radio?, nav: Boolean, stby: Boolean) = (if (stby) radio?.standbyMhz else radio?.activeMhz)?.let { (if (nav) "%.2f" else "%.3f").format(java.util.Locale.US, it) } ?: "---.---"

    // Wstawienie notatnika do linii zapasowej; błędna wartość — komunikat jak na TCP
    fun insert(nav: Boolean, event: String) {
        val hz = ControlMath.parseMhz(scratch)?.let { if (nav) ControlMath.navHz(it) else ControlMath.comHz(it) }
        if (hz == null) { msg = "INVALID ENTRY"; return }
        cmd(event, hz); scratch = ""; msg = null
    }

    D7Plate("TCP") {
        Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(6.dp)).background(D7_SCREEN).border(4.dp, Color(0xFF2A3036), RoundedCornerShape(6.dp)).padding(10.dp)) {
            Text(when (page) { TcpPage.VHF -> "VHF"; TcpPage.NAV -> "NAV RAD"; TcpPage.XPDR -> "XPDR" }, fontFamily = Mono, fontSize = 14.sp, color = X_WHITE,
                modifier = Modifier.fillMaxWidth(), textAlign = TextAlign.Center)
            Spacer(Modifier.height(6.dp))
            when (page) {
                TcpPage.VHF, TcpPage.NAV -> {
                    val nav = page == TcpPage.NAV
                    listOf("L", "R").forEach { side ->
                        val radio = if (nav) (if (side == "L") r.nav1 else r.nav2) else (if (side == "L") r.com1 else r.com2)
                        val swap = if (nav) (if (side == "L") "NAV1_RADIO_SWAP" else "NAV2_RADIO_SWAP") else (if (side == "L") "COM_STBY_RADIO_SWAP" else "COM2_RADIO_SWAP")
                        val stbyEv = if (nav) (if (side == "L") "NAV1_STBY_SET_HZ" else "NAV2_STBY_SET_HZ") else (if (side == "L") "COM_STBY_RADIO_SET_HZ" else "COM2_STBY_RADIO_SET_HZ")
                        val name = (if (nav) "NAV " else "VHF ") + side
                        Row(Modifier.fillMaxWidth().padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                            TcpLsk(loc("Zamień $name", "Transfer $name")) { cmd(swap) }
                            Column(Modifier.weight(1f).padding(horizontal = 8.dp)) {
                                Text(name, fontFamily = Mono, fontSize = 10.sp, color = X_WHITE.copy(alpha = 0.7f))
                                Row(verticalAlignment = Alignment.Bottom) {
                                    Text(f(radio, nav, false), fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 18.sp, color = X_WHITE, modifier = Modifier.weight(1f))
                                    Text(f(radio, nav, true), fontFamily = Mono, fontSize = 16.sp, color = X_CYAN)
                                }
                            }
                            TcpLsk(loc("Wstaw notatnik do zapasowej $name", "Insert scratchpad into $name standby")) { insert(nav, stbyEv) }
                        }
                    }
                }
                TcpPage.XPDR -> {
                    val code = x?.code ?: "----"
                    val emergency = x?.code in setOf("7500", "7600", "7700")
                    Row(Modifier.fillMaxWidth().padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                        TcpLsk("IDENT") { cmd("XPNDR_IDENT_ON") }
                        Column(Modifier.weight(1f).padding(horizontal = 8.dp)) {
                            Text("CODE", fontFamily = Mono, fontSize = 10.sp, color = X_WHITE.copy(alpha = 0.7f))
                            Text(code, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 20.sp, color = if (emergency) X_RED else X_WHITE)
                            Text(if (x?.ident == true) "IDENT" else "<IDENT", fontFamily = Mono, fontSize = 11.sp, color = if (x?.ident == true) X_GREEN else X_CYAN)
                        }
                        TcpLsk(loc("Wstaw kod z notatnika", "Insert code from scratchpad")) {
                            ControlMath.squawk(scratch)?.let { cmd("XPNDR_SET", it); scratch = ""; msg = null } ?: run { msg = "INVALID ENTRY" }
                        }
                    }
                    Row(Modifier.fillMaxWidth().padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        listOf("STBY" to ("standby" to 1), "ALT OFF" to ("on" to 3), "XPDR" to ("alt" to 4)).forEach { (label, mv) ->
                            val on = x?.mode == mv.first
                            Text(label, fontFamily = Mono, fontSize = 13.sp, color = if (on) X_GREEN else X_CYAN, textAlign = TextAlign.Center,
                                modifier = Modifier.weight(1f).clip(RoundedCornerShape(3.dp)).border(1.dp, if (on) X_GREEN else Color(0x55FFFFFF), RoundedCornerShape(3.dp))
                                    .clickable(onClickLabel = loc("Tryb $label", "Mode $label")) { cmd("XPNDR_MODE", mv.second) }.padding(vertical = 6.dp))
                        }
                    }
                }
            }
            Spacer(Modifier.height(6.dp))
            Box(Modifier.fillMaxWidth().height(1.dp).background(X_WHITE.copy(alpha = 0.25f)))
            // Notatnik (scratchpad) — to, co wpisano na klawiaturze, albo komunikat
            Text(msg ?: scratch.ifEmpty { "<DATA" }, fontFamily = Mono, fontSize = 15.sp, color = if (msg != null) X_AMBER else if (scratch.isEmpty()) X_WHITE.copy(alpha = 0.4f) else X_WHITE,
                modifier = Modifier.padding(top = 6.dp))
        }
        Spacer(Modifier.height(10.dp))
        // Klawisze stron pod ekranem, jak rząd VHF / HF / SAT … XPDR na TCP
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            TcpPage.entries.forEach { p ->
                D7Detent(p.name, page == p, Modifier.weight(1f), loc("Strona ${p.name}", "${p.name} page")) { pageName = p.name; msg = null }
            }
        }
        Spacer(Modifier.height(8.dp))
        listOf(listOf("1", "2", "3", "CLR"), listOf("4", "5", "6", "DEL"), listOf("7", "8", "9", ""), listOf(".", "0", "", "")).forEach { row ->
            Row(Modifier.fillMaxWidth().padding(bottom = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                row.forEach { k ->
                    if (k.isEmpty()) Spacer(Modifier.weight(1f))
                    else Box(
                        Modifier.weight(1f).height(38.dp).clip(RoundedCornerShape(5.dp)).background(D7_KEY).border(1.dp, Color(0x33FFFFFF), RoundedCornerShape(5.dp))
                            .clickable(role = Role.Button, onClickLabel = when (k) { "CLR" -> loc("Wyczyść notatnik", "Clear scratchpad"); "DEL" -> loc("Usuń znak", "Delete character"); else -> k }) {
                                msg = null
                                scratch = when (k) { "CLR" -> ""; "DEL" -> scratch.dropLast(1); else -> (scratch + k).take(7) }
                            },
                        contentAlignment = Alignment.Center,
                    ) { Text(k, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 14.sp, color = D7_LEGEND) }
                }
            }
        }
    }
}

/** Klawisz wyboru linii przy ekranie TCP: krótka pozioma belka. */
@Composable
private fun TcpLsk(desc: String, onClick: () -> Unit) {
    Box(
        Modifier.size(width = 28.dp, height = 32.dp).clip(RoundedCornerShape(3.dp)).background(D7_KEY)
            .semantics { contentDescription = desc; role = Role.Button }.clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { Box(Modifier.size(width = 14.dp, height = 3.dp).background(D7_LEGEND)) }
}

// ---------------- Systemy ----------------

/**
 * Systemy 787: oświetlenie z nadgłowia (LANDING, TAXI, BEACON, NAV, LOGO, WING, STROBE, CABIN), dźwignia podwozia
 * z polem GEAR na EICAS, klapy UP–30 (9 pozycji), SPEED BRAKE z ARMED i osią, PARKING BRAKE z osią hamulców kół
 * i STAB TRIM. Pominięte (bez komendy w Link): RUNWAY TURNOFF, osobne L/NOSE/R lądowania, AUTOBRAKE.
 */
@Composable
fun B787Systems(link: LinkClient, s: FlightState) {
    fun cmd(n: String, v: Number? = null) { link.command(n, v?.toDouble()) }
    val ctl = s.controls
    val l = s.lights
    var confirmGear by remember { mutableStateOf(false) }

    D7Plate("EXTERIOR LIGHTS") {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
            D7Toggle("LANDING", l.landing == true) { cmd("LANDING_LIGHTS_TOGGLE") }
            D7Toggle("TAXI", l.taxi == true) { cmd("TOGGLE_TAXI_LIGHTS") }
            D7Toggle("BEACON", l.beacon == true) { cmd("TOGGLE_BEACON_LIGHTS") }
            D7Toggle("NAV", l.nav == true) { cmd("TOGGLE_NAV_LIGHTS") }
        }
        Spacer(Modifier.height(8.dp))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
            D7Toggle("LOGO", l.logo == true) { cmd("TOGGLE_LOGO_LIGHTS") }
            D7Toggle("WING", l.wing == true) { cmd("TOGGLE_WING_LIGHTS") }
            D7Toggle("STROBE", l.strobe == true) { cmd("STROBES_TOGGLE") }
            D7Toggle("CABIN", l.cabin == true) { cmd("TOGGLE_CABIN_LIGHTS") }
        }
    }
    Spacer(Modifier.height(10.dp))

    // Podwozie: na 787 brak lampek na panelu — stan pokazuje pole GEAR na EICAS (DOWN zielone, w ruchu białe kreski, UP)
    val gearDown = ctl.gearDown == true
    val moving = ctl.gearPct?.let { it > 1 && it < 99 } == true
    D7Plate("LANDING GEAR") {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Box(
                Modifier.weight(1f).height(40.dp).clip(RoundedCornerShape(3.dp)).background(Color.Black)
                    .border(2.dp, if (moving) X_WHITE else if (gearDown) X_GREEN else Color(0xFF3A4046), RoundedCornerShape(3.dp)),
                contentAlignment = Alignment.Center,
            ) {
                Text(if (moving) "/ / / / /" else if (gearDown) "DOWN" else "UP", fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 15.sp,
                    color = if (moving) X_WHITE else if (gearDown) X_GREEN else X_WHITE.copy(alpha = 0.7f))
            }
            D7Detent(if (gearDown) "DN" else "UP", true, Modifier.width(80.dp), loc("Dźwignia podwozia", "Gear lever")) {
                if (gearDown && s.aircraft.onGround == true) confirmGear = true else cmd("GEAR_TOGGLE")
            }
        }
    }
    Spacer(Modifier.height(10.dp))

    // Klapy 787: UP, 1, 5, 15, 17, 18, 20, 25, 30 (9 pozycji); przy innej liczbie — numery
    val positions = ctl.flapsPositions?.takeIf { it > 1 }
    D7Plate("FLAPS") {
        if (positions == null) Text("—", color = D7_LEGEND)
        else {
            val names = if (positions == 9) listOf("UP", "1", "5", "15", "17", "18", "20", "25", "30") else (0 until positions).map { "$it" }
            names.chunked(5).forEachIndexed { row, chunk ->
                Row(Modifier.fillMaxWidth().padding(bottom = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    chunk.forEachIndexed { j, name ->
                        val i = row * 5 + j
                        D7Detent(name, ctl.flapsIndex == i, Modifier.weight(1f), loc("Klapy $name", "Flaps $name")) { cmd("FLAPS_SET", (16383.0 * i / (positions - 1)).roundToInt()) }
                    }
                    repeat(5 - chunk.size) { Spacer(Modifier.weight(1f)) }
                }
            }
        }
    }
    Spacer(Modifier.height(10.dp))

    D7Plate("SPEED BRAKE") {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            D7Detent("ARMED", ctl.spoilersArmed == true, Modifier.width(78.dp), loc("Spoilery uzbrojone", "Speed brake armed")) { cmd("SPOILERS_ARM_TOGGLE") }
            SpeedBrakeAxis(link, s, Modifier.weight(1f), label = "DOWN — UP", labelColor = D7_LEGEND)
        }
    }
    Spacer(Modifier.height(10.dp))

    D7Plate("PARKING BRAKE / BRAKES") {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            D7Detent(if (ctl.parkingBrake == true) "SET" else "OFF", ctl.parkingBrake == true, Modifier.width(64.dp), loc("Hamulec postojowy", "Parking brake")) { cmd("PARKING_BRAKES") }
            WheelBrakeAxis(link, s, Modifier.weight(1f), labelColor = D7_LEGEND)
        }
    }
    Spacer(Modifier.height(10.dp))

    D7Plate("STAB TRIM") {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            D7HoldKey("NOSE DN", loc("Trym nosem w dół", "Trim nose down"), Modifier.weight(1f)) { cmd("ELEV_TRIM_DN") }
            Text(ctl.elevatorTrimPct?.let { "%+.1f".format(it) } ?: "--", fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 16.sp, color = X_WHITE,
                textAlign = TextAlign.Center, modifier = Modifier.weight(1f).clip(RoundedCornerShape(4.dp)).background(D7_WINDOW).padding(vertical = 8.dp))
            D7HoldKey("NOSE UP", loc("Trym nosem w górę", "Trim nose up"), Modifier.weight(1f)) { cmd("ELEV_TRIM_UP") }
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

// ---------------- Silniki: EICAS 787 i dźwignie ----------------

@Composable
fun B787Engines(link: LinkClient, s: FlightState) {
    EngineDisplayBox { B787Eicas(s) }
    Spacer(Modifier.height(12.dp))
    Quadrant(link, s, AircraftProfile.B787)
    if (s.autopilot.athr) Text(loc("A/T włączony — symulator może sam przestawiać dźwignie.", "A/T is on — the simulator may move the thrust levers itself."),
        style = MaterialTheme.typography.labelSmall, color = Fm.c.caution, modifier = Modifier.padding(top = 8.dp))
}

/**
 * EICAS 787: TAT u góry z lewej; na silnik zegary N1 i EGT (szary sektor, biały łuk, odczyt w ramce nad zegarem,
 * zielony znacznik dźwigni przy N1), pod nimi N2 i FF w ramkach. Z prawej GEAR (DOWN zielone / UP / w ruchu),
 * FLAPS z paskiem położenia, trym i FUEL TOTAL. Brak danych o oleju i wibracjach w Link — tych pól nie ma.
 */
@Composable
fun B787Eicas(s: FlightState) {
    val m = rememberTextMeasurer()
    val engines = s.engines.take(2).ifEmpty { return }
    val desc = engines.mapIndexed { i, e -> "ENG ${i + 1} N1 ${"%.1f".format(e.n1Pct ?: 0.0)}%" }.joinToString(", ")
    Canvas(Modifier.fillMaxWidth().aspectRatio(1f).clip(RoundedCornerShape(10.dp)).semantics { contentDescription = "EICAS: $desc" }) {
        val w = size.width; val h = size.height
        drawRect(Color.Black)
        (s.avionics?.tatC ?: s.environment.oatC)?.let { d7Text(m, "TAT %+dc".format(it.roundToInt()), w * 0.12f, h * 0.04f, X_WHITE, 12f) }
        val cxs = listOf(w * 0.17f, w * 0.43f)
        val r = w * 0.1f
        engines.forEachIndexed { i, e ->
            val cx = cxs[i]
            val thr = e.throttlePct ?: s.controls.throttlePct
            d7Dial(m, cx, h * 0.2f, r, (e.n1Pct ?: 0.0) / 110.0, 104.0 / 110.0, "%.1f".format(e.n1Pct ?: 0.0), thr?.let { it * 1.04 / 110.0 })
            d7Dial(m, cx, h * 0.46f, r * 0.9f, (e.egtC ?: 0.0) / 1100.0, 0.95, "${(e.egtC ?: 0.0).roundToInt()}", null)
            d7Text(m, e.n2Pct?.let { "%.1f".format(it) } ?: "--", cx, h * 0.64f, X_WHITE, 15f, box = true)
            d7Text(m, e.fuelFlowKgH?.let { "%.1f".format(it / 1000) } ?: "--", cx, h * 0.74f, X_WHITE, 15f, box = true)
        }
        val lx = w * 0.3f
        d7Text(m, "N1", lx, h * 0.27f, X_WHITE, 12f)
        d7Text(m, "EGT", lx, h * 0.52f, X_WHITE, 12f)
        d7Text(m, "N2", lx, h * 0.64f, X_WHITE, 12f)
        d7Text(m, "FF", lx, h * 0.74f, X_WHITE, 12f)

        // Prawa kolumna: GEAR, FLAPS, trym, paliwo
        val rx = w * 0.79f
        val down = s.controls.gearDown == true
        val moving = s.controls.gearPct?.let { it > 1 && it < 99 } == true
        d7Text(m, "GEAR", rx, h * 0.08f, X_WHITE, 11f)
        val gearCol = if (moving) X_WHITE else if (down) X_GREEN else X_WHITE
        drawRect(gearCol, Offset(rx - w * 0.11f, h * 0.11f), Size(w * 0.22f, h * 0.07f), style = Stroke(2.5f))
        d7Text(m, if (moving) "/////" else if (down) "DOWN" else "UP", rx, h * 0.145f, gearCol, 15f)

        d7Text(m, "FLAPS", rx, h * 0.27f, X_WHITE, 11f)
        val fi = s.controls.flapsIndex; val fp = s.controls.flapsPositions
        val frac = if (fi != null && fp != null && fp > 1) fi.toFloat() / (fp - 1) else 0f
        // Pionowy pasek położenia klap: biała ramka, wypełnienie do bieżącego położenia, napis obok
        val bx = rx - w * 0.06f; val by = h * 0.31f; val bh = h * 0.2f
        drawRect(X_WHITE, Offset(bx, by), Size(w * 0.03f, bh), style = Stroke(2f))
        drawRect(X_WHITE, Offset(bx, by), Size(w * 0.03f, bh * frac))
        d7Text(m, if (fi != null && fp == 9) listOf("UP", "1", "5", "15", "17", "18", "20", "25", "30")[fi.coerceIn(0, 8)] else fi?.toString() ?: "--",
            rx + w * 0.06f, by + bh * frac.coerceIn(0.1f, 0.9f), X_MAGENTA.takeIf { moving } ?: X_GREEN, 16f)

        d7Text(m, "STAB", rx, h * 0.58f, X_WHITE, 11f)
        d7Text(m, s.controls.elevatorTrimPct?.let { "%+.1f".format(it) } ?: "--", rx, h * 0.63f, X_GREEN, 15f, box = true)

        drawLine(X_WHITE.copy(alpha = 0.5f), Offset(w * 0.04f, h * 0.83f), Offset(w * 0.96f, h * 0.83f), 1.5f)
        val t = s.fuel.tanks
        fun kg(k: String) = t[k]?.let { "%.1f".format(it / 1000) } ?: "--"
        d7Text(m, "FUEL KG × 1000", w * 0.3f, h * 0.87f, X_WHITE, 11f)
        d7Text(m, kg("leftKg"), w * 0.12f, h * 0.93f, X_WHITE, 15f)
        d7Text(m, kg("centerKg"), w * 0.3f, h * 0.93f, X_WHITE, 15f)
        d7Text(m, kg("rightKg"), w * 0.48f, h * 0.93f, X_WHITE, 15f)
        d7Text(m, "TOTAL FUEL", rx, h * 0.87f, X_WHITE, 11f)
        d7Text(m, s.fuel.totalKg?.let { "%.1f".format(it / 1000) } ?: "--", rx, h * 0.93f, X_WHITE, 18f, box = true)
        if (s.controls.parkingBrake == true) d7Text(m, "PARKING BRAKE SET", rx, h * 0.76f, X_AMBER, 10f)
    }
}

private fun DrawScope.d7Text(m: TextMeasurer, t: String, x: Float, y: Float, col: Color, sz: Float, box: Boolean = false) {
    if (t.isEmpty()) return
    val l = m.measure(t, TextStyle(color = col, fontSize = sz.sp, fontFamily = Mono, fontWeight = FontWeight.Bold))
    val tl = Offset(x - l.size.width / 2f, y - l.size.height / 2f)
    if (box) drawRect(X_WHITE.copy(alpha = 0.85f), Offset(tl.x - 8f, tl.y - 2f), Size(l.size.width + 16f, l.size.height + 4f), style = Stroke(2f))
    drawText(l, topLeft = tl)
}

/** Zegar EICAS 787: łuk 210° od godziny 9, szary sektor do wartości, biała wskazówka, czerwona kreska limitu, odczyt w ramce nad zegarem. */
private fun DrawScope.d7Dial(m: TextMeasurer, cx: Float, cy: Float, r: Float, frac: Double, redFrac: Double, value: String, cmdFrac: Double?) {
    val start = 180f; val sweep = 210f
    val f = frac.coerceIn(0.0, 1.0).toFloat()
    val tl = Offset(cx - r, cy - r); val sz = Size(r * 2, r * 2)
    val path = Path().apply { moveTo(cx, cy); arcTo(androidx.compose.ui.geometry.Rect(tl, sz), start, sweep * f, false); close() }
    drawPath(path, X_FILL)
    drawArc(X_WHITE, start, sweep, false, tl, sz, style = Stroke(2.5f))
    val ra = Math.toRadians(start + sweep * redFrac).toFloat()
    drawLine(X_RED, Offset(cx + r * 0.95f * cos(ra), cy + r * 0.95f * sin(ra)), Offset(cx + r * 1.15f * cos(ra), cy + r * 1.15f * sin(ra)), 4f)
    cmdFrac?.let { c ->
        val ca = Math.toRadians(start + sweep * c.coerceIn(0.0, 1.0)).toFloat()
        drawLine(X_GREEN, Offset(cx + r * 1.02f * cos(ca), cy + r * 1.02f * sin(ca)), Offset(cx + r * 1.2f * cos(ca), cy + r * 1.2f * sin(ca)), 4f, cap = StrokeCap.Round)
    }
    val na = Math.toRadians(start + sweep * f.toDouble()).toFloat()
    drawLine(X_WHITE, Offset(cx, cy), Offset(cx + r * cos(na), cy + r * sin(na)), 3f, cap = StrokeCap.Round)
    val over = f >= redFrac
    d7Text(m, value, cx + r * 0.35f, cy - r * 1.25f, if (over) X_RED else if (f >= redFrac * 0.97) X_AMBER else X_WHITE, 15f, box = true)
}
