package com.freemetar.app.ui.screens

import androidx.compose.foundation.layout.wrapContentWidth

import com.freemetar.app.domain.loc

import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.AUTHOR_SITE
import com.freemetar.app.ui.theme.AUTHOR_URL
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.ui.draw.clip
import androidx.compose.material.icons.outlined.RecordVoiceOver
import androidx.compose.material.icons.outlined.ChevronRight
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Autorenew
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.Storage
import androidx.compose.material.icons.outlined.Straighten
import androidx.compose.material.icons.outlined.Tune
import androidx.compose.material.icons.outlined.ViewDay
import androidx.compose.material.icons.outlined.History
import androidx.compose.material.icons.outlined.Fullscreen
import androidx.compose.material.icons.outlined.ScreenRotation
import androidx.compose.material.icons.outlined.TouchApp
import androidx.compose.material.icons.outlined.Upload
import androidx.compose.runtime.remember
import androidx.compose.material3.Icon
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.freemetar.app.BuildConfig
import com.freemetar.app.DbState
import com.freemetar.app.MainViewModel
import com.freemetar.app.data.AltUnit
import com.freemetar.app.data.PressureUnit
import com.freemetar.app.ui.components.SectionCard
import java.text.DateFormat
import java.util.Date

@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
fun SettingsScreen(vm: MainViewModel, onOpenCallouts: () -> Unit = {}) {
    val s by vm.settings.collectAsStateWithLifecycle()
    val db by vm.dbState.collectAsStateWithLifecycle()
    Column(
        Modifier.fillMaxSize().wrapContentWidth(Alignment.CenterHorizontally).widthIn(max = 760.dp)
            .statusBarsPadding().verticalScroll(rememberScrollState()).padding(horizontal = 12.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(loc("Ustawienia", "Settings"), style = MaterialTheme.typography.headlineSmall, modifier = Modifier.padding(top = 4.dp))

        SectionCard(loc("Ogólne", "General"), icon = Icons.Outlined.Straighten, dense = true) {
            ChipRow(loc("Ciśnienie", "Pressure")) {
                FilterChip(s.pressureUnit == PressureUnit.HPA, { vm.setPressure(PressureUnit.HPA) }, { Text("hPa") })
                FilterChip(s.pressureUnit == PressureUnit.INHG, { vm.setPressure(PressureUnit.INHG) }, { Text("inHg") })
            }
            ChipRow(loc("Wysokość", "Altitude")) {
                FilterChip(s.altUnit == AltUnit.FT, { vm.setAlt(AltUnit.FT) }, { Text(loc("stopy", "feet")) })
                FilterChip(s.altUnit == AltUnit.M, { vm.setAlt(AltUnit.M) }, { Text(loc("metry", "metres")) })
            }
            ChipRow(loc("Język", "Language")) {
                listOf("SYSTEM" to loc("System", "System"), "PL" to "Polski", "EN" to "English").forEach { (code, label) ->
                    FilterChip(s.language == code, { vm.setLanguage(code) }, { Text(label) })
                }
            }
        }

        SectionCard(loc("Wyświetlanie", "Display"), icon = Icons.Outlined.Tune, dense = true) {
            ChipRow(loc("Motyw", "Theme")) {
                listOf(
                    com.freemetar.app.data.ThemeMode.SYSTEM to "AMOLED",
                    com.freemetar.app.data.ThemeMode.NIGHT to loc("Grafit", "Graphite"),
                    com.freemetar.app.data.ThemeMode.DAY to loc("Jasny", "Bright"),
                ).forEach { (mode, label) ->
                    FilterChip(s.themeMode == mode, { vm.setTheme(mode) }, { Text(label) })
                }
            }
            ToggleRow(Icons.Outlined.Fullscreen, loc("Ukryj pasek stanu", "Hide the status bar"),
                loc("Pasek wysuniesz przeciągnięciem od góry", "Swipe down from the top to show it"),
                s.hideStatusBar) { vm.setHideStatusBar(it) }
            ToggleRow(Icons.Outlined.Autorenew, loc("Automatyczne odświeżanie", "Auto refresh"),
                loc("Co 5 minut, gdy aplikacja jest otwarta", "Every 5 minutes while the app is open"), s.autoRefresh) { vm.setAutoRefresh(it) }
            ToggleRow(Icons.Outlined.ViewDay, loc("Stały pasek rozdziałów", "Always show section bar"),
                loc("W briefingu lotniska; wyłączony pokazuje się tylko podczas przewijania", "In the airport briefing; when off it appears only while scrolling"),
                s.pinJumpBar) { vm.setPinJumpBar(it) }
        }

        SectionCard(loc("Sterowanie samolotem", "Flying the aircraft"), icon = Icons.Outlined.ScreenRotation, dense = true) {
            ChipRow(loc("Tryb", "Mode")) {
                listOf("TILT" to loc("Żyroskop", "Gyro"), "TOUCH" to loc("Drążek", "Stick"), "GAMEPAD" to loc("Kontroler", "Controller")).forEach { (code, label) ->
                    FilterChip(s.controlMode == code, { vm.setControlMode(code) }, { Text(label) })
                }
            }
            ChipRow(loc("Dźwignie mocy", "Power levers")) {
                FilterChip(s.leversLeft, { vm.setLeversLeft(true) }, { Text(loc("Z lewej", "Left")) })
                FilterChip(!s.leversLeft, { vm.setLeversLeft(false) }, { Text(loc("Z prawej", "Right")) })
            }
            ToggleRow(Icons.Outlined.TouchApp, loc("Pływający przycisk sterowania", "Floating control button"),
                loc("Na każdej zakładce; przytrzymaj, żeby sterować", "On every tab; hold it to fly"),
                s.floatingControl) { vm.setFloatingControl(it) }
            if (s.floatingControl) ChipRow(loc("Wariant", "Style")) {
                FilterChip(s.floatingStyle == "SIMPLE", { vm.setFloatingStyle("SIMPLE") }, { Text(loc("Prosty", "Simple")) })
                FilterChip(s.floatingStyle == "TRIM", { vm.setFloatingStyle("TRIM") }, { Text(loc("Z trymem", "With trim")) })
            }
            if (s.floatingControl && s.floatingStyle == "TRIM") Text(
                loc("Żyroskop: palec w dół = nos w górę, w górę = nos w dół. Drążek: osobne przyciski trymu.",
                    "Gyro: drag down = nose up, drag up = nose down. Stick: separate trim buttons."),
                color = Fm.c.ink2, fontSize = 12.sp,
            )
        }

        SectionCard(loc("Zapowiedzi głosowe", "Voice callouts"), icon = Icons.Outlined.RecordVoiceOver, dense = true) {
            Row(Modifier.fillMaxWidth().clip(androidx.compose.foundation.shape.RoundedCornerShape(8.dp)).clickable(onClick = onOpenCallouts).padding(vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(if (s.calloutsEnabled) loc("Włączone", "On") else loc("Wyłączone", "Off"), color = Fm.c.ink)
                    Text(loc("Wysokości przy lądowaniu, V1 i Rotate, autopilot — czytane przez syntezator mowy",
                        "Landing altitudes, V1 and Rotate, autopilot — read by text-to-speech"), color = Fm.c.ink2, fontSize = 12.sp)
                }
                Icon(Icons.Outlined.ChevronRight, null, tint = Fm.c.ink2)
            }
        }

        SectionCard(loc("Zapis lotów", "Flight recording"), icon = Icons.Outlined.History, dense = true) {
            ChipRow(loc("Próbki", "Rate")) {
                listOf(250 to "4/s", 500 to "2/s", 1000 to "1/s", 2000 to "2 s", 5000 to "5 s").forEach { (ms, label) ->
                    FilterChip(s.recordIntervalMs == ms, { vm.setRecordInterval(ms) }, { Text(label) })
                }
            }
            val logs by vm.flightLogs.collectAsStateWithLifecycle()
            val ctx = androidx.compose.ui.platform.LocalContext.current
            val kb = remember(logs) { vm.flightArchiveBytes() / 1024 }
            val exporter = androidx.activity.compose.rememberLauncherForActivityResult(androidx.activity.result.contract.ActivityResultContracts.CreateDocument("application/gzip")) { uri ->
                uri?.let { vm.exportFlightArchive(ctx, it) }
            }
            val importer = androidx.activity.compose.rememberLauncherForActivityResult(androidx.activity.result.contract.ActivityResultContracts.OpenDocument()) { uri ->
                uri?.let { vm.importFlightArchive(ctx, it) }
            }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(loc("${logs.size} lotów · $kb KB", "${logs.size} flights · $kb KB"), color = Fm.c.ink2, fontSize = 13.sp, modifier = Modifier.weight(1f))
                OutlinedButton({ exporter.launch("OpenTakeoff-loty.oab") }, enabled = logs.isNotEmpty()) {
                    Icon(Icons.Outlined.Upload, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text(loc("Eksport", "Export"))
                }
                OutlinedButton({ importer.launch(arrayOf("*/*")) }) {
                    Icon(Icons.Outlined.Download, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text(loc("Import", "Import"))
                }
            }
        }

        SectionCard(loc("Baza lotnisk", "Airport database"), icon = Icons.Outlined.Storage, dense = true) {
            val txt = when (val d = db) {
                is DbState.Ready -> loc("${d.count} lotnisk · ", "${d.count} airports · ") +
                    (d.updated?.let { DateFormat.getDateInstance().format(Date(it)) } ?: "—") + loc(" · sama odświeża się co 30 dni", " · refreshes every 30 days")
                is DbState.Loading -> d.message
                is DbState.Error -> loc("Błąd: ${d.message}", "Error: ${d.message}")
                DbState.Idle -> "—"
            }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(txt, color = Fm.c.ink2, fontSize = 13.sp, modifier = Modifier.weight(1f))
                OutlinedButton({ vm.loadDb(force = true) }, enabled = db !is DbState.Loading) {
                    Icon(Icons.Outlined.Download, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text(loc("Pobierz", "Download"))
                }
            }
        }

        com.freemetar.app.ui.components.AboutSection(vm)
    }
}

/** Etykieta po lewej, opcje obok — jedna linia zamiast dwóch. */
@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
private fun ChipRow(label: String, chips: @Composable () -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = Fm.c.ink, fontSize = 14.sp, modifier = Modifier.width(84.dp))
        androidx.compose.foundation.layout.FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) { chips() }
    }
}

@Composable
private fun ToggleRow(icon: ImageVector, title: String, sub: String, value: Boolean, onChange: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth().padding(vertical = 2.dp), verticalAlignment = Alignment.CenterVertically) {
        Icon(icon, null, tint = Fm.c.ink2, modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(title, color = Fm.c.ink)
            Text(sub, color = Fm.c.ink2, fontSize = 12.sp)
        }
        Switch(value, onChange)
    }
}
