package com.freemetar.app.ui.flight

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.freemetar.app.MainViewModel
import com.freemetar.app.data.link.FlightState
import com.freemetar.app.data.link.TrackPoint
import com.freemetar.app.data.nav.FixResult
import com.freemetar.app.domain.Atmo
import com.freemetar.app.domain.Geo
import com.freemetar.app.domain.loc
import com.freemetar.app.ui.components.Fact
import com.freemetar.app.ui.components.FactGrid
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.Mono
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import kotlin.math.abs
import kotlin.math.atan
import kotlin.math.cos
import kotlin.math.roundToInt
import kotlin.math.sin

private fun hm(min: Double): String = min.roundToInt().coerceAtLeast(0).let { "%d:%02d".format(it / 60, it % 60) }
private fun utc(epochMs: Long): String = Instant.ofEpochMilli(epochMs).atZone(ZoneOffset.UTC).format(DateTimeFormatter.ofPattern("HH:mm")) + "Z"
private fun d360(a: Double) = ((a % 360) + 360) % 360
private fun deg(a: Double) = Math.round(d360(a)).toInt().let { if (it == 0) 360 else it }.let { "%03d°".format(it) }
private fun ft(v: Double) = "%,d ft".format(v.roundToInt()).replace(',', ' ')

private data class TrackStats(val distNm: Double, val maxAlt: Int?, val maxGs: Int?, val maxVs: Int?, val minVs: Int?)

/** Faza lotu z prędkości, wysokości nad terenem i prędkości pionowej. */
private fun phase(s: FlightState): String {
    val gs = s.speed.gsKt ?: 0.0
    val vs = s.speed.vsFpm ?: 0.0
    val agl = s.avionics?.radioAltFt ?: s.position.altAglFt ?: 99_999.0
    return when {
        s.aircraft.paused == true -> loc("Pauza", "Paused")
        s.aircraft.onGround == true -> when {
            gs < 2 -> loc("Postój", "Parked")
            gs < 35 -> loc("Kołowanie", "Taxi")
            else -> loc("Rozbieg / dobieg", "Takeoff / landing roll")
        }
        agl < 1_500 && vs > 300 -> loc("Wznoszenie początkowe", "Initial climb")
        vs > 300 -> loc("Wznoszenie", "Climb")
        vs < -300 && agl < 3_000 && s.controls.gearDown == true -> loc("Podejście", "Approach")
        vs < -300 -> loc("Zniżanie", "Descent")
        agl < 3_000 -> loc("Lot poziomy nisko", "Level, low")
        else -> loc("Przelot", "Cruise")
    }
}

/**
 * Postęp lotu: trasa z planu GPS w symulatorze (następny punkt, cel, ETA, TOD, odchylenie), przebieg lotu z toru
 * (czas, odległość, maksima), prędkości i składowe wiatru, wysokość i cel autopilota, kurs i znoszenie,
 * atmosfera, paliwo (zużycie, starczalność, zasięg, zapas na celu) i nawigacja (najbliższe lotnisko, NAV1/NAV2,
 * transponder). Pola bez danych z symulatora są pomijane.
 */
