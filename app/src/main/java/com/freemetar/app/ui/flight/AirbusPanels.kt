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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
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

// ---------------- Wybór panelu wg profilu ----------------
// A320, 737, 787 i E190 mają własne widoki w stylu kabiny producenta; pozostałe profile — panele ogólne dopasowane do profilu.

@Composable
fun AutopilotFor(link: LinkClient, s: FlightState, p: AircraftProfile, compact: Boolean = false) = when (p) {
    AircraftProfile.A320 -> AirbusFcu(link, s, compact)
    AircraftProfile.B737 -> BoeingMcp(link, s, compact)
    AircraftProfile.B787 -> B787Mcp(link, s, compact)
    AircraftProfile.E190 -> EmbraerGuidancePanel(link, s, compact)
    else -> AutopilotPanelV2(link, s, p, compact)
}

@Composable
fun RadioFor(link: LinkClient, s: FlightState, p: AircraftProfile) = when (p) {
    AircraftProfile.A320 -> { AirbusRmp(link, s); Spacer(Modifier.height(10.dp)); AirbusAtc(link, s) }
    AircraftProfile.B737 -> { BoeingRadioPanel(link, s); Spacer(Modifier.height(10.dp)); BoeingAtc(link, s) }
    AircraftProfile.B787 -> B787Tcp(link, s)
    AircraftProfile.E190 -> EmbraerRadio(link, s)
    else -> RadioPanel(link, s, p)
}

@Composable
fun SystemsFor(link: LinkClient, s: FlightState, p: AircraftProfile) = when (p) {
    AircraftProfile.A320 -> AirbusSystems(link, s)
    AircraftProfile.B737 -> BoeingSystems(link, s)
    AircraftProfile.B787 -> B787Systems(link, s)
    AircraftProfile.E190 -> EmbraerSystems(link, s)
    else -> SystemsPanel(link, s, p)
}

@Composable
fun EnginesFor(link: LinkClient, s: FlightState, p: AircraftProfile) = when (p) {
    AircraftProfile.A320 -> AirbusEngines(link, s)
    AircraftProfile.B737 -> BoeingEngines(link, s)
    AircraftProfile.B787 -> B787Engines(link, s)
    AircraftProfile.E190 -> EmbraerEngines(link, s)
    else -> EnginePanel(link, s, p)
}

// ---------------- Wygląd paneli Airbusa ----------------

/** Szaroniebieska płyta paneli A320, białe opisy, czarne okienka z pomarańczowymi (FCU) albo białymi (RMP) cyframi. */
private val AB_PLATE = Color(0xFF5C768E)
private val AB_PLATE_EDGE = Color(0xFF3B4F61)
private val AB_LEGEND = Color(0xFFF4F7FA)
private val AB_WINDOW = Color(0xFF0A0B0C)
private val AB_ORANGE = Color(0xFFFF9F2E)
private val AB_LAMP = Color(0xFF3FE07A)
private val AB_KEY = Color(0xFF2B3138)
private val AB_CYAN = Color(0xFF4FD1E8)
private val AB_AMBER = Color(0xFFFFB020)
private val AB_RED = Color(0xFFFF3B47)

@Composable
private fun airbusSkin() = panelSkin(AircraftProfile.A320).copy(
    plate = AB_PLATE, plateEdge = AB_PLATE_EDGE, key = AB_KEY, legend = AB_LEGEND, window = AB_WINDOW, windowText = AB_ORANGE,
    windowLabel = AB_LEGEND, lampOn = AB_LAMP, knobOuter = Color(0xFFB9C0C6), knobInner = Color(0xFFDCE1E5), knobMark = Color(0xFF22272C),
)

/** Płyta panelu z nazwą (FCU, RMP, ATC…) w lewym górnym rogu. */
@Composable
private fun AirbusPlate(title: String, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Column(modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(AB_PLATE).border(1.dp, AB_PLATE_EDGE, RoundedCornerShape(12.dp)).padding(10.dp)) {
        Text(title, fontWeight = FontWeight.Bold, fontSize = 11.sp, color = AB_LEGEND, modifier = Modifier.padding(bottom = 8.dp))
        content()
    }
}

/** Czarne okienko z cyframi jak na wyświetlaczach siedmiosegmentowych; dotknięcie — wpisanie wartości. */
@Composable
private fun SegWindow(value: String, color: Color, modifier: Modifier = Modifier, size: Int = 24, desc: String, onClick: (() -> Unit)? = null) {
    Box(
        modifier.clip(RoundedCornerShape(5.dp)).background(AB_WINDOW).border(1.dp, Color(0xFF22282E), RoundedCornerShape(5.dp))
            .semantics { contentDescription = desc }
            .let { if (onClick != null) it.clickable(onClickLabel = loc("Wpisz wartość", "Type a value"), onClick = onClick) else it }
            .padding(horizontal = 8.dp, vertical = 4.dp),
        contentAlignment = Alignment.Center,
    ) { Text(value, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = size.sp, color = color, maxLines = 1, softWrap = false) }
}

/** Mały klawisz PUSH/PULL pod pokrętłem FCU (wciśnięcie = tryb zarządzany, wyciągnięcie = wybrany). */
@Composable
private fun PushPull(label: String, desc: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val haptic = LocalHapticFeedback.current
    Box(
        modifier.height(30.dp).clip(RoundedCornerShape(15.dp)).background(Color(0x33000000)).border(1.dp, Color(0x55FFFFFF), RoundedCornerShape(15.dp))
            .semantics { contentDescription = desc }
            .clickable(role = Role.Button) { haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick() },
        contentAlignment = Alignment.Center,
    ) { Text(label, fontWeight = FontWeight.Bold, fontSize = 10.sp, color = AB_LEGEND) }
}

// ---------------- FCU ----------------

/** Ciemnoszara płyta FCU (jak w MSFS A320neo) i miedziany pierścień u podstawy pokręteł. */
private val FCU_PLATE = Color(0xFF3B4246)
private val FCU_COLLAR = Color(0xFF8C5A30)

/** Kształt pokrętła FCU — każde w A320 wygląda inaczej. */
private enum class FcuKnob { SPD, HDG, ALT, VS }

/**
 * Panel FCU A320 w układzie z kabiny: czarny pas wyświetlacza z pomarańczowymi cyframi (SPD, HDG, ALT, V/S),
 * pod nim pokrętła — każde w swoim kształcie — z PUSH/PULL tam, gdzie MSFS ma odpowiednik
 * (HDG: PULL — kurs wybrany, PUSH — NAV; ALT: PULL — OP CLB/DES, PUSH — utrzymanie; V/S: PULL — V/S, PUSH — wyrównanie),
 * klawisze AP1, FD i A/THR na środku, LOC pod pokrętłami SPD/HDG, APPR pod ALT/V/S.
 * Tablet: wszystko w jednym rzędzie jak na prawdziwym panelu. Telefon: dwa piętra (SPD/HDG, potem ALT/V/S).
 * Pominięte (bez odpowiednika w Link): AP2, EXPED, SPD/MACH, HDG-TRK, METRIC ALT.
 */
