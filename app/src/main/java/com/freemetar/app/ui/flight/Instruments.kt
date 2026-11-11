package com.freemetar.app.ui.flight

import com.freemetar.app.domain.loc

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.runtime.remember
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.sp
import com.freemetar.app.data.RoutePoint
import com.freemetar.app.data.link.FlightState
import com.freemetar.app.domain.AircraftProfile
import com.freemetar.app.domain.Geo
import com.freemetar.app.ui.flight.avionics.AirbusPfd
import com.freemetar.app.ui.flight.avionics.AirbusVariant
import com.freemetar.app.ui.flight.avionics.AtrPfd
import com.freemetar.app.ui.flight.avionics.B787Pfd
import com.freemetar.app.ui.flight.avionics.BoeingPfd
import com.freemetar.app.ui.flight.avionics.CessnaPanel
import com.freemetar.app.ui.flight.avionics.DisplayStyle
import com.freemetar.app.ui.flight.avionics.E190Pfd
import com.freemetar.app.ui.flight.avionics.GarminPfd
import com.freemetar.app.ui.flight.avionics.MfdRenderer
import com.freemetar.app.ui.flight.avionics.PfdRenderer
import com.freemetar.app.ui.flight.avionics.TerrainProvider
import com.freemetar.app.ui.flight.avionics.toFlightData
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.Mono
import kotlin.math.abs
import kotlin.math.roundToInt

// Kolory przyrządów są stałe jak w kabinie — niezależne od motywu aplikacji
private val WHITE = Color(0xFFF2F5F8)
private val SYMBOL = Color(0xFFFFC21A)
private val FPV = Color(0xFF3FE07A)

/** Przyspieszenie (kt/s) z kolejnych odczytów IAS, wygładzone — do strzałki trendu na taśmie prędkości. */
@androidx.compose.runtime.Composable
private fun rememberSpeedTrend(ias: Double): Double {
    val holder = androidx.compose.runtime.remember { doubleArrayOf(Double.NaN, 0.0, 0.0) } // ostatnia IAS, czas [s], trend
    val now = System.nanoTime() / 1e9
    if (holder[0].isNaN()) { holder[0] = ias; holder[1] = now }
    val dt = now - holder[1]
    if (dt >= 0.2) {
        val a = (ias - holder[0]) / dt
        holder[2] = holder[2] * 0.7 + a * 0.3
        holder[0] = ias; holder[1] = now
    }
    return holder[2]
}

internal fun DrawScope.label(m: TextMeasurer, text: String, center: Offset, color: Color, size: TextUnit, bold: Boolean = false) {
    val l = m.measure(text, TextStyle(color = color, fontSize = size, fontFamily = Mono, fontWeight = if (bold) FontWeight.Bold else FontWeight.Normal))
    drawText(l, topLeft = Offset(center.x - l.size.width / 2f, center.y - l.size.height / 2f))
}

/**
 * Główny wyświetlacz lotu w stylu samolotu (renderery z OpenPFD): Airbus, Boeing 737/787, ATR, Embraer,
 * Garmin G1000 z widokiem syntetycznym, a w samolotach z zegarami — six-pack (śmigłowiec, szybowiec).
 * Styl wybiera [DisplayStyle.of] z danych samolotu albo z ręcznie wybranego [profile].
 * Wszystkie wskazania, nastawy, tryby FMA, dyrektor lotu i ILS pochodzą z symulatora przez Link.
 */
@Composable
fun Pfd(s: FlightState, modifier: Modifier = Modifier, profile: AircraftProfile? = null) {
    val ias = s.speed.iasKt ?: 0.0
    val trend = rememberSpeedTrend(ias)
    val style = remember(s.aircraft.title, s.aircraft.atcModel, s.aircraft.category, s.aircraft.engineType, profile) { DisplayStyle.of(s, profile) }
    val renderers = remember { PfdRenderers() }
    // Teren do widoku syntetycznego pobierany tylko dla G1000
    val lat = s.position.lat; val lon = s.position.lon
    if (style == DisplayStyle.GARMIN && lat != null && lon != null) TerrainProvider.onLocation(lat, lon)
    val alt = s.position.altIndFt ?: s.position.altMslFt ?: 0.0
    val desc = loc("instruments.pfd_speed_knots_altitude_feet_heading", style.label, ias.roundToInt(), alt.roundToInt(), (s.attitude.hdgMagDeg ?: 0.0).roundToInt())
    Canvas(modifier.fillMaxWidth().aspectRatio(1.12f).semantics { contentDescription = desc }) {
        drawRect(Color.Black)
        val d = s.toFlightData((trend * 10).toFloat(), if (style == DisplayStyle.GARMIN) TerrainProvider.grid else null)
        drawIntoCanvas { cv ->
            val nc = cv.nativeCanvas
            if (style.analog) {
                renderers.panel(style).draw(nc, d, size.width, size.height)
            } else {
                // Wirtualny prostokąt wyświetlacza wpasowany w pole z zachowaniem proporcji
                val r = renderers.pfd(style)
                val k = minOf(size.width / r.vw, size.height / r.vh)
                nc.save()
                nc.translate((size.width - r.vw * k) / 2, (size.height - r.vh * k) / 2)
                nc.scale(k, k)
                nc.clipRect(0f, 0f, r.vw, r.vh)
                r.draw(nc, d)
                nc.restore()
            }
        }
    }
}

