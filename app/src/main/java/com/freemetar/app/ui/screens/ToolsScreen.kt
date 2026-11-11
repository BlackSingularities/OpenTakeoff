package com.freemetar.app.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.ui.draw.clip

import com.freemetar.app.domain.loc

import com.freemetar.app.ui.theme.Fm
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
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
import androidx.compose.material.icons.automirrored.outlined.MenuBook
import androidx.compose.material.icons.automirrored.outlined.TrendingDown
import androidx.compose.material.icons.outlined.Air
import androidx.compose.material.icons.outlined.Height
import androidx.compose.material.icons.outlined.LocalGasStation
import androidx.compose.material.icons.outlined.Route
import androidx.compose.material.icons.outlined.SwapHoriz
import androidx.compose.material.icons.outlined.Translate
import androidx.compose.material3.Icon
import androidx.compose.ui.graphics.vector.ImageVector
import com.freemetar.app.MainViewModel
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.freemetar.app.domain.Atmo
import com.freemetar.app.domain.MetarDecoder
import com.freemetar.app.domain.Wind
import com.freemetar.app.domain.fmt
import androidx.compose.foundation.layout.wrapContentWidth
import com.freemetar.app.ui.components.Panel
import com.freemetar.app.ui.components.SectionCard
import com.freemetar.app.ui.components.StatTile
import com.freemetar.app.ui.theme.Mono
import com.freemetar.app.ui.theme.RawText
import kotlin.math.abs
import kotlin.math.roundToInt

private enum class Tool(private val labelOf: () -> String, val icon: ImageVector) {
    ROUTE({ loc("common.route") }, Icons.Outlined.Route),
    WIND({ loc("common.wind") }, Icons.Outlined.Air),
    DENSITY({ loc("tools.density_alt") }, Icons.Outlined.Height),
    DESCENT({ loc("common.descent") }, Icons.AutoMirrored.Outlined.TrendingDown),
    FUEL({ loc("common.fuel") }, Icons.Outlined.LocalGasStation),
    UNITS({ loc("tools.units") }, Icons.Outlined.SwapHoriz),
    DECODE({ loc("tools.decoder") }, Icons.Outlined.Translate),
    CHEAT({ loc("tools.cheat_sheet") }, Icons.AutoMirrored.Outlined.MenuBook),
    ;

    val label: String get() = labelOf()
}

/**
 * Narzędzia: wszystkie widoczne naraz jako kafelki w siatce 4 × 2 (telefon) albo lista w panelu po lewej
 * (tablet / poziomo), wybrane narzędzie pod spodem lub obok. Wygląd jak reszta aplikacji: panele z zaokrągleniem 10 dp,
 * aktywny kafelek z jaśniejszym tłem, obwódką w kolorze akcentu i kropką — bez odwróconego bloku.
 */