@Composable
fun AirbusFcu(link: LinkClient, s: FlightState, compact: Boolean = false) {
    val ap = s.autopilot
    val m = ap.modes
    var enter by remember { mutableStateOf<String?>(null) }
    // Krok pokrętła ALT: 100 albo 1000 ft (przełącznik przy pokrętle, jak na FCU)
    var altStep by rememberSaveable { mutableStateOf(100) }
    fun cmd(n: String, v: Number? = null) { link.command(n, v?.toDouble()) }
    fun live() = link.state.value?.autopilot
    val skin = airbusSkin()

    val spdV = ap.spdKt?.let { "${it.roundToInt()}" } ?: "---"
    val hdgV = ap.hdgDeg?.let { "%03d".format(it.roundToInt() % 360) } ?: "---"
    val altV = ap.altFt?.let { "%05d".format(it.roundToInt()) } ?: "-----"
    val vsV = ap.vsFpm?.let { "%+05d".format(it.roundToInt()) } ?: "-----"
    val levelOff = { cmd("AP_VS_VAR_SET_ENGLISH", 0); if (!m.vs) cmd("AP_VS_HOLD") }

    val spdKnob: @Composable (Dp, Modifier) -> Unit = { kn, mod ->
        FcuKnobColumn(FcuKnob.SPD, "SPD", spdV, kn, mod,
            { d -> cmd("AP_SPD_VAR_SET", ControlMath.speed(live()?.spdKt, d)) }, { d -> cmd("AP_SPD_VAR_SET", ControlMath.speed(live()?.spdKt, d * 10)) },
            onTap = { enter = "SPD" }, push = null, pull = { cmd("AP_PANEL_SPEED_HOLD") })
    }
    val hdgKnob: @Composable (Dp, Modifier) -> Unit = { kn, mod ->
        FcuKnobColumn(FcuKnob.HDG, "HDG", hdgV, kn, mod,
            { d -> cmd("HEADING_BUG_SET", ControlMath.heading(live()?.hdgDeg, d)) }, { d -> cmd("HEADING_BUG_SET", ControlMath.heading(live()?.hdgDeg, d * 10)) },
            onTap = { cmd("AP_NAV1_HOLD") }, push = { cmd("AP_NAV1_HOLD") }, pull = { cmd("AP_HDG_HOLD") })
    }
    val altKnob: @Composable (Dp, Modifier) -> Unit = { kn, mod ->
        FcuKnobColumn(FcuKnob.ALT, "ALT", altV, kn, mod,
            { d -> cmd("AP_ALT_VAR_SET_ENGLISH", ControlMath.altitude(live()?.altFt, d * altStep)) }, { d -> cmd("AP_ALT_VAR_SET_ENGLISH", ControlMath.altitude(live()?.altFt, d * 1000)) },
            onTap = { cmd("AP_ALT_HOLD") }, push = { cmd("AP_ALT_HOLD") }, pull = { cmd("FLIGHT_LEVEL_CHANGE") },
            top = { AltStepSelector(altStep) { altStep = it } })
    }
    val vsKnob: @Composable (Dp, Modifier) -> Unit = { kn, mod ->
        FcuKnobColumn(FcuKnob.VS, "V/S", vsV, kn, mod,
            { d -> cmd("AP_VS_VAR_SET_ENGLISH", ControlMath.verticalSpeed(live()?.vsFpm, d * 100)) }, { d -> cmd("AP_VS_VAR_SET_ENGLISH", ControlMath.verticalSpeed(live()?.vsFpm, d * 500)) },
            onTap = levelOff, push = levelOff, pull = { cmd("AP_VS_HOLD") })
    }
    val apKeys: @Composable (Modifier) -> Unit = { mod ->
        Column(mod, verticalArrangement = Arrangement.spacedBy(8.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                AfdsKey("AP1", ap.master, Modifier.weight(1f), skin) { cmd("AP_MASTER") }
                AfdsKey("FD", ap.fd, Modifier.weight(1f), skin) { cmd("TOGGLE_FLIGHT_DIRECTOR") }
            }
            AfdsKey("A/THR", ap.athr, Modifier.fillMaxWidth(0.62f), skin) { cmd("AUTO_THROTTLE_ARM") }
        }
    }
    val locKey: @Composable (Modifier) -> Unit = { mod -> Box(mod, contentAlignment = Alignment.Center) { AfdsKey("LOC", m.loc, Modifier.width(80.dp), skin) { cmd("AP_LOC_HOLD") } } }
    val apprKey: @Composable (Modifier) -> Unit = { mod -> Box(mod, contentAlignment = Alignment.Center) { AfdsKey("APPR", m.apr, Modifier.width(80.dp), skin) { cmd("AP_APR_HOLD") } } }

    BoxWithConstraints(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(FCU_PLATE).border(1.dp, Color(0xFF1E2225), RoundedCornerShape(10.dp)).padding(10.dp),
    ) {
        if (maxWidth >= 520.dp) {
            // Tablet: jeden pas wyświetlacza i jeden rząd pokręteł, jak na prawdziwym FCU
            val kn = (maxWidth / 7.5f).coerceIn(60.dp, 96.dp)
            Column {
                Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(4.dp)).background(AB_WINDOW).padding(horizontal = 12.dp, vertical = 6.dp), verticalAlignment = Alignment.Bottom) {
                    FcuReadout("SPD", spdV, Modifier.weight(1f)) { enter = "SPD" }
                    FcuReadout("HDG    LAT", hdgV, Modifier.weight(1.1f)) { enter = "HDG" }
                    Text("HDG\nV/S", fontFamily = Mono, fontSize = 9.sp, lineHeight = 10.sp, color = AB_ORANGE, modifier = Modifier.padding(end = 10.dp))
                    Box(Modifier.width(1.dp).height(40.dp).background(Color(0xFF3A3F44)))
                    FcuReadout("ALT ─── LVL/CH", altV, Modifier.weight(1.4f).padding(start = 10.dp)) { enter = "ALT" }
                    FcuReadout("── V/S", vsV, Modifier.weight(1.1f)) { enter = "VS" }
                }
                Spacer(Modifier.height(10.dp))
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Top) {
                    spdKnob(kn, Modifier.weight(1f))
                    hdgKnob(kn, Modifier.weight(1f))
                    apKeys(Modifier.weight(1.2f).padding(horizontal = 6.dp, vertical = 14.dp))
                    altKnob(kn, Modifier.weight(1f))
                    vsKnob(kn, Modifier.weight(1f))
                }
                Spacer(Modifier.height(8.dp))
                Row(Modifier.fillMaxWidth()) { locKey(Modifier.weight(2f)); Spacer(Modifier.weight(1.2f)); apprKey(Modifier.weight(2f)) }
            }
        } else {
            // Telefon: dwa piętra — SPD/HDG z LOC, klawisze AP, potem ALT/V/S z APPR; pasy wyświetlacza nad pokrętłami
            val kn = if (compact) 64.dp else ((maxWidth - 40.dp) / 3.2f).coerceIn(68.dp, 96.dp)
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(4.dp)).background(AB_WINDOW).padding(horizontal = 12.dp, vertical = 6.dp), verticalAlignment = Alignment.Bottom) {
                    FcuReadout("SPD", spdV, Modifier.weight(1f)) { enter = "SPD" }
                    FcuReadout("HDG    LAT", hdgV, Modifier.weight(1f)) { enter = "HDG" }
                }
                Row(Modifier.fillMaxWidth()) { spdKnob(kn, Modifier.weight(1f)); hdgKnob(kn, Modifier.weight(1f)) }
                locKey(Modifier.fillMaxWidth())
                apKeys(Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 4.dp))
                Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(4.dp)).background(AB_WINDOW).padding(horizontal = 12.dp, vertical = 6.dp), verticalAlignment = Alignment.Bottom) {
                    FcuReadout("ALT ── LVL/CH", altV, Modifier.weight(1.2f)) { enter = "ALT" }
                    FcuReadout("── V/S", vsV, Modifier.weight(1f)) { enter = "VS" }
                }
                Row(Modifier.fillMaxWidth()) { altKnob(kn, Modifier.weight(1f)); vsKnob(kn, Modifier.weight(1f)) }
                apprKey(Modifier.fillMaxWidth())
            }
        }
    }
    if (!compact) Text(loc("Kręć pokrętłem palcem: brzeg — duży krok, środek — mały. Dotknięcie = PUSH. Dotknij cyfr, aby wpisać wartość.",
        "Turn a knob with your finger: the edge for big steps, the centre for small ones. A tap = PUSH. Tap the digits to type a value."),
        style = MaterialTheme.typography.labelSmall, color = Fm.c.ink2, modifier = Modifier.padding(top = 8.dp))

    when (enter) {
        "HDG" -> EnterDialog(loc("Kurs (HDG)", "Heading (HDG)"), loc("np. 270", "e.g. 270"), KeyboardType.Number, { it.toIntOrNull() in 0..359 }, { cmd("HEADING_BUG_SET", it.toInt()) }) { enter = null }
        "ALT" -> EnterDialog(loc("Wysokość (ALT, ft)", "Altitude (ALT, ft)"), loc("np. 12000", "e.g. 12000"), KeyboardType.Number, { it.toIntOrNull() in 0..50_000 }, { cmd("AP_ALT_VAR_SET_ENGLISH", it.toInt()) }) { enter = null }
        "SPD" -> EnterDialog(loc("Prędkość (SPD, kt)", "Speed (SPD, kt)"), loc("np. 250", "e.g. 250"), KeyboardType.Number, { it.toIntOrNull() in 0..500 }, { cmd("AP_SPD_VAR_SET", it.toInt()) }) { enter = null }
        "VS" -> EnterDialog(loc("Prędkość pionowa (V/S, ft/min)", "Vertical speed (V/S, ft/min)"), loc("np. -1500", "e.g. -1500"), KeyboardType.Number, { it.toIntOrNull() in -8000..8000 }, { cmd("AP_VS_VAR_SET_ENGLISH", it.toInt()) }) { enter = null }
    }
}

