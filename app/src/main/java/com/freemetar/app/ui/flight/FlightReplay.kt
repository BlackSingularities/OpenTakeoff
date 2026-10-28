package com.freemetar.app.ui.flight

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Edit
import androidx.compose.material.icons.outlined.Flag
import androidx.compose.material.icons.outlined.Pause
import androidx.compose.material.icons.outlined.PlayArrow
import androidx.compose.material.icons.outlined.SkipNext
import androidx.compose.material.icons.outlined.SkipPrevious
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameMillis
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.freemetar.app.data.link.FlightLog
import com.freemetar.app.data.link.TrackPoint
import com.freemetar.app.domain.EventKind
import com.freemetar.app.domain.FlightAnalysis
import com.freemetar.app.domain.FlightEvent
import com.freemetar.app.domain.loc
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.Mono
import kotlin.math.abs
import kotlin.math.roundToInt

@Composable
fun eventColor(k: EventKind): Color {
    val c = Fm.c
    return when (k) {
        EventKind.TAKEOFF -> c.green
        EventKind.LANDING -> c.accent
        EventKind.TOC, EventKind.TOD -> c.ink2
        EventKind.MARKER -> c.magenta
        EventKind.EXCEEDANCE -> c.caution
    }
}

/** Seria danych do wykresu odtwarzania: wartość z punktu toru (null — brak). */
private class Series(val label: String, val unit: String, val color: Color, val value: (TrackPoint) -> Double?)

/**
 * Odtworzenie lotu: mapa z pozycją samolotu, odczyty w chwili kursora, oś czasu ze zdarzeniami,
 * wykresy zsynchronizowane z kursorem (dotknięcie lub przeciągnięcie przewija), lista zdarzeń i znaczników.
 */
