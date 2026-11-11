package com.freemetar.app.ui.flight

import com.freemetar.app.domain.loc

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
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.ExpandMore
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.freemetar.app.data.link.FlightState
import com.freemetar.app.data.link.LinkClient
import com.freemetar.app.domain.AircraftProfile
import com.freemetar.app.domain.AutopilotKind
import com.freemetar.app.domain.ControlMath
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.Mono
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.atan2
import kotlin.math.hypot
import kotlin.math.roundToInt

/** Granica między wewnętrzną gałką a zewnętrznym pierścieniem (ułamek promienia). */
private const val INNER = 0.58f

/**
 * Podwójne pokrętło jak w awionice Garmina: zewnętrzny pierścień = duży krok, wewnętrzna gałka = mały.
 * Obrót palcem wokół środka; zgodnie z ruchem wskazówek zegara = w górę. Każdy krok to „ząbek” z wibracją.
 * Dotknięcie gałki bez obrotu = „wciśnięcie” (wpisanie wartości).
 */
@Composable
fun DualKnob(
    size: Dp, desc: String, modifier: Modifier = Modifier, skin: PanelSkin? = null,
    onStep: (Int) -> Unit, onBigStep: (Int) -> Unit, onPush: () -> Unit,
) {
    val c = Fm.c
    val haptic = LocalHapticFeedback.current
    var inner by remember { mutableFloatStateOf(0f) }   // kąt wizualny gałki (stopnie)
    var outer by remember { mutableFloatStateOf(0f) }
    var grip by remember { mutableStateOf(0) }           // 0 nic, 1 gałka, 2 pierścień — podświetlenie
    val step by rememberUpdatedState(onStep)
    val big by rememberUpdatedState(onBigStep)
    val push by rememberUpdatedState(onPush)

    Canvas(
        modifier.size(size)
            .semantics {
                contentDescription = desc
                role = Role.Button
                onClick { push(); true }
                customActions = listOf(
                    CustomAccessibilityAction(loc("common.increase")) { step(1); true },
                    CustomAccessibilityAction(loc("common.decrease")) { step(-1); true },
                    CustomAccessibilityAction(loc("autopilots.big_step_up")) { big(1); true },
                    CustomAccessibilityAction(loc("autopilots.big_step_down")) { big(-1); true },
                )
            }
            .pointerInput(Unit) {
                awaitEachGesture {
                    val down = awaitFirstDown()
                    val ctr = Offset(this.size.width / 2f, this.size.height / 2f)
                    val r = this.size.width / 2f
                    val ring = (down.position - ctr).getDistance() > r * INNER
                    grip = if (ring) 2 else 1
                    down.consume()
                    val detent = if (ring) 30f else 20f      // stopnie na ząbek
                    var last = Math.toDegrees(atan2((down.position.y - ctr.y).toDouble(), (down.position.x - ctr.x).toDouble())).toFloat()
                    var acc = 0f
                    var turned = false
                    while (true) {
                        val ev = awaitPointerEvent()
                        val ch = ev.changes.firstOrNull { it.id == down.id } ?: break
                        if (!ch.pressed) break
                        ch.consume()
                        val p = ch.position - ctr
                        if (hypot(p.x, p.y) < r * 0.12f) continue   // przy samym środku kąt skacze
                        val a = Math.toDegrees(atan2(p.y.toDouble(), p.x.toDouble())).toFloat()
                        var d = a - last
                        if (d > 180f) d -= 360f
                        if (d < -180f) d += 360f
                        last = a
                        acc += d
                        if (ring) outer += d else inner += d
                        while (acc >= detent || acc <= -detent) {
                            val dir = if (acc > 0) 1 else -1
                            acc -= dir * detent
                            turned = true
                            if (ring) big(dir) else step(dir)
                            haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                        }
                    }
                    if (!turned && !ring) { haptic.performHapticFeedback(HapticFeedbackType.LongPress); push() }
                    grip = 0
                }
            },
    ) {
        val ctr = center
        val r = this.size.minDimension / 2f
        // Pierścień zewnętrzny z radełkowaniem
        val outerC = skin?.knobOuter ?: Color(0xFF1A1A1A)
        val innerC = skin?.knobInner ?: Color(0xFF2A2A2A)
        val markC = skin?.knobMark ?: Color.White
        drawCircle(Brush.radialGradient(listOf(outerC.lighten(0.12f), outerC.darken(0.35f)), ctr, r), r, ctr)
        drawCircle(if (grip == 2) c.accent else Color(0x40FFFFFF), r - 1f, ctr, style = Stroke(if (grip == 2) 3f else 1.5f))
        rotate(outer, ctr) {
            for (i in 0 until 36) {
                rotate(i * 10f, ctr) {
                    drawLine(Color(0x33FFFFFF), Offset(ctr.x, ctr.y - r * 0.96f), Offset(ctr.x, ctr.y - r * 0.74f), 2f)
                }
            }
        }
        // Gałka wewnętrzna: jaśniejsza, z kreską położenia
        val ri = r * INNER
        drawCircle(Color.Black, ri + 3f, ctr)
        drawCircle(Brush.radialGradient(listOf(innerC.lighten(0.18f), innerC.darken(0.3f)), Offset(ctr.x - ri * 0.3f, ctr.y - ri * 0.3f), ri * 1.4f), ri, ctr)
        drawCircle(if (grip == 1) c.accent else Color(0x4DFFFFFF), ri, ctr, style = Stroke(if (grip == 1) 3f else 1f))
        rotate(inner, ctr) {
            drawLine(if (grip == 1) c.accent else markC, Offset(ctr.x, ctr.y - ri * 0.86f), Offset(ctr.x, ctr.y - ri * 0.45f), 5f, cap = StrokeCap.Round)
        }
    }
}