/** Odczyt na pasie FCU: mały pomarańczowy opis nad dużymi cyframi; dotknięcie — wpisanie wartości. */
@Composable
private fun FcuReadout(label: String, value: String, modifier: Modifier, onClick: () -> Unit) {
    Column(modifier.clickable(onClickLabel = loc("Wpisz wartość", "Type a value"), onClick = onClick).semantics { contentDescription = "${label.filter { it.isLetter() || it == '/' }} $value" }) {
        Text(label, fontFamily = Mono, fontSize = 9.sp, color = AB_ORANGE, maxLines = 1, softWrap = false)
        Text(value, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 24.sp, color = AB_ORANGE, maxLines = 1, softWrap = false)
    }
}

/** Przełącznik kroku ALT 100 / 1000 ft — mała dźwigienka nad pokrętłem, jak na FCU. */
@Composable
private fun AltStepSelector(step: Int, onStep: (Int) -> Unit) {
    Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
        listOf(100, 1000).forEach { v ->
            Text("$v", fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 10.sp,
                color = if (step == v) Color.Black else AB_LEGEND,
                modifier = Modifier.clip(RoundedCornerShape(3.dp)).background(if (step == v) AB_LEGEND else Color(0x33000000))
                    .semantics { contentDescription = loc("Krok ALT $v ft", "ALT step $v ft") + if (step == v) loc(", wybrany", ", selected") else ""; role = Role.RadioButton }
                    .clickable { onStep(v) }.padding(horizontal = 5.dp, vertical = 2.dp))
        }
    }
}

/** Kolumna pokrętła: (opcjonalnie coś nad nim), pokrętło w kształcie z FCU, pod nim PUSH / PULL. */
@Composable
private fun FcuKnobColumn(
    kind: FcuKnob, label: String, value: String, size: Dp, modifier: Modifier,
    onStep: (Int) -> Unit, onBig: (Int) -> Unit, onTap: () -> Unit,
    push: (() -> Unit)?, pull: (() -> Unit)?, top: (@Composable () -> Unit)? = null,
) {
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally) {
        Box(Modifier.height(20.dp), contentAlignment = Alignment.Center) { top?.invoke() }
        AirbusKnob(kind, size, loc("Pokrętło $label: $value", "$label knob: $value"), onStep, onBig, onTap)
        Spacer(Modifier.height(6.dp))
        // Na wąskiej kolumnie (tablet, pięć kolumn) klawisze się zwężają, na szerokiej mają do 56 dp
        Row(Modifier.widthIn(max = 116.dp).padding(horizontal = 2.dp), horizontalArrangement = Arrangement.spacedBy(4.dp, Alignment.CenterHorizontally)) {
            if (push != null) PushPull("PUSH", "$label PUSH", Modifier.weight(1f), push)
            if (pull != null) PushPull("PULL", "$label PULL", Modifier.weight(1f), pull)
        }
    }
}

/**
 * Pokrętło FCU obracane palcem wokół środka (zgodnie z ruchem wskazówek = w górę); brzeg — duży krok, środek — mały,
 * dotknięcie bez obrotu = PUSH. Wygląd zależy od pokrętła, jak w A320:
 * SPD — gładki walec z drobnym karbowaniem i kropką, HDG — z wypukłym trójkątem-wskaźnikiem,
 * ALT — wyższy, z żebrowanym chwytem i kreską, V/S — z rowkiem i strzałkami UP/DN.
 */