@Composable
fun FlightReplay(
    f: FlightLog,
    events: List<FlightEvent>,
    onSetMarker: (Long, String) -> Unit,
    onDeleteMarker: (Long) -> Unit,
) {
    val c = Fm.c
    val tr = f.track
    if (tr.size < 2) {
        Text(loc("Za mało punktów toru do odtworzenia.", "Not enough track points to replay."), style = MaterialTheme.typography.bodyMedium, color = c.ink2)
        return
    }
    val t0 = tr.first().t; val t1 = tr.last().t
    var cursor by remember(f.id) { mutableLongStateOf(t0) }
    var playing by remember(f.id) { mutableStateOf(false) }
    var speed by remember { mutableStateOf(30) }
    var editing by remember { mutableStateOf<Pair<Long, String>?>(null) }

    LaunchedEffect(playing, speed, f.id) {
        if (!playing) return@LaunchedEffect
        if (cursor >= t1) cursor = t0
        var last = withFrameMillis { it }
        while (playing) {
            val now = withFrameMillis { it }
            cursor = (cursor + (now - last) * speed).coerceAtMost(t1)
            last = now
            if (cursor >= t1) playing = false
        }
    }
    val p = FlightAnalysis.at(tr, cursor) ?: tr.first()

    ReplayMap(tr, events, cursor, p)
    Spacer(Modifier.size(8.dp))
    // Odczyty w chwili kursora
    Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(14.dp)) {
        Readout("T+", FlightAnalysis.timeOf(f, cursor))
        Readout("ALT", FlightAnalysis.altLabel(p.altFt))
        p.aglFt?.let { Readout("AGL", "$it") }
        Readout("GS", "${p.gsKt}")
        p.iasKt?.let { Readout("IAS", "$it") }
        p.vsFpm?.let { Readout("V/S", (if (it > 0) "+" else "") + it) }
        p.bankDeg?.let { Readout(loc("PRZECH", "BANK"), "${abs(it).roundToInt()}°") }
        p.pitchDeg?.let { Readout(loc("POCH", "PITCH"), "%.1f°".format(it)) }
        p.hdg?.let { Readout("HDG", "%03d".format(it)) }
    }
    Spacer(Modifier.size(6.dp))
    Slider(
        value = (cursor - t0).toFloat(), onValueChange = { cursor = t0 + it.toLong(); playing = false },
        valueRange = 0f..(t1 - t0).toFloat().coerceAtLeast(1f),
    )
    Row(verticalAlignment = Alignment.CenterVertically) {
        IconButton({ events.lastOrNull { it.t < cursor - 1000 }?.let { cursor = it.t } ?: run { cursor = t0 } }) {
            Icon(Icons.Outlined.SkipPrevious, loc("Poprzednie zdarzenie", "Previous event"), tint = c.ink)
        }
        IconButton({ playing = !playing }, Modifier.clip(CircleShape).background(c.terrain)) {
            Icon(if (playing) Icons.Outlined.Pause else Icons.Outlined.PlayArrow, if (playing) loc("Pauza", "Pause") else loc("Odtwórz", "Play"), tint = c.accent)
        }
        IconButton({ events.firstOrNull { it.t > cursor + 1000 }?.let { cursor = it.t } }) {
            Icon(Icons.Outlined.SkipNext, loc("Następne zdarzenie", "Next event"), tint = c.ink)
        }
        Row(Modifier.weight(1f).horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            listOf(10, 30, 120).forEach { s -> FilterChip(speed == s, { speed = s }, { Text("×$s", fontFamily = Mono, fontSize = 12.sp) }) }
        }
        IconButton({ playing = false; editing = cursor to (f.markers.firstOrNull { it.t == cursor }?.label ?: "") }) {
            Icon(Icons.Outlined.Flag, loc("Dodaj znacznik w tym miejscu", "Add a marker here"), tint = c.magenta)
        }
    }

    // Wykresy zsynchronizowane z kursorem
    val series = remember(f.id) {
        buildList {
            add(Series(loc("Wysokość", "Altitude"), "ft", Color.Unspecified) { it.altFt.toDouble() })
            add(Series(loc("Prędkość", "Speed"), "kt", Color.Unspecified) { (it.iasKt ?: it.gsKt).toDouble() })
            if (tr.any { it.vsFpm != null }) add(Series(loc("Prędkość pionowa", "Vertical speed"), "ft/min", Color.Unspecified) { it.vsFpm?.toDouble() })
            if (tr.any { it.bankDeg != null }) add(Series(loc("Przechylenie", "Bank"), "°", Color.Unspecified) { it.bankDeg?.toDouble() })
            if (tr.any { it.pitchDeg != null }) add(Series(loc("Pochylenie", "Pitch"), "°", Color.Unspecified) { it.pitchDeg?.toDouble() })
        }
    }
    val colors = listOf(c.magenta, c.accent, c.green, c.caution, c.ink)
    series.forEachIndexed { i, s ->
        Spacer(Modifier.size(10.dp))
        ReplayChart(tr, s, colors[i % colors.size], events, cursor) { cursor = it; playing = false }
    }

    // Lista zdarzeń
    Spacer(Modifier.size(12.dp))
    Text(loc("Zdarzenia", "Events"), style = MaterialTheme.typography.labelLarge, color = c.ink2)
    Spacer(Modifier.size(4.dp))
    events.forEach { e ->
        val active = abs(e.t - cursor) < 1500
        Row(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(8.dp)).background(if (active) c.terrain else Color.Transparent)
                .clickable { cursor = e.t; playing = false }.padding(horizontal = 6.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.size(9.dp).clip(CircleShape).background(eventColor(e.kind)))
            Spacer(Modifier.width(10.dp))
            Text(FlightAnalysis.timeOf(f, e.t), fontFamily = Mono, fontSize = 12.sp, color = c.ink2, modifier = Modifier.width(62.dp))
            Text(e.label, style = MaterialTheme.typography.bodyMedium, color = c.ink, modifier = Modifier.weight(1f))
            if (e.kind == EventKind.MARKER) {
                IconButton({ editing = e.t to e.label }, Modifier.size(32.dp)) { Icon(Icons.Outlined.Edit, loc("Zmień opis", "Edit note"), tint = c.ink2, modifier = Modifier.size(16.dp)) }
                IconButton({ onDeleteMarker(e.t) }, Modifier.size(32.dp)) { Icon(Icons.Outlined.Delete, loc("Usuń znacznik", "Delete marker"), tint = c.ink2, modifier = Modifier.size(16.dp)) }
            }
        }
    }

    editing?.let { (t, initial) ->
        var text by remember(t) { mutableStateOf(initial) }
        AlertDialog(
            onDismissRequest = { editing = null },
            title = { Text(loc("Znacznik w T+", "Marker at T+") + FlightAnalysis.timeOf(f, t)) },
            text = { OutlinedTextField(text, { text = it }, placeholder = { Text(loc("np. za późno wypuszczone klapy", "e.g. flaps set too late")) }, singleLine = false) },
            confirmButton = { TextButton({ onSetMarker(t, text.ifBlank { loc("Znacznik", "Marker") }); editing = null }) { Text(loc("Zapisz", "Save")) } },
            dismissButton = { TextButton({ editing = null }) { Text(loc("Anuluj", "Cancel")) } },
        )
    }
}

