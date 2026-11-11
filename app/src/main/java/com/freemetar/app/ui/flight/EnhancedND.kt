package com.freemetar.app.ui.flight

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.freemetar.app.data.link.FlightState
import com.freemetar.app.domain.loc
import com.freemetar.app.ui.theme.Fm

/**
 * Tryby Navigation Display:
 * - ARC: wycinek przed samolotem (klasyczny ND)
 * - ROSE: pełna róża kompasowa
 * - PLAN: mapa od góry bez samolotu (tactical)
 * - MAP: mapa 3D z terenu (nowy tryb)
 */
enum class NdMode {
    ARC,    // Wycinek przed samolotem
    ROSE,   // Pełna róża
    PLAN,   // Widok z góry (tactical)
    MAP     // Mapa 3D z terenu
}

/**
 * Rozszerzony Navigation Display z przełączaniem trybów.
 *
 * Tryby:
 * - ARC: klasyczny ND, wycinek 140° przed samolotem
 * - ROSE: pełna róża kompasowa 360°
 * - PLAN: widok z góry bez orientacji, cała trasa widoczna
 * - MAP: mapa 3D z terenu, budynkami, ILS paths (integracja z MapLibre)
 *
 * Warstwy (EFIS controls):
 * - WPT: waypoints z trasy
 * - VOR: radiolatarnie VOR
 * - NDB: radiolatarnie NDB
 * - APT: lotniska
 * - WXR: radar pogodowy (overlay opadów)
 * - TERR: teren (włączone w trybie MAP)
 */
@Composable
fun EnhancedNavDisplay(
    state: FlightState,
    route: List<com.freemetar.app.data.RoutePoint>,
    rangeNm: Int,
    nav: List<com.freemetar.app.data.nav.FixResult> = emptyList(),
    modifier: Modifier = Modifier,
    onModeChange: ((NdMode) -> Unit)? = null,
    onRangeChange: ((Int) -> Unit)? = null,
) {
    var mode by remember { mutableStateOf(NdMode.ARC) }
    var layers by remember { mutableStateOf(setOf("airport", "vor", "ndb", "waypoint")) }
    var showWeather by remember { mutableStateOf(false) }

    val c = Fm.c

    Box(modifier) {
        // Główny wyświetlacz zależnie od trybu
        when (mode) {
            NdMode.ARC -> NavDisplay(
                s = state,
                route = route,
                rangeNm = rangeNm,
                rose = false,
                nav = nav,
                layers = layers,
                modifier = Modifier.fillMaxSize()
            )
            NdMode.ROSE -> NavDisplay(
                s = state,
                route = route,
                rangeNm = rangeNm,
                rose = true,
                nav = nav,
                layers = layers,
                modifier = Modifier.fillMaxSize()
            )
            NdMode.PLAN -> PlanView(
                state = state,
                route = route,
                rangeNm = rangeNm,
                nav = nav,
                layers = layers,
                modifier = Modifier.fillMaxSize()
            )
            NdMode.MAP -> MapView3D(
                state = state,
                route = route,
                rangeNm = rangeNm,
                showWeather = showWeather,
                modifier = Modifier.fillMaxSize()
            )
        }

        // EFIS Controls (lewy górny róg)
        Column(
            Modifier
                .align(Alignment.TopStart)
                .padding(8.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp)
        ) {
            // Przełącznik trybu
            EfisButton(
                label = when (mode) {
                    NdMode.ARC -> "ARC"
                    NdMode.ROSE -> "ROSE"
                    NdMode.PLAN -> "PLAN"
                    NdMode.MAP -> "MAP"
                },
                active = false
            ) {
                mode = when (mode) {
                    NdMode.ARC -> NdMode.ROSE
                    NdMode.ROSE -> NdMode.PLAN
                    NdMode.PLAN -> NdMode.MAP
                    NdMode.MAP -> NdMode.ARC
                }
                onModeChange?.invoke(mode)
            }

            // Range
            EfisButton(
                label = "${rangeNm}NM",
                active = false
            ) {
                val ranges = listOf(5, 10, 20, 40, 80, 160, 320)
                val current = ranges.indexOf(rangeNm)
                val next = if (current < ranges.size - 1) ranges[current + 1] else ranges[0]
                onRangeChange?.invoke(next)
            }

            // Warstwy
            EfisButton("WPT", "waypoint" in layers) {
                layers = if ("waypoint" in layers) layers - "waypoint" else layers + "waypoint"
            }

            EfisButton("VOR", "vor" in layers) {
                layers = if ("vor" in layers) layers - "vor" else layers + "vor"
            }

            EfisButton("NDB", "ndb" in layers) {
                layers = if ("ndb" in layers) layers - "ndb" else layers + "ndb"
            }

            EfisButton("APT", "airport" in layers) {
                layers = if ("airport" in layers) layers - "airport" else layers + "airport"
            }

            // Weather radar (tylko w MAP mode)
            if (mode == NdMode.MAP) {
                EfisButton("WXR", showWeather) {
                    showWeather = !showWeather
                }
            }
        }
    }
}

/**
 * Przycisk EFIS w stylu Boeing/Airbus.
 */
@Composable
private fun EfisButton(
    label: String,
    active: Boolean,
    onClick: () -> Unit
) {
    Box(
        Modifier
            .size(width = 56.dp, height = 32.dp)
            .clip(androidx.compose.foundation.shape.RoundedCornerShape(4.dp))
            .background(if (active) Fm.c.accent else Color(0xFF1A1E24))
            .clickable(onClick = onClick)
            .padding(4.dp),
        contentAlignment = Alignment.Center
    ) {
        Text(
            label,
            color = if (active) Color.Black else Color(0xFF00FF00),
            fontSize = 11.sp,
            fontWeight = FontWeight.Bold,
            fontFamily = com.freemetar.app.ui.theme.Mono
        )
    }
}

/**
 * Widok PLAN - mapa z góry, cała trasa, bez orientacji samolotu.
 * Tactical view dla planowania.
 */
@Composable
private fun PlanView(
    state: FlightState,
    route: List<com.freemetar.app.data.RoutePoint>,
    rangeNm: Int,
    nav: List<com.freemetar.app.data.nav.FixResult>,
    layers: Set<String>,
    modifier: Modifier = Modifier
) {
    val m = rememberTextMeasurer()
    val c = Fm.c

    Canvas(modifier.fillMaxSize()) {
        val w = size.width
        val h = size.height
        val center = Offset(w / 2, h / 2)

        drawRect(Color(0xFF0E1216))

        // TODO: Implement tactical view
        // - Pokaż całą trasę w kadrze
        // - Brak orientacji (północ do góry)
        // - Wszystkie waypoints widoczne
        // - Dystanse między punktami

        label(m, "PLAN VIEW", center, c.ink2, 14.sp)
        label(m, loc("common.in_development"), Offset(center.x, center.y + 30), c.ink2, 11.sp)
    }
}

/**
 * Widok MAP 3D - integracja z MapLibre.
 * Pokazuje teren, budynki, ILS paths, weather overlay.
 */
@Composable
private fun MapView3D(
    state: FlightState,
    route: List<com.freemetar.app.data.RoutePoint>,
    rangeNm: Int,
    showWeather: Boolean,
    modifier: Modifier = Modifier
) {
    // Użyj embedded MapLibre view
    EmbeddedMapView(
        state = state,
        route = route,
        rangeNm = rangeNm,
        showWeather = showWeather,
        modifier = modifier
    )
}
