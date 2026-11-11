package com.freemetar.app.ui.flight

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.material3.Text
import com.freemetar.app.data.link.FlightState
import com.freemetar.app.ui.theme.Mono
import com.freemetar.app.ui.theme.tr
import kotlin.math.roundToInt

// Kolory nakładki są stałe (jak grafika transmisji), niezależne od motywu
private val STRIP_BG = Color(0xE60B0F14)
private val STRIP_LABEL = Color(0xFF8FA3B5)
private val STRIP_VALUE = Color(0xFFF2F5F8)
private val STRIP_ACCENT = Color(0xFF4FD1E8)
private val STRIP_LIVE = Color(0xFFFF4D4D)

/**
 * Pasek aktywnego lotu nad innymi zakładkami — jak nakładka z danymi lotu na transmisjach z symulatorów:
 * samolot i następny punkt planu z symulatora, potem IAS, wysokość / FL, V/S, kurs, GS, odległość do punktu i czas do celu.
 * Dotknięcie przenosi do zakładki Lot.
 */
@Composable
fun LiveFlightStrip(s: FlightState, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val desc = tr("live_strip.live_flight_tap_to_open_the")
    val alt = (s.position.altIndFt ?: s.position.altMslFt)?.roundToInt()
    val vs = s.speed.vsFpm?.roundToInt()
    val gs = s.speed.gsKt
    // Plan lotu z GPS symulatora: następny punkt i czas do celu
    val g = s.avionics?.gps?.takeIf { it.activeFlightPlan }
    val next = g?.nextId?.takeIf { it.isNotBlank() }
    val ete = g?.destEteSec?.takeIf { it > 0 }?.let { it / 60 }
    val items = buildList {
        add("IAS" to (s.speed.iasKt?.roundToInt()?.toString() ?: "—"))
        add((if ((alt ?: 0) >= 18_000) "FL" else "ALT") to (alt?.let { if (it >= 18_000) "%03d".format((it + 50) / 100) else "$it" } ?: "—"))
        add("V/S" to (vs?.let { (if (it > 0) "+" else "") + (it / 10 * 10) } ?: "—"))
        add("HDG" to (s.attitude.hdgMagDeg?.let { "%03d".format(((it.roundToInt() % 360) + 360) % 360) } ?: "—"))
        add("GS" to (gs?.roundToInt()?.toString() ?: "—"))
        if (next != null) g?.nextDistNm?.let { add(next to "%.1f NM".format(it)) }
        ete?.let { add("ETE" to "%d:%02d".format(it.toInt() / 60, (it % 60).toInt())) }
    }
    Row(
        modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp).clip(RoundedCornerShape(10.dp)).background(STRIP_BG)
            .semantics { contentDescription = desc }
            .clickable(role = Role.Button, onClick = onClick)
            .height(40.dp).padding(horizontal = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // „LIVE” z czerwoną kropką, potem samolot i trasa
        Box(Modifier.size(7.dp).clip(CircleShape).background(if (s.aircraft.paused == true) Color(0xFFFFB020) else STRIP_LIVE))
        Spacer(Modifier.width(6.dp))
        Column(Modifier.weight(1f, fill = false).padding(end = 10.dp)) {
            Text(
                listOfNotNull(next?.let { "→ $it" }, if (s.aircraft.paused == true) tr("live_strip.paused") else null).joinToString("  ")
                    .ifEmpty { tr("live_strip.live_flight") },
                fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 11.sp, color = STRIP_ACCENT, maxLines = 1,
            )
            Text(s.aircraft.atcModel?.takeIf { it.isNotBlank() } ?: s.aircraft.title ?: "", fontSize = 10.sp, color = STRIP_LABEL,
                maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
            items.forEach { (k, v) ->
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Text(k, fontFamily = Mono, fontSize = 8.sp, color = STRIP_LABEL, maxLines = 1)
                    Text(v, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 13.sp, color = STRIP_VALUE, maxLines = 1)
                }
            }
        }
    }
}
