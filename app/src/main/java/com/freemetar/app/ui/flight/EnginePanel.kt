package com.freemetar.app.ui.flight

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
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
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.freemetar.app.data.link.FlightState
import com.freemetar.app.data.link.LinkClient
import com.freemetar.app.domain.AircraftProfile
import com.freemetar.app.domain.Lever as LeverKind
import com.freemetar.app.domain.loc
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.Mono
import kotlin.math.cos
import kotlin.math.roundToInt
import kotlin.math.sin

private val EICAS_BG = Color(0xFF05070A)
private val EICAS_WHITE = Color(0xFFEFF3F6)
private val EICAS_GREEN = Color(0xFF3FE07A)
private val EICAS_AMBER = Color(0xFFFFB020)
private val EICAS_RED = Color(0xFFFF3B47)
private val EICAS_CYAN = Color(0xFF4FD1E8)

/**
 * Ekran parametrów silników w wielkości ND z zakładki na żywo: na szerokim ekranie (ND zajmuje tam niecałą połowę
 * szerokości obok PFD) wyśrodkowany w tej samej szerokości, na telefonie na całą szerokość jak ND.
 */
@Composable
internal fun EngineDisplayBox(content: @Composable () -> Unit) {
    BoxWithConstraints(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
        val w = if (maxWidth >= 560.dp) (maxWidth - 12.dp) / 2.1f else maxWidth
        Box(Modifier.width(w)) { content() }
    }
}

/** Zapadka dźwigni ciągu (odrzutowce): położenie 0–100 % i podpis. */
private data class Detent(val pct: Int, val label: String)

/**
 * Silniki: wyświetlacz EICAS/ECAM (wskaźniki N1 albo obrotów, EGT, przepływ i zapas paliwa)
 * oraz pulpit dźwigni — moc dla każdego samolotu, mieszanka i śmigło dla tłokowych,
 * śmigło dla turbośmigłowych. Odrzutowce mają zapadki i przyciski TO/GA oraz odwracacza ciągu.
 */
@Composable
fun EnginePanel(link: LinkClient, s: FlightState, profile: AircraftProfile) {
    val c = Fm.c
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val side = maxWidth >= 560.dp
        if (side) {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Column(Modifier.weight(1f)) { Eicas(s, profile) }
                Column(Modifier.weight(1f)) { Quadrant(link, s, profile) }
            }
        } else {
            Column {
                EngineDisplayBox { Eicas(s, profile) }
                Spacer(Modifier.height(12.dp))
                Quadrant(link, s, profile)
            }
        }
    }
    if (s.autopilot.athr && profile.hasAutothrottle) {
        Text(loc("Automat ciągu jest włączony — symulator może sam przestawiać moc.", "Autothrottle is on — the simulator may move the thrust levers itself."),
            style = MaterialTheme.typography.labelSmall, color = c.caution, modifier = Modifier.padding(top = 8.dp))
    }
}