/** Renderery tworzone raz na styl (trzymają pędzle, gradienty i stan wskazówek). */
private class PfdRenderers {
    private val pfds = HashMap<DisplayStyle, PfdRenderer>()
    private val panels = HashMap<DisplayStyle, CessnaPanel>()

    fun pfd(st: DisplayStyle): PfdRenderer = pfds.getOrPut(st) {
        when (st) {
            DisplayStyle.AIRBUS -> AirbusPfd(AirbusVariant.A320)
            DisplayStyle.BOEING -> BoeingPfd()
            DisplayStyle.B787 -> B787Pfd()
            DisplayStyle.ATR -> AtrPfd()
            DisplayStyle.EMBRAER -> E190Pfd()
            else -> GarminPfd()
        }
    }

    fun panel(st: DisplayStyle): CessnaPanel = panels.getOrPut(st) {
        CessnaPanel(
            when (st) {
                DisplayStyle.HELI -> CessnaPanel.Kind.HELI
                DisplayStyle.GLIDER -> CessnaPanel.Kind.GLIDER
                else -> CessnaPanel.Kind.CESSNA
            },
        )
    }
}

/**
 * Wyświetlacz nawigacyjny (ND/MFD) w stylu samolotu: Airbus ND, Boeing ND (też 787, ATR i E190 w ich kolorach),
 * Garmin G1000 MFD z paskiem silnika (EIS) albo mapa GPS w stylu GNS dla samolotów z zegarami.
 * ARC (wycinek przed samolotem) albo ROSE (pełna róża). Trasa z planu, następny punkt z GPS symulatora,
 * nastawa kursu, kurs ILS/VOR z odchyleniem, wiatr, GS/TAS; z bazy MSFS (przez Link) lotniska, VOR, NDB
 * i punkty trasy — warstwy włączane w [layers].
 */
@Composable
fun NavDisplay(
    s: FlightState, route: List<RoutePoint>, rangeNm: Int, modifier: Modifier = Modifier, rose: Boolean = false,
    nav: List<com.freemetar.app.data.nav.FixResult> = emptyList(), layers: Set<String> = setOf("airport", "vor", "ndb"),
    aspect: Float = 1f, profile: AircraftProfile? = null,
) {
    val style = remember(s.aircraft.title, s.aircraft.atcModel, s.aircraft.category, s.aircraft.engineType, profile) { DisplayStyle.of(s, profile) }
    val renderer = remember(style) { MfdRenderer(style) }
    val desc = loc("instruments.navigation_display_range_miles", style.label, rangeNm)
    Canvas(modifier.fillMaxWidth().aspectRatio(aspect).semantics { contentDescription = desc }) {
        val d = s.toFlightData(0f, null)
        drawIntoCanvas { cv -> renderer.draw(cv.nativeCanvas, s, d, size.width, size.height, rangeNm, rose, route, nav, layers) }
    }
}

/**
 * Profil pionowy (VSD) pod mapą: wysokość na osi pionowej, odległość przed samolotem na poziomej.
 * Samolot po lewej, w cyjanie wysokość wybrana w autopilocie, w zieleni tor przy bieżącej
 * prędkości pionowej i GS, w magencie ograniczenia wysokości punktów trasy (gdy są w planie).
 */