@Composable
private fun AirbusKnob(kind: FcuKnob, size: Dp, desc: String, onStep: (Int) -> Unit, onBig: (Int) -> Unit, onTap: () -> Unit) {
    RotaryKnob(size, desc, onStep, onBig, onTap) { angle, grip ->
        val c = center
        val r = this.size.minDimension / 2f
        // Miedziany pierścień u podstawy i ciemna tuleja
        drawCircle(FCU_COLLAR, r, c, style = Stroke(r * 0.07f))
        drawCircle(Color(0xFF1C2023), r * 0.92f, c)
        // Korpus: jasnoszary, cieniowany jak walec oświetlony z góry
        val body = when (kind) { FcuKnob.ALT -> r * 0.8f; FcuKnob.VS -> r * 0.84f; else -> r * 0.78f }
        drawCircle(androidx.compose.ui.graphics.Brush.radialGradient(listOf(Color(0xFFE9ECEE), Color(0xFFA9AFB4), Color(0xFF6E757B)),
            Offset(c.x - body * 0.35f, c.y - body * 0.45f), body * 1.6f), body, c)
        drawCircle(if (grip != 0) AB_LAMP else Color(0x55000000), body, c, style = Stroke(if (grip != 0) 3f else 1.5f))
        rotate(angle, c) {
            when (kind) {
                FcuKnob.SPD -> {
                    // Drobne karbowanie na obwodzie i kropka-znacznik
                    for (i in 0 until 48) rotate(i * 7.5f, c) {
                        drawLine(Color(0x55000000), Offset(c.x, c.y - body * 0.98f), Offset(c.x, c.y - body * 0.86f), 1.5f)
                    }
                    drawCircle(Color(0xFF3A3F44), body * 0.09f, Offset(c.x, c.y - body * 0.6f))
                }
                FcuKnob.HDG -> {
                    // Wypukły trójkąt na czapce (jak strzałka kursu)
                    val tri = Path().apply {
                        moveTo(c.x, c.y - body * 0.7f); lineTo(c.x + body * 0.42f, c.y + body * 0.42f); lineTo(c.x - body * 0.42f, c.y + body * 0.42f); close()
                    }
                    drawPath(tri, androidx.compose.ui.graphics.Brush.verticalGradient(listOf(Color(0xFFF4F6F7), Color(0xFF9EA5AA)), c.y - body * 0.7f, c.y + body * 0.42f))
                    drawPath(tri, Color(0x66000000), style = Stroke(2f))
                }
                FcuKnob.ALT -> {
                    // Żebrowany chwyt (12 żeber) i biała kreska położenia
                    for (i in 0 until 12) rotate(i * 30f, c) {
                        drawLine(Color(0x66000000), Offset(c.x, c.y - body * 0.98f), Offset(c.x, c.y - body * 0.62f), 3f)
                    }
                    drawCircle(Color(0xFFCDD2D6), body * 0.55f, c)
                    drawLine(Color(0xFF2A2F33), Offset(c.x, c.y - body * 0.52f), Offset(c.x, c.y - body * 0.1f), 4f, cap = StrokeCap.Round)
                }
                FcuKnob.VS -> {
                    // Rowek przez czapkę (kółko V/S obraca się „w górę / w dół”)
                    drawLine(Color(0x77000000), Offset(c.x - body * 0.75f, c.y), Offset(c.x + body * 0.75f, c.y), 5f, cap = StrokeCap.Round)
                    for (i in 0 until 24) rotate(i * 15f, c) {
                        drawLine(Color(0x44000000), Offset(c.x, c.y - body * 0.98f), Offset(c.x, c.y - body * 0.88f), 2f)
                    }
                }
            }
        }
        // Stałe oznaczenia na płycie przy V/S: strzałki UP / DN po prawej
        if (kind == FcuKnob.VS) {
            drawLine(AB_LEGEND, Offset(c.x + r * 0.97f, c.y - r * 0.55f), Offset(c.x + r * 0.97f, c.y - r * 0.2f), 2f)
            drawLine(AB_LEGEND, Offset(c.x + r * 0.97f, c.y + r * 0.2f), Offset(c.x + r * 0.97f, c.y + r * 0.55f), 2f)
        }
    }
}

/**
 * Pokrętło obracane palcem wokół środka (zgodnie z ruchem wskazówek = w górę): brzeg — duży krok, środek — mały,
 * dotknięcie bez obrotu = PUSH. Wygląd rysuje [draw] (kąt obrotu w stopniach, chwyt: 0 nic, 1 środek, 2 brzeg) —
 * wspólne dla pokręteł FCU Airbusa i MCP Boeinga.
 */
@Composable
internal fun RotaryKnob(size: Dp, desc: String, onStep: (Int) -> Unit, onBig: (Int) -> Unit, onTap: () -> Unit, draw: DrawScope.(Float, Int) -> Unit) {
    val haptic = LocalHapticFeedback.current
    var angle by remember { mutableStateOf(0f) }
    var grip by remember { mutableStateOf(0) }
    val step by rememberUpdatedState(onStep)
    val big by rememberUpdatedState(onBig)
    val tap by rememberUpdatedState(onTap)
    Canvas(
        Modifier.size(size)
            .semantics {
                contentDescription = desc
                role = Role.Button
                onClick { tap(); true }
                customActions = listOf(
                    androidx.compose.ui.semantics.CustomAccessibilityAction(loc("W górę", "Increase")) { step(1); true },
                    androidx.compose.ui.semantics.CustomAccessibilityAction(loc("W dół", "Decrease")) { step(-1); true },
                )
            }
            .pointerInput(Unit) {
                awaitEachGesture {
                    val down = awaitFirstDown()
                    down.consume()
                    val ctr = Offset(this.size.width / 2f, this.size.height / 2f)
                    val r = this.size.width / 2f
                    val ring = (down.position - ctr).getDistance() > r * 0.62f
                    grip = if (ring) 2 else 1
                    val detent = if (ring) 30f else 18f
                    var last = Math.toDegrees(kotlin.math.atan2((down.position.y - ctr.y).toDouble(), (down.position.x - ctr.x).toDouble())).toFloat()
                    var acc = 0f
                    var turned = false
                    while (true) {
                        val ev = awaitPointerEvent()
                        val ch = ev.changes.firstOrNull { it.id == down.id } ?: break
                        if (!ch.pressed) break
                        ch.consume()
                        val p = ch.position - ctr
                        if (kotlin.math.hypot(p.x, p.y) < r * 0.12f) continue
                        val a = Math.toDegrees(kotlin.math.atan2(p.y.toDouble(), p.x.toDouble())).toFloat()
                        var d = a - last
                        if (d > 180f) d -= 360f
                        if (d < -180f) d += 360f
                        last = a; acc += d; angle += d
                        while (acc >= detent || acc <= -detent) {
                            val dir = if (acc > 0) 1 else -1
                            acc -= dir * detent
                            turned = true
                            if (ring) big(dir) else step(dir)
                            haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                        }
                    }
                    if (!turned) { haptic.performHapticFeedback(HapticFeedbackType.LongPress); tap() }
                    grip = 0
                }
            },
    ) {
        draw(angle, grip)
    }
}

// ---------------- RMP i ATC ----------------

private enum class RmpSel(val label: String, val com: Boolean) { VHF1("VHF1", true), VHF2("VHF2", true), NAV1("NAV1", false), NAV2("NAV2", false) }

