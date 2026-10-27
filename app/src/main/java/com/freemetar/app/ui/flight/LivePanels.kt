package com.freemetar.app.ui.flight

import com.freemetar.app.ui.flight.avionics.detectedProfile
import com.freemetar.app.domain.loc

import androidx.compose.foundation.background
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
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Computer
import androidx.compose.material.icons.outlined.LinkOff
import androidx.compose.material.icons.outlined.LocalGasStation
import androidx.compose.material.icons.outlined.Speed
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.freemetar.app.MainViewModel
import com.freemetar.app.ui.flight.avionics.gpsLeg
import com.freemetar.app.data.link.FlightState
import com.freemetar.app.data.link.LinkClient
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import com.freemetar.app.data.link.LinkStatus
import com.freemetar.app.ui.components.Fact
import com.freemetar.app.ui.components.FactGrid
import com.freemetar.app.ui.components.Section
import com.freemetar.app.ui.components.isWide
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.Mono
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import kotlin.math.roundToInt

/** Połączenie z OpenTakeoff Link: znalezione komputery, parowanie kodem, ręczne IP. */
@Composable
fun ConnectionSection(vm: MainViewModel) {
    val c = Fm.c
    val status by vm.linkStatus.collectAsStateWithLifecycle()
    val servers by vm.linkServers.collectAsStateWithLifecycle()
    val err by vm.link.lastError.collectAsStateWithLifecycle()
    // Nasłuch w sieci tylko, gdy ten panel jest na ekranie
    DisposableEffect(Unit) { vm.startLinkDiscovery(); onDispose { vm.stopLinkDiscovery() } }

    Section(loc("Symulator", "Simulator"), Icons.Outlined.Computer, trailing = {
        if (status is LinkStatus.Connected || status is LinkStatus.Lost) IconButton({ vm.linkDisconnect() }) { Icon(Icons.Outlined.LinkOff, loc("Rozłącz", "Disconnect"), tint = c.ink2) }
    }) {
        val (dot, text) = when (val s = status) {
            LinkStatus.Idle -> c.ink2 to loc("Nie połączono", "Not connected")
            is LinkStatus.Connecting -> c.caution to loc("Łączenie z ${s.ip}…", "Connecting to ${s.ip}…")
            is LinkStatus.NeedsCode -> c.caution to loc("Wpisz kod parowania z programu", "Enter the pairing code from the PC app")
            is LinkStatus.Connected -> (if (s.source == "none") c.caution else c.green) to when (s.source) {
                "msfs" -> loc("Połączono z MSFS", "Connected to MSFS")
                "simulation" -> loc("Połączono — tryb symulacji", "Connected — simulation mode")
                else -> s.message ?: loc("Połączono, czekam na MSFS", "Connected, waiting for MSFS")
            }
            is LinkStatus.Lost -> c.danger to loc("Utracono połączenie, ponawiam… (${s.reason})", "Connection lost, retrying… (${s.reason})")
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(10.dp).clip(CircleShape).background(dot))
            Spacer(Modifier.width(10.dp))
            Text(text, style = MaterialTheme.typography.titleMedium, color = c.ink, modifier = Modifier.weight(1f))
            if (status is LinkStatus.Connecting) CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp, color = c.accent)
        }
        err?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = c.caution, modifier = Modifier.padding(top = 4.dp)) }

        if (status is LinkStatus.Idle || status is LinkStatus.NeedsCode) {
            Spacer(Modifier.size(10.dp))
            if (servers.isEmpty()) {
                Text(
                    loc("Szukam OpenTakeoff Link w sieci… Uruchom program na komputerze z MSFS 2024; telefon musi być w tej samej sieci Wi-Fi.", "Looking for OpenTakeoff Link on the network… Run it on the PC with MSFS 2024; the phone must be on the same Wi-Fi network."),
                    style = MaterialTheme.typography.bodyMedium, color = c.ink2,
                )
            }
            servers.forEach { sv ->
                Row(Modifier.fillMaxWidth().padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(sv.host, style = MaterialTheme.typography.titleSmall, color = c.ink)
                        Text("${sv.ip} · ${when (sv.source) { "msfs" -> loc("MSFS połączony", "MSFS connected"); "simulation" -> loc("symulacja", "simulation"); else -> loc("czeka na MSFS", "waiting for MSFS") }}",
                            style = MaterialTheme.typography.bodySmall, color = c.ink2)
                    }
                    Button({ vm.linkConnect(sv.ip, sv.port) }) { Text(loc("Połącz", "Connect")) }
                }
            }
            var manual by rememberSaveable { mutableStateOf("") }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 6.dp)) {
                OutlinedTextField(manual, { manual = it.trim() }, Modifier.weight(1f), label = { Text(loc("Adres IP komputera", "PC IP address")) }, singleLine = true,
                    shape = RoundedCornerShape(8.dp), keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri))
                OutlinedButton({ vm.linkConnect(manual) }, enabled = manual.count { it == '.' } == 3) { Text(loc("Połącz", "Connect")) }
            }
        }
    }

    (status as? LinkStatus.NeedsCode)?.let { s ->
        var code by remember(s) { mutableStateOf("") }
        AlertDialog(
            onDismissRequest = { vm.linkDisconnect() },
            title = { Text(loc("Parowanie z ${s.ip}", "Pairing with ${s.ip}")) },
            text = {
                Column {
                    Text(loc("Wpisz 6-cyfrowy kod widoczny w oknie OpenTakeoff Link. Telefon zapamięta parowanie.", "Enter the 6-digit code shown in the OpenTakeoff Link window. The phone will remember the pairing."), style = MaterialTheme.typography.bodyMedium)
                    s.error?.let { Text(it, color = c.danger, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(top = 6.dp)) }
                    OutlinedTextField(code, { code = it.filter(Char::isDigit).take(6) }, Modifier.padding(top = 10.dp), singleLine = true,
                        textStyle = MaterialTheme.typography.headlineSmall.copy(fontFamily = Mono), keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword))
                }
            },
            confirmButton = { TextButton({ vm.linkConnect(s.ip, s.port, code) }, enabled = code.length == 6) { Text(loc("Sparuj", "Pair")) } },
            dismissButton = { TextButton({ vm.linkDisconnect() }) { Text(loc("Anuluj", "Cancel")) } },
        )
    }
}

