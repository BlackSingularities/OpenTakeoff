package com.freemetar.app.ui.screens

import androidx.compose.foundation.layout.wrapContentWidth

import com.freemetar.app.domain.loc

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Checklist
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.freemetar.app.MainViewModel
import com.freemetar.app.ui.flight.ConnectionSection
import com.freemetar.app.ui.flight.CockpitView
import com.freemetar.app.domain.AircraftProfile
import com.freemetar.app.ui.components.Section
import com.freemetar.app.ui.theme.Fm

/** Sekcja Lot: kokpit przy połączonym symulatorze, a pod przełącznikiem połączenie, checklisty i dziennik lotów. */
@Composable
fun FlightScreen(vm: MainViewModel, onOpenLogbook: () -> Unit = {}, onOpenChecklists: () -> Unit = {}) {
    val c = Fm.c
    val settings by vm.settings.collectAsStateWithLifecycle()
    val message by vm.flightMessage.collectAsStateWithLifecycle()
    val snack = remember { SnackbarHostState() }

    LaunchedEffect(message) {
        message?.let { snack.showSnackbar(it); vm.consumeFlightMessage() }
    }

    val live by vm.flightState.collectAsStateWithLifecycle()
    // Po połączeniu z symulatorem domyślnie kokpit (PFD + panele) na cały ekran; połączenie i dziennik pod przełącznikiem.
    var cockpit by rememberSaveable { mutableStateOf(true) }

    Box(Modifier.fillMaxSize()) {
        val state = live
        if (state != null && cockpit) {
            Column(Modifier.fillMaxSize().statusBarsPadding()) {
                CockpitView(vm.link, state, AircraftProfile.fromName(settings.aircraftProfile), { p -> vm.setAircraftProfile(p) },
                    settings.controlMode, { vm.setControlMode(it) }, settings.floatingControl, { vm.setFloatingControl(it) },
                    vm.nearbyNav.collectAsStateWithLifecycle().value, floatingTrim = settings.floatingStyle == "TRIM", leversLeft = settings.leversLeft,
                    onShowPlan = { cockpit = false },
                    onShowChecklists = onOpenChecklists,
                    recording = vm.recordingInfo.collectAsStateWithLifecycle().value,
                    onStopRecording = { vm.stopRecording() }, onResumeRecording = { vm.resumeRecording() }, onMark = { vm.markLive() },
                    track = vm.track.collectAsStateWithLifecycle().value)
            }
        } else Column(
            Modifier.fillMaxSize().wrapContentWidth(Alignment.CenterHorizontally).widthIn(max = 820.dp)
                .statusBarsPadding().verticalScroll(rememberScrollState()).padding(bottom = 24.dp),
        ) {
            if (state != null) ModeSwitch(cockpit) { cockpit = it }
            else Text(loc("Lot", "Flight"), style = MaterialTheme.typography.headlineLarge, color = c.ink, modifier = Modifier.padding(horizontal = 20.dp, vertical = 12.dp))

            ConnectionSection(vm)

            Section(loc("Checklisty", "Checklists"), Icons.Outlined.Checklist) {
                OutlinedButton(onOpenChecklists, modifier = Modifier.fillMaxWidth()) {
                    Icon(Icons.Outlined.Checklist, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp))
                    Text(state?.aircraft?.title?.let { loc("Checklisty: $it", "Checklists: $it") } ?: loc("Otwórz checklisty", "Open checklists"))
                }
            }

            com.freemetar.app.ui.flight.LogbookEntry(vm, onOpenLogbook)
        }
        SnackbarHost(snack, Modifier.align(Alignment.BottomCenter).padding(12.dp).widthIn(max = 560.dp))
    }
}

@Composable
private fun ModeSwitch(cockpit: Boolean, onChange: (Boolean) -> Unit) {
    val c = Fm.c
    Row(
        Modifier.padding(horizontal = 10.dp, vertical = 4.dp).clip(RoundedCornerShape(50)).border(1.dp, c.rule, RoundedCornerShape(50)).padding(2.dp),
    ) {
        listOf(true to loc("Kokpit", "Cockpit"), false to loc("Połączenie i loty", "Connection & flights")).forEach { (v, label) ->
            val on = v == cockpit
            Text(
                label, fontWeight = FontWeight.SemiBold, fontSize = 12.sp, color = if (on) c.paper else c.ink2,
                modifier = Modifier.clip(RoundedCornerShape(50)).background(if (on) c.ink else c.paper)
                    .clickable { onChange(v) }.padding(horizontal = 14.dp, vertical = 5.dp),
            )
        }
    }
}
