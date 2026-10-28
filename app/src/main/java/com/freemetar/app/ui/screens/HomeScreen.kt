package com.freemetar.app.ui.screens

import com.freemetar.app.domain.loc

import android.Manifest
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.Image
import androidx.compose.foundation.clickable
import kotlinx.coroutines.launch
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.staggeredgrid.LazyVerticalStaggeredGrid
import androidx.compose.foundation.lazy.staggeredgrid.StaggeredGridCells
import androidx.compose.foundation.lazy.staggeredgrid.StaggeredGridItemSpan
import androidx.compose.foundation.lazy.staggeredgrid.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.ArrowDownward
import androidx.compose.material.icons.outlined.ArrowUpward
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.CloudOff
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Edit
import androidx.compose.material.icons.outlined.Flight
import androidx.compose.material.icons.outlined.History
import androidx.compose.material.icons.outlined.LocalAirport
import androidx.compose.material.icons.outlined.MyLocation
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material.icons.outlined.SearchOff
import androidx.compose.material.icons.outlined.StarBorder
import androidx.compose.material.icons.outlined.Water
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.freemetar.app.DbState
import com.freemetar.app.MainViewModel
import com.freemetar.app.NearMe
import com.freemetar.app.R
import com.freemetar.app.data.Airport
import com.freemetar.app.data.Locator
import com.freemetar.app.data.Metar
import com.freemetar.app.domain.fmt
import com.freemetar.app.ui.components.CategoryBlock
import com.freemetar.app.ui.components.EmptyState
import com.freemetar.app.ui.components.Panel
import com.freemetar.app.ui.components.Pill
import com.freemetar.app.ui.components.windowColumns
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.Mono
import kotlinx.coroutines.delay
import java.time.Instant
import java.time.ZoneId
import java.time.ZoneOffset
import java.time.ZonedDateTime
import java.time.format.DateTimeFormatter
import kotlin.math.roundToInt

private val Full = StaggeredGridItemSpan.FullLine
private val hhmmZ = DateTimeFormatter.ofPattern("HH:mm").withZone(ZoneOffset.UTC)