private enum class LiveView(private val labelOf: () -> String, val instrument: Boolean) {
    PFD({ "PFD" }, true), ND({ loc("Mapa", "Map") }, true), AP({ "Autopilot" }, false), RADIO({ "Radio" }, false),
    CONTROLS({ loc("Systemy", "Systems") }, false), YOKE({ loc("Wolant", "Yoke") }, false), ENGINES({ loc("Silniki", "Engines") }, false),
    ;

    val label: String get() = labelOf()
}

/** Widoki na żywo: PFD, mapa nawigacyjna, silniki i paliwo, postęp lotu. Na tablecie PFD i mapa obok siebie. */
@Composable
fun LiveSection(s: FlightState, link: LinkClient, profile: com.freemetar.app.domain.AircraftProfile? = null, onProfile: (com.freemetar.app.domain.AircraftProfile?) -> Unit = {}) {
    val c = Fm.c
    var view by rememberSaveable { mutableStateOf(LiveView.PFD) }
    var panel by rememberSaveable { mutableStateOf(LiveView.AP) }
    var range by rememberSaveable { mutableStateOf(40) }
    val wide = isWide()
    Section(s.aircraft.title ?: loc("Lot na żywo", "Live flight"), Icons.Outlined.Speed) {
        if (s.aircraft.paused == true) Text(loc("Symulator wstrzymany", "Simulator paused"), style = MaterialTheme.typography.labelMedium, color = c.caution, modifier = Modifier.padding(bottom = 6.dp))
        if (wide) {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Box(Modifier.weight(1.1f).clip(RoundedCornerShape(8.dp))) { Pfd(s, profile = profile) }
                Box(Modifier.weight(1f).clip(RoundedCornerShape(8.dp))) { NavDisplay(s, s.gpsLeg(), range, profile = profile) }
            }
            RangeRow(range) { range = it }
            Spacer(Modifier.size(12.dp))
            Progress(s)
            Spacer(Modifier.size(12.dp))
            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                LiveView.entries.filterNot { it.instrument }.forEach { v -> FilterChip(panel == v, { panel = v }, { Text(v.label) }) }
            }
            Spacer(Modifier.size(10.dp))
            PanelFor(panel, s, link, profile, onProfile)
        } else {
            Row(Modifier.horizontalScroll(rememberScrollState()).padding(bottom = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                LiveView.entries.forEach { v -> FilterChip(view == v, { view = v }, { Text(v.label) }) }
            }
            when (view) {
                LiveView.PFD -> Box(Modifier.clip(RoundedCornerShape(8.dp))) { Pfd(s, profile = profile) }
                LiveView.ND -> {
                    Box(Modifier.clip(RoundedCornerShape(8.dp))) { NavDisplay(s, s.gpsLeg(), range, profile = profile) }
                    RangeRow(range) { range = it }
                }
                else -> PanelFor(view, s, link, profile, onProfile)
            }
            // Postęp lotu pod przyrządami; przy panelach sterowania nie zabiera miejsca
            if (view.instrument) { Spacer(Modifier.size(12.dp)); Progress(s) }
        }
    }
}