/**
 * Panel radiowy RMP: okienka ACTIVE i STBY/CRS, zielony przycisk zamiany i klawisze wyboru radia (VHF1, VHF2,
 * a pod napisem STBY NAV — NAV1 i NAV2). Pokrętło zmienia częstotliwość zapasową: pierścień o 1 MHz, środek o 25 kHz
 * (VHF) albo 50 kHz (NAV). Dotknięcie okienka STBY — wpisanie częstotliwości.
 */
@Composable
fun AirbusRmp(link: LinkClient, s: FlightState) {
    var selName by rememberSaveable { mutableStateOf(RmpSel.VHF1.name) }
    val sel = RmpSel.valueOf(selName)
    var enter by remember { mutableStateOf(false) }
    fun cmd(n: String, v: Number? = null) { link.command(n, v?.toDouble()) }
    val r = s.radios
    val radio: Radio? = when (sel) { RmpSel.VHF1 -> r.com1; RmpSel.VHF2 -> r.com2; RmpSel.NAV1 -> r.nav1; RmpSel.NAV2 -> r.nav2 }
    val fmt = if (sel.com) "%.3f" else "%.2f"
    fun liveStby(): Double? = link.state.value?.radios?.let { when (sel) { RmpSel.VHF1 -> it.com1; RmpSel.VHF2 -> it.com2; RmpSel.NAV1 -> it.nav1; RmpSel.NAV2 -> it.nav2 } }?.standbyMhz
    // Nowa częstotliwość zapasowa: krok w kHz, zawinięcie w zakresie pasma (VHF 118–136,975, NAV 108–117,95)
    fun setStby(stepKhz: Int) {
        val cur = liveStby() ?: return
        val (lo, hi) = if (sel.com) 118_000 to 136_975 else 108_000 to 117_950
        var khz = (cur * 1000).roundToInt() + stepKhz
        if (khz > hi) khz = lo + (khz - hi - 1)
        if (khz < lo) khz = hi - (lo - khz - 1)
        val hz = khz.toLong() * 1000
        when (sel) {
            RmpSel.VHF1 -> cmd("COM_STBY_RADIO_SET_HZ", hz)
            RmpSel.VHF2 -> cmd("COM2_STBY_RADIO_SET_HZ", hz)
            RmpSel.NAV1 -> cmd("NAV1_STBY_SET_HZ", hz)
            RmpSel.NAV2 -> cmd("NAV2_STBY_SET_HZ", hz)
        }
    }
    val swap = when (sel) { RmpSel.VHF1 -> "COM_STBY_RADIO_SWAP"; RmpSel.VHF2 -> "COM2_RADIO_SWAP"; RmpSel.NAV1 -> "NAV1_RADIO_SWAP"; RmpSel.NAV2 -> "NAV2_RADIO_SWAP" }
    val active = radio?.activeMhz?.let { fmt.format(java.util.Locale.US, it) } ?: "---.---"
    val stby = radio?.standbyMhz?.let { fmt.format(java.util.Locale.US, it) } ?: "---.---"

    AirbusPlate("RMP") {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Column(Modifier.weight(1f), horizontalAlignment = Alignment.CenterHorizontally) {
                Text("ACTIVE", fontWeight = FontWeight.Bold, fontSize = 11.sp, color = AB_LEGEND)
                SegWindow(active, Color.White, Modifier.fillMaxWidth().padding(top = 3.dp), size = 22, desc = loc("${sel.label} aktywna $active", "${sel.label} active $active"))
            }
            Box(
                Modifier.size(width = 52.dp, height = 40.dp).clip(RoundedCornerShape(6.dp)).background(AB_KEY)
                    .semantics { contentDescription = loc("Zamień aktywną i zapasową ${sel.label}", "Swap active and standby ${sel.label}") }
                    .clickable(role = Role.Button) { cmd(swap) },
                contentAlignment = Alignment.Center,
            ) { Icon(Icons.Outlined.SwapHoriz, null, tint = AB_LAMP, modifier = Modifier.size(30.dp)) }
            Column(Modifier.weight(1f), horizontalAlignment = Alignment.CenterHorizontally) {
                Text("STBY/CRS", fontWeight = FontWeight.Bold, fontSize = 11.sp, color = AB_LEGEND)
                SegWindow(stby, Color.White, Modifier.fillMaxWidth().padding(top = 3.dp), size = 22, desc = loc("${sel.label} zapasowa $stby", "${sel.label} standby $stby")) { enter = true }
            }
        }
        Spacer(Modifier.height(12.dp))
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    RmpKey("VHF1", sel == RmpSel.VHF1, Modifier.weight(1f)) { selName = RmpSel.VHF1.name }
                    RmpKey("VHF2", sel == RmpSel.VHF2, Modifier.weight(1f)) { selName = RmpSel.VHF2.name }
                }
                // Napis STBY NAV z klamrą nad klawiszami nawigacji, jak na panelu
                Text("— STBY NAV —", fontWeight = FontWeight.Bold, fontSize = 10.sp, color = AB_LEGEND, modifier = Modifier.fillMaxWidth(), textAlign = TextAlign.Center)
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    RmpKey("NAV1", sel == RmpSel.NAV1, Modifier.weight(1f)) { selName = RmpSel.NAV1.name }
                    RmpKey("NAV2", sel == RmpSel.NAV2, Modifier.weight(1f)) { selName = RmpSel.NAV2.name }
                }
            }
            DualKnob(84.dp, loc("Pokrętło częstotliwości ${sel.label}", "${sel.label} frequency knob"), skin = airbusSkin(),
                onStep = { setStby(it * if (sel.com) 25 else 50) }, onBigStep = { setStby(it * 1000) }, onPush = { enter = true })
        }
    }
    if (enter) {
        val name = sel.label
        if (sel.com) EnterDialog(loc("$name — częstotliwość zapasowa (MHz)", "$name — standby frequency (MHz)"), "np. 121.500", KeyboardType.Decimal,
            { ControlMath.parseMhz(it)?.let(ControlMath::comHz) != null },
            { t -> ControlMath.parseMhz(t)?.let(ControlMath::comHz)?.let { hz -> cmd(if (sel == RmpSel.VHF1) "COM_STBY_RADIO_SET_HZ" else "COM2_STBY_RADIO_SET_HZ", hz) } }) { enter = false }
        else EnterDialog(loc("$name — częstotliwość zapasowa (MHz)", "$name — standby frequency (MHz)"), "np. 110.30", KeyboardType.Decimal,
            { ControlMath.parseMhz(it)?.let(ControlMath::navHz) != null },
            { t -> ControlMath.parseMhz(t)?.let(ControlMath::navHz)?.let { hz -> cmd(if (sel == RmpSel.NAV1) "NAV1_STBY_SET_HZ" else "NAV2_STBY_SET_HZ", hz) } }) { enter = false }
    }
}