@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun HomeScreen(vm: MainViewModel, onFlight: () -> Unit = {}, onOpen: (String) -> Unit) {
    val settings by vm.settings.collectAsStateWithLifecycle()
    val db by vm.dbState.collectAsStateWithLifecycle()
    val metars by vm.favMetars.collectAsStateWithLifecycle()
    val loading by vm.favLoading.collectAsStateWithLifecycle()
    val results by vm.search.collectAsStateWithLifecycle()
    val nearMe by vm.nearMe.collectAsStateWithLifecycle()
    var query by rememberSaveable { mutableStateOf("") }
    var editing by rememberSaveable { mutableStateOf(false) }
    val c = Fm.c
    val live by vm.flightState.collectAsStateWithLifecycle()
    // Puste elementy siatki też dostają odstęp, więc karty bez treści w ogóle nie dodajemy
    val hasFlight = live != null

    val gridState = androidx.compose.foundation.lazy.staggeredgrid.rememberLazyStaggeredGridState()
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    val focus = androidx.compose.ui.platform.LocalFocusManager.current

    PullToRefreshBox(isRefreshing = loading, onRefresh = { vm.refreshFavorites() }, modifier = Modifier.fillMaxSize()) {
        LazyVerticalStaggeredGrid(
            state = gridState,
            columns = StaggeredGridCells.Fixed(windowColumns()),
            modifier = Modifier.fillMaxSize().statusBarsPadding(),
            contentPadding = PaddingValues(start = 20.dp, end = 20.dp, top = 8.dp, bottom = 28.dp),
            verticalItemSpacing = 10.dp,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            // Logo: powrót do pulpitu — czyści wyszukiwanie i przewija na górę
            item(span = Full) {
                Header {
                    query = ""; vm.onQuery(""); editing = false; focus.clearFocus()
                    scope.launch { gridState.animateScrollToItem(0) }
                }
            }
            item(span = Full) {
                OutlinedTextField(
                    value = query,
                    onValueChange = { query = it; vm.onQuery(it) },
                    modifier = Modifier.fillMaxWidth().padding(bottom = 4.dp),
                    placeholder = { Text(loc("Lotnisko: ICAO, IATA lub miasto", "Airport: ICAO, IATA or city")) },
                    leadingIcon = { Icon(Icons.Outlined.Search, null) },
                    trailingIcon = { if (query.isNotEmpty()) IconButton({ query = ""; vm.onQuery("") }) { Icon(Icons.Outlined.Close, loc("Wyczyść", "Clear")) } },
                    singleLine = true,
                    shape = RoundedCornerShape(10.dp),
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Characters, imeAction = ImeAction.Search),
                    keyboardActions = KeyboardActions(onSearch = { if (query.length >= 3) onOpen(results.firstOrNull()?.ident ?: query.uppercase().trim()) }),
                    colors = OutlinedTextFieldDefaults.colors(
                        unfocusedContainerColor = c.card, focusedContainerColor = c.card,
                        unfocusedBorderColor = c.rule, focusedBorderColor = c.ink,
                    ),
                )
            }
            if (db is DbState.Loading || db is DbState.Error) item(span = Full) { DbBanner(db) { vm.loadDb(force = true) } }

            if (query.length >= 2) {
                if (results.isEmpty()) item(span = Full) {
                    EmptyState(
                        Icons.Outlined.SearchOff, loc("Nie ma takiego lotniska w bazie", "No such airport in the database"),
                        if (db is DbState.Ready) loc("Sprawdź kod albo naciśnij Szukaj na klawiaturze, żeby pobrać METAR dla ${query.uppercase()} bezpośrednio.", "Check the code, or press Search on the keyboard to fetch the METAR for ${query.uppercase()} directly.") else loc("Baza lotnisk jeszcze się wczytuje.", "The airport database is still loading."),
                    )
                }
                items(results, key = { "s" + it.ident }) { a -> SearchRow(a) { onOpen(a.ident) } }
            } else {
                if (hasFlight) item(span = Full) { YourFlightCard(vm, onFlight, onOpen) }
                if (settings.recent.isNotEmpty()) item(span = Full) {
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(bottom = 6.dp)) {
                        settings.recent.forEach { id -> Pill(id, icon = Icons.Outlined.History, mono = true) { onOpen(id) } }
                        Text(loc("Wyczyść", "Clear"), color = c.ink2, style = MaterialTheme.typography.labelMedium,
                            modifier = Modifier.clip(RoundedCornerShape(50)).clickable { vm.clearRecent() }.padding(horizontal = 8.dp, vertical = 7.dp))
                    }
                }

                item(span = Full) {
                    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(top = 6.dp)) {
                        Column(Modifier.weight(1f)) {
                            Text(loc("Twoje lotniska", "Your airports"), style = MaterialTheme.typography.titleLarge, color = c.ink)
                            val newest = metars.values.maxOfOrNull { it.obsTimeEpoch }
                            Text(
                                if (loading) loc("Pobieranie pogody…", "Loading weather…") else newest?.let { loc("Najnowsza obserwacja ${hhmmZ.format(Instant.ofEpochSecond(it))}Z", "Latest observation ${hhmmZ.format(Instant.ofEpochSecond(it))}Z") } ?: loc("Pociągnij w dół, aby odświeżyć", "Pull down to refresh"),
                                style = MaterialTheme.typography.bodySmall, color = c.ink2,
                            )
                        }
                        if (settings.favorites.isNotEmpty()) {
                            TextButton({ editing = !editing }) {
                                Icon(if (editing) Icons.Outlined.Close else Icons.Outlined.Edit, null, Modifier.size(18.dp))
                                Spacer(Modifier.width(6.dp))
                                Text(if (editing) loc("Gotowe", "Done") else loc("Ułóż", "Arrange"))
                            }
                        }
                        IconButton({ vm.refreshFavorites() }) { Icon(Icons.Outlined.Refresh, loc("Odśwież pogodę", "Refresh weather"), tint = c.ink2) }
                    }
                }
                if (settings.favorites.isEmpty()) item(span = Full) {
                    EmptyState(Icons.Outlined.StarBorder, loc("Nie masz jeszcze ulubionych lotnisk", "No favourite airports yet"), loc("Wyszukaj lotnisko i dotknij gwiazdki na jego karcie, a pojawi się tutaj z bieżącą pogodą.", "Search for an airport and tap the star on its page — it will appear here with current weather."))
                }
                items(settings.favorites, key = { "f$it" }) { id ->
                    FlightStrip(
                        ident = id,
                        name = vm.airportName(id) ?: metars[id]?.name,
                        metar = metars[id],
                        loading = loading,
                        editing = editing,
                        onClick = { if (!editing) onOpen(id) },
                        onUp = { vm.moveFavorite(id, -1) },
                        onDown = { vm.moveFavorite(id, 1) },
                        onRemove = { vm.toggleFavorite(id) },
                    )
                }

                item(span = Full) { NearMeBlock(vm, nearMe, onOpen) }
                item(span = Full) { Column {
                    Text(
                        loc("Pogoda: NOAA Aviation Weather Center. Lotniska: OurAirports. Tylko do symulacji lotów — nie używaj do rzeczywistej nawigacji.", "Weather: NOAA Aviation Weather Center. Airports: OurAirports. For flight simulation only — not for real-world navigation."),
                        style = MaterialTheme.typography.bodySmall, color = c.ink2, modifier = Modifier.padding(top = 12.dp),
                    )
                    val uri = androidx.compose.ui.platform.LocalUriHandler.current
                    Text(
                        loc("Pomysł i wykonanie: Adam Rędzikowski · ", "Idea and development: Adam Rędzikowski · ") + com.freemetar.app.ui.theme.AUTHOR_SITE,
                        style = MaterialTheme.typography.bodySmall, color = c.ink,
                        modifier = Modifier.padding(top = 6.dp, bottom = 8.dp).clickable { uri.openUri(com.freemetar.app.ui.theme.AUTHOR_URL) },
                    )
                } }
            }
        }
    }
}

