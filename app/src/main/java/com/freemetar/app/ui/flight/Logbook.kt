package com.freemetar.app.ui.flight

import com.freemetar.app.domain.flightCount
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.automirrored.outlined.MenuBook
import androidx.compose.material.icons.outlined.ChevronRight
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.Upload
import androidx.compose.material3.FilterChip
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.freemetar.app.MainViewModel
import com.freemetar.app.data.link.FlightLog
import com.freemetar.app.domain.Lang
import com.freemetar.app.domain.loc
import com.freemetar.app.ui.components.Fact
import com.freemetar.app.ui.components.FactGrid
import com.freemetar.app.ui.components.Rule
import com.freemetar.app.ui.components.Section
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.Mono
import java.text.DateFormat
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Date
import kotlin.math.roundToInt

private enum class LogFilter { ALL, SCORED, RECENT }

/** Wejście do dziennika na ekranie Lot: jedna karta z podsumowaniem, zamiast długiej listy. */
@Composable
fun LogbookEntry(vm: MainViewModel, onOpen: () -> Unit) {
    val c = Fm.c
    val logs by vm.flightLogs.collectAsStateWithLifecycle()
    Section(loc("logbook.logbook"), Icons.AutoMirrored.Outlined.MenuBook) {
        Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).clickable(onClick = onOpen).padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                if (logs.isEmpty()) {
                    Text(loc("logbook.no_saved_flights_yet"), style = MaterialTheme.typography.titleSmall, color = c.ink)
                    Text(loc("logbook.flights_are_recorded_automatically_once_the"),
                        style = MaterialTheme.typography.bodySmall, color = c.ink2)
                } else {
                    val last = logs.first()
                    val minutes = logs.sumOf { it.durationMin }
                    Text("${flightCount(logs.size)}, %d:%02d h".format(minutes / 60, minutes % 60),
                        style = MaterialTheme.typography.titleMedium, color = c.ink)
                    Text(loc("logbook.last") + "${last.from ?: "?"} → ${last.to ?: "?"}, " + DateFormat.getDateInstance(DateFormat.MEDIUM, Lang.locale).format(Date(last.start)),
                        style = MaterialTheme.typography.bodySmall, color = c.ink2, maxLines = 1)
                }
            }
            Icon(Icons.Outlined.ChevronRight, loc("logbook.open_logbook"), tint = c.ink2)
        }
    }
}

/**
 * Dziennik lotów: podsumowanie, filtr, loty pogrupowane po miesiącach. Każdy wiersz ma trasę,
 * czas, dystans, wysokość, ocenę lądowania i miniaturę profilu wysokości. Eksport i import archiwum w nagłówku.
 */