/** Klawisz wyboru radia RMP: zielony trójkąt nad klawiszem, gdy radio jest wybrane. */
@Composable
private fun RmpKey(label: String, on: Boolean, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val haptic = LocalHapticFeedback.current
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally) {
        Text("▼", fontSize = 9.sp, color = if (on) AB_LAMP else Color(0x33FFFFFF))
        Box(
            Modifier.fillMaxWidth().height(38.dp).clip(RoundedCornerShape(5.dp)).background(AB_KEY).border(1.dp, Color(0x40FFFFFF), RoundedCornerShape(5.dp))
                .semantics { contentDescription = label + if (on) loc(", wybrane", ", selected") else ""; role = Role.RadioButton }
                .clickable { haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove); onClick() },
            contentAlignment = Alignment.Center,
        ) { Text(label, fontWeight = FontWeight.Bold, fontSize = 12.sp, color = AB_LEGEND) }
    }
}

/**
 * Panel ATC (transponder) A320: okienko kodu, klawiatura 0–7 z CLR — czwarta cyfra wysyła kod,
 * przełącznik trybu STBY / AUTO / ON i klawisz IDENT.
 */
@Composable
fun AirbusAtc(link: LinkClient, s: FlightState) {
    val x = s.radios.transponder
    var typed by rememberSaveable { mutableStateOf("") }
    fun cmd(n: String, v: Number? = null) { link.command(n, v?.toDouble()) }
    val shown = if (typed.isNotEmpty()) typed.padEnd(4, '-') else x?.code ?: "----"
    val emergency = typed.isEmpty() && x?.code in setOf("7500", "7600", "7700")

    AirbusPlate("ATC / XPDR") {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            SegWindow(shown, if (emergency) AB_RED else AB_ORANGE, Modifier.weight(1f), size = 26, desc = loc("Kod transpondera $shown", "Transponder code $shown"))
            Box(
                Modifier.size(width = 72.dp, height = 44.dp).clip(RoundedCornerShape(5.dp)).background(AB_KEY)
                    .border(1.dp, if (x?.ident == true) AB_LAMP else Color(0x40FFFFFF), RoundedCornerShape(5.dp))
                    .clickable(role = Role.Button, onClickLabel = "IDENT") { cmd("XPNDR_IDENT_ON") },
                contentAlignment = Alignment.Center,
            ) { Text("IDENT", fontWeight = FontWeight.Bold, fontSize = 12.sp, color = if (x?.ident == true) AB_LAMP else AB_LEGEND) }
        }
        Spacer(Modifier.height(10.dp))
        // Klawiatura jak na panelu ATC: 0–7 i CLR (kasuje wpisywane cyfry)
        listOf(listOf("1", "2", "3", "4"), listOf("5", "6", "7", "0")).forEach { row ->
            Row(Modifier.fillMaxWidth().padding(bottom = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                row.forEach { d ->
                    AtcKey(d, Modifier.weight(1f)) {
                        val next = typed + d
                        if (next.length == 4) { ControlMath.squawk(next)?.let { cmd("XPNDR_SET", it) }; typed = "" } else typed = next
                    }
                }
            }
        }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            AtcKey("CLR", Modifier.weight(1f)) { typed = "" }
            // Tryb: STBY (1), AUTO = z wysokością (4), ON (3)
            listOf("STBY" to ("standby" to 1), "AUTO" to ("alt" to 4), "ON" to ("on" to 3)).forEach { (label, mv) ->
                val on = x?.mode == mv.first
                Box(
                    Modifier.weight(1f).height(40.dp).clip(RoundedCornerShape(5.dp)).background(if (on) Color(0xFF1C2228) else AB_KEY)
                        .border(1.dp, if (on) AB_LAMP else Color(0x40FFFFFF), RoundedCornerShape(5.dp))
                        .semantics { contentDescription = loc("Tryb $label", "Mode $label") + if (on) loc(", wybrany", ", selected") else ""; role = Role.RadioButton }
                        .clickable { cmd("XPNDR_MODE", mv.second) },
                    contentAlignment = Alignment.Center,
                ) { Text(label, fontWeight = FontWeight.Bold, fontSize = 12.sp, color = if (on) AB_LAMP else AB_LEGEND) }
            }
        }
    }
}

@Composable
private fun AtcKey(label: String, modifier: Modifier, onClick: () -> Unit) {
    val haptic = LocalHapticFeedback.current
    Box(
        modifier.height(40.dp).clip(RoundedCornerShape(5.dp)).background(AB_KEY).border(1.dp, Color(0x40FFFFFF), RoundedCornerShape(5.dp))
            .clickable(role = Role.Button, onClickLabel = label) { haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove); onClick() },
        contentAlignment = Alignment.Center,
    ) { Text(label, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 15.sp, color = AB_LEGEND) }
}

// ---------------- Systemy: EXT LT, podwozie, klapy, hamulec aerodynamiczny, hamulec postojowy, trymer ----------------

/** Klawisz-przełącznik w stylu nadgłowia Airbusa: ciemny, z białym „ON”, gdy włączony. */
@Composable
private fun OverheadSwitch(label: String, on: Boolean, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val haptic = LocalHapticFeedback.current
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally) {
        Box(
            Modifier.fillMaxWidth().height(44.dp).clip(RoundedCornerShape(4.dp)).background(Color(0xFF1A1E22)).border(1.dp, Color(0x55FFFFFF), RoundedCornerShape(4.dp))
                .semantics { contentDescription = label + if (on) loc(", włączone", ", on") else loc(", wyłączone", ", off"); role = Role.Switch }
                .clickable { haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick() },
            contentAlignment = Alignment.Center,
        ) { Text(if (on) "ON" else "OFF", fontWeight = FontWeight.Bold, fontSize = 13.sp, color = if (on) Color.White else Color(0x66FFFFFF)) }
        Text(label, fontWeight = FontWeight.Bold, fontSize = 10.sp, color = AB_LEGEND, maxLines = 1, modifier = Modifier.padding(top = 3.dp))
    }
}

/** Klawisz pozycji dźwigni (klapy, hamulec aerodynamiczny): podświetlony, gdy dźwignia jest w tej pozycji. */
@Composable
private fun DetentKey(label: String, on: Boolean, modifier: Modifier = Modifier, desc: String = label, onClick: () -> Unit) {
    val haptic = LocalHapticFeedback.current
    Box(
        modifier.height(42.dp).clip(RoundedCornerShape(5.dp)).background(if (on) Color.White else AB_KEY).border(1.dp, Color(0x40FFFFFF), RoundedCornerShape(5.dp))
            .semantics { contentDescription = desc + if (on) loc(", ustawione", ", set") else ""; role = Role.RadioButton }
            .clickable { haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick() },
        contentAlignment = Alignment.Center,
    ) { Text(label, fontWeight = FontWeight.Bold, fontSize = 12.sp, color = if (on) Color.Black else AB_LEGEND, maxLines = 1) }
}