@Composable
private fun Header(onLogo: () -> Unit) {
    var now by remember { mutableStateOf(ZonedDateTime.now(ZoneOffset.UTC)) }
    LaunchedEffect(Unit) { while (true) { now = ZonedDateTime.now(ZoneOffset.UTC); delay(1000) } }
    val local = now.withZoneSameInstant(ZoneId.systemDefault())
    val c = Fm.c
    Row(Modifier.fillMaxWidth().padding(top = 8.dp, bottom = 10.dp), verticalAlignment = Alignment.Bottom) {
        Image(
            painter = painterResource(R.drawable.opentakeoff_mark),
            contentDescription = loc("OpenTakeoff — pulpit", "OpenTakeoff — dashboard"),
            modifier = Modifier.size(48.dp)
                .clickable(onClickLabel = loc("Pulpit", "Dashboard"), onClick = onLogo),
        )
        Spacer(Modifier.width(10.dp))
        Text(
            buildAnnotatedString {
                append("Open")
                withStyle(SpanStyle(color = c.ink2)) { append("Takeoff") }
            },
            style = MaterialTheme.typography.headlineLarge, color = c.ink,
            modifier = Modifier.weight(1f).clickable(onClickLabel = loc("Pulpit", "Dashboard"), onClick = onLogo),
        )
        Column(horizontalAlignment = Alignment.End) {
            Text(now.format(DateTimeFormatter.ofPattern("HH:mm:ss")) + "Z", fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 20.sp, color = c.green)
            Text(loc("czas lokalny ", "local time ") + local.format(DateTimeFormatter.ofPattern("HH:mm")), style = MaterialTheme.typography.bodySmall, color = c.ink2)
        }
    }
}

/**
 * „Twój lot”: dane z symulatora i następny punkt planu lotu z GPS symulatora.
 * Nic nie rysuje bez połączenia z symulatorem.
 */