@Composable
private fun PanelFor(v: LiveView, s: FlightState, link: LinkClient, profile: com.freemetar.app.domain.AircraftProfile?, onProfile: (com.freemetar.app.domain.AircraftProfile?) -> Unit) {
    val effective = profile ?: s.detectedProfile()
    when (v) {
        LiveView.AP -> AutopilotFor(link, s, effective)
        LiveView.RADIO -> RadioFor(link, s, effective)
        LiveView.CONTROLS -> SystemsFor(link, s, effective)
        LiveView.YOKE -> YokePanel(link, s)
        LiveView.ENGINES -> EnginesFor(link, s, effective)
        else -> {}
    }
}

@Composable
private fun RangeRow(range: Int, onRange: (Int) -> Unit) {
    Row(horizontalArrangement = Arrangement.spacedBy(6.dp), modifier = Modifier.padding(top = 8.dp)) {
        listOf(10, 20, 40, 80, 160).forEach { r -> FilterChip(range == r, { onRange(r) }, { Text("$r NM") }) }
    }
}

@Composable
internal fun Systems(s: FlightState) {
    val c = Fm.c
    s.engines.forEachIndexed { i, e ->
        val turbine = (e.n1Pct ?: 0.0) > 0.1
        Row(Modifier.fillMaxWidth().padding(vertical = 5.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(loc("SIL ${i + 1}", "ENG ${i + 1}"), fontFamily = Mono, fontSize = 12.sp, color = c.ink2, modifier = Modifier.width(52.dp))
            val pct = if (turbine) (e.n1Pct ?: 0.0) / 100 else ((e.rpm ?: 0.0) / 2700).coerceIn(0.0, 1.0)
            Box(Modifier.weight(1f).height(10.dp).clip(RoundedCornerShape(5.dp)).background(c.terrain)) {
                Box(Modifier.fillMaxWidth(pct.toFloat().coerceIn(0f, 1f)).height(10.dp).clip(RoundedCornerShape(5.dp)).background(c.green))
            }
            Spacer(Modifier.width(10.dp))
            Text(if (turbine) "N1 ${"%.1f".format(e.n1Pct)}" else "${(e.rpm ?: 0.0).roundToInt()} RPM", fontFamily = Mono, fontSize = 13.sp, color = c.ink, modifier = Modifier.width(96.dp))
        }
        Text(
            listOfNotNull(e.egtC?.let { "EGT ${it.roundToInt()} °C" }, e.fuelFlowKgH?.let { "FF ${it.roundToInt()} kg/h" }, e.n2Pct?.takeIf { turbine }?.let { "N2 ${"%.1f".format(it)}" }).joinToString("   "),
            fontFamily = Mono, fontSize = 12.sp, color = c.ink2, modifier = Modifier.padding(start = 52.dp),
        )
    }
    if (s.fuel.tanks.isNotEmpty()) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(top = 8.dp)) {
            Icon(Icons.Outlined.LocalGasStation, null, tint = c.ink2, modifier = Modifier.size(16.dp))
            Spacer(Modifier.width(6.dp))
            Text(
                s.fuel.tanks.entries.joinToString("   ") { (k, v) ->
                    val name = when (k) { "leftKg" -> "L"; "centerKg" -> "C"; "rightKg" -> "P"; else -> k.removeSuffix("Kg") }
                    "$name ${v.roundToInt()}"
                } + " kg",
                fontFamily = Mono, fontSize = 13.sp, color = c.ink,
            )
        }
    }
    val ctl = s.controls
    Text(
        listOfNotNull(
            ctl.gearDown?.let { if (it) loc("Podwozie wypuszczone", "Gear down") else loc("Podwozie schowane", "Gear up") },
            ctl.flapsIndex?.let { loc("klapy $it/${(ctl.flapsPositions ?: 1) - 1}", "flaps $it/${(ctl.flapsPositions ?: 1) - 1}") },
            ctl.parkingBrake?.takeIf { it }?.let { loc("hamulec postojowy", "parking brake") },
            s.radios.transponder?.code?.let { "squawk $it" },
        ).joinToString(", "),
        style = MaterialTheme.typography.bodySmall, color = c.ink2, modifier = Modifier.padding(top = 8.dp),
    )
}