/** Mały okrągły przycisk −/+ z powtarzaniem przy przytrzymaniu (400 ms, potem co 90 ms). */
@Composable
private fun NudgeKey(label: String, desc: String, modifier: Modifier = Modifier, skin: PanelSkin? = null, size: Dp = 38.dp, action: () -> Unit) {
    val c = Fm.c
    val scope = rememberCoroutineScope()
    val act by rememberUpdatedState(action)
    val haptic = LocalHapticFeedback.current
    var pressed by remember { mutableStateOf(false) }
    Box(
        modifier.size(size).clip(CircleShape).background(if (pressed) (skin?.keyPressed ?: c.ink) else (skin?.key ?: c.terrain))
            .semantics { contentDescription = desc; role = Role.Button; onClick { act(); true } }
            .pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown().consume()
                    pressed = true
                    haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                    act()
                    val job = scope.launch { delay(400); while (true) { act(); delay(90) } }
                    waitForUpOrCancellation()
                    job.cancel()
                    pressed = false
                }
            },
        contentAlignment = Alignment.Center,
    ) { Text(label, fontSize = 20.sp, fontWeight = FontWeight.Bold, color = if (pressed && skin == null) Color.Black else (skin?.keyText ?: c.ink)) }
}

/**
 * Klawisz trybu w stylu producenta (patrz [LampStyle]): Airbus — zielone kreski nad napisem,
 * Boeing — lampka pod napisem na kwadratowym klawiszu, Garmin — cyjanowy napis z kropką,
 * GA — obwódka i jasny napis. Bez skórki: wygląd ogólny aplikacji.
 */