@Composable
private fun YourFlightCard(vm: MainViewModel, onFlight: () -> Unit, onOpen: (String) -> Unit) {
    val c = Fm.c
    val live by vm.flightState.collectAsStateWithLifecycle()
    val s = live ?: return
    val gps = s.avionics?.gps?.takeIf { it.activeFlightPlan && !it.nextId.isNullOrBlank() }

    Panel(onClick = onFlight) {
        Column(Modifier.padding(14.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(8.dp).clip(RoundedCornerShape(50)).background(c.green))
                Spacer(Modifier.width(8.dp))
                Text(
                    s.aircraft.title ?: loc("Symulator połączony", "Simulator connected"),
                    style = MaterialTheme.typography.labelLarge, color = c.ink2, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
                )
                Text(
                    loc("Kokpit", "Cockpit"),
                    fontWeight = FontWeight.SemiBold, fontSize = 13.sp, color = c.paper,
                    modifier = Modifier.clip(RoundedCornerShape(50)).background(c.ink).padding(horizontal = 12.dp, vertical = 5.dp),
                )
            }
            if (gps != null) {
                Spacer(Modifier.size(8.dp))
                Text("→ ${gps.nextId}" + (gps.nextDistNm?.let { "  %.1f NM".format(it) } ?: ""),
                    fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 15.sp, color = c.magenta, maxLines = 1)
            }
            run {
                Spacer(Modifier.size(10.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    StripField("ALT", s.position.altIndFt?.let { "%,d".format(it.roundToInt()).replace(',', ' ') } ?: "—", Modifier.weight(1.2f))
                    StripField("IAS", s.speed.iasKt?.let { "${it.roundToInt()} kt" } ?: "—", Modifier.weight(1f))
                    StripField("GS", s.speed.gsKt?.let { "${it.roundToInt()} kt" } ?: "—", Modifier.weight(1f))
                    StripField("HDG", s.attitude.hdgMagDeg?.let { "%03d°".format(((it.roundToInt() % 360) + 360) % 360) } ?: "—", Modifier.weight(0.9f))
                }
            }
        }
    }
}

@Composable
private fun DbBanner(db: DbState, onRetry: () -> Unit) {
    val c = Fm.c
    when (db) {
        is DbState.Loading -> Panel {
            Column(Modifier.padding(14.dp)) {
                Text(db.message, style = MaterialTheme.typography.bodyMedium, color = c.ink)
                Spacer(Modifier.size(8.dp))
                LinearProgressIndicator(Modifier.fillMaxWidth(), color = c.ink, trackColor = c.terrain)
                Spacer(Modifier.size(6.dp))
                Text(loc("Pierwsze uruchomienie pobiera ok. 15 MB danych lotnisk. Potem aplikacja korzysta z kopii w telefonie.", "The first launch downloads about 15 MB of airport data. After that the app uses the copy on the phone."), style = MaterialTheme.typography.bodySmall, color = c.ink2)
            }
        }
        is DbState.Error -> Panel {
            Column(Modifier.padding(14.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Outlined.CloudOff, null, tint = c.ifr)
                    Spacer(Modifier.width(10.dp))
                    Text(loc("Nie pobrano bazy lotnisk: ${db.message}. Sprawdź połączenie z internetem.", "Airport database not downloaded: ${db.message}. Check your internet connection."), style = MaterialTheme.typography.bodyMedium, color = c.ink, modifier = Modifier.weight(1f))
                }
                Spacer(Modifier.size(10.dp))
                Button(onRetry) { Text(loc("Pobierz bazę ponownie", "Download the database again")) }
            }
        }
        else -> {}
    }
}