@Composable
internal fun Progress(
    s: FlightState,
    track: List<TrackPoint> = emptyList(),
    recording: MainViewModel.RecordingInfo = MainViewModel.RecordingInfo(),
    nearby: List<FixResult> = emptyList(),
) {
    val c = Fm.c
    val av = s.avionics
    val g = av?.gps?.takeIf { it.activeFlightPlan }
    val gs = s.speed.gsKt ?: 0.0
    val airborne = s.aircraft.onGround != true
    val magVar = av?.magVarDeg ?: s.attitude.hdgTrueDeg?.let { t -> s.attitude.hdgMagDeg?.let { m -> ((t - m + 540) % 360) - 180 } } ?: 0.0
    val now = System.currentTimeMillis()
    val lat = s.position.lat; val lon = s.position.lon

    // Przebieg z zapisanego toru: odległość, maksima — liczone tylko po dojściu nowego punktu
    val stats = remember(track.size, track.firstOrNull()?.t) {
        var dist = 0.0
        for (i in 1 until track.size) dist += Geo.distanceNm(track[i - 1].lat, track[i - 1].lon, track[i].lat, track[i].lon)
        TrackStats(dist, track.maxOfOrNull { it.altFt }, track.maxOfOrNull { it.gsKt },
            track.mapNotNull { it.vsFpm }.maxOrNull(), track.mapNotNull { it.vsFpm }.minOrNull())
    }
    val startedAt = recording.startedAt ?: track.firstOrNull()?.t
    val flownMin = startedAt?.let { (now - it) / 60_000.0 }

    // Trasa
    val destEteMin = g?.destEteSec?.takeIf { it > 0 }?.let { it / 60 }
    val toDest = destEteMin?.let { if (gs > 50) it / 60 * gs else null }
    val alt = s.position.altIndFt ?: s.position.altMslFt ?: 0.0
    val agl = av?.radioAltFt ?: s.position.altAglFt
    val todNm = ((agl ?: alt) / 1000.0 * 3).coerceAtLeast(0.0)
    val descentFpm = gs * 5.3 // ścieżka 3°
    val nextEte = g?.nextEteSec?.takeIf { it > 0 }

    // Wiatr względem śladu: składowa czołowa (+) / tylna (−) i boczna (+ z prawej)
    val trkTrue = s.attitude.trackTrueDeg ?: s.attitude.hdgTrueDeg
    val wDir = s.environment.windDirDeg; val wKt = s.environment.windSpdKt
    val head = if (wDir != null && wKt != null && trkTrue != null) wKt * cos(Math.toRadians(wDir - trkTrue)) else null
    val cross = if (wDir != null && wKt != null && trkTrue != null) wKt * sin(Math.toRadians(wDir - trkTrue)) else null

    // Paliwo
    val fuel = s.fuel.totalKg
    val ff = s.engines.mapNotNull { it.fuelFlowKgH }.sum()
    val endurance = if (fuel != null && ff > 1) fuel / ff * 60 else null
    val used = recording.startFuelKg?.let { st -> fuel?.let { st - it } }?.takeIf { it >= 0 }
    val atDest = if (fuel != null && ff > 1 && destEteMin != null) fuel - ff * destEteMin / 60 else null
    val reserveMin = atDest?.let { it / ff * 60 }

    val hdgMag = s.attitude.hdgMagDeg ?: s.attitude.hdgTrueDeg?.minus(magVar)
    val trkMag = s.attitude.trackTrueDeg?.minus(magVar)
    val drift = if (hdgMag != null && trkMag != null && gs > 30) ((trkMag - hdgMag + 540) % 360) - 180 else null
    val pressAlt = av?.pressureAltFt ?: s.environment.qnhHpa?.let { Atmo.pressureAltitude(alt, it) }
    val isaDev = s.environment.oatC?.let { oat -> pressAlt?.let { oat - Atmo.isaTemp(it) } }
    val apAlt = s.autopilot.altFt?.takeIf { it > 0 }
    val vs = s.speed.vsFpm ?: 0.0
    val toApAlt = apAlt?.let { a -> val d = a - alt; if (abs(d) < 100) 0.0 else if (vs * d > 0 && abs(vs) > 100) d / vs else null }
    val nearestApt = if (lat != null && lon != null) nearby.filter { it.type == "airport" }
        .minByOrNull { Geo.distanceNm(lat, lon, it.lat, it.lon) } else null

    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val cols = if (maxWidth >= 560.dp) 3 else 2
        Column(Modifier.fillMaxWidth()) {
            // Pasek: faza lotu i postęp trasy (punkty planu albo odległość przebyta / pozostała)
            val wpCount = g?.wpCount?.takeIf { it > 1 }
            val share = when {
                toDest != null && stats.distNm > 0 -> stats.distNm / (stats.distNm + toDest)
                wpCount != null -> (g?.wpIndex ?: 0).toDouble() / (wpCount - 1)
                else -> null
            }
            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(bottom = 6.dp)) {
                Text(phase(s), fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 15.sp, color = c.accent)
                Spacer(Modifier.weight(1f))
                share?.let { Text(loc("trasa %d%%", "route %d%%").format((it * 100).roundToInt().coerceIn(0, 100)), fontFamily = Mono, fontSize = 13.sp, color = c.ink2) }
            }
            share?.let { p ->
                Box(Modifier.fillMaxWidth().height(6.dp).clip(RoundedCornerShape(3.dp)).background(c.terrain)) {
                    Box(Modifier.fillMaxWidth(p.toFloat().coerceIn(0f, 1f)).height(6.dp).clip(RoundedCornerShape(3.dp)).background(c.magenta))
                }
            }

            Group(loc("Trasa", "Route"))
            FactGrid(cols = cols, facts = if (g == null) listOf(
                Fact(loc("Plan lotu", "Flight plan"), "—", sub = loc("brak planu w symulatorze", "no flight plan in the simulator")),
            ) else listOfNotNull(
                Fact(g.nextId?.takeIf { it.isNotBlank() } ?: loc("Następny punkt", "Next waypoint"), g.nextDistNm?.let { "%.1f NM".format(it) } ?: "—", accent = c.magenta,
                    sub = listOfNotNull(nextEte?.let { hm(it / 60) }, nextEte?.let { utc(now + (it * 1000).toLong()) }).joinToString("  ").ifEmpty { null }),
                Fact(loc("Namiar / DTK", "Bearing / DTK"), g.nextBearingMagDeg?.let { deg(it) } ?: "—", sub = g.desiredTrackMagDeg?.let { "DTK ${deg(it)}" }),
                g.crossTrackNm?.let { x ->
                    Fact(loc("Odchylenie XTK", "Cross-track"), if (abs(x) < 0.05) loc("na kursie", "on course") else "%.2f NM %s".format(abs(x), if (x > 0) loc("P", "R") else "L"),
                        accent = if (abs(x) > (if (g.approachActive) 0.3 else 1.0)) c.caution else null, sub = if (g.approachActive) loc("podejście aktywne", "approach active") else null)
                },
                g.wpCount?.takeIf { it > 0 }?.let { n -> Fact(loc("Punkt planu", "Waypoint"), "${g.wpIndex ?: 0}/$n", sub = loc("zostało ${(n - (g.wpIndex ?: 0)).coerceAtLeast(0)}", "${(n - (g.wpIndex ?: 0)).coerceAtLeast(0)} to go") + (g.prevId?.takeIf { it.isNotBlank() }?.let { " · $it →" } ?: "")) },
                Fact(loc("Przylot", "Arrival"), destEteMin?.let { utc(now + (it * 60_000).toLong()) } ?: "—",
                    sub = listOfNotNull(destEteMin?.let { loc("za ${hm(it)}", "in ${hm(it)}") }, toDest?.let { "~${it.roundToInt()} NM" }).joinToString(" · ").ifEmpty { null }),
                Fact(loc("Początek zniżania", "Top of descent"),
                    if (toDest == null || !airborne) "—" else if (toDest <= todNm) loc("teraz", "now") else loc("za ${(toDest - todNm).roundToInt()} NM", "in ${(toDest - todNm).roundToInt()} NM"),
                    accent = if (toDest != null && toDest - todNm < 10 && airborne) c.caution else null,
                    sub = listOfNotNull(
                        toDest?.takeIf { gs > 50 && it > todNm }?.let { hm((it - todNm) / gs * 60) },
                        loc("${todNm.roundToInt()} NM przed celem", "${todNm.roundToInt()} NM out"),
                    ).joinToString(" · ")),
                Fact(loc("Zniżanie 3°", "3° descent"), "${descentFpm.roundToInt()} ft/min", sub = loc("przy GS ${gs.roundToInt()} kt", "at GS ${gs.roundToInt()} kt")),
            ))

            Group(loc("Lot", "Flight"))
            FactGrid(cols = cols, facts = listOfNotNull(
                Fact(loc("Czas lotu", "Flight time"), flownMin?.let { hm(it) } ?: "—", sub = startedAt?.let { loc("start ${utc(it)}", "off ${utc(it)}") } ?: loc("zapis od startu", "recorded from takeoff")),
                Fact(loc("Przebyto", "Flown"), if (stats.distNm > 0) "%.0f NM".format(stats.distNm) else "—",
                    sub = flownMin?.takeIf { it > 1 && stats.distNm > 0 }?.let { loc("średnio %d kt", "avg %d kt").format((stats.distNm / it * 60).roundToInt()) }),
                stats.maxAlt?.let { Fact(loc("Maks. wysokość", "Max altitude"), ft(it.toDouble()), sub = stats.maxGs?.let { m -> loc("maks. GS $m kt", "max GS $m kt") }) },
                if (stats.maxVs != null || stats.minVs != null) Fact(loc("Maks. V/S", "Max V/S"), "+${stats.maxVs ?: 0}", sub = "${stats.minVs ?: 0} ft/min") else null,
                s.simTimeUtc?.let { t -> Fact(loc("Czas symulatora", "Sim time"), t.substringAfter('T').take(5).ifEmpty { t } + "Z", sub = loc("teraz ${utc(now)}", "now ${utc(now)}")) },
                Fact(loc("Na ziemi / w locie", "Ground / air"), if (airborne) loc("w powietrzu", "airborne") else loc("na ziemi", "on ground"),
                    sub = s.touchdown?.vsFpm?.let { loc("ost. przyziemienie ${it.roundToInt()} ft/min", "last touchdown ${it.roundToInt()} ft/min") }),
            ))

            Group(loc("Prędkość", "Speed"))
            FactGrid(cols = cols, facts = listOfNotNull(
                Fact("IAS", s.speed.iasKt?.let { "${it.roundToInt()} kt" } ?: "—", sub = s.speed.mach?.takeIf { it > 0.3 }?.let { "M %.3f".format(it) }),
                Fact("TAS", s.speed.tasKt?.let { "${it.roundToInt()} kt" } ?: "—", sub = "GS ${gs.roundToInt()} kt"),
                head?.let { h ->
                    Fact(if (h >= 0) loc("Wiatr czołowy", "Headwind") else loc("Wiatr tylny", "Tailwind"), "${abs(h).roundToInt()} kt",
                        accent = if (h < -10) c.green else null,
                        sub = cross?.let { x -> loc("boczny ${abs(x).roundToInt()} kt ${if (x >= 0) "z prawej" else "z lewej"}", "cross ${abs(x).roundToInt()} kt ${if (x >= 0) "from right" else "from left"}") })
                },
                Fact("V/S", "${vs.roundToInt()} ft/min",
                    sub = if (gs > 30) "FPA %.1f°".format(Math.toDegrees(atan(vs / (gs * 101.27)))) else null),
                av?.speeds?.vmoKt?.takeIf { it > 0 }?.let { vmo -> s.speed.iasKt?.let { ias ->
                    Fact(loc("Do VMO", "To VMO"), "${(vmo - ias).roundToInt()} kt", accent = if (vmo - ias < 10) c.caution else null, sub = "VMO ${vmo.roundToInt()} kt")
                } },
                av?.speeds?.vs1Kt?.takeIf { it > 0 }?.let { vs1 -> s.speed.iasKt?.takeIf { airborne }?.let { ias ->
                    Fact(loc("Nad VS1", "Above VS1"), "${(ias - vs1).roundToInt()} kt", accent = if (ias - vs1 < 15) c.caution else null, sub = "VS1 ${vs1.roundToInt()} kt")
                } },
            ))

            Group(loc("Wysokość", "Altitude"))
            FactGrid(cols = cols, facts = listOfNotNull(
                Fact(loc("Wysokość", "Altitude"), ft(alt), sub = s.position.altMslFt?.let { "MSL ${ft(it)}" }),
                agl?.let { Fact(if (av?.radioAltFt != null) "RA" else "AGL", ft(it), sub = s.position.altMslFt?.let { m -> loc("teren ${ft(m - (s.position.altAglFt ?: it))}", "terrain ${ft(m - (s.position.altAglFt ?: it))}") }) },
                apAlt?.let { a ->
                    Fact(loc("Cel autopilota", "AP altitude"), ft(a), accent = c.accent,
                        sub = when { toApAlt == 0.0 -> loc("osiągnięta", "reached"); toApAlt != null -> loc("za ${hm(toApAlt)}", "in ${hm(toApAlt)}"); else -> "%+d ft".format((a - alt).roundToInt()) })
                },
                pressAlt?.let { Fact(loc("Wys. ciśnieniowa", "Pressure alt"), ft(it), sub = if (av?.baroStd == true) "STD" else av?.baroHpa?.let { b -> "QNH ${b.roundToInt()}" }) },
                s.environment.qnhHpa?.let { q -> s.environment.oatC?.let { t -> Fact(loc("Wys. gęstościowa", "Density alt"), ft(Atmo.densityAltitude(alt, q, t))) } },
                av?.decisionHeightFt?.takeIf { it > 0 }?.let { Fact(loc("Minima", "Minimums"), ft(it), sub = agl?.let { a -> "%+d ft".format((a - it).roundToInt()) }) },
            ))

            Group(loc("Kierunek", "Direction"))
            FactGrid(cols = cols, facts = listOfNotNull(
                Fact("HDG", hdgMag?.let { deg(it) } ?: "—", sub = trkMag?.takeIf { gs > 30 }?.let { "TRK ${deg(it)}" }),
                Fact(loc("Znoszenie", "Drift"), drift?.let { "%+.0f°".format(it) } ?: "—", sub = loc("deklinacja %.0f°%s", "variation %.0f°%s").format(abs(magVar), if (magVar >= 0) "E" else "W")),
                s.autopilot.hdgDeg?.let { b ->
                    val turn = hdgMag?.let { ((b - it + 540) % 360) - 180 }
                    Fact(loc("Nastawa HDG", "HDG bug"), deg(b), accent = c.accent,
                        sub = turn?.let { t -> if (abs(t) < 1) loc("na kursie", "on heading") else loc("${abs(t).roundToInt()}° w ${if (t > 0) "prawo" else "lewo"}", "${abs(t).roundToInt()}° ${if (t > 0) "right" else "left"}") })
                },
                Fact(loc("Pochylenie / przechylenie", "Pitch / bank"), "%.1f° / %.0f°".format(s.attitude.pitchDeg ?: 0.0, s.attitude.bankDeg ?: 0.0),
                    sub = av?.turnRateDps?.let { loc("zakręt %.1f°/s", "turn %.1f°/s").format(abs(it)) }),
                av?.let { a -> a.gForce?.let { Fact(loc("Przeciążenie", "Load"), "%.2f g".format(it), sub = a.aoaDeg?.let { d -> "AoA %.1f°".format(d) }) } },
            ))

            Group(loc("Atmosfera", "Atmosphere"))
            FactGrid(cols = cols, facts = listOfNotNull(
                Fact(loc("Wiatr", "Wind"), wDir?.let { d -> "${deg(d - magVar).dropLast(1)}/${(wKt ?: 0.0).roundToInt()} kt" } ?: "—", sub = wDir?.let { loc("z ${Geo.compass(it)}", "from ${Geo.compass(it)}") }),
                Fact("OAT", s.environment.oatC?.let { "${it.roundToInt()} °C" } ?: "—", sub = isaDev?.let { "ISA %+d".format(it.roundToInt()) }),
                av?.tatC?.let { Fact("TAT", "${it.roundToInt()} °C", sub = null) },
                s.environment.qnhHpa?.let { q -> Fact("QNH", "${q.roundToInt()} hPa", sub = "%.2f inHg".format(Atmo.hpaToInHg(q))) },
            ))

            Group(loc("Paliwo", "Fuel"))
            FactGrid(cols = cols, facts = listOfNotNull(
                Fact(loc("Na pokładzie", "On board"), fuel?.let { "${it.roundToInt()} kg" } ?: "—", sub = used?.let { loc("zużyto ${it.roundToInt()} kg", "used ${it.roundToInt()} kg") }),
                Fact(loc("Przepływ", "Fuel flow"), if (ff > 0) "${ff.roundToInt()} kg/h" else "—",
                    sub = if (s.engines.size > 1 && ff > 0) s.engines.mapNotNull { it.fuelFlowKgH?.roundToInt() }.joinToString(" / ") else null),
                Fact(loc("Starczalność", "Endurance"), endurance?.let { hm(it) } ?: "—",
                    accent = endurance?.takeIf { it < 45 }?.let { c.caution },
                    sub = endurance?.takeIf { gs > 30 }?.let { loc("zasięg ~${(it / 60 * gs).roundToInt()} NM", "range ~${(it / 60 * gs).roundToInt()} NM") }),
                atDest?.let { f ->
                    Fact(loc("Na celu", "At destination"), "${f.roundToInt()} kg", accent = if ((reserveMin ?: 99.0) < 30) c.danger else if ((reserveMin ?: 99.0) < 45) c.caution else c.green,
                        sub = reserveMin?.let { loc("zapas ${hm(it)}", "reserve ${hm(it)}") })
                },
                if (ff > 1 && gs > 30) Fact(loc("Zużycie", "Consumption"), "%.1f kg/NM".format(ff / gs), sub = loc("%.0f NM na tonę", "%.0f NM per tonne").format(gs / ff * 1000)) else null,
                used?.takeIf { flownMin != null && flownMin > 5 }?.let { u -> Fact(loc("Średnio", "Average"), "${(u / flownMin!! * 60).roundToInt()} kg/h", sub = loc("od startu", "since takeoff")) },
            ))

            Group(loc("Nawigacja", "Navigation"))
            FactGrid(cols = cols, facts = listOfNotNull(
                if (lat != null && lon != null) Fact(loc("Pozycja", "Position"), "%.4f".format(lat), sub = "%.4f".format(lon)) else null,
                nearestApt?.let { a ->
                    val d = Geo.distanceNm(lat!!, lon!!, a.lat, a.lon)
                    Fact(loc("Najbliższe lotnisko", "Nearest airport"), a.ident, accent = c.accent,
                        sub = "%.0f NM · %s".format(d, deg(Geo.bearing(lat, lon, a.lat, a.lon) - magVar)) + (if (gs > 30) " · ${hm(d / gs * 60)}" else ""))
                },
                av?.nav1?.takeIf { it.hasNav }?.let { n -> Fact("NAV1 ${n.ident.orEmpty()}".trim(), if (n.hasLoc) "LOC ${n.locCourseDeg?.let { deg(it) } ?: ""}" else n.radialDeg?.let { "R-${deg(it).dropLast(1)}" } ?: "—",
                    sub = listOfNotNull(n.dmeNm?.takeIf { n.hasDme }?.let { "DME %.1f".format(it) }, n.toFrom?.uppercase(), s.radios.nav1?.activeMhz?.let { "%.2f".format(it) }).joinToString(" · ").ifEmpty { null }) },
                av?.nav2?.takeIf { it.hasNav }?.let { n -> Fact("NAV2 ${n.ident.orEmpty()}".trim(), n.radialDeg?.let { "R-${deg(it).dropLast(1)}" } ?: "—",
                    sub = listOfNotNull(n.dmeNm?.takeIf { n.hasDme }?.let { "DME %.1f".format(it) }, s.radios.nav2?.activeMhz?.let { "%.2f".format(it) }).joinToString(" · ").ifEmpty { null }) },
                s.radios.com1?.let { r -> r.activeMhz?.let { Fact("COM1", "%.3f".format(it), sub = r.standbyMhz?.let { sb -> "STBY %.3f".format(sb) }) } },
                s.radios.transponder?.let { x -> x.code?.let { Fact("XPDR", it, accent = if (it in setOf("7500", "7600", "7700")) c.danger else null, sub = x.mode?.uppercase()) } },
            ))
        }
    }
}

@Composable
private fun Group(title: String) {
    Text(title.uppercase(), style = MaterialTheme.typography.labelSmall, fontWeight = FontWeight.Bold, color = Fm.c.ink2,
        letterSpacing = 1.2.sp, modifier = Modifier.padding(top = 14.dp, bottom = 2.dp))
}

@Composable
private fun FactGrid(cols: Int, facts: List<Fact>) = FactGrid(facts, columns = cols)