@Composable
fun LogbookScreen(vm: MainViewModel, onBack: () -> Unit, onOpenFlight: (String) -> Unit) {
    val c = Fm.c
    val ctx = LocalContext.current
    val logs by vm.flightLogs.collectAsStateWithLifecycle()
    var filter by rememberSaveable { mutableStateOf(LogFilter.ALL) }
    val exporter = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/gzip")) { uri -> uri?.let { vm.exportFlightArchive(ctx, it) } }
    val importer = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri -> uri?.let { vm.importFlightArchive(ctx, it) } }
    val now = System.currentTimeMillis()
    val shown = remember(logs, filter) {
        when (filter) {
            LogFilter.ALL -> logs
            LogFilter.SCORED -> logs.filter { it.allLandings.isNotEmpty() }
            LogFilter.RECENT -> logs.filter { now - it.start < 30L * 24 * 3600 * 1000 }
        }
    }
    val monthFmt = remember(Lang.current) { DateTimeFormatter.ofPattern("LLLL yyyy", Lang.locale) }
    val groups = remember(shown, monthFmt) {
        shown.groupBy { Instant.ofEpochMilli(it.start).atZone(ZoneId.systemDefault()).let { z -> z.year * 100 + z.monthValue } }
            .toSortedMap(compareByDescending { it })
            .map { (_, list) -> monthFmt.format(Instant.ofEpochMilli(list.first().start).atZone(ZoneId.systemDefault())).replaceFirstChar { it.uppercase() } to list }
    }

    Column(Modifier.fillMaxSize().background(c.paper)) {
        Row(Modifier.fillMaxWidth().statusBarsPadding().padding(horizontal = 4.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onBack) { Icon(Icons.AutoMirrored.Outlined.ArrowBack, loc("common.back"), tint = c.ink) }
            Text(loc("logbook.logbook"), style = MaterialTheme.typography.headlineSmall, color = c.ink, modifier = Modifier.weight(1f))
            IconButton({ importer.launch(arrayOf("*/*")) }) { Icon(Icons.Outlined.Download, loc("logbook.import_flights"), tint = c.ink) }
            IconButton({ exporter.launch("OpenTakeoff-loty.oab") }, enabled = logs.isNotEmpty()) { Icon(Icons.Outlined.Upload, loc("logbook.export_flights"), tint = if (logs.isEmpty()) c.ink2 else c.ink) }
        }
        Rule()
        LazyColumn(Modifier.fillMaxSize().widthIn(max = 900.dp).align(Alignment.CenterHorizontally), contentPadding = androidx.compose.foundation.layout.PaddingValues(bottom = 32.dp)) {
            if (logs.isEmpty()) {
                item {
                    Column(Modifier.fillMaxWidth().padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                        Icon(Icons.AutoMirrored.Outlined.MenuBook, null, tint = c.ink2, modifier = Modifier.size(40.dp))
                        Spacer(Modifier.height(12.dp))
                        Text(loc("logbook.the_logbook_is_empty"), style = MaterialTheme.typography.titleMedium, color = c.ink)
                        Text(
                            loc("logbook.connect_the_phone_to_opentakeoff_link"),
                            style = MaterialTheme.typography.bodyMedium, color = c.ink2, modifier = Modifier.padding(top = 6.dp),
                        )
                    }
                }
                return@LazyColumn
            }
            item {
                LogbookStatistics(shown)
            }
            item {
                Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    FilterChip(filter == LogFilter.ALL, { filter = LogFilter.ALL }, { Text(loc("common.all")) })
                    FilterChip(filter == LogFilter.SCORED, { filter = LogFilter.SCORED }, { Text(loc("logbook.with_landing_score")) })
                    FilterChip(filter == LogFilter.RECENT, { filter = LogFilter.RECENT }, { Text(loc("logbook.last_30_days")) })
                }
            }
            if (shown.isEmpty()) item {
                Text(loc("logbook.no_flights_match_this_filter"), style = MaterialTheme.typography.bodyMedium, color = c.ink2, modifier = Modifier.padding(16.dp))
            }
            groups.forEach { (month, list) ->
                item(key = "m$month") {
                    Text(month, style = MaterialTheme.typography.titleSmall, color = c.ink2, modifier = Modifier.padding(start = 16.dp, top = 16.dp, bottom = 6.dp))
                }
                items(list, key = { it.id }) { f -> LogRow(f) { onOpenFlight(f.id) } }
            }
        }
    }
}

@Composable
private fun LogRow(f: FlightLog, onClick: () -> Unit) {
    val c = Fm.c
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp).clip(RoundedCornerShape(12.dp))
            .border(1.dp, c.rule, RoundedCornerShape(12.dp)).clickable(onClick = onClick).padding(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        ScoreBadge(f.landing, 46)
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text("${f.from ?: "?"} → ${f.to ?: "?"}", fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 17.sp, color = c.ink)
            Text(
                DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT, Lang.locale).format(Date(f.start)) + (f.aircraft?.let { ", $it" } ?: ""),
                style = MaterialTheme.typography.bodySmall, color = c.ink2, maxLines = 1,
            )
            Text(
                listOfNotNull(
                    "%d:%02d h".format(f.durationMin / 60, f.durationMin % 60),
                    "${f.distanceNm} NM",
                    if (f.maxAltFt >= 18_000) "FL%03d".format(f.maxAltFt / 100) else "${f.maxAltFt} ft",
                    f.landing?.let { "${it.vsFpm} ft/min" },
                    f.allLandings.size.takeIf { it > 1 }?.let { loc("logbook.landings", it) },
                ).joinToString("   "),
                fontFamily = Mono, fontSize = 12.sp, color = c.ink, modifier = Modifier.padding(top = 2.dp),
            )
        }
        Spacer(Modifier.width(10.dp))
        Sparkline(f, Modifier.width(72.dp).height(36.dp))
    }
}

/** Miniatura profilu wysokości lotu. */
@Composable
private fun Sparkline(f: FlightLog, modifier: Modifier) {
    val c = Fm.c
    val alts = remember(f.id) {
        val t = f.track
        if (t.size < 2) emptyList() else { val step = (t.size / 60).coerceAtLeast(1); t.filterIndexed { i, _ -> i % step == 0 }.map { it.altFt.toFloat() } }
    }
    if (alts.size < 2) { Box(modifier); return }
    Canvas(modifier) {
        val max = alts.max().coerceAtLeast(1f); val min = alts.min()
        val path = Path()
        alts.forEachIndexed { i, a ->
            val x = size.width * i / (alts.size - 1)
            val y = size.height - (a - min) / (max - min).coerceAtLeast(1f) * size.height * 0.9f
            if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
        }
        drawPath(path, c.magenta, style = Stroke(2.5f, cap = StrokeCap.Round))
        drawLine(c.rule, Offset(0f, size.height), Offset(size.width, size.height), 1f)
    }
}