/** Klawisz z powtarzaniem przy przytrzymaniu (trymer): 400 ms, potem co 90 ms. */
@Composable
private fun HoldKey(label: String, desc: String, modifier: Modifier = Modifier, action: () -> Unit) {
    val scope = rememberCoroutineScope()
    val act by rememberUpdatedState(action)
    Box(
        modifier.height(44.dp).clip(RoundedCornerShape(5.dp)).background(AB_KEY).border(1.dp, Color(0x40FFFFFF), RoundedCornerShape(5.dp))
            .semantics { contentDescription = desc; role = Role.Button; onClick { act(); true } }
            .pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown()
                    act()
                    val job = scope.launch { delay(400); while (true) { act(); delay(90) } }
                    waitForUpOrCancellation()
                    job.cancel()
                }
            },
        contentAlignment = Alignment.Center,
    ) { Text(label, fontWeight = FontWeight.Bold, fontSize = 13.sp, color = AB_LEGEND) }
}

@Composable
fun AirbusSystems(link: LinkClient, s: FlightState) {
    fun cmd(n: String, v: Number? = null) { link.command(n, v?.toDouble()) }
    val ctl = s.controls
    val l = s.lights
    var confirmGear by remember { mutableStateOf(false) }

    // Nadgłowie: oświetlenie zewnętrzne (EXT LT) w kolejności z panelu
    AirbusPlate("EXT LT") {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            OverheadSwitch("STROBE", l.strobe == true, Modifier.weight(1f)) { cmd("STROBES_TOGGLE") }
            OverheadSwitch("BEACON", l.beacon == true, Modifier.weight(1f)) { cmd("TOGGLE_BEACON_LIGHTS") }
            OverheadSwitch("WING", l.wing == true, Modifier.weight(1f)) { cmd("TOGGLE_WING_LIGHTS") }
            OverheadSwitch("NAV", l.nav == true, Modifier.weight(1f)) { cmd("TOGGLE_NAV_LIGHTS") }
        }
        Spacer(Modifier.height(8.dp))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            OverheadSwitch("LOGO", l.logo == true, Modifier.weight(1f)) { cmd("TOGGLE_LOGO_LIGHTS") }
            OverheadSwitch("LAND", l.landing == true, Modifier.weight(1f)) { cmd("LANDING_LIGHTS_TOGGLE") }
            OverheadSwitch("NOSE", l.taxi == true, Modifier.weight(1f)) { cmd("TOGGLE_TAXI_LIGHTS") }
            OverheadSwitch("CABIN", l.cabin == true, Modifier.weight(1f)) { cmd("TOGGLE_CABIN_LIGHTS") }
        }
    }
    Spacer(Modifier.height(10.dp))

    // Podwozie: dźwignia UP/DOWN i trzy wskaźniki położenia (zielone trójkąty = zablokowane w dole)
    val gearDown = ctl.gearDown == true
    val moving = ctl.gearPct?.let { it > 1 && it < 99 } == true
    AirbusPlate("L/G") {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(Modifier.weight(1f), horizontalArrangement = Arrangement.SpaceEvenly) {
                repeat(3) {
                    Text(if (moving) "UNLK" else if (gearDown) "▼" else "", fontFamily = Mono, fontWeight = FontWeight.Bold,
                        fontSize = if (moving) 11.sp else 22.sp, color = if (moving) AB_RED else AB_LAMP,
                        modifier = Modifier.width(44.dp).height(34.dp).clip(RoundedCornerShape(4.dp)).background(AB_WINDOW).padding(top = 4.dp),
                        textAlign = TextAlign.Center)
                }
            }
            DetentKey(if (gearDown) "DOWN" else "UP", true, Modifier.width(96.dp), loc("Dźwignia podwozia", "Gear lever")) {
                if (gearDown && s.aircraft.onGround == true) confirmGear = true else cmd("GEAR_TOGGLE")
            }
        }
    }
    Spacer(Modifier.height(10.dp))

    // Klapy: dźwignia CONF 0 / 1 / 2 / 3 / FULL (FLAPS_SET); przy innej liczbie pozycji — numery
    val positions = ctl.flapsPositions?.takeIf { it > 1 }
    AirbusPlate("FLAPS") {
        if (positions == null) Text("—", color = AB_LEGEND)
        else Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            repeat(positions) { i ->
                val label = if (positions == 5) listOf("0", "1", "2", "3", "FULL")[i] else "$i"
                DetentKey(label, ctl.flapsIndex == i, Modifier.weight(1f), loc("Klapy $label", "Flaps $label")) {
                    cmd("FLAPS_SET", (16383.0 * i / (positions - 1)).roundToInt())
                }
            }
        }
    }
    Spacer(Modifier.height(10.dp))

    // Hamulec aerodynamiczny: ARM (uzbrojenie spoilerów do lądowania) i obok oś dźwigni
    AirbusPlate("SPEED BRAKE") {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            DetentKey("ARM", ctl.spoilersArmed == true, Modifier.width(72.dp), loc("Uzbrojenie spoilerów", "Ground spoilers arm")) { cmd("SPOILERS_ARM_TOGGLE") }
            SpeedBrakeAxis(link, s, Modifier.weight(1f), label = "SPEED BRAKE", labelColor = AB_LEGEND)
        }
    }
    Spacer(Modifier.height(10.dp))

    // Hamulec postojowy (mały klawisz) i obok oś hamulców kół (pedały)
    AirbusPlate("PARK BRK / BRAKES") {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            DetentKey(if (ctl.parkingBrake == true) "ON" else "OFF", ctl.parkingBrake == true, Modifier.width(72.dp), loc("Hamulec postojowy", "Parking brake")) { cmd("PARKING_BRAKES") }
            WheelBrakeAxis(link, s, Modifier.weight(1f), labelColor = AB_LEGEND)
        }
    }
    Spacer(Modifier.height(10.dp))

    // Trymer wysokości (koło trymera na pedestale)
    AirbusPlate("PITCH TRIM") {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            HoldKey("DN", loc("Trym nosem w dół", "Trim nose down"), Modifier.weight(1f)) { cmd("ELEV_TRIM_DN") }
            SegWindow(ctl.elevatorTrimPct?.let { "%+.1f".format(it) } ?: "--", Color.White, Modifier.weight(1.2f), size = 16, desc = loc("Trym", "Trim"))
            HoldKey("UP", loc("Trym nosem w górę", "Trim nose up"), Modifier.weight(1f)) { cmd("ELEV_TRIM_UP") }
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

// ---------------- Silniki: E/WD i dźwignie ciągu ----------------

@Composable
fun AirbusEngines(link: LinkClient, s: FlightState) {
    EngineDisplayBox { AirbusEwd(s) }
    Spacer(Modifier.height(12.dp))
    Quadrant(link, s, AircraftProfile.A320)
    if (s.autopilot.athr) Text(loc("A/THR włączony — symulator może sam przestawiać ciąg.", "A/THR is on — the simulator may move the thrust itself."),
        style = MaterialTheme.typography.labelSmall, color = Fm.c.caution, modifier = Modifier.padding(top = 8.dp))
}

/**
 * Górny ekran ECAM (E/WD) A320: na silnik łuk N1 z odczytem, niebieski znacznik położenia dźwigni i czerwony limit,
 * pod nim łuk EGT; w środku kolumna opisów (N1 %, EGT °C, N2 %, FF KG/H) jak na ekranie, po prawej FOB i położenie klap.
 */
@Composable
fun AirbusEwd(s: FlightState) {
    val m = rememberTextMeasurer()
    val engines = s.engines.take(2).ifEmpty { return }
    val desc = engines.mapIndexed { i, e -> "ENG ${i + 1} N1 ${"%.1f".format(e.n1Pct ?: 0.0)}%" }.joinToString(", ")
    Canvas(Modifier.fillMaxWidth().aspectRatio(1.15f).clip(RoundedCornerShape(10.dp)).semantics { contentDescription = "E/WD: $desc" }) {
        val w = size.width; val h = size.height
        drawRect(Color.Black)
        fun text(t: String, x: Float, y: Float, col: Color, sz: Float, bold: Boolean = false, alignLeft: Boolean = false) {
            val l = m.measure(t, TextStyle(color = col, fontSize = sz.sp, fontFamily = Mono, fontWeight = if (bold) FontWeight.Bold else FontWeight.Normal))
            drawText(l, topLeft = Offset(if (alignLeft) x else x - l.size.width / 2f, y - l.size.height / 2f))
        }
        // Dwa silniki symetrycznie po bokach kolumny opisów
        val cxs = listOf(w * 0.24f, w * 0.62f)
        engines.forEachIndexed { i, e ->
            val cx = cxs[i]
            // Łuk N1: 0–110 %, kąt początkowy 150°, rozpiętość 220°
            val n1 = e.n1Pct ?: 0.0
            val r = w * 0.15f; val cy = h * 0.2f
            val start = 150f; val sweep = 220f
            fun ang(v: Double, max: Double) = Math.toRadians(start + sweep * (v / max).coerceIn(0.0, 1.0)).toFloat()
            drawArc(Color.White, start, sweep * (104f / 110f), false, Offset(cx - r, cy - r), Size(r * 2, r * 2), style = Stroke(3f))
            drawArc(AB_RED, start + sweep * (104f / 110f), sweep * (6f / 110f), false, Offset(cx - r, cy - r), Size(r * 2, r * 2), style = Stroke(5f))
            // Podziałka co 10 % od 20 %
            for (v in 20..100 step 20) {
                val a = ang(v.toDouble(), 110.0)
                drawLine(Color.White, Offset(cx + r * 0.85f * cos(a), cy + r * 0.85f * sin(a)), Offset(cx + r * cos(a), cy + r * sin(a)), 2f)
            }
            // Wskazówka N1 (zielona) i znacznik dźwigni ciągu (niebieskie kółko na łuku)
            val na = ang(n1, 110.0)
            drawLine(AB_LAMP, Offset(cx, cy), Offset(cx + r * cos(na), cy + r * sin(na)), 4f, cap = StrokeCap.Round)
            (e.throttlePct ?: s.controls.throttlePct)?.let { thr ->
                val ta = ang(thr * 1.04, 110.0)
                drawCircle(AB_CYAN, 5f, Offset(cx + r * 1.12f * cos(ta), cy + r * 1.12f * sin(ta)), style = Stroke(2.5f))
            }
            // Odczyt N1 w ramce
            drawRect(Color.White.copy(alpha = 0.7f), Offset(cx - r * 0.05f, cy + r * 0.12f), Size(r * 1.05f, r * 0.5f), style = Stroke(2f))
            text("%.1f".format(n1), cx + r * 0.47f, cy + r * 0.37f, AB_LAMP, 17f, true)
            // Łuk EGT: 0–1000 °C, czerwień od 950
            val egt = e.egtC ?: 0.0
            val r2 = r * 0.72f; val cy2 = h * 0.48f
            drawArc(Color.White, start, sweep * 0.95f, false, Offset(cx - r2, cy2 - r2), Size(r2 * 2, r2 * 2), style = Stroke(3f))
            drawArc(AB_RED, start + sweep * 0.95f, sweep * 0.05f, false, Offset(cx - r2, cy2 - r2), Size(r2 * 2, r2 * 2), style = Stroke(5f))
            val ea = ang(egt, 1000.0)
            drawLine(if (egt > 950) AB_RED else if (egt > 850) AB_AMBER else AB_LAMP, Offset(cx, cy2), Offset(cx + r2 * cos(ea), cy2 + r2 * sin(ea)), 4f, cap = StrokeCap.Round)
            text("${egt.roundToInt()}", cx + r2 * 0.4f, cy2 + r2 * 0.45f, AB_LAMP, 15f, true)
            // N2 i przepływ paliwa w wierszach
            text(e.n2Pct?.let { "%.1f".format(it) } ?: "XX", cx, h * 0.68f, AB_LAMP, 15f, true)
            text(e.fuelFlowKgH?.let { "${(it / 10).roundToInt() * 10}" } ?: "XX", cx, h * 0.78f, AB_LAMP, 15f, true)
        }
        // Kolumna opisów między silnikami
        val lx = w * 0.43f
        text("N1", lx, h * 0.2f, Color.White, 12f, true); text("%", lx, h * 0.25f, AB_CYAN, 11f)
        text("EGT", lx, h * 0.48f, Color.White, 12f, true); text("°C", lx, h * 0.53f, AB_CYAN, 11f)
        text("N2", lx, h * 0.68f, Color.White, 12f, true); text("%", lx + w * 0.045f, h * 0.68f, AB_CYAN, 11f)
        text("FF", lx, h * 0.78f, Color.White, 12f, true); text("KG/H", lx, h * 0.83f, AB_CYAN, 10f)
        // Prawa kolumna: FOB i klapy (S/F z numerem pozycji)
        val rx = w * 0.82f
        text("FOB :", rx, h * 0.08f, Color.White, 12f)
        text(s.fuel.totalKg?.let { "${(it / 10).roundToInt() * 10}" } ?: "XX", rx, h * 0.14f, AB_LAMP, 16f, true)
        text("KG", rx + w * 0.11f, h * 0.14f, AB_CYAN, 10f)
        val fi = s.controls.flapsIndex
        val fp = s.controls.flapsPositions
        text("FLAP", rx, h * 0.62f, Color.White, 11f)
        text(if (fi != null && fp == 5) listOf("0", "1", "2", "3", "FULL")[fi.coerceIn(0, 4)] else fi?.toString() ?: "--", rx, h * 0.68f, AB_LAMP, 16f, true)
        if (s.controls.spoilersArmed == true) text("SPLR ARM", rx, h * 0.78f, AB_LAMP, 11f)
        s.environment.oatC?.let { text("SAT %+d°C".format(it.roundToInt()), w * 0.06f, h * 0.95f, AB_CYAN, 11f, alignLeft = true) }
        if (s.autopilot.athr) text("A/THR", w * 0.82f, h * 0.95f, Color.White, 12f, true)
    }
}
