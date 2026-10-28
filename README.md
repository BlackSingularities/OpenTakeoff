# OpenTakeoff

**Projekt i wykonanie: [redzikowski.com](https://redzikowski.com)**

[Polski](#polski) · [English](#english)

---

## Polski

Aplikacja na Androida dla wirtualnych pilotów (MSFS, X-Plane, VATSIM, IVAO). Pogoda, dane lotnisk, plan lotu i kokpit na telefonie. Korzysta wyłącznie z darmowych, otwartych źródeł danych: nie wymaga kont ani kluczy API i nie ma reklam. Interfejs jest po polsku i angielsku (zgodnie z językiem systemu albo do wyboru w ustawieniach).

Do sterowania samolotem i danych z symulatora służy **OpenTakeoff Link**, mały program na Windows, który łączy telefon z MSFS 2024 w sieci lokalnej. Instrukcja: [docs/link-setup.md](docs/link-setup.md).

### Funkcje

**Ekran główny**
- „W pobliżu”: lotniska najbliżej samolotu, gdy symulator jest połączony, a bez niego najbliżej telefonu; z bieżącą pogodą.

**Briefing lotniska**
- pogoda teraz i interaktywny METAR (dotknięcie grupy pokazuje jej znaczenie);
- wcześniejsze depesze z godziną i kategorią lotu w jednej linii;
- TAF w okresach z kategorią lotu, SIGMET i PIREP, wiatr na wysokościach;
- pasy ze składowymi wiatru, częstotliwości i ILS, okolica;
- ATIS czytany na głos z bieżącego METAR-u;
- charty FAA dla lotnisk w USA z przeglądarką PDF.

**Mapa**
- lotniska w kolorach kategorii lotu, obszary SIGMET, aktywna trasa z procedurami;
- **widok 3D**: pochylenie i obrót mapy, a przy śledzeniu kamera patrzy w kierunku lotu;
- **ślad lotu z wysokością**: linia zmienia kolor wraz z wysokością, na śladzie są etykiety wysokości (stopy albo FL), a w 3D ślad to cienka linia zawieszona na wysokości lotu (przewyższonej 3×, żeby profil był czytelny), z samolotem na jej końcu;
- bieżąca wysokość przy symbolu samolotu, budynki 3D przy dużym przybliżeniu;
- przyciski mapy w jednej grupie przy prawej krawędzi (warstwy, śledzenie samolotu, przybliżenie) i kompas pod paskiem stanu; w menu warstw METAR, SIGMET, bieżący odcinek planu lotu z GPS symulatora oraz lotniska, VOR, NDB i punkty z bazy MSFS wokół samolotu;
- każdy zapisany lot można otworzyć na mapie 3D z dziennika lotów.

**Plan lotu** — prosto z symulatora: następny punkt, odległość, czasy i bieżący odcinek z GPS MSFS (bez planowania tras w aplikacji).

**Kokpit (z OpenTakeoff Link)**
- **jeden widok do latania**: PFD, ND, dźwignie mocy i pływający przycisk sterowania na ekranie naraz, bez przewijania (telefon pionowo: PFD nad ND, dźwignie z boku; poziomo: wszystko w rzędzie; tablet: obok stała kolumna albo pod spodem stały panel autopilota, radia itd.). Tryb i zasięg ND oraz warstwy to małe przyciski EFIS na samym wyświetlaczu, a przycisk rozglądania się jest w nagłówku. Autopilot, radio, systemy, silniki i postęp otwierają się z paska ikon w miejscu ND, więc PFD i dźwignie zostają na widoku;
- **PFD**: horyzont, taśmy prędkości i wysokości z nastawami, wskaźnik prędkości pionowej ze skalą 1–2–6 tys. ft/min, wektor toru lotu (znos i kąt toru), trend prędkości na 10 s, Mach, QNH, wysokość nad terenem z pasem ziemi, znacznik toru, wiatr i tryby autopilota (FMA);
- **MFD/ND** w trybach ARC i ROSE: trasa, nastawa kursu, następny punkt z odległością, namiarem i czasem, a z bazy MSFS lotniska, VOR (z częstotliwością), NDB i punkty trasy wokół samolotu (warstwy do włączenia); pod mapą profil pionowy (VSD) z wysokością wybraną i torem przy bieżącej prędkości pionowej;
- **silniki**: wyświetlacz EICAS/ECAM (N1 albo obroty, EGT, przepływ i zapas paliwa) oraz pulpit dźwigni przeciąganych palcem — moc, a w tłokowych i turbośmigłowych także mieszanka i śmigło; odrzutowce mają zapadki IDLE/CL/FLX/TOGA, TO/GA i odwracacz ciągu;
- **rozglądanie się w MSFS**: przytrzymaj przycisk i przechylaj telefon albo przesuwaj palec — w kabinie obraca się głowa pilota, w pozostałych widokach kamera krąży wokół samolotu; puszczenie wraca do poprzedniego widoku, a krótkie dotknięcie wyjaśnia, jak to działa;
- **autopilot z pokrętłami** jak w awionice: podwójne pokrętło (zewnętrzny pierścień zmienia o duży krok, środek o mały), przyciski −/+ z powtarzaniem i okienko, w które można wpisać wartość. Pod każdą nastawą są jej przyciski trybów. Panel wygląda jak u producenta — Airbus FCU (bursztynowe okienka, zielone kreski lampek), Boeing MCP (kwadratowe klawisze z lampką), Garmin (cyjanowe wartości), GA w stylu KAP140 — i dopasowuje się do motywu aplikacji. Profil wybiera się automatycznie albo ręcznie;
- radio (COM, NAV, transponder) i systemy (podwozie, klapy, spoilery, trym, światła).

**Sterowanie samolotem**
- **żyroskop** (domyślnie): trzymasz telefon jak wolant; steruje tylko podczas przytrzymania, a puszczenie od razu oddaje stery;
- drążek pod kciukiem albo fizyczny kontroler lub joystick (Bluetooth, USB-C);
- **pływający przycisk sterowania**: widoczny na każdej zakładce, więc można przeglądać briefing czy mapę i mieć stery pod ręką. Przytrzymaj go i przechylaj telefon (żyroskop) albo przesuwaj kciuk (drążek). Da się go przesunąć za uchwyt. Wariant **z trymem**: przy żyroskopie trymujesz przeciągnięciem palca na przycisku (w dół — nos w górę, w górę — nos w dół), przy drążku trym ma osobne przyciski.

**Dziennik lotów**
- zapis można zakończyć ręcznie wskaźnikiem nagrywania w nagłówku kokpitu (kolejny zacznie się po wylądowaniu albo po wznowieniu);
- osobny ekran z podsumowaniem, filtrem i lotami pogrupowanymi po miesiącach; przy każdym trasa, czas, dystans, wysokość, ocena lądowania i miniatura profilu;
- automatyczny zapis każdego lotu: pozycja, wysokość, prędkość względem ziemi i przyrządowa, prędkość pionowa, kurs; **restart lotu w symulatorze** (skok pozycji albo inny samolot) zamyka bieżący wpis i zaczyna nowy; lot zapisuje się także, gdy symulator przestanie wysyłać dane albo system zamknie aplikację w trakcie lotu;
- częstotliwość zapisu do wyboru w ustawieniach: 4×, 2× lub 1× na sekundę, co 2 s albo co 5 s;
- kompaktowe archiwum: tor jest zapisywany jako różnice między punktami i pakowany gzipem, więc zajmuje ułamek zwykłego JSON-a;
- eksport i import całej historii w jednym pliku `.oab` (kopia zapasowa, przeniesienie na inny telefon) oraz eksport pojedynczego lotu do GPX;
- wykresy wysokości, prędkości i prędkości pionowej oraz ocena lądowania (prędkość pionowa, przeciążenie, położenie względem osi i progu pasa, odbicia).

**Pozostałe**
- narzędzia: kalkulator wiatru, wysokość gęstościowa, zniżanie, paliwo, jednostki, dekoder METAR i TAF, ściąga radiowa;
- motywy AMOLED (czysta czerń), grafitowy i jasny; układy na telefon pionowo i poziomo oraz tablet;
- dotknięcie logo wraca na ekran główny;
- automatyczne aktualizacje z GitHuba.

### Źródła danych

| Dane | Źródło | Licencja |
|---|---|---|
| METAR, TAF, SIGMET, PIREP | [aviationweather.gov Data API](https://aviationweather.gov/data/api/) | domena publiczna (NOAA) |
| Lotniska, pasy, częstotliwości, pomoce nawigacyjne | [OurAirports](https://ourairports.com/data/) | domena publiczna |
| Wiatr i temperatura na wysokościach | [Open-Meteo](https://open-meteo.com/) | CC BY 4.0 |
| Podkład mapy i budynki | [OpenFreeMap](https://openfreemap.org/) / OpenStreetMap | ODbL |
| Charty (USA) | [FAA d-TPP](https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/dtpp/) | domena publiczna |
| Procedury, ILS, lotniska i pomoce na ND, stan lotu | baza i SimConnect Twojego MSFS przez OpenTakeoff Link | — |

Baza OurAirports pobiera się raz przy pierwszym uruchomieniu i odświeża co 30 dni. Historia lotów i trasy są przechowywane tylko na telefonie.

> Tylko do symulacji lotów. Nie używaj do rzeczywistej nawigacji.

### Budowanie

Kotlin, Jetpack Compose i Material 3, minSdk 26. OpenTakeoff Link: C# i .NET 8 (folder `bridge/`).

Lokalnie skrypt `tools/build-local.ps1` buduje APK i Link oraz uruchamia testy:

```bash
powershell -ExecutionPolicy Bypass -File tools/build-local.ps1
```

### Wydania

Każdy wypchnięty tag `vX.Y.Z` uruchamia [workflow](.github/workflows/release.yml), który tylko buduje APK i `OpenTakeoff-Link.exe`. Wydanie powstaje po wypchnięciu tagu `vX.Y.Z` (np. `git tag v2.0.1 && git push origin v2.0.1`): workflow buduje obie aplikacje w wersji `X.Y.Z` i publikuje je w wydaniu o tej nazwie. Człony `Y` i `Z` muszą być mniejsze niż 100.

Bez sekretów APK jest podpisane kluczem debug. Żeby aktualizacje instalowały się na stałym kluczu, dodaj sekrety repozytorium: `SIGNING_KEYSTORE_BASE64`, `SIGNING_STORE_PASSWORD`, `SIGNING_KEY_ALIAS`, `SIGNING_KEY_PASSWORD`.

---

## English

An Android app for flight-sim pilots (MSFS, X-Plane, VATSIM, IVAO): weather, airport data, flight planning and a cockpit on your phone. It uses free and open data sources only, with no accounts, API keys or ads. The interface is in English and Polish (follows the system language or can be set in Settings).

To fly the aircraft and read live data from the simulator, run **OpenTakeoff Link**, a small Windows program that connects the phone to MSFS 2024 over your local network. Setup: [docs/link-setup.md](docs/link-setup.md) (in Polish).

### Features

**Home screen**
- “Nearby”: the airports closest to the aircraft while the simulator is connected, otherwise closest to the phone, with current weather.

**Airport briefing**
- current weather and an interactive METAR (tap a group to see what it means);
- earlier reports with time and flight category on one line;
- TAF split into periods with flight category, SIGMETs and PIREPs, winds aloft;
- runways with wind components, frequencies and ILS, nearby airports;
- ATIS read aloud from the latest METAR;
- FAA charts for US airports with a PDF viewer.

**Map**
- airports coloured by flight category, SIGMET areas, the active route with procedures;
- **3D view**: tilt and rotate the map; while following the aircraft the camera looks along the flight path;
- **flight track with altitude**: the line changes colour with altitude and carries altitude labels (feet or FL). In 3D the track is a thin line hanging at flight altitude (exaggerated 3× so the profile is readable) with the aircraft at its end;
- current altitude next to the aircraft symbol, 3D buildings when zoomed in;
- map buttons in one group along the right edge (layers, follow aircraft, zoom) and the compass below the status bar; the layers menu has METAR, SIGMET, the current flight-plan leg from the simulator GPS, and airports, VORs, NDBs and waypoints from the MSFS database around the aircraft;
- any saved flight can be opened on the 3D map from the logbook.

**Flight plan** — straight from the simulator: next waypoint, distance, times and the current leg from the MSFS GPS (no route planning in the app).

**Cockpit (with OpenTakeoff Link)**
- **one view for flying**: PFD, ND, thrust levers and the floating control button on screen at once, without scrolling (phone in portrait: PFD above ND, levers at the side; landscape: all in one row; tablet: a fixed column or a fixed panel below for the autopilot, radios and more). ND mode, range and layers are small EFIS buttons on the display itself, and the look-around button sits in the header. Autopilot, radios, systems, engines and progress open from an icon bar in place of the ND, so the PFD and levers stay in view;
- **PFD**: horizon, speed and altitude tapes with selected values, a vertical speed indicator with a 1–2–6 thousand ft/min scale, flight path vector (drift and flight path angle), 10-second speed trend, Mach, QNH, height above terrain with a ground band, track marker, wind and autopilot modes (FMA);
- **MFD/ND** in ARC and ROSE modes: route, selected heading, next waypoint with distance, bearing and time, and from the MSFS database the airports, VORs (with frequency), NDBs and waypoints around the aircraft (layers can be toggled); below it a vertical situation display (VSD) with the selected altitude and the path at the current vertical speed;
- **engines**: an EICAS/ECAM display (N1 or RPM, EGT, fuel flow and fuel on board) and a throttle quadrant you drag with your finger — thrust, plus mixture and propeller on piston and turboprop aircraft; jets get IDLE/CL/FLX/TOGA detents, TO/GA and reverse thrust;
- **look around in MSFS**: hold the button and tilt the phone or slide your finger — in the cockpit the pilot's head turns, in other views the camera circles the aircraft; let go to return to the previous view, tap it for a short explanation;
- **autopilot with knobs** like real avionics: a dual knob (the outer ring moves in big steps, the centre in small ones), −/+ buttons that repeat when held, and a window you can tap to type a value. Each setting has its mode buttons right below it. The panel looks like the manufacturer's — Airbus FCU (amber windows, green lamp bars), Boeing MCP (square keys with a lamp), Garmin (cyan values), GA in KAP140 style — and adapts to the app theme. The profile is picked automatically or by hand;
- radios (COM, NAV, transponder) and systems (gear, flaps, spoilers, trim, lights).

**Flying the aircraft**
- **gyro** (default): hold the phone like a yoke; it only flies while you hold the pad, and letting go hands control back at once;
- an on-screen thumb stick, or a physical controller or joystick (Bluetooth, USB-C);
- **floating control button**: shown on every tab, so you can read the briefing or the map with the controls under your thumb. Hold it and tilt the phone (gyro) or move your thumb (stick). Drag it by the handle to move it. The **with trim** variant: with the gyro you trim by dragging your finger on the button (down — nose up, up — nose down); with the stick, trim has its own buttons.

**Logbook**
- recording can be stopped by hand from the recording indicator in the cockpit header (the next one starts after landing or when you resume it);
- its own screen with a summary, a filter and flights grouped by month; each shows the route, time, distance, altitude, landing score and a profile thumbnail;
- every flight is recorded automatically: position, altitude, ground and indicated speed, vertical speed, heading; **restarting the flight in the simulator** (a position jump or another aircraft) closes the current entry and starts a new one; a flight is also saved when the simulator stops sending data or the system closes the app mid-flight;
- recording rate is set in Settings: 4, 2 or 1 times per second, every 2 s or every 5 s;
- compact archive: the track is stored as differences between points and gzip-compressed, a fraction of the size of plain JSON;
- export and import of the whole history as a single `.oab` file (backup or moving to another phone), plus GPX export of a single flight;
- altitude, speed and vertical speed charts, and a landing score (vertical speed, g-force, position relative to the runway centreline and threshold, bounces).

**Also**
- tools: wind calculator, density altitude, descent, fuel, unit converter, METAR and TAF decoder, radio phraseology;
- AMOLED (true black), graphite and bright themes; layouts for phones in portrait and landscape and for tablets;
- tapping the logo takes you back to the home screen;
- automatic updates from GitHub.

### Data sources

| Data | Source | Licence |
|---|---|---|
| METAR, TAF, SIGMET, PIREP | [aviationweather.gov Data API](https://aviationweather.gov/data/api/) | public domain (NOAA) |
| Airports, runways, frequencies, navaids | [OurAirports](https://ourairports.com/data/) | public domain |
| Winds and temperatures aloft | [Open-Meteo](https://open-meteo.com/) | CC BY 4.0 |
| Base map and buildings | [OpenFreeMap](https://openfreemap.org/) / OpenStreetMap | ODbL |
| Charts (USA) | [FAA d-TPP](https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/dtpp/) | public domain |
| Procedures, ILS, airports and navaids on the ND, live flight data | your MSFS database and SimConnect via OpenTakeoff Link | — |

The OurAirports database downloads once on first launch and refreshes every 30 days. Flight history and routes stay on the phone.

> For flight simulation only. Do not use for real-world navigation.

### Building

Kotlin, Jetpack Compose and Material 3, minSdk 26. OpenTakeoff Link: C# and .NET 8 (in `bridge/`).

Locally, `tools/build-local.ps1` builds the APK and Link and runs the tests:

```bash
powershell -ExecutionPolicy Bypass -File tools/build-local.ps1
```

### Releases

Every push to `main` runs the [workflow](.github/workflows/release.yml), which only builds the APK and `OpenTakeoff-Link.exe`. A release is created by pushing a `vX.Y.Z` tag (e.g. `git tag v2.0.1 && git push origin v2.0.1`): the workflow builds both apps as version `X.Y.Z` and publishes them under that release. `Y` and `Z` must be below 100.

Without secrets the APK is signed with the debug key. To keep updates on a fixed key, add these repository secrets: `SIGNING_KEYSTORE_BASE64`, `SIGNING_STORE_PASSWORD`, `SIGNING_KEY_ALIAS`, `SIGNING_KEY_PASSWORD`.

---

<a href="https://buycoffee.to/adamredzikowski" target="_blank"><img src="https://buycoffee.to/static/img/share/share-button-dark--pl.png" style="width: 351px; height: 92px" width="351" height="92" alt="Postaw kawę dla adamredzikowski na buycoffee.to"></a>
