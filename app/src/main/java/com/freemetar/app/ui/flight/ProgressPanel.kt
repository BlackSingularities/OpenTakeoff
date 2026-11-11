package com.freemetar.app.ui.flight

import com.freemetar.app.domain.grouped
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
private fun ft(v: Double) = "${v.roundToInt().grouped()} ft"

private data class TrackStats(val distNm: Double, val maxAlt: Int?, val maxGs: Int?, val maxVs: Int?, val minVs: Int?)

/** Faza lotu z prędkości, wysokości nad terenem i prędkości pionowej. */
private fun phase(s: FlightState): String {
    val gs = s.speed.gsKt ?: 0.0
    val vs = s.speed.vsFpm ?: 0.0
    val agl = s.avionics?.radioAltFt ?: s.position.altAglFt ?: 99_999.0
    return when {
        s.aircraft.paused == true -> loc("progress.paused")
        s.aircraft.onGround == true -> when {
            gs < 2 -> loc("progress.parked")
            gs < 35 -> loc("progress.taxi")
            else -> loc("progress.takeoff_landing_roll")
        }
        agl < 1_500 && vs > 300 -> loc("progress.initial_climb")
        vs > 300 -> loc("progress.climb")
        vs < -300 && agl < 3_000 && s.controls.gearDown == true -> loc("common.approach")
        vs < -300 -> loc("common.descent")
        agl < 3_000 -> loc("progress.level_low")
        else -> loc("progress.cruise")
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
                share?.let { Text(loc("progress.route_d").format((it * 100).roundToInt().coerceIn(0, 100)), fontFamily = Mono, fontSize = 13.sp, color = c.ink2) }
            }
            share?.let { p ->
                Box(Modifier.fillMaxWidth().height(6.dp).clip(RoundedCornerShape(3.dp)).background(c.terrain)) {
                    Box(Modifier.fillMaxWidth(p.toFloat().coerceIn(0f, 1f)).height(6.dp).clip(RoundedCornerShape(3.dp)).background(c.magenta))
                }
            }

            Group(loc("common.route"))
            FactGrid(cols = cols, facts = if (g == null) listOf(
                Fact(loc("progress.flight_plan"), "—", sub = loc("progress.no_flight_plan_in_the_simulator")),
            ) else listOfNotNull(
                Fact(g.nextId?.takeIf { it.isNotBlank() } ?: loc("progress.next_waypoint"), g.nextDistNm?.let { "%.1f NM".format(it) } ?: "—", accent = c.magenta,
                    sub = listOfNotNull(nextEte?.let { hm(it / 60) }, nextEte?.let { utc(now + (it * 1000).toLong()) }).joinToString("  ").ifEmpty { null }),
                Fact(loc("progress.bearing_dtk"), g.nextBearingMagDeg?.let { deg(it) } ?: "—", sub = g.desiredTrackMagDeg?.let { "DTK ${deg(it)}" }),
                g.crossTrackNm?.let { x ->
                    Fact(loc("progress.cross_track"), if (abs(x) < 0.05) loc("progress.on_course") else "%.2f NM %s".format(abs(x), if (x > 0) loc("common.r") else loc("common.l")),
                        accent = if (abs(x) > (if (g.approachActive) 0.3 else 1.0)) c.caution else null, sub = if (g.approachActive) loc("progress.approach_active") else null)
                },
                g.wpCount?.takeIf { it > 0 }?.let { n -> Fact(loc("progress.waypoint"), "${g.wpIndex ?: 0}/$n", sub = loc("progress.to_go", (n - (g.wpIndex ?: 0)).coerceAtLeast(0)) + (g.prevId?.takeIf { it.isNotBlank() }?.let { " · $it →" } ?: "")) },
                Fact(loc("common.arrival"), destEteMin?.let { utc(now + (it * 60_000).toLong()) } ?: "—",
                    sub = listOfNotNull(destEteMin?.let { loc("progress.in", hm(it)) }, toDest?.let { "~${it.roundToInt()} NM" }).joinToString(" · ").ifEmpty { null }),
                Fact(loc("common.top_of_descent"),
                    if (toDest == null || !airborne) "—" else if (toDest <= todNm) loc("common.now") else loc("progress.in_nm", (toDest - todNm).roundToInt()),
                    accent = if (toDest != null && toDest - todNm < 10 && airborne) c.caution else null,
                    sub = listOfNotNull(
                        toDest?.takeIf { gs > 50 && it > todNm }?.let { hm((it - todNm) / gs * 60) },
                        loc("progress.nm_out", todNm.roundToInt()),
                    ).joinToString(" · ")),
                Fact(loc("progress.3_descent"), "${descentFpm.roundToInt()} ft/min", sub = loc("progress.at_gs_kt", gs.roundToInt())),
            ))

            Group(loc("common.flight"))
            FactGrid(cols = cols, facts = listOfNotNull(
                Fact(loc("common.flight_time"), flownMin?.let { hm(it) } ?: "—", sub = startedAt?.let { loc("progress.off", utc(it)) } ?: loc("progress.recorded_from_takeoff")),
                Fact(loc("progress.flown"), if (stats.distNm > 0) "%.0f NM".format(stats.distNm) else "—",
                    sub = flownMin?.takeIf { it > 1 && stats.distNm > 0 }?.let { loc("progress.avg_d_kt").format((stats.distNm / it * 60).roundToInt()) }),
                stats.maxAlt?.let { Fact(loc("progress.max_altitude"), ft(it.toDouble()), sub = stats.maxGs?.let { m -> loc("progress.max_gs_kt", m) }) },
                if (stats.maxVs != null || stats.minVs != null) Fact(loc("progress.max_v_s"), "+${stats.maxVs ?: 0}", sub = "${stats.minVs ?: 0} ft/min") else null,
                s.simTimeUtc?.let { t -> Fact(loc("progress.sim_time"), t.substringAfter('T').take(5).ifEmpty { t } + "Z", sub = loc("progress.now", utc(now))) },
                Fact(loc("progress.ground_air"), if (airborne) loc("progress.airborne") else loc("progress.on_ground"),
                    sub = s.touchdown?.vsFpm?.let { loc("progress.last_touchdown_ft_min", it.roundToInt()) }),
            ))

            Group(loc("common.speed"))
            FactGrid(cols = cols, facts = listOfNotNull(
                Fact("IAS", s.speed.iasKt?.let { "${it.roundToInt()} kt" } ?: "—", sub = s.speed.mach?.takeIf { it > 0.3 }?.let { "M %.3f".format(it) }),
                Fact("TAS", s.speed.tasKt?.let { "${it.roundToInt()} kt" } ?: "—", sub = "GS ${gs.roundToInt()} kt"),
                head?.let { h ->
                    Fact(if (h >= 0) loc("progress.headwind") else loc("progress.tailwind"), "${abs(h).roundToInt()} kt",
                        accent = if (h < -10) c.green else null,
                        sub = cross?.let { x -> loc("progress.cross_kt", abs(x).roundToInt(), if (x >= 0) loc("progress.from_right") else loc("progress.from_left")) })
                },
                Fact("V/S", "${vs.roundToInt()} ft/min",
                    sub = if (gs > 30) "FPA %.1f°".format(Math.toDegrees(atan(vs / (gs * 101.27)))) else null),
                av?.speeds?.vmoKt?.takeIf { it > 0 }?.let { vmo -> s.speed.iasKt?.let { ias ->
                    Fact(loc("progress.to_vmo"), "${(vmo - ias).roundToInt()} kt", accent = if (vmo - ias < 10) c.caution else null, sub = "VMO ${vmo.roundToInt()} kt")
                } },
                av?.speeds?.vs1Kt?.takeIf { it > 0 }?.let { vs1 -> s.speed.iasKt?.takeIf { airborne }?.let { ias ->
                    Fact(loc("progress.above_vs1"), "${(ias - vs1).roundToInt()} kt", accent = if (ias - vs1 < 15) c.caution else null, sub = "VS1 ${vs1.roundToInt()} kt")
                } },
            ))

            Group(loc("common.altitude"))
            FactGrid(cols = cols, facts = listOfNotNull(
                Fact(loc("common.altitude"), ft(alt), sub = s.position.altMslFt?.let { "MSL ${ft(it)}" }),
                agl?.let { Fact(if (av?.radioAltFt != null) "RA" else "AGL", ft(it), sub = s.position.altMslFt?.let { m -> loc("progress.terrain", ft(m - (s.position.altAglFt ?: it))) }) },
                apAlt?.let { a ->
                    Fact(loc("progress.ap_altitude"), ft(a), accent = c.accent,
                        sub = when { toApAlt == 0.0 -> loc("progress.reached"); toApAlt != null -> loc("progress.in", hm(toApAlt)); else -> "%+d ft".format((a - alt).roundToInt()) })
                },
                pressAlt?.let { Fact(loc("progress.pressure_alt"), ft(it), sub = if (av?.baroStd == true) "STD" else av?.baroHpa?.let { b -> "QNH ${b.roundToInt()}" }) },
                s.environment.qnhHpa?.let { q -> s.environment.oatC?.let { t -> Fact(loc("progress.density_alt"), ft(Atmo.densityAltitude(alt, q, t))) } },
                av?.decisionHeightFt?.takeIf { it > 0 }?.let { Fact(loc("progress.minimums"), ft(it), sub = agl?.let { a -> "%+d ft".format((a - it).roundToInt()) }) },
            ))

            Group(loc("progress.direction"))
            FactGrid(cols = cols, facts = listOfNotNull(
                Fact("HDG", hdgMag?.let { deg(it) } ?: "—", sub = trkMag?.takeIf { gs > 30 }?.let { "TRK ${deg(it)}" }),
                Fact(loc("progress.drift"), drift?.let { "%+.0f°".format(it) } ?: "—", sub = loc("progress.variation_0f_s").format(abs(magVar), if (magVar >= 0) "E" else "W")),
                s.autopilot.hdgDeg?.let { b ->
                    val turn = hdgMag?.let { ((b - it + 540) % 360) - 180 }
                    Fact(loc("progress.hdg_bug"), deg(b), accent = c.accent,
                        sub = turn?.let { t -> if (abs(t) < 1) loc("progress.on_heading") else "${abs(t).roundToInt()}° ${if (t > 0) loc("common.right") else loc("common.left")}" })
                },
                Fact(loc("progress.pitch_bank"), "%.1f° / %.0f°".format(s.attitude.pitchDeg ?: 0.0, s.attitude.bankDeg ?: 0.0),
                    sub = av?.turnRateDps?.let { loc("progress.turn_1f_s").format(abs(it)) }),
                av?.let { a -> a.gForce?.let { Fact(loc("progress.load"), "%.2f g".format(it), sub = a.aoaDeg?.let { d -> "AoA %.1f°".format(d) }) } },
            ))

            Group(loc("progress.atmosphere"))
            FactGrid(cols = cols, facts = listOfNotNull(
                Fact(loc("common.wind"), wDir?.let { d -> "${deg(d - magVar).dropLast(1)}/${(wKt ?: 0.0).roundToInt()} kt" } ?: "—", sub = wDir?.let { loc("progress.from", Geo.compass(it)) }),
                Fact("OAT", s.environment.oatC?.let { "${it.roundToInt()} °C" } ?: "—", sub = isaDev?.let { "ISA %+d".format(it.roundToInt()) }),
                av?.tatC?.let { Fact("TAT", "${it.roundToInt()} °C", sub = null) },
                s.environment.qnhHpa?.let { q -> Fact("QNH", "${q.roundToInt()} hPa", sub = "%.2f inHg".format(Atmo.hpaToInHg(q))) },
            ))

            Group(loc("common.fuel"))
            FactGrid(cols = cols, facts = listOfNotNull(
                Fact(loc("progress.on_board"), fuel?.let { "${it.roundToInt()} kg" } ?: "—", sub = used?.let { loc("progress.used_kg", it.roundToInt()) }),
                Fact(loc("progress.fuel_flow"), if (ff > 0) "${ff.roundToInt()} kg/h" else "—",
                    sub = if (s.engines.size > 1 && ff > 0) s.engines.mapNotNull { it.fuelFlowKgH?.roundToInt() }.joinToString(" / ") else null),
                Fact(loc("progress.endurance"), endurance?.let { hm(it) } ?: "—",
                    accent = endurance?.takeIf { it < 45 }?.let { c.caution },
                    sub = endurance?.takeIf { gs > 30 }?.let { loc("progress.range_nm", (it / 60 * gs).roundToInt()) }),
                atDest?.let { f ->
                    Fact(loc("progress.at_destination"), "${f.roundToInt()} kg", accent = if ((reserveMin ?: 99.0) < 30) c.danger else if ((reserveMin ?: 99.0) < 45) c.caution else c.green,
                        sub = reserveMin?.let { loc("progress.reserve", hm(it)) })
                },
                if (ff > 1 && gs > 30) Fact(loc("progress.consumption"), "%.1f kg/NM".format(ff / gs), sub = loc("progress.0f_nm_per_tonne").format(gs / ff * 1000)) else null,
                used?.takeIf { flownMin != null && flownMin > 5 }?.let { u -> Fact(loc("progress.average"), "${(u / flownMin!! * 60).roundToInt()} kg/h", sub = loc("progress.since_takeoff")) },
            ))

            Group(loc("progress.navigation"))
            FactGrid(cols = cols, facts = listOfNotNull(
                if (lat != null && lon != null) Fact(loc("progress.position"), "%.4f".format(lat), sub = "%.4f".format(lon)) else null,
                nearestApt?.let { a ->
                    val d = Geo.distanceNm(lat!!, lon!!, a.lat, a.lon)
                    Fact(loc("progress.nearest_airport"), a.ident, accent = c.accent,
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