@Composable
fun VerticalProfile(s: FlightState, route: List<RoutePoint>, rangeNm: Int, modifier: Modifier = Modifier) {
    val m = rememberTextMeasurer()
    val c = Fm.c
    val alt = s.position.altMslFt ?: return
    val lat = s.position.lat; val lon = s.position.lon
    val sel = s.autopilot.altFt
    val gs = s.speed.gsKt ?: 0.0
    val vs = s.speed.vsFpm ?: 0.0
    // Punkty trasy przed samolotem z ograniczeniem wysokości
    val heading = s.attitude.trackTrueDeg ?: s.attitude.hdgTrueDeg ?: 0.0
    val pts = if (lat != null && lon != null) route.filter { it.hasPosition && it.altitudeFt != null }.mapNotNull { p ->
        val d = Geo.distanceNm(lat, lon, p.lat!!, p.lon!!)
        val rel = abs(((Geo.bearing(lat, lon, p.lat, p.lon) - heading + 540) % 360) - 180)
        if (rel < 90 && d <= rangeNm) Triple(p.ident, d, p.altitudeFt!!) else null
    } else emptyList()
    val values = listOfNotNull(alt, sel) + pts.map { it.third.toDouble() }
    val top = ((values.max() + 2000) / 1000).let { kotlin.math.ceil(it) * 1000 }
    val bottom = (((values.min() - 2000) / 1000).let { kotlin.math.floor(it) * 1000 }).coerceAtLeast(0.0)
    Canvas(modifier.fillMaxWidth().aspectRatio(3.2f).semantics { contentDescription = loc("instruments.vertical_profile_over_miles", rangeNm) }) {
        val w = size.width; val h = size.height
        val left = w * 0.12f; val right = w * 0.98f; val tp = h * 0.1f; val bt = h * 0.86f
        fun x(d: Double) = left + ((d / rangeNm) * (right - left)).toFloat()
        fun y(a: Double) = bt - (((a - bottom) / (top - bottom)) * (bt - tp)).toFloat()
        drawRect(Color(0xFF0E1216))
        // Siatka wysokości
        val step = if (top - bottom > 12000) 5000 else if (top - bottom > 5000) 2000 else 1000
        var a = (bottom / step).toInt() * step.toDouble()
        while (a <= top) {
            drawLine(WHITE.copy(alpha = 0.15f), Offset(left, y(a)), Offset(right, y(a)), 1f)
            label(m, if (a >= 18000) "FL${(a / 100).roundToInt()}" else "${a.roundToInt()}", Offset(left * 0.5f, y(a)), WHITE.copy(alpha = 0.7f), 9.sp)
            a += step
        }
        // Odległość: połowa i pełny zasięg
        listOf(0.5, 1.0).forEach { f ->
            drawLine(WHITE.copy(alpha = 0.25f), Offset(x(rangeNm * f), tp), Offset(x(rangeNm * f), bt), 1f, pathEffect = PathEffect.dashPathEffect(floatArrayOf(6f, 6f)))
            label(m, "${(rangeNm * f).roundToInt()}", Offset(x(rangeNm * f) - 10, h * 0.94f), c.accent, 9.sp)
        }
        // Wysokość wybrana
        sel?.let { drawLine(c.accent, Offset(left, y(it)), Offset(right, y(it)), 2f, pathEffect = PathEffect.dashPathEffect(floatArrayOf(10f, 6f))) }
        // Ograniczenia trasy
        if (pts.size >= 2) drawPath(Path().apply { pts.sortedBy { it.second }.forEachIndexed { i, p -> if (i == 0) moveTo(x(p.second), y(p.third.toDouble())) else lineTo(x(p.second), y(p.third.toDouble())) } }, c.magenta, style = Stroke(2f))
        pts.forEach { (id, d, al) ->
            drawCircle(c.magenta, 4f, Offset(x(d), y(al.toDouble())))
            label(m, id, Offset(x(d), y(al.toDouble()) - 12), c.magenta, 9.sp)
        }
        // Tor przy bieżącej prędkości pionowej, aż do wysokości wybranej
        if (gs > 40) {
            val ftPerNm = vs / gs * 60
            val endAlt = if (sel != null && abs(vs) > 100 && (sel - alt) * vs > 0) sel else alt + ftPerNm * rangeNm
            val dEnd = if (abs(ftPerNm) < 1) rangeNm.toDouble() else ((endAlt - alt) / ftPerNm).coerceIn(0.0, rangeNm.toDouble())
            drawLine(FPV, Offset(x(0.0), y(alt)), Offset(x(dEnd), y(alt + ftPerNm * dEnd)), 3f)
            if (dEnd < rangeNm) drawLine(FPV, Offset(x(dEnd), y(alt + ftPerNm * dEnd)), Offset(right, y(alt + ftPerNm * dEnd)), 3f)
        }
        // Samolot
        val ay = y(alt)
        drawPath(Path().apply { moveTo(left + 14, ay); lineTo(left, ay - 7); lineTo(left, ay + 7); close() }, SYMBOL)
        label(m, "VSD", Offset(w * 0.95f, h * 0.06f), WHITE.copy(alpha = 0.6f), 9.sp)
    }
}