/** Wyświetlacz silników: okrągły wskaźnik na silnik, pod nim EGT i przepływ, na dole paliwo. */
@Composable
internal fun Eicas(s: FlightState, profile: AircraftProfile) {
    val m = rememberTextMeasurer()
    val engines = s.engines.ifEmpty { return }
    // Wskaźnik obrotów (RPM) dla tłokowych, N1 dla turbinowych (w ATR to NH)
    val piston = !profile.jet && profile != AircraftProfile.ATR72
    val title = when (profile) {
        AircraftProfile.A320 -> "E/WD"; AircraftProfile.B737, AircraftProfile.B787, AircraftProfile.E190 -> "EICAS"
        AircraftProfile.ATR72 -> "EWD"; AircraftProfile.G1000 -> "EIS"; else -> "ENGINE"
    }
    val desc = engines.mapIndexed { i, e -> loc("Silnik ${i + 1}: ", "Engine ${i + 1}: ") + if (piston) "${(e.rpm ?: 0.0).roundToInt()} RPM" else "N1 ${"%.1f".format(e.n1Pct ?: 0.0)}%" }.joinToString(", ")
    Canvas(Modifier.fillMaxWidth().aspectRatio(if (engines.size > 2) 1.5f else 1.25f).clip(RoundedCornerShape(10.dp)).semantics { contentDescription = desc }) {
        val w = size.width; val h = size.height
        drawRect(EICAS_BG)
        fun text(t: String, at: Offset, col: Color, sz: Float, bold: Boolean = false) {
            val l = m.measure(t, TextStyle(color = col, fontSize = sz.sp, fontFamily = Mono, fontWeight = if (bold) FontWeight.Bold else FontWeight.Normal))
            drawText(l, topLeft = Offset(at.x - l.size.width / 2f, at.y - l.size.height / 2f))
        }
        text(title, Offset(w * 0.08f, h * 0.06f), EICAS_WHITE.copy(alpha = 0.6f), 10f)
        val n = engines.size
        val cellW = w / n
        val r = minOf(cellW * 0.36f, h * 0.24f)
        engines.forEachIndexed { i, e ->
            val cx = cellW * i + cellW / 2; val cy = h * 0.34f
            // Wskaźnik: łuk 210°, zielony zakres pracy, czerwona kreska limitu
            val (value, max, redline, unit) = if (piston) {
                listOf((e.rpm ?: 0.0), 2800.0, 2700.0, 0.0)
            } else listOf((e.n1Pct ?: 0.0), 110.0, 104.0, 0.0)
            val start = 150f; val sweep = 210f
            val tl = Offset(cx - r, cy - r); val sz = Size(r * 2, r * 2)
            drawArc(EICAS_WHITE.copy(alpha = 0.25f), start, sweep, false, tl, sz, style = Stroke(r * 0.09f))
            val frac = (value / max).coerceIn(0.0, 1.0).toFloat()
            val col = when { value >= redline -> EICAS_RED; value >= redline * 0.97 -> EICAS_AMBER; else -> EICAS_GREEN }
            drawArc(col.copy(alpha = 0.85f), start, sweep * frac, false, tl, sz, style = Stroke(r * 0.09f))
            // Kreska limitu
            val ra = Math.toRadians((start + sweep * (redline / max)).toDouble())
            drawLine(EICAS_RED, Offset(cx + (r * 0.82f) * cos(ra).toFloat(), cy + (r * 0.82f) * sin(ra).toFloat()),
                Offset(cx + (r * 1.12f) * cos(ra).toFloat(), cy + (r * 1.12f) * sin(ra).toFloat()), 4f)
            // Wskazówka
            val na = Math.toRadians((start + sweep * frac).toDouble())
            drawLine(EICAS_WHITE, Offset(cx, cy), Offset(cx + r * 0.92f * cos(na).toFloat(), cy + r * 0.92f * sin(na).toFloat()), 5f, cap = StrokeCap.Round)
            drawCircle(EICAS_WHITE, 5f, Offset(cx, cy))
            // Odczyt w ramce
            val txt = if (piston) "${value.roundToInt()}" else "%.1f".format(value)
            val bw = r * 1.15f; val bh = r * 0.42f
            drawRect(Color.Black, Offset(cx + r * 0.05f, cy + r * 0.12f), Size(bw, bh))
            drawRect(EICAS_WHITE.copy(alpha = 0.6f), Offset(cx + r * 0.05f, cy + r * 0.12f), Size(bw, bh), style = Stroke(2f))
            text(txt, Offset(cx + r * 0.05f + bw / 2, cy + r * 0.12f + bh / 2), col, 15f, true)
            text(if (piston) "RPM" else "N1 %", Offset(cx, cy + r * 0.85f), EICAS_CYAN, 10f)
            // EGT, przepływ, N2
            val y0 = h * 0.66f
            text(e.egtC?.let { "EGT ${it.roundToInt()}°" } ?: "EGT —", Offset(cx, y0), EICAS_WHITE, 11f)
            text(e.fuelFlowKgH?.let { "FF ${it.roundToInt()}" } ?: "FF —", Offset(cx, y0 + h * 0.08f), EICAS_GREEN, 11f)
            if (!piston) e.n2Pct?.let { text("N2 %.1f".format(it), Offset(cx, y0 + h * 0.16f), EICAS_WHITE.copy(alpha = 0.8f), 10f) }
            // Słupek EGT (0–1000 °C) po lewej stronie wskaźnika
            e.egtC?.let { egt ->
                val bx = cx - r * 1.2f; val top = cy - r * 0.7f; val bhh = r * 1.4f
                drawRect(EICAS_WHITE.copy(alpha = 0.18f), Offset(bx, top), Size(6f, bhh))
                val f = (egt / 1000.0).coerceIn(0.0, 1.0).toFloat()
                drawRect(if (egt > 900) EICAS_AMBER else EICAS_GREEN, Offset(bx, top + bhh * (1 - f)), Size(6f, bhh * f))
            }
        }
        // Paliwo i ustawienie dźwigni
        val fuel = s.fuel.totalKg
        val ff = engines.mapNotNull { it.fuelFlowKgH }.sum()
        text(listOfNotNull(fuel?.let { "FOB ${it.roundToInt()} kg" }, if (ff > 0) "FF ${ff.roundToInt()} kg/h" else null).joinToString("   "), Offset(w / 2, h * 0.92f), EICAS_CYAN, 12f, true)
        s.controls.throttlePct?.let { text("THR ${it.roundToInt()}%", Offset(w * 0.88f, h * 0.06f), EICAS_WHITE.copy(alpha = 0.8f), 10f) }
    }
}