@Composable
fun AfdsKey(label: String, on: Boolean, modifier: Modifier = Modifier, skin: PanelSkin? = null, onClick: () -> Unit) {
    val c = Fm.c
    val haptic = LocalHapticFeedback.current
    val s = skin
    val shape = RoundedCornerShape(s?.keyCorner ?: 8.dp)
    val lampOn = s?.lampOn ?: c.green
    val lampOff = s?.lampOff ?: Color(0x26FFFFFF)
    val style = s?.lamp ?: LampStyle.BARS
    val textColor = when {
        style == LampStyle.TEXT && on -> lampOn
        style == LampStyle.OUTLINE && on -> lampOn
        s == null && on -> c.green
        else -> s?.keyText ?: c.ink
    }
    Box(
        modifier.height(s?.keyHeight ?: 42.dp).clip(shape)
            // Lekki gradient: klawisz wygląda na wypukły
            .background(Brush.verticalGradient(listOf((s?.key ?: c.terrain).lighten(0.08f), (s?.key ?: c.terrain).darken(0.18f))))
            .border(if (style == LampStyle.OUTLINE && on) 1.5.dp else 1.dp, if (style == LampStyle.OUTLINE && on) lampOn else (s?.plateEdge ?: Color(0x1FFFFFFF)), shape)
            .semantics { contentDescription = label + if (on) loc("autopilots.on") else loc("autopilots.off") }
            .clickable(role = Role.Switch) { haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick() },
        contentAlignment = Alignment.Center,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            when (style) {
                LampStyle.BARS -> {
                    Row(horizontalArrangement = Arrangement.spacedBy(3.dp)) {
                        repeat(3) { Box(Modifier.width(7.dp).height(3.dp).clip(RoundedCornerShape(50)).background(if (on) lampOn else lampOff)) }
                    }
                    Spacer(Modifier.height(5.dp))
                    Text(label, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 12.sp, color = textColor, maxLines = 1)
                }
                LampStyle.BAR_BELOW -> {
                    Text(label, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 11.sp, color = textColor, maxLines = 1)
                    Spacer(Modifier.height(5.dp))
                    Box(Modifier.width(30.dp).height(5.dp).clip(RoundedCornerShape(1.dp)).background(if (on) lampOn else lampOff))
                }
                LampStyle.TEXT -> Row(verticalAlignment = Alignment.CenterVertically) {
                    if (on) { Box(Modifier.size(6.dp).clip(CircleShape).background(lampOn)); Spacer(Modifier.width(5.dp)) }
                    Text(label, fontWeight = FontWeight.SemiBold, fontSize = 13.sp, color = textColor, maxLines = 1)
                }
                LampStyle.OUTLINE -> Text(label, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 12.sp, color = textColor, maxLines = 1)
            }
        }
    }
}

private data class Key(val label: String, val on: Boolean, val event: String)

/** Jedna nastawa autopilota: okienko z wartością, pokrętło, −/+ i klawisze trybów, które jej dotyczą. */
private data class Selector(
    val label: String, val unit: String, val value: String,
    val step: (Int) -> Unit, val big: (Int) -> Unit, val enter: String, val keys: List<Key>,
)

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun SelectorModule(sel: Selector, skin: PanelSkin, onEnter: (String) -> Unit, onKey: (String) -> Unit, modifier: Modifier = Modifier, compact: Boolean = false) {
    Column(
        modifier.clip(RoundedCornerShape(12.dp)).border(1.dp, skin.plateEdge, RoundedCornerShape(12.dp)).padding(if (compact) 6.dp else 10.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        // Okienko wartości: dotknięcie = wpisanie liczby
        Column(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(6.dp)).background(skin.window).border(1.dp, skin.plateEdge, RoundedCornerShape(6.dp))
                .clickable(onClickLabel = loc("common.type_a_value")) { onEnter(sel.enter) }
                .padding(horizontal = 10.dp, vertical = 6.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(sel.label, fontFamily = Mono, fontSize = 10.sp, color = skin.windowLabel, modifier = Modifier.weight(1f), maxLines = 1)
                Text(sel.unit, fontFamily = Mono, fontSize = 10.sp, color = skin.windowLabel.copy(alpha = 0.7f))
            }
            Text(sel.value, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = if (compact) 19.sp else 26.sp, color = skin.windowText, maxLines = 1)
        }
        Spacer(Modifier.height(10.dp))
        // Pokrętło z −/+ w dolnych narożnikach; średnica wg szerokości modułu, żeby −/+ go nie zachodziły
        BoxWithConstraints(Modifier.fillMaxWidth()) {
            val nudge = if (compact) 30.dp else 38.dp
            val knob = (maxWidth - nudge - 14.dp).coerceIn(if (compact) 48.dp else 64.dp, 96.dp)
            Box(Modifier.fillMaxWidth().height(knob + 14.dp)) {
            DualKnob(
                knob, loc("common.knob", sel.label, sel.value), Modifier.align(Alignment.TopCenter), skin,
                onStep = sel.step, onBigStep = sel.big, onPush = { onEnter(sel.enter) },
            )
            NudgeKey("−", loc("autopilots.down", sel.label), Modifier.align(Alignment.BottomStart), skin, nudge) { sel.step(-1) }
            NudgeKey("+", loc("autopilots.up", sel.label), Modifier.align(Alignment.BottomEnd), skin, nudge) { sel.step(1) }
            }
        }
        if (sel.keys.isNotEmpty()) {
            Spacer(Modifier.height(10.dp))
            FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp), maxItemsInEachRow = 2) {
                sel.keys.forEach { k -> AfdsKey(k.label, k.on, Modifier.weight(1f), skin) { onKey(k.event) } }
                if (sel.keys.size % 2 == 1) Spacer(Modifier.weight(1f))
            }
        }
    }
}

