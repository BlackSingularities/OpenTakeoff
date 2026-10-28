package com.freemetar.app.ui.flight

import com.freemetar.app.domain.loc

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.FileDownload
import androidx.compose.material.icons.outlined.FlightLand
import androidx.compose.material.icons.outlined.History
import androidx.compose.material.icons.automirrored.outlined.ShowChart
import androidx.compose.material.icons.outlined.Timeline
import androidx.compose.material.icons.outlined.ViewInAr
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.foundation.horizontalScroll
import androidx.compose.material3.FilterChip
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.freemetar.app.MainViewModel
import com.freemetar.app.data.link.FlightLog
import com.freemetar.app.domain.FlightAnalysis
import com.freemetar.app.domain.LandingReport
import com.freemetar.app.domain.Lang
import com.freemetar.app.domain.Scenario
import com.freemetar.app.ui.components.Fact
import com.freemetar.app.ui.components.FactGrid
import com.freemetar.app.ui.components.Rule
import com.freemetar.app.ui.components.Section
import com.freemetar.app.ui.components.TrendChart
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.Mono
import java.text.DateFormat
import java.util.Date
import kotlin.math.abs
import kotlin.math.roundToInt

/** Kolor oceny: zieleń / biel / bursztyn / czerwień — jedyne miejsce, gdzie ocena ma kolor. */
@Composable
fun scoreColor(score: Int): Color {
    val c = Fm.c
    return when {
        score >= 90 -> c.green
        score >= 75 -> c.ink
        score >= 55 -> c.caution
        else -> c.danger
    }
}

@Composable
fun ScoreBadge(l: LandingReport?, size: Int = 44) {
    val c = Fm.c
    val col = l?.let { scoreColor(it.score) } ?: c.ink2
    Box(Modifier.size(size.dp).clip(CircleShape).border(2.dp, col, CircleShape), contentAlignment = Alignment.Center) {
        Text(l?.score?.toString() ?: "—", fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = (size / 3).sp, color = col)
    }
}

/** Szczegóły lotu: odtworzenie z osią czasu i znacznikami, analiza (lądowanie, podejście, płynność, ćwiczenie, porównanie), raport instruktorski. */
@Composable
fun FlightDetailScreen(vm: MainViewModel, id: String, onBack: () -> Unit, onShowMap: () -> Unit = {}) {
    val c = Fm.c
    val logs by vm.flightLogs.collectAsStateWithLifecycle()
    val f = logs.firstOrNull { it.id == id } ?: return
    val ctx = LocalContext.current
    val exporter = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/gpx+xml")) { uri ->
        uri?.let { u -> vm.exportFlightGpx(ctx, f, u) }
    }
    Column(Modifier.fillMaxSize().background(c.paper)) {
        Row(Modifier.fillMaxWidth().statusBarsPadding().padding(horizontal = 4.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onBack) { Icon(Icons.AutoMirrored.Outlined.ArrowBack, loc("Wróć", "Back"), tint = c.ink) }
            Column(Modifier.weight(1f)) {
                Text("${f.from ?: "?"} → ${f.to ?: "?"}", fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 20.sp, color = c.ink)
                Text(DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(Date(f.start)) + (f.aircraft?.let { ", $it" } ?: ""),
                    style = MaterialTheme.typography.bodySmall, color = c.ink2, maxLines = 1)
            }
            IconButton({ exporter.launch("lot-${f.from ?: "x"}-${f.to ?: "y"}.gpx") }) { Icon(Icons.Outlined.FileDownload, loc("Eksportuj tor do GPX", "Export track to GPX"), tint = c.ink) }
            IconButton({ vm.deleteFlightLog(f.id); onBack() }) { Icon(Icons.Outlined.Delete, loc("Usuń lot", "Delete flight"), tint = c.danger) }
        }
        Rule()
        val analysis = remember(f, Lang.current) { FlightAnalysis.analyze(f) }
        val scenario = Scenario.of(f.scenario)
        val scenarioResult = remember(f, scenario, Lang.current) { scenario?.let { FlightAnalysis.scenario(f, it, analysis) } }
        val comparison = remember(f, logs, Lang.current) { FlightAnalysis.compare(f, logs, analysis) }
        val report = remember(f, analysis, scenarioResult, comparison, Lang.current) { FlightAnalysis.instructorReport(f, analysis, scenarioResult, comparison) }
        var tab by rememberSaveable { mutableStateOf(0) }
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 14.dp, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(loc("Odtworzenie", "Replay"), loc("Analiza", "Analysis"), loc("Raport", "Report")).forEachIndexed { i, t ->
                FilterChip(tab == i, { tab = i }, { Text(t) })
            }
        }
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(bottom = 24.dp)) {
            when (tab) {
                0 -> {
                    Section(loc("Odtworzenie lotu", "Flight replay"), Icons.Outlined.History) {
                        FlightReplay(f, analysis.events, { t, label -> vm.setFlightMarker(f.id, t, label) }, { t -> vm.deleteFlightMarker(f.id, t) })
                        androidx.compose.material3.OutlinedButton({ vm.showFlightOnMap(f.id); onShowMap() }, Modifier.padding(top = 10.dp)) {
                            Icon(Icons.Outlined.ViewInAr, null, Modifier.size(18.dp))
                            Spacer(Modifier.width(8.dp))
                            Text(loc("Pokaż na mapie 3D", "Show on the 3D map"))
                        }
                    }
                    Section(loc("Lot", "Flight"), Icons.Outlined.Timeline) {
                        FactGrid(listOf(
                            Fact(loc("Czas", "Time"), "%d:%02d".format(f.durationMin / 60, f.durationMin % 60)),
                            Fact(loc("Dystans", "Distance"), "${f.distanceNm} NM"),
                            Fact(loc("Najwyżej", "Highest"), "FL%03d".format(f.maxAltFt / 100)),
                            Fact(loc("Średnia GS", "Average GS"), f.track.map { it.gsKt }.filter { it > 50 }.takeIf { it.isNotEmpty() }?.average()?.roundToInt()?.let { "$it kt" } ?: "—"),
                        ))
                    }
                }
                1 -> {
                    val landings = f.allLandings
                    landings.forEachIndexed { i, l -> LandingCard(l, if (landings.size > 1) "${i + 1}/${landings.size}" else null) }
                    if (landings.isEmpty()) Section(loc("Lądowanie", "Landing"), Icons.Outlined.FlightLand) {
                        Text(loc("Brak danych o przyziemieniu (lot zakończony w powietrzu albo nagrany starszą wersją).", "No touchdown data (the flight ended in the air or was recorded by an older version)."), style = MaterialTheme.typography.bodyMedium, color = c.ink2)
                    }
                    analysis.approaches.forEachIndexed { i, a -> ApproachSection(a, if (analysis.approaches.size > 1) "${i + 1}/${analysis.approaches.size}" else null) }
                    SmoothnessSection(analysis.smoothness)
                    ScenarioSection(scenario, scenarioResult) { vm.setFlightScenario(f.id, it?.id) }
                    ComparisonSection(comparison)
                }
                else -> InstructorReportSection(report, "${f.from ?: "?"} → ${f.to ?: "?"}")
            }
        }
    }
}