/**
 * Pulpit dźwigni: osobna dźwignia mocy dla każdego silnika (THROTTLEn_SET), obok dźwignie wspólne
 * dla wszystkich silników (śmigło, mieszanka, dźwignia warunku w ATR). Każda to pionowa szczelina z rączką.
 */
@Composable
internal fun Quadrant(link: LinkClient, s: FlightState, p: AircraftProfile) {
    val c = Fm.c
    if (!p.hasThrottle) return
    val skin = panelSkin(p)
    val detents = p.detents.map { (pct, label) -> Detent(pct, label) }
    // Liczba silników z symulatora (najwyżej 4 — tyle obsługuje Link), bez danych — z profilu
    val count = s.engines.size.takeIf { it > 0 }?.coerceAtMost(4) ?: p.engines
    fun send(name: String, pct: Float) = link.command(name, (pct / 100f * 16383f).roundToInt().coerceIn(0, 16383).toDouble())

    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(skin.plate).border(1.dp, skin.plateEdge, RoundedCornerShape(12.dp)).padding(10.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
            repeat(count) { i ->
                val label = if (count == 1) loc("MOC", "THRUST") else loc("SIL. ${i + 1}", "ENG ${i + 1}")
                val actual = s.engines.getOrNull(i)?.throttlePct ?: s.controls.throttlePct
                Lever(label, actual, Color(0xFF2A2D31), Color(0xFFE6E9EC), detents, skin, Modifier.weight(1f)) {
                    if (count == 1) send("THROTTLE_SET", it) else send("THROTTLE${i + 1}_SET", it)
                }
            }
            p.levers.forEach { lever ->
                when (lever) {
                    LeverKind.PROP -> Lever(loc("ŚMIGŁO", "PROP"), s.controls.propPct, Color(0xFF1F5FBF), Color.White, listOf(Detent(0, "LOW"), Detent(100, "HIGH")), skin, Modifier.weight(1f)) { send("PROPELLER_SET", it) }
                    // ATR: dźwignia warunku ustawia obroty śmigła (FTR / AUTO / 100 OVRD)
                    LeverKind.CONDITION -> Lever("CL", s.controls.propPct, Color(0xFF1F5FBF), Color.White, listOf(Detent(0, "FTR"), Detent(70, "AUTO"), Detent(100, "OVRD")), skin, Modifier.weight(1f)) { send("PROPELLER_SET", it) }
                    LeverKind.MIXTURE -> Lever(loc("MIESZANKA", "MIXTURE"), s.controls.mixturePct, Color(0xFFC62828), Color.White, listOf(Detent(0, "CUT"), Detent(100, "RICH")), skin, Modifier.weight(1f)) { send("MIXTURE_SET", it) }
                }
            }
        }
        Spacer(Modifier.height(10.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            AfdsKey("IDLE", (s.controls.throttlePct ?: 1.0) < 1, Modifier.weight(1f), skin) { link.command("THROTTLE_CUT") }
            if (p.jet) AfdsKey("TO/GA", (s.controls.throttlePct ?: 0.0) > 99, Modifier.weight(1f), skin) { toga(link) }
            else AfdsKey(loc("PEŁNA", "FULL"), (s.controls.throttlePct ?: 0.0) > 99, Modifier.weight(1f), skin) { link.command("THROTTLE_FULL") }
            if (p.hasReverser) AfdsKey("REV", false, Modifier.weight(1f), skin) { if (s.aircraft.onGround != false) link.command("THROTTLE_REVERSE_THRUST_TOGGLE") }
        }
    }
    Text(if (count > 1) loc("Każda dźwignia ustawia jeden silnik. Wspólna dźwignia przy wyświetlaczach ustawia wszystkie naraz.",
            "Each lever sets one engine. The shared lever next to the displays sets all of them at once.")
        else loc("Przeciągnij rączkę w górę lub w dół.", "Drag a handle up or down."),
        style = MaterialTheme.typography.labelSmall, color = c.ink2, modifier = Modifier.padding(top = 6.dp))
}

/**
 * Dźwignia: szczelina z podziałką i zapadkami, rączka w kolorze dźwigni (moc — czarna, śmigło — niebieska,
 * mieszanka — czerwona). Podczas przeciągania wartość idzie do symulatora co 120 ms, po puszczeniu raz jeszcze.
 * Przy zapadce rączka „wskakuje” (±3 %) z wibracją, jak w prawdziwym pulpicie.
 */
