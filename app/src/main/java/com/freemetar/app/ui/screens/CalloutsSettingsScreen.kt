package com.freemetar.app.ui.screens

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.ArrowBack
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.freemetar.app.data.CalloutsList
import com.freemetar.app.domain.loc

/**
 * Ekran ustawień Callouts - konfiguracja zapowiedzi głosowych.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CalloutsSettingsScreen(
    enabled: Boolean,
    enabledCallouts: Set<String>,
    speechRate: Float,
    onEnabledChange: (Boolean) -> Unit,
    onCalloutToggle: (String, Boolean) -> Unit,
    onSpeechRateChange: (Float) -> Unit,
    onBack: () -> Unit
) {
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Callouts") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.Outlined.ArrowBack, contentDescription = loc("common.back"))
                    }
                }
            )
        }
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            // Główny przełącznik
            Card(
                Modifier.fillMaxWidth()
            ) {
                Row(
                    Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(
                            loc("common.voice_callouts"),
                            style = MaterialTheme.typography.titleMedium
                        )
                        Text(
                            loc("callouts_settings.automatic_announcements_during_flight"),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                    Switch(
                        checked = enabled,
                        onCheckedChange = onEnabledChange
                    )
                }
            }

            if (enabled) {
                // Prędkość mowy
                Card(Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(
                            loc("callouts_settings.speech_rate"),
                            style = MaterialTheme.typography.titleMedium
                        )
                        Slider(
                            value = speechRate,
                            onValueChange = onSpeechRateChange,
                            valueRange = 0.5f..1.5f,
                            steps = 4
                        )
                        Text(
                            "${(speechRate * 100).toInt()}%",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                }

                // Callouts przy lądowaniu
                CalloutGroupCard(
                    title = loc("common.landing"),
                    callouts = CalloutsList.LANDING,
                    enabledCallouts = enabledCallouts,
                    onToggle = onCalloutToggle
                )

                // Callouts przy starcie
                CalloutGroupCard(
                    title = loc("callouts_settings.takeoff"),
                    callouts = CalloutsList.TAKEOFF,
                    enabledCallouts = enabledCallouts,
                    onToggle = onCalloutToggle
                )

                // Callouts autopilota
                CalloutGroupCard(
                    title = "Autopilot",
                    callouts = CalloutsList.AUTOPILOT,
                    enabledCallouts = enabledCallouts,
                    onToggle = onCalloutToggle
                )
            }
        }
    }
}

@Composable
private fun CalloutGroupCard(
    title: String,
    callouts: List<Pair<String, String>>,
    enabledCallouts: Set<String>,
    onToggle: (String, Boolean) -> Unit
) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(title, style = MaterialTheme.typography.titleMedium)

            callouts.forEach { (id, description) ->
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(id, style = MaterialTheme.typography.bodyMedium)
                        Text(
                            description,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                    Checkbox(
                        checked = enabledCallouts.contains(id),
                        onCheckedChange = { onToggle(id, it) }
                    )
                }
            }
        }
    }
}