@Composable
private fun Readout(label: String, value: String) {
    val c = Fm.c
    Column {
        Text(label, fontSize = 10.sp, color = c.ink2, fontFamily = Mono)
        Text(value, fontSize = 15.sp, fontWeight = FontWeight.Bold, color = c.ink, fontFamily = Mono, maxLines = 1)
    }
}

/** Tor w rzucie z góry: przeleciany odcinek w kolorze trasy, reszta przerywana, samolot jako strzałka w kierunku lotu. */
@Composable
private fun ReplayMap(tr: List<TrackPoint>, events: List<FlightEvent>, cursor: Long, p: TrackPoint) {
    val c = Fm.c
    val pts = remember(tr) { val step = (tr.size / 1500).coerceAtLeast(1); tr.filterIndexed { i, _ -> i % step == 0 } + tr.last() }
    val evPts = remember(events, tr) { events.mapNotNull { e -> FlightAnalysis.at(tr, e.t)?.let { e.kind to it } } }
    val evColors = EventKind.entries.associateWith { eventColor(it) }
    Canvas(Modifier.fillMaxWidth().aspectRatio(1.5f).clip(RoundedCornerShape(10.dp)).background(c.terrain)) {
        val minLat = pts.minOf { it.lat }; val maxLat = pts.maxOf { it.lat }
        val minLon = pts.minOf { it.lon }; val maxLon = pts.maxOf { it.lon }
        val k = kotlin.math.cos(Math.toRadians((minLat + maxLat) / 2))
        val spanX = ((maxLon - minLon) * k).coerceAtLeast(1e-4); val spanY = (maxLat - minLat).coerceAtLeast(1e-4)
        val pad = 24f
        val scale = minOf((size.width - 2 * pad) / spanX, (size.height - 2 * pad) / spanY).toFloat()
        val ox = (size.width - spanX.toFloat() * scale) / 2; val oy = (size.height - spanY.toFloat() * scale) / 2
        fun o(t: TrackPoint) = Offset(ox + ((t.lon - minLon) * k).toFloat() * scale, size.height - oy - ((t.lat - minLat).toFloat() * scale))
        val done = Path(); val rest = Path()
        var split = false
        pts.forEachIndexed { i, t ->
            val q = o(t)
            if (i == 0) { done.moveTo(q.x, q.y); rest.moveTo(q.x, q.y) }
            if (t.t <= cursor) done.lineTo(q.x, q.y)
            else { if (!split) { val a = o(p); rest.moveTo(a.x, a.y); split = true }; rest.lineTo(q.x, q.y) }
        }
        drawPath(rest, c.ink2.copy(alpha = 0.5f), style = Stroke(2f, pathEffect = PathEffect.dashPathEffect(floatArrayOf(10f, 8f))))
        drawPath(done, c.magenta, style = Stroke(3.5f, cap = StrokeCap.Round))
        evPts.forEach { (kind, t) -> drawCircle(evColors.getValue(kind), 6f, o(t)) }
        val a = o(p)
        rotate((p.hdg ?: 0).toFloat(), a) {
            val tri = Path().apply { moveTo(a.x, a.y - 16f); lineTo(a.x + 10f, a.y + 11f); lineTo(a.x, a.y + 6f); lineTo(a.x - 10f, a.y + 11f); close() }
            drawPath(tri, c.ink)
        }
    }
}

