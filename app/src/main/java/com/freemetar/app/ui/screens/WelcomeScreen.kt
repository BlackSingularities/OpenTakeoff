package com.freemetar.app.ui.screens

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.FlightTakeoff
import androidx.compose.material.icons.outlined.Map
import androidx.compose.material.icons.outlined.Speed
import androidx.compose.material.icons.outlined.Public
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.freemetar.app.R
import com.freemetar.app.domain.loc
import com.freemetar.app.ui.theme.Fm

/**
 * Ekran powitalny pokazywany przy pierwszym uruchomieniu aplikacji.
 */
@Composable
fun WelcomeScreen(
    onGetStarted: () -> Unit,
) {
    Box(
        Modifier
            .fillMaxSize()
            .background(Fm.c.paper)
            .statusBarsPadding()
    ) {
        Column(
            Modifier
                .fillMaxSize()
                .widthIn(max = 600.dp)
                .align(Alignment.TopCenter)
                .verticalScroll(rememberScrollState())
                .padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(24.dp)
        ) {
            Spacer(Modifier.height(16.dp))

            // Logo
            Image(
                painter = painterResource(R.drawable.opentakeoff_mark),
                contentDescription = null,
                modifier = Modifier.size(80.dp)
            )

            // Tytuł
            Text(
                loc("Witaj w OpenTakeoff", "Welcome to OpenTakeoff"),
                style = MaterialTheme.typography.headlineMedium,
                fontWeight = FontWeight.Bold,
                color = Fm.c.ink
            )

            Text(
                loc(
                    "Twój kokpit dla symulatorów lotu — pogoda, mapy, plan lotu i dane z MSFS na telefonie",
                    "Your flight sim companion — weather, maps, flight planning and MSFS data on your phone"
                ),
                style = MaterialTheme.typography.bodyMedium,
                color = Fm.c.ink2,
                textAlign = TextAlign.Center
            )

            Spacer(Modifier.height(8.dp))

            // Funkcje
            FeatureItem(
                icon = Icons.Outlined.Public,
                title = loc("Bez kont i API", "No accounts or APIs"),
                description = loc(
                    "Darmowe, otwarte źródła danych. Bez reklam i subskrypcji.",
                    "Free, open data sources. No ads or subscriptions."
                )
            )

            FeatureItem(
                icon = Icons.Outlined.FlightTakeoff,
                title = loc("Kokpit na telefonie", "Cockpit on your phone"),
                description = loc(
                    "PFD, ND, autopilot i sterowanie przez OpenTakeoff Link (MSFS 2024)",
                    "PFD, ND, autopilot and controls via OpenTakeoff Link (MSFS 2024)"
                )
            )

            FeatureItem(
                icon = Icons.Outlined.Map,
                title = loc("Mapa 3D ze śladem lotu", "3D map with flight track"),
                description = loc(
                    "Lotniska z METAR, SIGMET, trasa i profil pionowy w czasie rzeczywistym",
                    "Airports with METAR, SIGMET, route and real-time vertical profile"
                )
            )

            FeatureItem(
                icon = Icons.Outlined.Speed,
                title = loc("Dziennik lotów", "Flight logbook"),
                description = loc(
                    "Automatyczny zapis każdego lotu z oceną lądowania i eksportem do GPX",
                    "Automatic recording with landing score and GPX export"
                )
            )

            Spacer(Modifier.height(16.dp))

            Button(
                onClick = onGetStarted,
                modifier = Modifier.fillMaxWidth().height(56.dp),
                shape = RoundedCornerShape(12.dp),
                colors = ButtonDefaults.buttonColors(
                    containerColor = Fm.c.accent,
                    contentColor = Fm.c.paper
                )
            ) {
                Text(loc("Rozpocznij", "Get started"), fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
            }
        }
    }
}

@Composable
private fun FeatureItem(icon: ImageVector, title: String, description: String) {
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(16.dp),
        verticalAlignment = Alignment.Top
    ) {
        Box(
            Modifier
                .size(48.dp)
                .background(Fm.c.accent.copy(alpha = 0.1f), CircleShape),
            contentAlignment = Alignment.Center
        ) {
            Icon(
                icon,
                contentDescription = null,
                tint = Fm.c.accent,
                modifier = Modifier.size(24.dp)
            )
        }

        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(
                title,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                color = Fm.c.ink
            )
            Text(
                description,
                style = MaterialTheme.typography.bodyMedium,
                color = Fm.c.ink2
            )
        }
    }
}
