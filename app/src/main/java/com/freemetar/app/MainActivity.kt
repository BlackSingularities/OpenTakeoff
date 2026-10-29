package com.freemetar.app

import com.freemetar.app.domain.loc
import androidx.compose.foundation.clickable
import androidx.compose.ui.draw.clip

import android.graphics.Color as AndroidColor
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.statusBars
import androidx.compose.material.icons.automirrored.outlined.MenuOpen
import androidx.compose.material.icons.outlined.Menu
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Explore
import androidx.compose.material.icons.filled.FlightTakeoff
import androidx.compose.material.icons.filled.Map
import androidx.compose.material.icons.outlined.FlightTakeoff
import androidx.compose.material.icons.outlined.Map
import androidx.compose.material.icons.filled.LocalAirport
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material.icons.outlined.Explore
import androidx.compose.material.icons.outlined.LocalAirport
import androidx.compose.material.icons.outlined.Tune
import androidx.compose.material.icons.outlined.Calculate
import androidx.compose.material.icons.filled.Calculate
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.outlined.Book
import androidx.compose.material.icons.filled.Book
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.NavigationRailItemDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import com.freemetar.app.ui.components.Rule
import com.freemetar.app.ui.components.UpdateDialog
import com.freemetar.app.ui.components.isWide
import com.freemetar.app.ui.screens.BriefingScreen
import com.freemetar.app.ui.flight.FlightDetailScreen
import com.freemetar.app.ui.screens.FlightScreen
import com.freemetar.app.ui.screens.MapScreen
import com.freemetar.app.ui.screens.HomeScreen
import com.freemetar.app.ui.screens.SettingsScreen
import com.freemetar.app.ui.screens.ToolsScreen
import com.freemetar.app.ui.screens.WelcomeScreen
import androidx.compose.runtime.collectAsState
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.OpenTakeoffTheme

private data class Tab(val route: String, val label: String, val icon: ImageVector, val selectedIcon: ImageVector)

// Getter: etykiety w bieżącym języku przy każdym rysowaniu paska
private val tabs get() = listOf(
    Tab("home", loc("Lotniska", "Airports"), Icons.Outlined.LocalAirport, Icons.Filled.LocalAirport),
    Tab("map", loc("Mapa", "Map"), Icons.Outlined.Map, Icons.Filled.Map),
    Tab("flight", loc("Lot", "Flight"), Icons.Outlined.FlightTakeoff, Icons.Filled.FlightTakeoff),
    Tab("logbook", loc("Dziennik", "Logbook"), Icons.Outlined.Book, Icons.Filled.Book),
    Tab("tools", loc("Narzędzia", "Tools"), Icons.Outlined.Calculate, Icons.Filled.Calculate),
    Tab("settings", loc("Ustawienia", "Settings"), Icons.Outlined.Settings, Icons.Filled.Settings),
)

private fun NavHostController.goTab(route: String) = navigate(route) {
    popUpTo(graph.findStartDestination().id) { saveState = true }
    launchSingleTop = true
    restoreState = true
}

class MainActivity : ComponentActivity() {
    private lateinit var viewModel: MainViewModel

    // Kontroler (Bluetooth/USB): osie i przyciski trafiają do Gamepad; panel sterowania decyduje, czy je wysłać
    override fun onGenericMotionEvent(event: android.view.MotionEvent): Boolean =
        com.freemetar.app.data.input.Gamepad.onMotion(event) || super.onGenericMotionEvent(event)