@Composable
private fun SearchRow(a: Airport, onClick: () -> Unit) {
    val c = Fm.c
    Panel(onClick = onClick) {
        Row(Modifier.padding(horizontal = 14.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(
                when (a.type) { "seaplane_base" -> Icons.Outlined.Water; "small_airport" -> Icons.Outlined.Flight; else -> Icons.Outlined.LocalAirport },
                null, tint = c.ink2, modifier = Modifier.size(22.dp),
            )
            Spacer(Modifier.width(12.dp))
            Column(Modifier.width(68.dp)) {
                Text(a.ident, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 17.sp, color = c.ink)
                if (a.iata.isNotBlank()) Text(a.iata, fontFamily = Mono, fontSize = 12.sp, color = c.ink2)
            }
            Column(Modifier.weight(1f)) {
                Text(a.name, style = MaterialTheme.typography.titleSmall, color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(listOf(a.municipality, a.country).filter { it.isNotBlank() }.joinToString(", ") + ", " + a.typeLabel.lowercase(),
                    style = MaterialTheme.typography.bodySmall, color = c.ink2, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

/**
 * Pasek lotu: identyfikator i nazwa, pod spodem stałe pola danych,
 * po prawej blok kategorii lotu w jej kolorze.
 */
@Composable
fun FlightStrip(
    ident: String,
    name: String?,
    metar: Metar?,
    loading: Boolean,
    editing: Boolean = false,
    trailingNote: String? = null,
    onClick: () -> Unit,
    onUp: () -> Unit = {},
    onDown: () -> Unit = {},
    onRemove: () -> Unit = {},
) {
    val c = Fm.c
    Panel(onClick = onClick) {
        Row(Modifier.fillMaxWidth().height(IntrinsicSize.Min)) {
            Column(Modifier.weight(1f).padding(start = 14.dp, end = 10.dp, top = 11.dp, bottom = 10.dp)) {
                Row(verticalAlignment = Alignment.Bottom) {
                    Text(ident, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 23.sp, color = c.ink)
                    Spacer(Modifier.width(10.dp))
                    Text(name.orEmpty(), style = MaterialTheme.typography.bodySmall, color = c.ink2, maxLines = 1, overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f).padding(bottom = 3.dp))
                    if (trailingNote != null) Text(trailingNote, fontFamily = Mono, fontSize = 12.sp, color = c.ink2, modifier = Modifier.padding(bottom = 3.dp))
                }
                Spacer(Modifier.size(7.dp))
                Box(Modifier.fillMaxWidth().height(1.dp).background(c.rule))
                Spacer(Modifier.size(6.dp))
                if (metar != null) {
                    // Kolumny o stałych proporcjach: wiatr z porywami potrzebuje najwięcej miejsca
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        StripField(loc("Wiatr", "Wind"), windShort(metar), Modifier.weight(1.5f))
                        StripField("QNH", metar.altimHpa?.roundToInt()?.toString() ?: "—", Modifier.weight(1f))
                        StripField("Temp", metar.tempC?.let { "${it.roundToInt()}°" } ?: "—", Modifier.weight(0.8f))
                        StripField(loc("Obs.", "Obs."), ageMinutes(metar.obsTimeEpoch), Modifier.weight(1.2f), stale = isStale(metar.obsTimeEpoch))
                    }
                } else {
                    Text(if (loading) loc("Pobieranie METAR…", "Loading METAR…") else loc("Brak bieżącego METAR", "No current METAR"), style = MaterialTheme.typography.bodySmall, color = c.ink2,
                        modifier = Modifier.padding(vertical = 6.dp))
                }
            }
            if (editing) {
                Column(Modifier.fillMaxHeight().background(c.terrain), verticalArrangement = Arrangement.Center) {
                    Row {
                        IconButton(onUp) { Icon(Icons.Outlined.ArrowUpward, loc("Przesuń wyżej", "Move up"), tint = c.ink) }
                        IconButton(onDown) { Icon(Icons.Outlined.ArrowDownward, loc("Przesuń niżej", "Move down"), tint = c.ink) }
                    }
                    IconButton(onRemove, Modifier.align(Alignment.CenterHorizontally)) { Icon(Icons.Outlined.Delete, loc("Usuń z ulubionych", "Remove from favourites"), tint = c.ifr) }
                }
            } else {
                CategoryBlock(metar)
            }
        }
    }
}

@Composable
private fun StripField(label: String, value: String, modifier: Modifier = Modifier, stale: Boolean = false) {
    val c = Fm.c
    Column(modifier) {
        Text(label, style = MaterialTheme.typography.labelSmall, color = c.ink2, maxLines = 1)
        Text(value, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 14.sp, color = if (stale) c.caution else c.ink,
            maxLines = 1, softWrap = false, overflow = TextOverflow.Clip)
    }
}

@Composable
private fun NearMeBlock(vm: MainViewModel, state: NearMe, onOpen: (String) -> Unit) {
    val c = Fm.c
    val ctx = LocalContext.current
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (granted) vm.findNearMe()
    }
    fun start() = if (Locator.hasPermission(ctx)) vm.findNearMe() else launcher.launch(Manifest.permission.ACCESS_COARSE_LOCATION)

    Column(Modifier.padding(top = 18.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            val byAircraft = (state as? NearMe.Found)?.aircraft == true
            Text(if (byAircraft) loc("W pobliżu samolotu", "Near the aircraft") else loc("W pobliżu Ciebie", "Near you"),
                style = MaterialTheme.typography.titleLarge, color = c.ink, modifier = Modifier.weight(1f))
            if (state is NearMe.Found && !byAircraft) IconButton({ start() }) { Icon(Icons.Outlined.MyLocation, loc("Ustal pozycję ponownie", "Locate again"), tint = c.ink2) }
        }
        Spacer(Modifier.size(8.dp))
        when (state) {
            NearMe.Idle -> Panel {
                Row(Modifier.padding(14.dp), verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Outlined.MyLocation, null, tint = c.ink, modifier = Modifier.size(26.dp))
                    Spacer(Modifier.width(12.dp))
                    Text(loc("Najbliższe lotniska z bieżącą pogodą, wg przybliżonej lokalizacji telefonu.", "The nearest airports with current weather, based on the phone's approximate location."), style = MaterialTheme.typography.bodyMedium, color = c.ink, modifier = Modifier.weight(1f))
                }
                Box(Modifier.padding(start = 14.dp, end = 14.dp, bottom = 14.dp)) {
                    OutlinedButton({ start() }) { Text(loc("Pokaż lotniska w pobliżu", "Show nearby airports")) }
                }
            }
            NearMe.Locating -> Row(Modifier.padding(vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp, color = c.ink)
                Spacer(Modifier.width(12.dp))
                Text(loc("Ustalanie pozycji…", "Locating…"), style = MaterialTheme.typography.bodyMedium, color = c.ink2)
            }
            is NearMe.Failed -> Column {
                Text(state.message, style = MaterialTheme.typography.bodyMedium, color = c.ink)
                TextButton({ start() }) { Text(loc("Spróbuj ponownie", "Try again")) }
            }
            is NearMe.Found -> Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                if (state.aircraft) Text(loc("Wg pozycji samolotu w symulatorze — odświeża się co ok. 10 NM lotu.", "From the aircraft's position in the simulator — refreshes about every 10 NM."),
                    style = MaterialTheme.typography.bodySmall, color = c.ink2)
                if (state.airports.isEmpty()) Text(loc("Brak lotnisk w promieniu 120 NM.", "No airports within 120 NM."), style = MaterialTheme.typography.bodyMedium, color = c.ink2)
                state.airports.forEach { (a, dist) ->
                    FlightStrip(
                        ident = a.ident, name = a.name, metar = state.metars[a.ident],
                        loading = !state.loaded,
                        trailingNote = "${dist.fmt(0)} NM",
                        onClick = { onOpen(a.ident) },
                    )
                }
            }
        }
    }
}

fun windShort(m: Metar): String {
    val spd = m.windKt ?: return "—"
    if (spd == 0) return "CALM"
    val d = if (m.windVariable) "VRB" else "%03d".format(m.windDir ?: 0)
    return "$d/%02d".format(spd) + (m.gustKt?.let { "G$it" } ?: "")
}

fun ageMinutes(epoch: Long): String {
    if (epoch == 0L) return "—"
    val min = (System.currentTimeMillis() / 1000 - epoch) / 60
    return when {
        min < 1 -> loc("teraz", "now")
        min < 60 -> "${min} min"
        else -> "${min / 60} h ${min % 60} min"
    }
}

/** METAR starszy niż 90 minut prawdopodobnie jest nieaktualny. */
fun isStale(epoch: Long) = epoch > 0 && System.currentTimeMillis() / 1000 - epoch > 90 * 60
