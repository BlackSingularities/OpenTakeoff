package com.freemetar.app.ui.screens

import com.freemetar.app.domain.flightCount
import com.freemetar.app.domain.airportCount
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
        Text(loc("common.settings"), style = MaterialTheme.typography.headlineSmall, modifier = Modifier.padding(top = 4.dp))

        SectionCard(loc("settings.general"), icon = Icons.Outlined.Straighten, dense = true) {
            ChipRow(loc("settings.pressure")) {
                FilterChip(s.pressureUnit == PressureUnit.HPA, { vm.setPressure(PressureUnit.HPA) }, { Text("hPa") })
                FilterChip(s.pressureUnit == PressureUnit.INHG, { vm.setPressure(PressureUnit.INHG) }, { Text("inHg") })
            }
            ChipRow(loc("common.altitude")) {
                FilterChip(s.altUnit == AltUnit.FT, { vm.setAlt(AltUnit.FT) }, { Text(loc("settings.feet")) })
                FilterChip(s.altUnit == AltUnit.M, { vm.setAlt(AltUnit.M) }, { Text(loc("settings.metres")) })
            }
            ChipRow(loc("settings.language")) {
                listOf("SYSTEM" to loc("settings.system"), "PL" to "Polski", "EN" to "English", "ES" to "Español").forEach { (code, label) ->
                    FilterChip(s.language == code, { vm.setLanguage(code) }, { Text(label) })
                }
            }
        }

        SectionCard(loc("settings.display"), icon = Icons.Outlined.Tune, dense = true) {
            ChipRow(loc("settings.theme")) {
                listOf(
                    com.freemetar.app.data.ThemeMode.SYSTEM to "AMOLED",
                    com.freemetar.app.data.ThemeMode.NIGHT to loc("settings.graphite"),
                    com.freemetar.app.data.ThemeMode.DAY to loc("settings.bright"),
                ).forEach { (mode, label) ->
                    FilterChip(s.themeMode == mode, { vm.setTheme(mode) }, { Text(label) })
                }
            }
            ToggleRow(Icons.Outlined.Fullscreen, loc("settings.hide_the_status_bar"),
                loc("settings.swipe_down_from_the_top_to"),
                s.hideStatusBar) { vm.setHideStatusBar(it) }
            ToggleRow(Icons.Outlined.Autorenew, loc("settings.auto_refresh"),
                loc("settings.every_5_minutes_while_the_app"), s.autoRefresh) { vm.setAutoRefresh(it) }
            ToggleRow(Icons.Outlined.ViewDay, loc("settings.always_show_section_bar"),
                loc("settings.in_the_airport_briefing_when_off"),
                s.pinJumpBar) { vm.setPinJumpBar(it) }
        }

        SectionCard(loc("settings.flying_the_aircraft"), icon = Icons.Outlined.ScreenRotation, dense = true) {
            ChipRow(loc("settings.mode")) {
                listOf("TILT" to loc("common.gyro"), "TOUCH" to loc("common.stick"), "GAMEPAD" to loc("common.controller")).forEach { (code, label) ->
                    FilterChip(s.controlMode == code, { vm.setControlMode(code) }, { Text(label) })
                }
            }
            ChipRow(loc("settings.power_levers")) {
                FilterChip(s.leversLeft, { vm.setLeversLeft(true) }, { Text(loc("settings.left")) })
                FilterChip(!s.leversLeft, { vm.setLeversLeft(false) }, { Text(loc("settings.right")) })
            }
            ToggleRow(Icons.Outlined.TouchApp, loc("common.floating_control_button"),
                loc("settings.on_every_tab_hold_it_to"),
                s.floatingControl) { vm.setFloatingControl(it) }
            if (s.floatingControl) ChipRow(loc("settings.style")) {
                FilterChip(s.floatingStyle == "SIMPLE", { vm.setFloatingStyle("SIMPLE") }, { Text(loc("settings.simple")) })
                FilterChip(s.floatingStyle == "TRIM", { vm.setFloatingStyle("TRIM") }, { Text(loc("settings.with_trim")) })
            }
            if (s.floatingControl && s.floatingStyle == "TRIM") Text(
                loc("settings.gyro_drag_down_nose_up_drag"),
                color = Fm.c.ink2, fontSize = 12.sp,
            )
        }

        SectionCard(loc("common.voice_callouts"), icon = Icons.Outlined.RecordVoiceOver, dense = true) {
            Row(Modifier.fillMaxWidth().clip(androidx.compose.foundation.shape.RoundedCornerShape(8.dp)).clickable(onClick = onOpenCallouts).padding(vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(if (s.calloutsEnabled) loc("settings.on") else loc("settings.off"), color = Fm.c.ink)
                    Text(loc("settings.landing_altitudes_positive_rate_autopilot_read"), color = Fm.c.ink2, fontSize = 12.sp)
                }
                Icon(Icons.Outlined.ChevronRight, null, tint = Fm.c.ink2)
            }
        }

        SectionCard(loc("settings.flight_recording"), icon = Icons.Outlined.History, dense = true) {
            ChipRow(loc("settings.rate")) {
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
                Text("${flightCount(logs.size)} · $kb KB", color = Fm.c.ink2, fontSize = 13.sp, modifier = Modifier.weight(1f))
                OutlinedButton({ exporter.launch("OpenTakeoff-loty.oab") }, enabled = logs.isNotEmpty()) {
                    Icon(Icons.Outlined.Upload, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text(loc("settings.export"))
                }
                OutlinedButton({ importer.launch(arrayOf("*/*")) }) {
                    Icon(Icons.Outlined.Download, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text(loc("settings.import"))
                }
            }
        }

        SectionCard(loc("settings.airport_database"), icon = Icons.Outlined.Storage, dense = true) {
            val txt = when (val d = db) {
                is DbState.Ready -> "${airportCount(d.count)} · " +
                    (d.updated?.let { DateFormat.getDateInstance().format(Date(it)) } ?: "—") + loc("settings.refreshes_every_30_days")
                is DbState.Loading -> d.message
                is DbState.Error -> loc("settings.error", d.message)
                DbState.Idle -> "—"
            }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(txt, color = Fm.c.ink2, fontSize = 13.sp, modifier = Modifier.weight(1f))
                OutlinedButton({ vm.loadDb(force = true) }, enabled = db !is DbState.Loading) {
                    Icon(Icons.Outlined.Download, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text(loc("common.download"))
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