    override fun dispatchKeyEvent(event: android.view.KeyEvent): Boolean =
        com.freemetar.app.data.input.Gamepad.onKey(event) || super.dispatchKeyEvent(event)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // ViewModel startuje (baza lotnisk, komunikaty) zanim wczytają się ustawienia — do tego czasu język systemu
        com.freemetar.app.domain.Lang.current = com.freemetar.app.domain.Lang.resolve(null)
        enableEdgeToEdge()
        setContent {
            val vm: MainViewModel = viewModel()
            viewModel = vm
            val settings by vm.settings.collectAsStateWithLifecycle()
            // Język: ustawienie albo system; Lang.current dla logiki, LocalLang dla przebudowy interfejsu
            val lang = com.freemetar.app.domain.Lang.resolve(settings.language)
            com.freemetar.app.domain.Lang.current = lang
            androidx.compose.runtime.CompositionLocalProvider(com.freemetar.app.ui.theme.LocalLang provides lang) {
            OpenTakeoffTheme(settings.themeMode) {
                val night = Fm.c.isNight
                // Ikony paska stanu muszą pasować do motywu wybranego w aplikacji, nie do systemu
                // Pełny ekran: pasek stanu schowany, wysuwa się na chwilę przeciągnięciem od góry.
                // Ekrany używają wstawek paska stanu, więc same zajmują zwolnione miejsce.
                LaunchedEffect(settings.hideStatusBar) {
                    val ctl = androidx.core.view.WindowCompat.getInsetsController(window, window.decorView)
                    if (settings.hideStatusBar) {
                        ctl.systemBarsBehavior = androidx.core.view.WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
                        ctl.hide(androidx.core.view.WindowInsetsCompat.Type.statusBars())
                    } else ctl.show(androidx.core.view.WindowInsetsCompat.Type.statusBars())
                }
                LaunchedEffect(night) {
                    val style = if (night) SystemBarStyle.dark(AndroidColor.TRANSPARENT)
                    else SystemBarStyle.light(AndroidColor.TRANSPARENT, AndroidColor.TRANSPARENT)
                    enableEdgeToEdge(statusBarStyle = style, navigationBarStyle = style)
                }

                val nav = rememberNavController()
                // „Wstecz” na pierwszym ekranie zamykało aplikację razem z połączeniem i zapisem lotu — w trakcie
                // połączenia z symulatorem aplikacja tylko schodzi do tła (NavHost niżej ma pierwszeństwo, gdy jest dokąd wrócić)
                val linkStatus by vm.linkStatus.collectAsStateWithLifecycle()
                val recInfo by vm.recordingInfo.collectAsStateWithLifecycle()
                androidx.activity.compose.BackHandler(
                    enabled = recInfo.recording || linkStatus is com.freemetar.app.data.link.LinkStatus.Connected || linkStatus is com.freemetar.app.data.link.LinkStatus.Lost,
                ) { moveTaskToBack(true) }
                val entry by nav.currentBackStackEntryAsState()
                val route = entry?.destination?.route
                val activeTab = when {
                    route?.startsWith("airport") == true -> "home"
                    route?.startsWith("flightlog") == true -> "logbook"
                    else -> route
                }
                val wide = isWide()
                val c = Fm.c
                val simLive = vm.flightState.collectAsStateWithLifecycle().value != null
                // Android 13+: zgoda na powiadomienie o zapisie lotu w tle — raz, przy pierwszym połączeniu z symulatorem
                val notifyPermission = androidx.activity.compose.rememberLauncherForActivityResult(
                    androidx.activity.result.contract.ActivityResultContracts.RequestPermission(),
                ) {}
                androidx.compose.runtime.LaunchedEffect(simLive) {
                    if (simLive && android.os.Build.VERSION.SDK_INT >= 33 &&
                        checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) != android.content.pm.PackageManager.PERMISSION_GRANTED
                    ) notifyPermission.launch(android.Manifest.permission.POST_NOTIFICATIONS)
                }
                // Ikona zakładki; przy Locie zielona kropka, gdy symulator jest połączony
                val tabIcon: @androidx.compose.runtime.Composable (Tab, Boolean) -> Unit = { t, sel ->
                    Box {
                        Icon(if (sel) t.selectedIcon else t.icon, null)
                        if (t.route == "flight" && simLive) Box(Modifier.align(Alignment.TopEnd).size(8.dp).background(Fm.c.green, androidx.compose.foundation.shape.CircleShape))
                    }
                }
                UpdateDialog(vm)

                Scaffold(
                    containerColor = c.paper,
                    // Pasek stanu obsługuje każdy ekran sam; tu tylko dolny pasek systemowy
                    contentWindowInsets = WindowInsets.navigationBars,
                    bottomBar = {
                        if (!wide && route in tabs.map { it.route }) {
                            androidx.compose.foundation.layout.Column {
                            Rule()
                            NavigationBar(containerColor = c.paper, tonalElevation = 0.dp) {
                                tabs.forEach { t ->
                                    val sel = activeTab == t.route
                                    NavigationBarItem(
                                        selected = sel,
                                        onClick = { nav.goTab(t.route) },
                                        icon = { tabIcon(t, sel) },
                                        label = { Text(t.label) },
                                        colors = NavigationBarItemDefaults.colors(
                                            indicatorColor = c.ink, selectedIconColor = c.paper, selectedTextColor = c.ink,
                                            unselectedIconColor = c.ink2, unselectedTextColor = c.ink2,
                                        ),
                                    )
                                }
                            }
                            }
                        }
                    },
                ) { pad ->
                    // Tablet: lewy pasek zakładek można schować, żeby kokpit i panele dostały całą szerokość
                    var railHidden by androidx.compose.runtime.saveable.rememberSaveable { androidx.compose.runtime.mutableStateOf(false) }
                    Row(Modifier.fillMaxSize().padding(pad)) {
                        if (wide && !railHidden) {
                            // Telefon poziomo (mało wysokości): bez logo i podpisów, żeby zmieściło się 5 zakładek
                            val compactRail = androidx.compose.ui.platform.LocalConfiguration.current.screenHeightDp < 520
                            NavigationRail(containerColor = c.paper, modifier = Modifier.fillMaxHeight()) {
                                if (compactRail) Spacer(Modifier.statusBarsPadding().size(4.dp))
                                else {
                                    Spacer(Modifier.statusBarsPadding().size(16.dp))
                                    // Logo prowadzi na pulpit (ekran główny) z każdego miejsca
                                    Icon(androidx.compose.ui.res.painterResource(R.drawable.opentakeoff_mark), loc("OpenTakeoff — pulpit", "OpenTakeoff — dashboard"), tint = androidx.compose.ui.graphics.Color.Unspecified,
                                        modifier = Modifier.size(40.dp).clip(androidx.compose.foundation.shape.CircleShape).clickable { nav.goTab("home") }.padding(6.dp))
                                    Spacer(Modifier.size(24.dp))
                                }
                                tabs.forEach { t ->
                                    val sel = activeTab == t.route
                                    NavigationRailItem(
                                        selected = sel,
                                        onClick = { nav.goTab(t.route) },
                                        icon = { tabIcon(t, sel) },
                                        label = if (compactRail) null else ({ Text(t.label) }),
                                        alwaysShowLabel = !compactRail,
                                        colors = NavigationRailItemDefaults.colors(
                                            indicatorColor = c.ink, selectedIconColor = c.paper, selectedTextColor = c.ink,
                                            unselectedIconColor = c.ink2, unselectedTextColor = c.ink2,
                                        ),
                                    )
                                }
                                Spacer(Modifier.weight(1f))
                                androidx.compose.material3.IconButton({ railHidden = true }) {
                                    Icon(Icons.AutoMirrored.Outlined.MenuOpen, loc("Schowaj pasek zakładek", "Hide the tab bar"), tint = c.ink2)
                                }
                                Spacer(Modifier.size(8.dp))
                            }
                            Box(Modifier.width(1.dp).fillMaxHeight().background(c.rule))
                        }
                        val liveState by vm.flightState.collectAsStateWithLifecycle()
                        // Połączony symulator: ekran nie wygasa (stery, przyrządy); po rozłączeniu znów normalnie
                        val rootView = androidx.compose.ui.platform.LocalView.current
                        val simActive = liveState != null
                        androidx.compose.runtime.DisposableEffect(simActive) {
                            rootView.keepScreenOn = simActive
                            onDispose { rootView.keepScreenOn = false }
                        }
                        Box(Modifier.weight(1f).fillMaxHeight(), contentAlignment = Alignment.TopCenter) {
                            // Pasek aktywnego lotu na pozostałych zakładkach (zajmuje miejsce paska stanu, ekran pod nim nie dodaje odstępu)
                            val showStrip = liveState != null && activeTab != "flight" && route in tabs.map { it.route }
                            androidx.compose.foundation.layout.Column(Modifier.fillMaxSize()) {
                            if (showStrip) com.freemetar.app.ui.flight.LiveFlightStrip(liveState!!, onClick = { nav.goTab("flight") },
                                Modifier.statusBarsPadding().padding(start = if (wide && railHidden) 44.dp else 0.dp))
                            Box(
                                (if (showStrip) Modifier.consumeWindowInsets(WindowInsets.statusBars) else Modifier).weight(1f).fillMaxWidth(),
                                contentAlignment = Alignment.TopCenter,
                            ) {
                            // Ekran powitalny przy pierwszym uruchomieniu
                            // Dopiero po wczytaniu ustawień — inaczej ekran mignąłby przy każdym starcie i zresetował nawigację
                            val settingsLoaded by vm.settingsLoaded.collectAsStateWithLifecycle()
                            if (settingsLoaded && !settings.welcomeShown) {
                                WelcomeScreen(onGetStarted = { vm.setWelcomeShown() })
                            } else {
                            // Pływające stery nad wszystkimi zakładkami (rysowane po NavHost, więc na wierzchu)
                            val floating: @androidx.compose.runtime.Composable () -> Unit = {
                                // Na ekranie Lot kokpit ma własny przycisk — tu tylko na pozostałych zakładkach
                                if (settings.floatingControl && liveState != null && settings.controlMode != "GAMEPAD" && activeTab != "flight")
                                    com.freemetar.app.ui.flight.FloatingControl(vm.link, settings.controlMode, liveState, withTrim = settings.floatingStyle == "TRIM")
                            }
                            NavHost(
                                nav, startDestination = "home",
                                modifier = Modifier.widthIn(max = 1280.dp),
                                enterTransition = { fadeIn() + slideInHorizontally { it / 14 } },
                                exitTransition = { fadeOut() },
                                popEnterTransition = { fadeIn() },
                                popExitTransition = { fadeOut() + slideOutHorizontally { it / 14 } },
                            ) {
                                composable("home") {
                                    HomeScreen(vm, onFlight = { nav.goTab("flight") }) { ident ->
                                        vm.open(ident)
                                        nav.navigate("airport/$ident")
                                    }
                                }
                                composable("airport/{ident}") { backStack ->
                                    // Po powrocie ze stosu (A → B → wstecz) przywróć briefing A
                                    val ident = backStack.arguments?.getString("ident").orEmpty()
                                    LaunchedEffect(ident) {
                                        val cur = vm.detail.value
                                        if (cur == null || (cur.ident != ident.uppercase() && cur.airport?.iata != ident.uppercase())) vm.open(ident)
                                    }
                                    BriefingScreen(vm, onBack = { nav.popBackStack() }, onOpen = { id ->
                                        vm.open(id)
                                        nav.navigate("airport/$id")
                                    })
                                }
                                composable("map") {
                                    MapScreen(vm) { ident ->
                                        vm.open(ident)
                                        nav.navigate("airport/$ident")
                                    }
                                }
                                composable("flight") {
                                    FlightScreen(
                                        vm,
                                        onOpenLogbook = { nav.goTab("logbook") },
                                        onOpenChecklists = { nav.navigate("checklists") },
                                    )
                                }
                                composable("logbook") {
                                    com.freemetar.app.ui.flight.LogbookScreen(vm, onBack = { nav.goTab("flight") }, onOpenFlight = { id -> nav.navigate("flightlog/$id") })
                                }
                                composable("flightlog/{id}") { backStack ->
                                    FlightDetailScreen(vm, backStack.arguments?.getString("id").orEmpty(), onBack = { nav.popBackStack() }, onShowMap = { nav.goTab("map") })
                                }
                                composable("tools") { ToolsScreen(vm) }
                                composable("settings") { SettingsScreen(vm, onOpenCallouts = { nav.navigate("settings/callouts") }) }
                                composable("settings/callouts") {
                                    com.freemetar.app.ui.screens.CalloutsSettingsScreen(
                                        enabled = vm.calloutsEnabled.collectAsState().value,
                                        enabledCallouts = vm.enabledCallouts.collectAsState().value,
                                        language = vm.calloutsLanguage.collectAsState().value,
                                        speechRate = vm.calloutsSpeechRate.collectAsState().value,
                                        onEnabledChange = { vm.setCalloutsEnabled(it) },
                                        onCalloutToggle = { id, enabled -> vm.toggleCallout(id, enabled) },
                                        onLanguageChange = { vm.setCalloutsLanguage(it) },
                                        onSpeechRateChange = { vm.setCalloutsSpeechRate(it) },
                                        onBack = { nav.popBackStack() }
                                    )
                                }
                                composable("checklists") {
                                    // Checklisty dla samolotu z symulatora (tytuł z MSFS/X-Plane)
                                    val title = liveState?.aircraft?.title
                                    LaunchedEffect(title) { vm.loadChecklistsForAircraft(title) }
                                    com.freemetar.app.ui.screens.ChecklistsScreen(
                                        aircraftChecklists = vm.currentChecklists.collectAsState().value,
                                        onBack = { nav.popBackStack() },
                                        available = vm.checklistAircraft,
                                        onPick = { vm.pickChecklists(it) },
                                    )
                                }
                            }
                            floating()
                            }
                            }
                            }
                            // Schowany pasek: mały przycisk w lewym górnym rogu przywraca zakładki
                            if (wide && railHidden) androidx.compose.material3.IconButton(
                                { railHidden = false },
                                Modifier.align(Alignment.TopStart).statusBarsPadding().padding(2.dp).size(40.dp)
                                    .clip(androidx.compose.foundation.shape.CircleShape).background(c.paper.copy(alpha = 0.85f)),
                            ) { Icon(Icons.Outlined.Menu, loc("Pokaż pasek zakładek", "Show the tab bar"), tint = c.ink) }
                        }
                    }
                }
            }
            }
        }
    }
}
