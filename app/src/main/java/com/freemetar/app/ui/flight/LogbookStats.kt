package com.freemetar.app.ui.flight

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.FlightTakeoff
import androidx.compose.material.icons.outlined.Schedule
import androidx.compose.material.icons.outlined.Public
import androidx.compose.material.icons.outlined.Star
import androidx.compose.material.icons.outlined.TrendingUp
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.material.icons.outlined.FlightLand
import androidx.compose.material.icons.outlined.Waves
import com.freemetar.app.domain.FlightAnalysis
import com.freemetar.app.domain.Lang
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.freemetar.app.data.link.FlightLog
import com.freemetar.app.domain.loc
import com.freemetar.app.ui.theme.Fm
import kotlin.math.roundToInt

/**
 * Rozszerzone statystyki dla dziennika lotów.
 */
@Composable
fun LogbookStatistics(logs: List<FlightLog>) {
    val c = Fm.c
    if (logs.isEmpty()) return

    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        // Główne statystyki
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            StatCard(
                icon = Icons.Outlined.FlightTakeoff,
                value = logs.size.toString(),
                label = loc("Lotów", "Flights"),
                modifier = Modifier.weight(1f)
            )

            val minutes = logs.sumOf { it.durationMin }
            StatCard(
                icon = Icons.Outlined.Schedule,
                value = "%d:%02d".format(minutes / 60, minutes % 60),
                label = loc("Czas", "Time"),
                modifier = Modifier.weight(1f)
            )

            StatCard(
                icon = Icons.Outlined.Public,
                value = "${logs.sumOf { it.distanceNm }}",
                label = "NM",
                modifier = Modifier.weight(1f)
            )
        }

        // Lądowania — każde osobno (touch and go i międzylądowania liczą się oddzielnie)
        val scored = logs.flatMap { f -> f.allLandings.map { it.score } }
        if (scored.isNotEmpty()) {
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(12.dp)
            ) {
                StatCard(
                    icon = Icons.Outlined.Star,
                    value = scored.average().roundToInt().toString(),
                    label = loc("Średnia ocena", "Avg score"),
                    subtitle = loc("najlepsza", "best") + " ${scored.maxOrNull()}",
                    modifier = Modifier.weight(1f)
                )

                StatCard(
                    icon = Icons.Outlined.TrendingUp,
                    value = scored.size.toString(),
                    label = loc("Lądowań", "Landings"),
                    subtitle = loc("lotów z oceną: ", "scored flights: ") + logs.count { it.allLandings.isNotEmpty() },
                    modifier = Modifier.weight(1f)
                )
            }
        }

        // Wykres postępów w czasie (ostatnie 10 lądowań; loty od najnowszego, lądowania w locie chronologicznie)
        val recent = logs.flatMap { f -> f.allLandings.reversed().map { it.score } }.take(10).reversed()
        if (recent.size >= 3) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .background(c.card, RoundedCornerShape(12.dp))
                    .padding(16.dp)
            ) {
                Text(
                    loc("Postępy (ostatnie 10 lądowań)", "Progress (last 10 landings)"),
                    style = MaterialTheme.typography.titleSmall,
                    color = c.ink
                )
                Spacer(Modifier.height(12.dp))
                ProgressChart(recent, modifier = Modifier.fillMaxWidth().height(120.dp))
            }
        }

        // Jakość pilotażu z analizy torów: podejścia, płynność; tendencja z porównania połówek okresu
        val quality = remember(logs, Lang.current) {
            val per = logs.map { it to FlightAnalysis.analyze(it) }
            val approaches = per.flatMap { it.second.approaches }
            val smooth = per.mapNotNull { it.second.smoothness?.score }
            val recentSmooth = per.take((per.size + 1) / 2).mapNotNull { it.second.smoothness?.score }
            val olderSmooth = per.drop((per.size + 1) / 2).mapNotNull { it.second.smoothness?.score }
            Quality(
                approaches.map { it.score }.takeIf { it.isNotEmpty() }?.average()?.roundToInt(),
                if (approaches.isEmpty()) null else approaches.count { it.stableFromFt >= 1000 } * 100 / approaches.size,
                smooth.takeIf { it.isNotEmpty() }?.average()?.roundToInt(),
                if (recentSmooth.isNotEmpty() && olderSmooth.isNotEmpty()) (recentSmooth.average() - olderSmooth.average()).roundToInt() else null,
            )
        }
        if (quality.approach != null || quality.smooth != null) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                StatCard(
                    icon = Icons.Outlined.FlightLand,
                    value = quality.approach?.toString() ?: "—",
                    label = loc("Śr. podejście", "Avg approach"),
                    subtitle = quality.stablePct?.let { loc("stabilne od 1000 ft: $it%", "stable by 1000 ft: $it%") },
                    modifier = Modifier.weight(1f)
                )
                StatCard(
                    icon = Icons.Outlined.Waves,
                    value = quality.smooth?.toString() ?: "—",
                    label = loc("Płynność", "Smoothness"),
                    subtitle = quality.smoothTrend?.let { (if (it >= 0) "+" else "") + it + loc(" w nowszych lotach", " in recent flights") },
                    modifier = Modifier.weight(1f)
                )
            }
        }

        // Samoloty: liczba lotów i czas
        val fleet = logs.groupBy { it.aircraft ?: "?" }.map { (a, l) -> Triple(a, l.size, l.sumOf { it.durationMin }) }.sortedByDescending { it.third }.take(5)
        if (fleet.size > 1 || fleet.firstOrNull()?.first != "?") {
            Column(
                Modifier
                    .fillMaxWidth()
                    .background(c.card, RoundedCornerShape(12.dp))
                    .padding(16.dp)
            ) {
                Text(loc("Samoloty", "Aircraft"), style = MaterialTheme.typography.titleSmall, color = c.ink)
                Spacer(Modifier.height(8.dp))
                val maxMin = fleet.maxOf { it.third }.coerceAtLeast(1)
                fleet.forEach { (a, n, min) ->
                    Row(Modifier.fillMaxWidth().padding(vertical = 3.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text(a, color = c.ink, fontSize = 13.sp, maxLines = 1, modifier = Modifier.weight(1f))
                        Text(loc("$n lot. ", "$n fl. ") + "%d:%02d h".format(min / 60, min % 60), fontSize = 12.sp, color = c.ink2)
                    }
                    Box(Modifier.fillMaxWidth(min.toFloat() / maxMin).height(4.dp).background(c.accent, RoundedCornerShape(2.dp)))
                }
            }
        }

        // Top lotniska: start i każde lądowanie (bez lądowań — lotnisko końcowe)
        val airports = logs.flatMap { f ->
            val landed = f.allLandings.mapNotNull { it.airport }
            listOfNotNull(f.from) + landed.ifEmpty { listOfNotNull(f.to) }
        }
            .groupingBy { it }.eachCount()
            .entries.sortedByDescending { it.value }.take(5)

        if (airports.isNotEmpty()) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .background(c.card, RoundedCornerShape(12.dp))
                    .padding(16.dp)
            ) {
                Text(
                    loc("Najczęstsze lotniska", "Most visited airports"),
                    style = MaterialTheme.typography.titleSmall,
                    color = c.ink
                )
                Spacer(Modifier.height(8.dp))
                airports.forEach { (icao, count) ->
                    Row(
                        Modifier.fillMaxWidth().padding(vertical = 4.dp),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text(icao, fontWeight = FontWeight.Bold, color = c.ink)
                        Text(
                            loc("$count wizyt", "$count visits"),
                            fontSize = 13.sp,
                            color = c.ink2
                        )
                    }
                }
            }
        }
    }
}