/** Panel autopilota w układzie kabiny profilu samolotu (FCU, MCP, FGCP, Primus, GFC 700, KAP140). */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun AutopilotPanelV2(link: LinkClient, s: FlightState, p: AircraftProfile, compact: Boolean = false) {
    val c = Fm.c
    val ap = s.autopilot
    val m = ap.modes
    var enter by remember { mutableStateOf<String?>(null) }
    fun cmd(n: String, v: Number? = null) { link.command(n, v?.toDouble()) }
    fun live() = link.state.value?.autopilot
    val k = p.autopilot

    if (k == AutopilotKind.NONE) {
        Text(loc("autopilots.has_no_autopilot", p.label), style = MaterialTheme.typography.bodyMedium, color = c.ink2,
            modifier = Modifier.padding(vertical = 12.dp))
        return
    }

    val skin = panelSkin(p)
    // Klawisze główne (autopilot, dyrektor lotu, tłumik odchyleń, automat ciągu) — w jednym rzędzie nad pokrętłami
    val ap1 = Key("AP", ap.master, "AP_MASTER")
    val fd = Key("FD", ap.fd, "TOGGLE_FLIGHT_DIRECTOR")
    val yd = Key("YD", false, "YAW_DAMPER_TOGGLE")
    val master = when (k) {
        AutopilotKind.FCU -> listOf(Key("AP1", ap.master, "AP_MASTER"), fd, Key("A/THR", ap.athr, "AUTO_THROTTLE_ARM"))
        AutopilotKind.MCP -> listOf(Key("CMD A", ap.master, "AP_MASTER"), Key("F/D", ap.fd, "TOGGLE_FLIGHT_DIRECTOR"), Key("A/T ARM", ap.athr, "AUTO_THROTTLE_ARM"))
        AutopilotKind.MCP_787 -> listOf(Key("A/P", ap.master, "AP_MASTER"), Key("F/D", ap.fd, "TOGGLE_FLIGHT_DIRECTOR"), Key("A/T", ap.athr, "AUTO_THROTTLE_ARM"))
        AutopilotKind.ATR_FGCP -> listOf(ap1, fd, yd)
        AutopilotKind.PRIMUS -> listOf(ap1, fd, yd, Key("AT", ap.athr, "AUTO_THROTTLE_ARM"))
        AutopilotKind.GFC700 -> listOf(ap1, fd, yd)
        else -> listOf(ap1)
    }
    // Płyta panelu: od tego miejsca wszystko na tle i w kolorach producenta
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(skin.plate).border(1.dp, skin.plateEdge, RoundedCornerShape(14.dp)).padding(if (compact) 6.dp else 10.dp)) {
    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(bottom = 8.dp)) {
        Text(skin.name, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 11.sp, color = skin.legend)
        Spacer(Modifier.width(8.dp))
        Box(Modifier.weight(1f).height(1.dp).background(skin.plateEdge))
    }
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        master.forEach { k -> AfdsKey(k.label, k.on, Modifier.weight(1f), skin) { cmd(k.event) } }
    }
    Spacer(Modifier.size(10.dp))

    // Nastawy wysyłane jako wartości bezwzględne (*_SET) — zgubiony pakiet nie kumuluje błędu
    val boeing = k == AutopilotKind.MCP || k == AutopilotKind.MCP_787
    val spd = Selector(
        when (k) { AutopilotKind.MCP -> "IAS/MACH"; AutopilotKind.ATR_FGCP -> "IAS"; else -> "SPD" }, "kt", ap.spdKt?.let { "${it.roundToInt()}" } ?: "---",
        { d -> cmd("AP_SPD_VAR_SET", ControlMath.speed(live()?.spdKt, d)) },
        { d -> cmd("AP_SPD_VAR_SET", ControlMath.speed(live()?.spdKt, d * 10)) }, "SPD",
        when (k) {
            AutopilotKind.MCP -> listOf(Key("N1", false, "AP_N1_HOLD"), Key("SPEED", false, "AP_PANEL_SPEED_HOLD"), Key("LVL CHG", m.flc, "FLIGHT_LEVEL_CHANGE"))
            AutopilotKind.MCP_787 -> listOf(Key("SPD", false, "AP_PANEL_SPEED_HOLD"), Key("FLCH", m.flc, "FLIGHT_LEVEL_CHANGE"))
            AutopilotKind.ATR_FGCP -> listOf(Key("IAS", m.flc, "FLIGHT_LEVEL_CHANGE"))
            AutopilotKind.PRIMUS -> listOf(Key("FLCH", m.flc, "FLIGHT_LEVEL_CHANGE"))
            AutopilotKind.FCU -> emptyList()
            else -> listOf(Key("FLC", m.flc, "FLIGHT_LEVEL_CHANGE"))
        },
    )
    val hdg = Selector(
        if (boeing) "HDG SEL" else "HDG", "°", ap.hdgDeg?.let { "%03d".format(it.roundToInt() % 360) } ?: "---",
        { d -> cmd("HEADING_BUG_SET", ControlMath.heading(live()?.hdgDeg, d)) },
        { d -> cmd("HEADING_BUG_SET", ControlMath.heading(live()?.hdgDeg, d * 10)) }, "HDG",
        when (k) {
            AutopilotKind.FCU -> listOf(Key("HDG", m.hdg, "AP_HDG_HOLD"), Key("NAV", m.nav, "AP_NAV1_HOLD"), Key("LOC", m.loc, "AP_LOC_HOLD"), Key("APPR", m.apr, "AP_APR_HOLD"))
            AutopilotKind.MCP -> listOf(Key("HDG SEL", m.hdg, "AP_HDG_HOLD"), Key("LNAV", m.nav, "AP_NAV1_HOLD"), Key("VOR/LOC", m.loc, "AP_LOC_HOLD"), Key("APP", m.apr, "AP_APR_HOLD"))
            AutopilotKind.MCP_787 -> listOf(Key("HDG HOLD", m.hdg, "AP_HDG_HOLD"), Key("LNAV", m.nav, "AP_NAV1_HOLD"), Key("LOC", m.loc, "AP_LOC_HOLD"), Key("APP", m.apr, "AP_APR_HOLD"))
            AutopilotKind.ATR_FGCP, AutopilotKind.PRIMUS, AutopilotKind.GFC700 ->
                listOf(Key("HDG", m.hdg, "AP_HDG_HOLD"), Key("NAV", m.nav, "AP_NAV1_HOLD"), Key(if (k == AutopilotKind.GFC700) "APR" else "APP", m.apr, "AP_APR_HOLD"), Key("BC", false, "AP_BC_HOLD"))
            else -> listOf(Key("HDG", m.hdg, "AP_HDG_HOLD"), Key("NAV", m.nav, "AP_NAV1_HOLD"), Key("APR", m.apr, "AP_APR_HOLD"), Key("REV", false, "AP_BC_HOLD"))
        },
    )
    val alt = Selector(
        "ALT", "ft", ap.altFt?.let { "${it.roundToInt()}" } ?: "-----",
        { d -> cmd("AP_ALT_VAR_SET_ENGLISH", ControlMath.altitude(live()?.altFt, d * 100)) },
        { d -> cmd("AP_ALT_VAR_SET_ENGLISH", ControlMath.altitude(live()?.altFt, d * 1000)) }, "ALT",
        when (k) {
            AutopilotKind.FCU -> listOf(Key("ALT", m.alt, "AP_ALT_HOLD"), Key("OP CLB", m.flc, "FLIGHT_LEVEL_CHANGE"))
            AutopilotKind.MCP, AutopilotKind.MCP_787 -> listOf(Key("ALT HOLD", m.alt, "AP_ALT_HOLD"))
            else -> listOf(Key("ALT", m.alt, "AP_ALT_HOLD"))
        },
    )
    val vsName = if (k == AutopilotKind.FCU || boeing) "V/S" else "VS"
    val vs = Selector(
        vsName, "fpm", ap.vsFpm?.let { "%+d".format(it.roundToInt()) } ?: "----",
        { d -> cmd("AP_VS_VAR_SET_ENGLISH", ControlMath.verticalSpeed(live()?.vsFpm, d * 100)) },
        { d -> cmd("AP_VS_VAR_SET_ENGLISH", ControlMath.verticalSpeed(live()?.vsFpm, d * 500)) }, "VS",
        listOf(Key(vsName, m.vs, "AP_VS_HOLD")),
    )
    // Kolejność jak na panelu: prędkość, kurs, wysokość, prędkość pionowa (KAP140 bez prędkości)
    val modules = if (k == AutopilotKind.KAP140) listOf(hdg, alt, vs) else listOf(spd, hdg, alt, vs)

    BoxWithConstraints(Modifier.fillMaxWidth()) {
        // Moduł potrzebuje ~168 dp; trzy kolumny przy czterech modułach zostawiłyby sierotę — wtedy dwie
        // Moduł mieści się od ~150 dp: telefon 2 kolumny, szeroki ekran wszystkie w rzędzie
        var cols = ((maxWidth + 8.dp) / (if (compact) 112.dp else 158.dp)).toInt().coerceIn(1, modules.size)
        if (cols == 3 && modules.size == 4) cols = 2
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            modules.chunked(cols).forEach { row ->
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    row.forEach { sel -> SelectorModule(sel, if (compact) skin.copy(keyHeight = 36.dp) else skin, { enter = it }, { cmd(it) }, Modifier.weight(1f), compact) }
                    repeat(cols - row.size) { Spacer(Modifier.weight(1f)) }
                }
            }
        }
    }
    }
    if (!compact) Text(loc("autopilots.turn_a_knob_with_your_finger"),
        style = MaterialTheme.typography.labelSmall, color = c.ink2, modifier = Modifier.padding(top = 8.dp))

    when (enter) {
        "HDG" -> EnterDialog(loc("common.autopilot_heading"), loc("common.e_g_270"), KeyboardType.Number, { it.toIntOrNull() in 0..359 }, { cmd("HEADING_BUG_SET", it.toInt()) }) { enter = null }
        "ALT" -> EnterDialog(loc("autopilots.altitude_ft"), loc("common.e_g_12000"), KeyboardType.Number, { it.toIntOrNull() in 0..50_000 }, { cmd("AP_ALT_VAR_SET_ENGLISH", it.toInt()) }) { enter = null }
        "SPD" -> EnterDialog(loc("autopilots.speed_kt"), loc("common.e_g_250"), KeyboardType.Number, { it.toIntOrNull() in 0..500 }, { cmd("AP_SPD_VAR_SET", it.toInt()) }) { enter = null }
        "VS" -> EnterDialog(loc("autopilots.vertical_speed_ft_min"), loc("common.e_g_1500"), KeyboardType.Number, { it.toIntOrNull() in -8000..8000 }, { cmd("AP_VS_VAR_SET_ENGLISH", it.toInt()) }) { enter = null }
    }
}

/** Jaśniejszy / ciemniejszy odcień — do gradientów klawiszy i pokręteł. */
internal fun Color.lighten(f: Float) = Color(red + (1 - red) * f, green + (1 - green) * f, blue + (1 - blue) * f, alpha)
internal fun Color.darken(f: Float) = Color(red * (1 - f), green * (1 - f), blue * (1 - f), alpha)