@Composable
private fun Lever(
    label: String, actual: Double?, handle: Color, handleText: Color, detents: List<Detent>, skin: PanelSkin,
    modifier: Modifier = Modifier, onSet: (Float) -> Unit,
) {
    val m = rememberTextMeasurer()
    val haptic = LocalHapticFeedback.current
    var dragging by remember { mutableStateOf(false) }
    var local by remember { mutableStateOf(0f) }
    var lastSent by remember { mutableLongStateOf(0L) }
    val set by rememberUpdatedState(onSet)
    val shown = if (dragging) local else (actual ?: 0.0).toFloat()
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally) {
        Text(label, fontFamily = Mono, fontSize = 10.sp, fontWeight = FontWeight.Bold, color = skin.legend)
        Text("${shown.roundToInt()}%", fontFamily = Mono, fontSize = 16.sp, fontWeight = FontWeight.Bold, color = skin.windowText,
            modifier = Modifier.padding(vertical = 4.dp).clip(RoundedCornerShape(4.dp)).background(skin.window).padding(horizontal = 8.dp, vertical = 2.dp))
        Canvas(
            Modifier.fillMaxWidth().height(210.dp)
                .semantics {
                    contentDescription = "$label ${shown.roundToInt()}%"
                    customActions = listOf(
                        CustomAccessibilityAction(loc("Więcej", "More")) { set(((actual ?: 0.0) + 10).toFloat().coerceAtMost(100f)); true },
                        CustomAccessibilityAction(loc("Mniej", "Less")) { set(((actual ?: 0.0) - 10).toFloat().coerceAtLeast(0f)); true },
                    )
                }
                .pointerInput(detents) {
                    val pad = 26.dp.toPx()
                    fun pctAt(y: Float): Float {
                        val raw = ((size.height - pad - y) / (size.height - 2 * pad) * 100f).coerceIn(0f, 100f)
                        val snap = detents.firstOrNull { kotlin.math.abs(it.pct - raw) <= 3f }
                        return snap?.pct?.toFloat() ?: raw
                    }
                    awaitEachGesture {
                        val down = awaitFirstDown()
                        down.consume()
                        dragging = true
                        local = pctAt(down.position.y)
                        var lastDetent: Int? = detents.firstOrNull { it.pct.toFloat() == local }?.pct
                        while (true) {
                            val ev = awaitPointerEvent()
                            val ch = ev.changes.firstOrNull { it.id == down.id } ?: break
                            if (!ch.pressed) break
                            ch.consume()
                            local = pctAt(ch.position.y)
                            val d = detents.firstOrNull { it.pct.toFloat() == local }?.pct
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
            val pad = 26.dp.toPx()
            val cx = size.width / 2
            val top = pad; val bottom = size.height - pad
            // Szczelina
            drawRoundRect(Color.Black, Offset(cx - 5.dp.toPx(), top - 6), Size(10.dp.toPx(), bottom - top + 12), androidx.compose.ui.geometry.CornerRadius(10f))
            // Podziałka co 10 %
            for (p in 0..100 step 10) {
                val y = bottom - (bottom - top) * p / 100f
                drawLine(skin.legend.copy(alpha = 0.5f), Offset(cx - 18.dp.toPx(), y), Offset(cx - 10.dp.toPx(), y), 2f)
            }
            // Zapadki z podpisami
            detents.forEach { d ->
                val y = bottom - (bottom - top) * d.pct / 100f
                drawLine(skin.legend, Offset(cx + 10.dp.toPx(), y), Offset(cx + 20.dp.toPx(), y), 3f)
                val l = m.measure(d.label, TextStyle(color = skin.legend, fontSize = 9.sp, fontFamily = Mono, fontWeight = FontWeight.Bold))
                drawText(l, topLeft = Offset(cx + 23.dp.toPx(), y - l.size.height / 2f))
            }
            // Rączka
            val y = bottom - (bottom - top) * shown / 100f
            val hw = 30.dp.toPx(); val hh = 22.dp.toPx()
            drawRoundRect(Brush.verticalGradient(listOf(handle.lighten(0.25f), handle.darken(0.25f)), y - hh / 2, y + hh / 2),
                Offset(cx - hw / 2, y - hh / 2), Size(hw, hh), androidx.compose.ui.geometry.CornerRadius(8f))
            drawLine(handleText.copy(alpha = 0.7f), Offset(cx - hw / 2 + 6, y), Offset(cx + hw / 2 - 6, y), 2f)
            if (dragging) drawRoundRect(EICAS_CYAN, Offset(cx - hw / 2 - 3, y - hh / 2 - 3), Size(hw + 6, hh + 6), androidx.compose.ui.geometry.CornerRadius(10f), style = Stroke(3f))
        }
    }
}
