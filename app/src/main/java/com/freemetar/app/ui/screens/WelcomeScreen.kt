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
                loc("welcome.welcome_to_opentakeoff"),
                style = MaterialTheme.typography.headlineMedium,
                fontWeight = FontWeight.Bold,
                color = Fm.c.ink
            )

            Text(
                loc("welcome.your_flight_sim_companion_weather_maps"),
                style = MaterialTheme.typography.bodyMedium,
                color = Fm.c.ink2,
                textAlign = TextAlign.Center
            )

            Spacer(Modifier.height(8.dp))

            // Funkcje
            FeatureItem(
                icon = Icons.Outlined.Public,
                title = loc("welcome.no_accounts_or_apis"),
                description = loc("welcome.free_open_data_sources_no_ads")
            )

            FeatureItem(
                icon = Icons.Outlined.FlightTakeoff,
                title = loc("welcome.cockpit_on_your_phone"),
                description = loc("welcome.pfd_nd_autopilot_and_controls_via")
            )

            FeatureItem(
                icon = Icons.Outlined.Map,
                title = loc("welcome.3d_map_with_flight_track"),
                description = loc("welcome.airports_with_metar_sigmet_route_and")
            )

            FeatureItem(
                icon = Icons.Outlined.Speed,
                title = loc("welcome.flight_logbook"),
                description = loc("welcome.automatic_recording_with_landing_score_and")
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
                Text(loc("welcome.get_started"), fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
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