private data class Quality(val approach: Int?, val stablePct: Int?, val smooth: Int?, val smoothTrend: Int?)

@Composable
private fun StatCard(
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    value: String,
    label: String,
    subtitle: String? = null,
    modifier: Modifier = Modifier
) {
    val c = Fm.c
    Column(
        modifier
            .background(c.card, RoundedCornerShape(12.dp))
            .padding(12.dp),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Icon(icon, null, tint = c.accent, modifier = Modifier.size(24.dp))
        Spacer(Modifier.height(8.dp))
        Text(
            value,
            style = MaterialTheme.typography.headlineMedium,
            fontWeight = FontWeight.Bold,
            color = c.ink
        )
        Text(label, fontSize = 12.sp, color = c.ink2)
        if (subtitle != null) {
            Text(subtitle, fontSize = 11.sp, color = c.ink2)
        }
    }
}

@Composable
private fun ProgressChart(scores: List<Int>, modifier: Modifier = Modifier) {
    val c = Fm.c
    Canvas(modifier) {
        if (scores.size < 2) return@Canvas

        val w = size.width
        val h = size.height
        val maxScore = 100f
        val minScore = scores.minOrNull()?.toFloat()?.coerceAtMost(50f) ?: 0f
        val range = maxScore - minScore

        val stepX = w / (scores.size - 1)

        // Linia wykresu
        val path = Path()
        scores.forEachIndexed { i, score ->
            val x = i * stepX
            val y = h - ((score - minScore) / range * h)
            if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
        }

        drawPath(
            path = path,
            color = c.accent,
            style = Stroke(width = 3.dp.toPx())
        )

        // Punkty
        scores.forEachIndexed { i, score ->
            val x = i * stepX
            val y = h - ((score - minScore) / range * h)
            drawCircle(
                color = c.accent,
                radius = 4.dp.toPx(),
                center = Offset(x, y)
            )
        }

        // Linia bazowa (80 punktów)
        val baseline80 = h - ((80f - minScore) / range * h)
        drawLine(
            color = c.ink2.copy(alpha = 0.3f),
            start = Offset(0f, baseline80),
            end = Offset(w, baseline80),
            strokeWidth = 1.dp.toPx()
        )
    }
}