@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
fun ToolsScreen(vm: MainViewModel) {
    var tool by rememberSaveable { mutableStateOf(Tool.ROUTE) }
    val content: @Composable () -> Unit = {
        when (tool) {
            Tool.ROUTE -> RouteTool(vm)
            Tool.CHEAT -> CheatSheet()
            Tool.WIND -> WindTool()
            Tool.DENSITY -> DensityTool()
            Tool.DESCENT -> DescentTool()
            Tool.FUEL -> FuelTool()
            Tool.UNITS -> UnitsTool()
            Tool.DECODE -> DecodeTool()
        }
    }
    val title: @Composable () -> Unit = {
        Text(loc("common.tools"), style = MaterialTheme.typography.headlineLarge, color = Fm.c.ink, modifier = Modifier.padding(top = 4.dp))
        Text(loc("tools.calculators_and_a_cheat_sheet_for"),
            style = MaterialTheme.typography.bodySmall, color = Fm.c.ink2, modifier = Modifier.padding(bottom = 4.dp))
    }
    androidx.compose.foundation.layout.BoxWithConstraints(Modifier.fillMaxSize().statusBarsPadding()) {
        if (maxWidth >= 720.dp) {
            // Tablet: lista narzędzi w panelu po lewej, wybrane narzędzie obok (szerokość treści jak w ustawieniach)
            Row(Modifier.fillMaxSize().padding(horizontal = 12.dp, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Column(Modifier.width(240.dp).fillMaxHeight().verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    title()
                    Panel {
                        Column(Modifier.padding(6.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            Tool.entries.forEach { t -> ToolRow(t, tool == t) { tool = t } }
                        }
                    }
                }
                Column(
                    Modifier.weight(1f).fillMaxHeight().verticalScroll(rememberScrollState()).padding(top = 4.dp).widthIn(max = 760.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) { content() }
            }
        } else {
            LazyColumn(
                Modifier.fillMaxSize().wrapContentWidth(androidx.compose.ui.Alignment.CenterHorizontally).widthIn(max = 760.dp),
                contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                item { Column { title() } }
                item {
                    androidx.compose.foundation.layout.FlowRow(
                        maxItemsInEachRow = 4,
                        horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Tool.entries.forEach { t -> ToolTile(t, tool == t, Modifier.weight(1f)) { tool = t } }
                    }
                }
                item { content() }
                item { Spacer(Modifier.size(16.dp)) }
            }
        }
    }
}

/** Kafelek narzędzia (telefon): panel z ikoną i podpisem; aktywny — jaśniejsze tło, obwódka akcentu, kropka pod podpisem. */
@Composable
private fun ToolTile(t: Tool, on: Boolean, modifier: Modifier, onClick: () -> Unit) {
    val c = Fm.c
    val shape = RoundedCornerShape(10.dp)
    androidx.compose.foundation.layout.Column(
        modifier.clip(shape).background(if (on) c.terrain else c.card).border(if (on) 1.5.dp else 1.dp, if (on) c.accent else c.rule, shape)
            .clickable(role = androidx.compose.ui.semantics.Role.Tab, onClick = onClick)
            .padding(top = 12.dp, bottom = 8.dp, start = 4.dp, end = 4.dp),
        horizontalAlignment = androidx.compose.ui.Alignment.CenterHorizontally,
    ) {
        Icon(t.icon, null, tint = if (on) c.accent else c.ink2, modifier = Modifier.size(22.dp))
        Spacer(Modifier.size(6.dp))
        Text(t.label, color = if (on) c.ink else c.ink2, style = MaterialTheme.typography.labelMedium,
            fontWeight = if (on) FontWeight.SemiBold else FontWeight.Medium, maxLines = 1,
            overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis)
        Spacer(Modifier.size(5.dp))
        androidx.compose.foundation.layout.Box(Modifier.size(5.dp).clip(RoundedCornerShape(50)).background(if (on) c.accent else androidx.compose.ui.graphics.Color.Transparent))
    }
}

/** Pozycja listy narzędzi (tablet): ikona i nazwa; aktywna — jaśniejsze tło i kropka akcentu z prawej. */
@Composable
private fun ToolRow(t: Tool, on: Boolean, onClick: () -> Unit) {
    val c = Fm.c
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(8.dp)).background(if (on) c.terrain else androidx.compose.ui.graphics.Color.Transparent)
            .clickable(role = androidx.compose.ui.semantics.Role.Tab, onClick = onClick).padding(horizontal = 12.dp, vertical = 11.dp),
        verticalAlignment = androidx.compose.ui.Alignment.CenterVertically,
    ) {
        Icon(t.icon, null, tint = if (on) c.accent else c.ink2, modifier = Modifier.size(19.dp))
        Spacer(Modifier.width(12.dp))
        Text(t.label, color = if (on) c.ink else c.ink2, style = MaterialTheme.typography.labelLarge,
            fontWeight = if (on) FontWeight.SemiBold else FontWeight.Medium, modifier = Modifier.weight(1f))
        if (on) androidx.compose.foundation.layout.Box(Modifier.size(6.dp).clip(RoundedCornerShape(50)).background(c.accent))
    }
}

@Composable
internal fun NumField(label: String, value: String, modifier: Modifier = Modifier, onChange: (String) -> Unit) {
    OutlinedTextField(
        value = value, onValueChange = onChange, label = { Text(label) }, singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
        textStyle = MaterialTheme.typography.bodyLarge.copy(fontFamily = Mono),
        shape = RoundedCornerShape(10.dp), modifier = modifier,
    )
}

/** Wiersz tabeli wyników (przelicznik, dekoder): opis z lewej, wartość z prawej, cienka linia pod spodem. */
@Composable
private fun ResultRow(label: String, value: String, last: Boolean = false) {
    Row(Modifier.fillMaxWidth().padding(vertical = 9.dp), verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
        Text(label, color = Fm.c.ink2, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f))
        Text(value, fontFamily = Mono, color = Fm.c.ink, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
    }
    if (!last) com.freemetar.app.ui.components.Rule()
}

internal fun String.num() = replace(',', '.').toDoubleOrNull()

@Composable
private fun WindTool() {
    var dir by rememberSaveable { mutableStateOf("240") }
    var spd by rememberSaveable { mutableStateOf("15") }
    var gust by rememberSaveable { mutableStateOf("") }
    var rwy by rememberSaveable { mutableStateOf("29") }
    SectionCard(loc("tools.wind_components"), icon = Icons.Outlined.Air) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            NumField(loc("tools.direction"), dir, Modifier.weight(1f)) { dir = it }
            NumField(loc("tools.speed_kt"), spd, Modifier.weight(1f)) { spd = it }
        }
        Spacer(Modifier.size(8.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            NumField(loc("tools.gusts_kt_3"), gust, Modifier.weight(1f)) { gust = it }
            NumField(loc("tools.runway_e_g_29_or_287"), rwy, Modifier.weight(1f)) { rwy = it }
        }
        val d = dir.num(); val s = spd.num(); val r = rwy.num()
        if (d != null && s != null && r != null) {
            val hdg = if (r <= 36) r * 10 else r
            val c = Wind.components(d, s, hdg)
            val g = gust.num()?.let { Wind.components(d, it, hdg) }
            Spacer(Modifier.size(12.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                StatTile(if (c.headwind >= 0) loc("tools.headwind") else loc("tools.tailwind"), "${abs(c.headwind).roundToInt()} kt", g?.let { loc("tools.gusts_kt_2", abs(it.headwind).roundToInt()) }, Modifier.weight(1f), accent = if (c.headwind < 0) Fm.c.ifr else Fm.c.ink)
                StatTile(loc("tools.crosswind"), "${abs(c.crosswind).roundToInt()} kt", (g?.let { loc("tools.gusts_kt", abs(it.crosswind).roundToInt()) } ?: "") + c.crossSide, Modifier.weight(1f), accent = if (abs(c.crosswind) > 15) Fm.c.caution else Fm.c.ink)
            }
            Spacer(Modifier.size(8.dp))
            Text(loc("tools.wind_correction_at_tas_100_kt", (Math.toDegrees(Math.asin((c.crosswind / 100).coerceIn(-1.0, 1.0)))).roundToInt()), color = Fm.c.ink2, fontSize = 12.sp)
        }
    }
}

@Composable
private fun DensityTool() {
    var elev by rememberSaveable { mutableStateOf("361") }
    var qnh by rememberSaveable { mutableStateOf("1013") }
    var oat by rememberSaveable { mutableStateOf("25") }
    var ias by rememberSaveable { mutableStateOf("110") }
    SectionCard(loc("tools.density_altitude_and_tas"), icon = Icons.Outlined.Height) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            NumField(loc("tools.altitude_ft"), elev, Modifier.weight(1f)) { elev = it }
            NumField("QNH hPa", qnh, Modifier.weight(1f)) { qnh = it }
        }
        Spacer(Modifier.size(8.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            NumField("OAT °C", oat, Modifier.weight(1f)) { oat = it }
            NumField("IAS kt", ias, Modifier.weight(1f)) { ias = it }
        }
        val e = elev.num(); val q = qnh.num(); val t = oat.num()
        if (e != null && q != null && t != null) {
            val pa = Atmo.pressureAltitude(e, q)
            val da = Atmo.densityAltitude(e, q, t)
            Spacer(Modifier.size(12.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                StatTile(loc("tools.pressure"), "${pa.roundToInt()} ft", null, Modifier.weight(1f))
                StatTile(loc("tools.density"), "${da.roundToInt()} ft", "ISA ${Atmo.isaTemp(pa).fmt(1)} °C", Modifier.weight(1f), accent = if (da - e > 2000) Fm.c.caution else Fm.c.ink)
            }
            ias.num()?.let {
                Spacer(Modifier.size(10.dp))
                StatTile(loc("tools.tas_approx"), "${Atmo.tas(it, da).roundToInt()} kt", null, Modifier.fillMaxWidth())
            }
        }
    }
}

@Composable
private fun DescentTool() {
    var from by rememberSaveable { mutableStateOf("35000") }
    var to by rememberSaveable { mutableStateOf("3000") }
    var gs by rememberSaveable { mutableStateOf("420") }
    var angle by rememberSaveable { mutableStateOf("3") }
    SectionCard(loc("tools.descent_planning_tod"), icon = Icons.AutoMirrored.Outlined.TrendingDown) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            NumField(loc("tools.from_altitude_ft"), from, Modifier.weight(1f)) { from = it }
            NumField(loc("tools.to_altitude_ft"), to, Modifier.weight(1f)) { to = it }
        }
        Spacer(Modifier.size(8.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            NumField("GS kt", gs, Modifier.weight(1f)) { gs = it }
            NumField(loc("tools.path"), angle, Modifier.weight(1f)) { angle = it }
        }
        val f = from.num(); val t = to.num(); val g = gs.num(); val a = angle.num()
        if (f != null && t != null && g != null && a != null && a > 0 && f > t) {
            val dist = (f - t) / (Math.tan(Math.toRadians(a)) * 6076.12)
            val vs = g * 6076.12 / 60 * Math.tan(Math.toRadians(a))
            Spacer(Modifier.size(12.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                StatTile(loc("common.top_of_descent"), "${dist.roundToInt()} NM", loc("tools.before_the_target_point"), Modifier.weight(1f), accent = Fm.c.accent)
                StatTile(loc("common.vertical_speed"), "${vs.roundToInt()} ft/min", loc("tools.time_min", (dist / g * 60).roundToInt()), Modifier.weight(1f))
            }
            Text(loc("tools.rule_of_thumb_3_fl_10"), color = Fm.c.ink2, fontSize = 12.sp, modifier = Modifier.padding(top = 8.dp))
        }
    }
}

@Composable
private fun FuelTool() {
    var dist by rememberSaveable { mutableStateOf("250") }
    var gs by rememberSaveable { mutableStateOf("120") }
    var burn by rememberSaveable { mutableStateOf("30") }
    var reserve by rememberSaveable { mutableStateOf("45") }
    SectionCard(loc("tools.fuel_and_flight_time"), icon = Icons.Outlined.LocalGasStation) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            NumField(loc("tools.distance_nm"), dist, Modifier.weight(1f)) { dist = it }
            NumField("GS kt", gs, Modifier.weight(1f)) { gs = it }
        }
        Spacer(Modifier.size(8.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            NumField(loc("tools.burn_h"), burn, Modifier.weight(1f)) { burn = it }
            NumField(loc("tools.reserve_min"), reserve, Modifier.weight(1f)) { reserve = it }
        }
        val d = dist.num(); val g = gs.num(); val b = burn.num(); val r = reserve.num() ?: 0.0
        if (d != null && g != null && g > 0 && b != null) {
            val h = d / g
            val trip = h * b
            val res = r / 60 * b
            Spacer(Modifier.size(12.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                StatTile(loc("common.time"), "%d:%02d".format(h.toInt(), ((h - h.toInt()) * 60).roundToInt()), null, Modifier.weight(1f))
                StatTile(loc("tools.trip"), trip.fmt(1), null, Modifier.weight(1f))
                StatTile(loc("tools.with_reserve"), (trip + res).fmt(1), "+${res.fmt(1)}", Modifier.weight(1f), accent = Fm.c.accent)
            }
            Text(loc("tools.fuel_units_match_the_burn_rate"), color = Fm.c.ink2, fontSize = 12.sp, modifier = Modifier.padding(top = 8.dp))
        }
    }
}

@Composable
private fun UnitsTool() {
    var v by rememberSaveable { mutableStateOf("1013") }
    val x = v.num()
    SectionCard(loc("tools.unit_converter"), icon = Icons.Outlined.SwapHoriz) {
        NumField(loc("tools.value"), v, Modifier.fillMaxWidth()) { v = it }
        Spacer(Modifier.size(10.dp))
        if (x != null) {
            val rows = listOf(
                "hPa → inHg" to Atmo.hpaToInHg(x).fmt(2),
                "inHg → hPa" to Atmo.inHgToHpa(x).fmt(1),
                "ft → m" to Atmo.ftToM(x).fmt(1),
                "m → ft" to Atmo.mToFt(x).fmt(0),
                "kt → km/h" to Atmo.ktToKmh(x).fmt(1),
                "kt → m/s" to Atmo.ktToMs(x).fmt(1),
                "NM → km" to Atmo.nmToKm(x).fmt(2),
                "SM → km" to Atmo.smToKm(x).fmt(2),
                "°C → °F" to Atmo.cToF(x).fmt(1),
                "°F → °C" to Atmo.fToC(x).fmt(1),
            )
            rows.forEachIndexed { i, (l, r) -> ResultRow(l, r, last = i == rows.lastIndex) }
        }
    }
}

@Composable
private fun DecodeTool() {
    var raw by rememberSaveable { mutableStateOf("EPWA 171030Z 24012G24KT 210V280 9999 -SHRA FEW012 BKN025CB 12/09 Q1008 TEMPO 4000 TSRA") }
    SectionCard(loc("tools.metar_taf_decoder"), icon = Icons.Outlined.Translate) {
        OutlinedTextField(raw, { raw = it.uppercase() }, Modifier.fillMaxWidth(), textStyle = RawText, shape = RoundedCornerShape(10.dp), minLines = 3)
        Spacer(Modifier.size(10.dp))
        val decoded = MetarDecoder.decode(raw).filter { it.text != "—" && it.text.isNotBlank() }
        decoded.forEachIndexed { i, d ->
            // Fragment depeszy w ramce kodu, obok jego znaczenie; cienka linia między pozycjami
            Row(Modifier.fillMaxWidth().padding(vertical = 8.dp)) {
                Text(d.token, fontFamily = Mono, color = Fm.c.accent, fontSize = 12.sp, fontWeight = FontWeight.SemiBold,
                    modifier = Modifier.width(104.dp).padding(end = 8.dp))
                Text(d.text, color = Fm.c.ink, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f))
            }
            if (i != decoded.lastIndex) com.freemetar.app.ui.components.Rule()
        }
    }
}