@Composable
private fun LandingCard(l: LandingReport, ordinal: String? = null) {
    val c = Fm.c
    val col = scoreColor(l.score)
    Section(loc("Lądowanie", "Landing") + (ordinal?.let { " $it" } ?: "") + ((l.runway ?: l.airport)?.let { " — $it" } ?: ""), Icons.Outlined.FlightLand, frame = if (l.score < 55) c.danger else null) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            ScoreBadge(l, 72)
            Spacer(Modifier.width(16.dp))
            Column {
                Text(l.gradeText, style = MaterialTheme.typography.headlineMedium, color = col)
                Text(loc("${l.vsFpm} ft/min przy przyziemieniu", "${l.vsFpm} ft/min at touchdown"), fontFamily = Mono, fontSize = 14.sp, color = c.ink)
            }
        }
        Spacer(Modifier.size(12.dp))
        FactGrid(listOfNotNull(
            Fact(loc("Prędkość pionowa", "Vertical speed"), "${l.vsFpm} ft/min"),
            Fact(loc("Przeciążenie", "G-force"), l.gForce?.let { "%.2f g".format(it) } ?: "—"),
            Fact(loc("Przechylenie", "Bank"), l.bankDeg?.let { "%.1f°".format(abs(it)) } ?: "—"),
            Fact(loc("Pochylenie", "Pitch"), l.pitchDeg?.let { "%.1f°".format(it) } ?: "—"),
            Fact(loc("Za progiem", "Past threshold"), l.alongFt?.let { "$it ft" } ?: "—"),
            Fact(loc("Od osi", "Off centreline"), l.crossFt?.let { "${abs(it)} ft " + if (it >= 0) "P" else "L" } ?: "—"),
            Fact(loc("Prędkość IAS", "IAS"), l.iasKt?.let { "$it kt" } ?: "—"),
            Fact(loc("Odbicia", "Bounces"), l.bounces.toString(), accent = if (l.bounces > 0) c.caution else null),
        ))
        Spacer(Modifier.size(10.dp))
        l.noteTexts.forEach { n -> Text("• $n", style = MaterialTheme.typography.bodyMedium, color = c.ink, modifier = Modifier.padding(vertical = 2.dp)) }
    }
}