/** Wykres jednej wielkości w czasie lotu z kursorem i kreskami zdarzeń; dotknięcie lub przeciągnięcie ustawia kursor. */
@Composable
private fun ReplayChart(tr: List<TrackPoint>, s: Series, color: Color, events: List<FlightEvent>, cursor: Long, onSeek: (Long) -> Unit) {
    val c = Fm.c
    val measurer = rememberTextMeasurer()
    val t0 = tr.first().t; val span = (tr.last().t - t0).coerceAtLeast(1)
    val pts = remember(tr, s) {
        val step = (tr.size / 500).coerceAtLeast(1)
        tr.filterIndexed { i, _ -> i % step == 0 }.mapNotNull { p -> s.value(p)?.let { p.t to it } }
    }
    val now = FlightAnalysis.at(tr, cursor)?.let(s.value)
    val evColors = EventKind.entries.associateWith { eventColor(it) }
    Row(verticalAlignment = Alignment.Bottom) {
        Text(s.label, style = MaterialTheme.typography.labelMedium, color = c.ink2, modifier = Modifier.weight(1f))
        Text(now?.let { "${it.roundToInt()} ${s.unit}" } ?: "—", fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 13.sp, color = color)
    }
    if (pts.size < 2) return
    Canvas(
        Modifier.fillMaxWidth().height(72.dp)
            .pointerInput(tr) { detectTapGestures { o -> onSeek(t0 + (o.x / size.width * span).toLong().coerceIn(0, span)) } }
            .pointerInput(tr) { detectHorizontalDragGestures { ch, _ -> onSeek(t0 + (ch.position.x / size.width * span).toLong().coerceIn(0, span)) } },
    ) {
        val min = pts.minOf { it.second }; val max = pts.maxOf { it.second }
        val spanV = (max - min).takeIf { it > 0 } ?: 1.0
        val pad = 6f
        fun x(t: Long) = (t - t0).toFloat() / span * size.width
        fun y(v: Double) = pad + (size.height - 2 * pad) * (1 - ((v - min) / spanV)).toFloat()
        if (min < 0 && max > 0) drawLine(c.rule, Offset(0f, y(0.0)), Offset(size.width, y(0.0)), 1f)
        events.forEach { e -> drawLine(evColors.getValue(e.kind).copy(alpha = 0.55f), Offset(x(e.t), 0f), Offset(x(e.t), size.height), 1.5f) }
        val path = Path()
        pts.forEachIndexed { i, (t, v) -> if (i == 0) path.moveTo(x(t), y(v)) else path.lineTo(x(t), y(v)) }
        drawPath(path, color, style = Stroke(2.5f, cap = StrokeCap.Round))
        drawLine(c.ink, Offset(x(cursor), 0f), Offset(x(cursor), size.height), 2f)
        now?.let { drawCircle(color, 5f, Offset(x(cursor), y(it))) }
        val st = TextStyle(color = c.ink2, fontSize = 9.sp, fontFamily = Mono)
        drawText(measurer.measure(max.roundToInt().toString(), st), topLeft = Offset(2f, -2f))
        drawText(measurer.measure(min.roundToInt().toString(), st), topLeft = Offset(2f, size.height - 14f))
    }
}
