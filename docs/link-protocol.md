# OpenTakeoff Link — protokół v1

Protokół między programem **OpenTakeoff Link** (Windows, most do MSFS 2024 przez SimConnect)
a aplikacją **OpenTakeoff** (Android) w sieci lokalnej.

| Kanał | Port | Kierunek | Format |
|---|---|---|---|
| Odkrywanie | UDP **47811**, broadcast | PC → sieć, co 2 s | JSON (jeden datagram UTF-8) |
| Sesja | TCP **47810**, WebSocket (`ws://<ip>:47810/`) | dwustronnie | ramki tekstowe, jeden obiekt JSON na ramkę |

Wersja protokołu: **`v: 1`**. Każda wiadomość (w obu kierunkach i w broadcaście) ma pola:

```json
{ "v": 1, "type": "<typ>", ... }
```

Zasady ogólne:

- Nazwy pól w `camelCase`. Nieznane pola należy ignorować (dodawanie pól nie zmienia wersji).
  Zmiana znaczenia lub usunięcie pola = nowa wersja `v`.
- Serwer odrzuca wiadomości z innym `v` błędem `unsupported_version` (sesja zostaje otwarta).
- Maksymalny rozmiar wiadomości od klienta: 16 KiB. Większe zamykają połączenie (kod 1009).
- Jednostki są w nazwach pól (`Ft`, `Kt`, `Fpm`, `Deg`, `Kg`, `C`, `Hz`, `Mhz`, `Pct`).
  Wartości procentowe są w zakresie 0–100 (trym −100…100).
- Kąty: kurs/namiar 0–360, `pitchDeg` dodatni = nos w górę, `bankDeg` dodatni = przechylenie w prawo.
- Brak danych (np. brak silnika) = pole pominięte lub `null`, nigdy wartość zmyślona.

## 1. Odkrywanie (UDP 47811)

Program co 2 s wysyła broadcast (`255.255.255.255` oraz adresy rozgłoszeniowe każdej karty):

```json
{
  "v": 1,
  "type": "announce",
  "name": "OpenTakeoff Link",
  "version": "1.0.12",
  "host": "PC-ADAMA",
  "port": 47810,
  "ips": ["192.168.1.20"],
  "source": "msfs"
}
```

`source`: `msfs` (połączony z MSFS), `simulation` (tryb symulacji), `none` (czeka na MSFS).
Telefon nasłuchuje na UDP 47811, a adres serwera bierze z adresu nadawcy datagramu
(lub z `ips`, gdy nadawca jest nieosiągalny). Ręczne wpisanie IP też jest dozwolone.

## 2. Parowanie i sesja

1. Telefon otwiera `ws://<ip>:47810/`.
2. W ciągu 10 s wysyła `hello`. Bez tego serwer zamyka połączenie.
3. Pierwsze parowanie — z 6-cyfrowym kodem widocznym w oknie programu:

```json
{ "v": 1, "type": "hello", "id": "1", "code": "482913",
  "client": { "name": "Pixel 8", "app": "OpenTakeoff", "version": "2.0.0" },
  "rateHz": 10 }
```

   Kolejne połączenia — z tokenem z poprzedniej sesji (bez kodu):

```json
{ "v": 1, "type": "hello", "id": "1", "token": "Qm9n...", "client": { "name": "Pixel 8" } }
```

4. Odpowiedź po sukcesie:

```json
{ "v": 1, "type": "hello_ok", "id": "1", "token": "Qm9n...", "server": "OpenTakeoff Link",
  "version": "1.0.12", "source": "msfs", "rateHz": 10,
  "limits": { "maxDeflection": 0.8, "axisTimeoutMs": 500, "navdataPerSecond": 10 },
  "features": ["navdata", "touchdown", "aircraft_category"] }
```

   `features` (od Link 1.1) wymienia funkcje dodane do protokołu v1 po pierwszym wydaniu.
   Brak pola = starszy Link bez `navdata_request`, `touchdown` i pól profilu samolotu.

   Telefon zapisuje `token` i używa go przy kolejnych połączeniach. Token jest ważny,
   dopóki użytkownik nie kliknie w programie „Unieważnij sparowania”.

5. Błąd: `error` z kodem `pairing_failed` (zły kod / nieznany token). Po 5 nieudanych próbach
   z jednego adresu IP serwer blokuje go na 60 s (`rate_limited`).

`rateHz` — żądana częstotliwość `state`, 1–20 (domyślnie 10). Serwer przycina do zakresu.
Kod parowania można odświeżyć w programie; zmiana kodu nie unieważnia wydanych tokenów.

Przed `hello_ok` serwer odpowiada na każdą inną wiadomość błędem `not_paired`
(wyjątek: `ping`).

## 3. Wiadomości serwer → telefon

### `state` (10–20 Hz)

Wysyłany tylko, gdy jest źródło danych (MSFS lub symulacja). `seq` rośnie o 1, `t` to czas
serwera w ms od epoki Unix.

```json
{
  "v": 1, "type": "state", "seq": 1234, "t": 1792231234567,
  "data": {
    "source": "msfs",
    "aircraft": { "title": "Airbus A320neo", "onGround": false, "paused": false,
                  "engineType": "jet", "atcModel": "A20N", "atcType": "AIRBUS", "category": "airliner" },
    "simTimeUtc": "2026-10-17T12:34:56Z",
    "position": { "lat": 52.1657, "lon": 20.9671, "altMslFt": 12000, "altAglFt": 11630, "altIndFt": 12010 },
    "speed": { "iasKt": 250, "tasKt": 298, "gsKt": 310, "mach": 0.46, "vsFpm": 1800 },
    "attitude": { "pitchDeg": 5.2, "bankDeg": -3.0, "hdgTrueDeg": 331.0, "hdgMagDeg": 325.6, "trackTrueDeg": 333.1 },
    "autopilot": {
      "master": true, "fd": true, "athr": true,
      "hdgDeg": 330, "altFt": 24000, "vsFpm": 1800, "spdKt": 250,
      "modes": { "hdg": true, "nav": false, "apr": false, "loc": false, "alt": false, "vs": false, "flc": true }
    },
    "radios": {
      "com1": { "activeMhz": 121.5, "standbyMhz": 118.3 },
      "com2": { "activeMhz": 121.5, "standbyMhz": 122.8 },
      "nav1": { "activeMhz": 110.3, "standbyMhz": 113.2 },
      "nav2": { "activeMhz": 110.3, "standbyMhz": 113.2 },
      "transponder": { "code": "7000", "mode": "alt", "ident": false }
    },
    "controls": {
      "gearDown": true, "gearPct": 100, "flapsIndex": 1, "flapsPositions": 5, "flapsPct": 25,
      "spoilersPct": 0, "spoilersArmed": true, "parkingBrake": false, "elevatorTrimPct": 2.5,
      "throttlePct": 85, "mixturePct": 100, "propPct": 100, "cameraState": 2,
      "cameraPitchDeg": 0, "cameraYawDeg": 0
    },
    "lights": { "beacon": true, "strobe": true, "nav": true, "landing": true, "taxi": false,
                "logo": true, "wing": false, "panel": true, "cabin": true },
    "fuel": { "totalKg": 5400, "tanks": { "leftKg": 2500, "centerKg": 400, "rightKg": 2500 } },
    "engines": [ { "n1Pct": 85.1, "n2Pct": 93.0, "rpm": 0, "egtC": 620, "fuelFlowKgH": 1250 } ],
    "environment": { "windDirDeg": 270, "windSpdKt": 25, "oatC": -9.5, "qnhHpa": 1013.2 },
    "axes": { "active": false },
    "touchdown": { "seq": 3, "t": 1792231200123, "vsFpm": -182, "gForce": 1.21, "pitchDeg": 3.4,
                   "bankDeg": -0.6, "headingTrueDeg": 291.8, "lat": 54.378512, "lon": 18.481730,
                   "iasKt": 136.5, "gsKt": 131.0 }
  }
}
```

- `transponder.mode`: `off`, `standby`, `test`, `on`, `alt`, `ground`.
- `transponder.code`: zawsze 4 cyfry ósemkowe jako tekst.
- `engines`: tyle elementów, ile silników ma samolot (maks. 4). `rpm` dla tłokowych/śmigłowych,
  `n1Pct`/`n2Pct` dla turbinowych (u innych 0).
- `axes.active`: `true`, gdy serwer właśnie steruje osiami z telefonu.
- `aircraft.engineType`: `piston`, `jet`, `turboprop`, `helicopter`, `none` (SimVar `ENGINE TYPE`;
  `none` także dla szybowców i nieobsługiwanych napędów).
- `aircraft.atcModel` / `aircraft.atcType`: `ATC MODEL` / `ATC TYPE` z MSFS; z kluczy lokalizacji
  (`TT:ATCCOM.AC_MODEL_A320.0.text`) zostaje sama nazwa (`A320`). Pusty tekst, gdy MSFS nie podaje.
- `aircraft.category`: `airliner`, `bizjet`, `turboprop`, `ga`, `helicopter` — heurystyka do wyboru
  profilu paneli: śmigłowiec po typie napędu/kategorii MSFS; odrzutowiec = `airliner` przy nazwie
  liniowca w tytule, ≥ 3 silnikach lub MTOW ≥ 100 000 lb, inaczej `bizjet`; turbośmigłowy = `airliner`
  (ATR, Dash 8…) przy MTOW ≥ 35 000 lb, inaczej `turboprop`; tłokowy i reszta = `ga`.
- `touchdown` — **zatrzaśnięte ostatnie przyziemienie**. Pole pojawia się po pierwszym lądowaniu
  od uruchomienia Link (wcześniej go nie ma) i zostaje do następnego. `seq` rośnie przy każdym
  przyziemieniu, także przy odbiciu (kontakt po ≥ 250 ms w powietrzu), więc telefon wykrywa nowe
  lądowanie i odbicia po zmianie `seq`. `t` — czas kontaktu (ms od epoki Unix).
  `vsFpm` (ujemne = opadanie), `pitchDeg`, `bankDeg`, `lat`, `lon` z SimVarów
  `PLANE TOUCHDOWN NORMAL VELOCITY / PITCH DEGREES / BANK DEGREES / LATITUDE / LONGITUDE`
  (gdy MSFS ich nie odświeży — z ostatniej klatki w powietrzu); `gForce` to szczyt `G FORCE`
  od ostatniej klatki w powietrzu do 0,6 s po kontakcie; `headingTrueDeg`, `iasKt`, `gsKt` — z chwili
  kontaktu. Wartości mogą się jeszcze zmienić w ciągu 0,6 s po kontakcie (ten sam `seq`).
  Tryb symulacji: przyziemienie w EPGD na pasie 29, −180 ft/min, 1,2 g.
- `avionics` — dane dla wyświetlaczy PFD/MFD (MSFS; brak pola = starszy Link albo tryb symulacji).
  Wszystkie kursy magnetyczne, prędkości w kt, wysokości w ft:

  ```json
  "avionics": {
    "baroHpa": 1013.2, "baroStd": false, "pressureAltFt": 3480, "radioAltFt": 2950, "decisionHeightFt": 200,
    "slipBall": 0.05, "turnRateDps": 1.2, "gForce": 1.02, "aoaDeg": 3.1,
    "attIndPitchDeg": 2.4, "attIndBankDeg": -1.0, "hdgIndDeg": 287.5, "suctionInHg": 5.0,
    "magVarDeg": 6.1, "tatC": 9.5,
    "speeds": { "vs0Kt": 40, "vs1Kt": 48, "vcKt": 129, "vmoKt": 163 },
    "stallWarning": false, "overspeedWarning": false, "marker": "none",
    "nav1": { "hasNav": true, "hasLoc": true, "hasGs": true, "hasDme": true, "ident": "IGD",
              "obsDeg": 292, "locCourseDeg": 292, "radialDeg": 112, "cdiDots": -0.4, "gsDots": 0.3,
              "dmeNm": 8.4, "toFrom": "to" },
    "nav2": { "hasNav": false, "...": "jak nav1, bez GS" },
    "adf": { "hasSignal": false, "relBearingDeg": 0 },
    "gps": { "activeFlightPlan": true, "approachActive": false, "drivesNav1": false, "nextId": "KABEL",
             "prevId": "SOXER", "nextDistNm": 12.4, "nextBearingMagDeg": 301, "desiredTrackMagDeg": 298,
             "crossTrackNm": 0.12, "nextEteSec": 260, "destEteSec": 2900,
             "nextLat": 54.41, "nextLon": 18.32, "prevLat": 54.12, "prevLon": 18.95, "wpIndex": 4, "wpCount": 9 },
    "fd": { "active": true, "pitchDeg": 2.5, "bankDeg": -8.0 },
    "ap": { "altArmed": true, "gsArmed": false, "gsActive": false, "backCourse": false, "wingLeveler": false,
            "pitchHold": false, "airspeedHold": true, "machHold": false, "toga": false, "yawDamper": true,
            "machVar": 0.78 },
    "brakeLeftPct": 0, "brakeRightPct": 0
  }
  ```

  - `slipBall` −1 (kulka w lewo) … 1 (w prawo) z `TURN COORDINATOR BALL`; `turnRateDps` + = w prawo.
  - `attInd*`, `hdgInd` — wskazania przyrządów żyroskopowych (przy awarii podciśnienia odbiegają
    od rzeczywistego położenia); telefon rysuje z nich zegary analogowe.
  - `speeds` z `DESIGN SPEED VS0/VS1/VC` i `AIRSPEED BARBER POLE` (0 = brak danych) — łuki i pasy prędkości.
  - `nav1/nav2.cdiDots` i `gsDots` w kropkach −2,5…2,5 (CDI ±127, GSI ±119 z MSFS): CDI + = kurs w prawo,
    GS + = ścieżka powyżej samolotu. `toFrom`: `to` / `from` / `off`.
  - `gps.*` z SimVarów `GPS WP *` (namiar i DTK przeliczone na magnetyczne przez `MAGVAR`);
    `crossTrackNm` + = samolot na prawo od kursu. `next*/prev*Lat/Lon` — punkty bieżącego odcinka
    (0/0 = brak), `wpIndex`/`wpCount` — numer następnego punktu i liczba punktów planu lotu w MSFS.
  - `fd` — polecenia dyrektora lotu (`AUTOPILOT FLIGHT DIRECTOR PITCH/BANK`): pochylenie + nos w górę,
    przechylenie + w prawo.
  - `ap` — tryby uzbrojone i dodatkowe (ALT ARM, G/S ARM/HOLD, BC, LVL, PITCH, SPD/MACH HOLD, TO/GA, YD).

### `status`

Wysyłany po `hello_ok` i przy każdej zmianie źródła danych:

```json
{ "v": 1, "type": "status", "source": "none", "simConnected": false, "message": "Czekam na MSFS" }
```

### `ack` / `error`

```json
{ "v": 1, "type": "ack", "id": "c42", "name": "AP_MASTER" }
{ "v": 1, "type": "error", "id": "c43", "code": "not_allowed", "message": "Komenda poza listą dozwolonych" }
```

`id` przepisywany z wiadomości klienta (może być `null`, gdy nie dało się go odczytać).

| `code` | Znaczenie |
|---|---|
| `bad_json` | Nie da się sparsować JSON lub brak `type` |
| `unsupported_version` | `v` różne od 1 |
| `unknown_type` | Nieznany `type` |
| `not_paired` | Wiadomość przed `hello_ok` |
| `pairing_failed` | Zły kod lub token |
| `rate_limited` | Za dużo prób parowania / komend (limit 50 komend/s) / zapytań `navdata_request` (10/s) |
| `not_allowed` | Komenda spoza listy dozwolonych |
| `invalid_value` | Brak wartości lub wartość poza zakresem |
| `sim_unavailable` | Brak MSFS i wyłączony tryb symulacji |
| `sim_error` | SimConnect odrzucił wywołanie |
| `not_found` | `navdata_request`: nie ma takiego lotniska, procedury, przejścia lub punktu |

### `pong`

```json
{ "v": 1, "type": "pong", "id": "p7", "t": 1792231234000, "serverT": 1792231234012 }
```

### `navdata`

Odpowiedź na `navdata_request` (opis zapytań w p. 4). `id` i `kind` przepisane z zapytania,
`data` zależy od `kind`. Pola opcjonalne mają w JSON jawne `null` (kształt jest stały).
Kursy `courseDeg` w odcinkach i `locCourseDeg` ILS są takie, jak w bazie MSFS (magnetyczne,
jak na mapach); `headingTrueDeg` pasa — rzeczywisty. Wysokości w stopach, odległości w NM.

**`kind: "airport"`**

```json
{ "v": 1, "type": "navdata", "id": "n1", "kind": "airport",
  "data": {
    "icao": "EPGD", "name": "Gdansk Lech Walesa", "lat": 54.3776, "lon": 18.4662, "elevFt": 489,
    "runways": [
      { "ident": "11", "headingTrueDeg": 112, "lengthFt": 9186, "widthFt": 148,
        "thresholdLat": 54.3884, "thresholdLon": 18.448, "elevFt": 470,
        "ils": { "ident": "IGDB", "freqMhz": 110.9, "locCourseDeg": 106, "gsAngleDeg": 3.0 } },
      { "ident": "29", "headingTrueDeg": 292, "lengthFt": 9186, "widthFt": 148,
        "thresholdLat": 54.379, "thresholdLon": 18.488, "elevFt": 489,
        "ils": { "ident": "IGDA", "freqMhz": 109.3, "locCourseDeg": 286, "gsAngleDeg": 3.0 } }
    ],
    "frequencies": [
      { "type": "ATIS", "name": "Rebiechowo Information", "mhz": 126.775 },
      { "type": "TWR", "name": "Rebiechowo Tower", "mhz": 118.1 }
    ]
  } }
```

- `runways` — każdy **koniec** pasa osobno (`ident`: `"29"`, `"11L"`, `"09"`, także `"N"`…`"NW"`
  dla pasów wodnych/kierunkowych). Próg = koniec pasa wyliczony ze środka, kursu i długości
  (przesunięty próg nie jest odejmowany). `elevFt` — wysokość środka pasa.
- `ils` — z obiektu lokalizera wskazanego przez `PRIMARY_ILS_ICAO`/`SECONDARY_ILS_ICAO` pasa;
  `null`, gdy koniec pasa nie ma ILS. `gsAngleDeg` = `null` dla LOC bez ścieżki schodzenia.
- `frequencies[].type`: `ATIS`, `CLR` (także pre-taxi i zdalne clearance), `GND`, `TWR`, `APP`,
  `DEP`, `CTR`, `UNICOM` (także CTAF i multicom), `OTHER` (AWOS, ASOS, FSS…).

**`kind: "procedures"`**

```json
{ "v": 1, "type": "navdata", "id": "n2", "kind": "procedures",
  "data": {
    "icao": "EPWA",
    "departures": [ { "name": "LOGDA1K", "runways": ["11", "15"], "transitions": ["NEPOX"] } ],
    "arrivals":   [ { "name": "AGAVA1W", "runways": ["33", "29"], "transitions": [] } ],
    "approaches": [
      { "name": "ILS 33", "type": "ILS", "runway": "33", "suffix": null, "transitions": ["WA412"] },
      { "name": "RNAV Z 29", "type": "RNAV", "runway": "29", "suffix": "Z", "transitions": ["BAMSO"] }
    ]
  } }
```

- `runways` SID/STAR — pasy, dla których procedura ma przejście pasowe (`"ALL"` = przejście dla
  wszystkich pasów); pusta lista = procedura bez przejść pasowych.
- `transitions` SID/STAR — przejścia trasowe (enroute); podejścia — przejścia (IAF).
- `approaches[].type`: `ILS`, `LOC` (także LOC BC), `RNAV`, `VOR` (także VOR/DME), `NDB`
  (także NDB/DME), `GPS`, `OTHER` (SDF, LDA…). `name` jest unikalne w lotnisku i służy do
  `procedure_legs` (np. `"ILS Z 29"`, `"LOC BC 11"`, `"VORDME 33"`). `runway` = `null` dla podejść
  okrężnych.

**`kind: "procedure_legs"`**

```json
{ "v": 1, "type": "navdata", "id": "n3", "kind": "procedure_legs",
  "data": {
    "icao": "EPGD", "procType": "approach", "name": "ILS 29", "runway": "29", "transition": "GD401",
    "legs": [
      { "fix": "GD401", "lat": 54.15, "lon": 19.0, "legType": "IF", "altDesc": "AT", "alt1Ft": 6000,
        "alt2Ft": null, "speedKt": null, "courseDeg": null, "distNm": null, "flyOver": false },
      { "fix": "GD291", "lat": 54.3166, "lon": 18.7533, "legType": "TF", "altDesc": "AT", "alt1Ft": 3500,
        "alt2Ft": null, "speedKt": null, "courseDeg": null, "distNm": null, "flyOver": false },
      { "fix": "FI29", "lat": 54.3415, "lon": 18.6472, "legType": "CF", "altDesc": "AT", "alt1Ft": 2400,
        "alt2Ft": null, "speedKt": 180, "courseDeg": 286, "distNm": null, "flyOver": false },
      { "fix": "RW29", "lat": 54.379, "lon": 18.488, "legType": "CF", "altDesc": "AT", "alt1Ft": 540,
        "alt2Ft": null, "speedKt": null, "courseDeg": 286, "distNm": null, "flyOver": true }
    ],
    "missed": [
      { "fix": "", "lat": null, "lon": null, "legType": "CA", "altDesc": "ABOVE", "alt1Ft": 1500,
        "alt2Ft": null, "speedKt": null, "courseDeg": 292, "distNm": null, "flyOver": false },
      { "fix": "GD510", "lat": 54.45, "lon": 18.3, "legType": "DF", "altDesc": "AT", "alt1Ft": 4000,
        "alt2Ft": null, "speedKt": null, "courseDeg": null, "distNm": null, "flyOver": false }
    ]
  } }
```

- Kolejność odcinków: SID = przejście pasowe → część wspólna → przejście trasowe;
  STAR = przejście trasowe → część wspólna → przejście pasowe; podejście = przejście (gdy podane)
  → odcinek końcowy. Punkt `IF` powtarzający ostatni punkt poprzedniej części jest pomijany.
- `missed` — odcinki nieudanego podejścia (tylko podejścia; dla SID/STAR pusta lista).
- `runway` w odpowiedzi: dla SID/STAR pas użytego przejścia (gdy procedura ma jedno przejście
  pasowe, zostaje użyte bez podawania `runway`); dla podejścia — pas podejścia.
- `legType` — typ odcinka ARINC 424: `IF`, `TF`, `CF`, `DF`, `RF`, `AF`, `CA`, `CD`, `CI`, `CR`,
  `FA`, `FC`, `FD`, `FM`, `HA`, `HF`, `HM`, `PI`, `VA`, `VD`, `VI`, `VM`, `VR` (`UNKNOWN` przy nieznanym).
- `fix` = `""` (oraz `lat`/`lon` = `null`) dla odcinków bez punktu końcowego (`CA`, `VA`, `VM`…).
  Współrzędne z pól `FIX_LATITUDE/LONGITUDE` odcinka, a gdy MSFS ich nie poda — z obiektu
  WAYPOINT/VOR/NDB tego punktu.
- `altDesc`: `AT`, `ABOVE` (na lub powyżej), `BELOW` (na lub poniżej), `BETWEEN` (`alt1Ft` = górna,
  `alt2Ft` = dolna granica) albo `null` (bez ograniczenia; wtedy `alt1Ft` = `null`).
- `speedKt` — ograniczenie prędkości (IAS) albo `null`. `distNm` — długość odcinka z bazy, jeśli
  podana w NM (odcinki czasowe → `null`). `flyOver` — punkt do przelotu nad nim (fly-over).

**`kind: "fix"`**

```json
{ "v": 1, "type": "navdata", "id": "n4", "kind": "fix",
  "data": { "results": [
    { "ident": "WAR", "type": "vor", "lat": 52.188, "lon": 20.962, "freqMhz": 113.45, "region": "EP" },
    { "ident": "WAR", "type": "waypoint", "lat": 48.1, "lon": 11.2, "freqMhz": null, "region": "ED" }
  ] } }
```

- `type`: `waypoint`, `vor` (także ILS/DME), `ndb`, `airport`. `freqMhz` — dla VOR i NDB
  (NDB w MHz, np. `0.389` = 389 kHz), inaczej `null`. `region` — kod regionu ICAO albo `null`.
- Wyniki posortowane rosnąco po odległości od `nearLat`/`nearLon` (bez nich — po typie i regionie).
- Brak wyników = `error` `not_found` (nie pusta lista).

**`kind: "nearby"`**

```json
{ "v": 1, "type": "navdata", "id": "n6", "kind": "nearby",
  "data": { "results": [
    { "ident": "WAR", "type": "vor", "lat": 52.188, "lon": 20.962, "freqMhz": 113.45, "region": "EP" },
    { "ident": "EPWA", "type": "airport", "lat": 52.1657, "lon": 20.9671, "freqMhz": null, "region": "EP" }
  ] } }
```

- Lotniska, VOR, NDB i punkty trasy w promieniu `radiusNm` od `lat`/`lon`, posortowane po odległości.
  Na typ najwyżej: 40 lotnisk, 30 VOR, 30 NDB, 120 punktów trasy.
- MSFS zwraca obiekty z „bańki rzeczywistości” wokół samolotu (zwykle ok. 100–200 km), więc
  zapytanie ma sens dla pozycji samolotu. Pusta okolica = pusta lista (nie `not_found`).

## 4. Wiadomości telefon → serwer

### `ping`

`{ "v": 1, "type": "ping", "id": "p7", "t": 1792231234000 }` — odpowiedź `pong` z tym samym
`id` i `t`. Telefon powinien pingować co 5 s; serwer zamyka sesję po 30 s bez żadnej wiadomości.

### `command`

```json
{ "v": 1, "type": "command", "id": "c42", "name": "HEADING_BUG_SET", "value": 270 }
```

Serwer przyjmuje **wyłącznie** nazwy z listy poniżej (dokładna pisownia, wielkie litery).
Nazwy to zdarzenia SimConnect (Event IDs) — z wyjątkiem `XPNDR_MODE`, `CAMERA_STATE`, `CAMERA_PITCH_SET` i `CAMERA_YAW_SET`, które zapisują SimVar.
`value` jest wymagane tylko tam, gdzie podano zakres; w pozostałych jest ignorowane.
Wartość musi być liczbą całkowitą (lub liczbą z zerową częścią ułamkową) w podanym zakresie.

**Autopilot**

| name | value |
|---|---|
| `AP_MASTER`, `AUTOPILOT_ON`, `AUTOPILOT_OFF`, `TOGGLE_FLIGHT_DIRECTOR`, `AUTO_THROTTLE_ARM`, `YAW_DAMPER_TOGGLE` | — |
| `AP_HDG_HOLD`, `AP_NAV1_HOLD`, `AP_APR_HOLD`, `AP_LOC_HOLD`, `AP_BC_HOLD`, `AP_ALT_HOLD`, `AP_VS_HOLD`, `FLIGHT_LEVEL_CHANGE`, `AP_PANEL_SPEED_HOLD` | — |
| `HEADING_BUG_SET` | 0–359 (°) |
| `HEADING_BUG_INC`, `HEADING_BUG_DEC`, `AP_ALT_VAR_INC`, `AP_ALT_VAR_DEC`, `AP_VS_VAR_INC`, `AP_VS_VAR_DEC`, `AP_SPD_VAR_INC`, `AP_SPD_VAR_DEC` | — |
| `AP_ALT_VAR_SET_ENGLISH` | 0–50000 (ft) |
| `AP_VS_VAR_SET_ENGLISH` | −8000–8000 (ft/min) |
| `AP_SPD_VAR_SET` | 0–500 (kt) |
| `AP_AIRSPEED_HOLD` (utrzymanie prędkości), `AP_N1_HOLD` (utrzymanie N1/ciągu), `AP_PANEL_MACH_HOLD` (utrzymanie Macha) | — |
| `AP_MACH_VAR_SET` | 10–99 (Mach × 100, np. `78` = M0,78) |
| `AP_BANK_HOLD` (utrzymanie przechylenia), `AP_WING_LEVELER` (poziomowanie skrzydeł) | — |

**Radia** (częstotliwości w Hz)

| name | value |
|---|---|
| `COM_RADIO_SET_HZ`, `COM_STBY_RADIO_SET_HZ`, `COM2_RADIO_SET_HZ`, `COM2_STBY_RADIO_SET_HZ` | 118000000–136990000 |
| `NAV1_RADIO_SET_HZ`, `NAV1_STBY_SET_HZ`, `NAV2_RADIO_SET_HZ`, `NAV2_STBY_SET_HZ` | 108000000–117950000 |
| `COM_STBY_RADIO_SWAP`, `COM2_RADIO_SWAP`, `NAV1_RADIO_SWAP`, `NAV2_RADIO_SWAP` | — |

**Transponder**

| name | value |
|---|---|
| `XPNDR_SET` | kod jako liczba dziesiętna o cyfrach 0–7, np. `7000` (serwer zamienia na BCD) |
| `XPNDR_IDENT_ON` | — |
| `XPNDR_MODE` | 0 off, 1 standby, 2 test, 3 on, 4 alt, 5 ground |

**Podwozie, klapy, spoilery, hamulec**

| name | value |
|---|---|
| `GEAR_UP`, `GEAR_DOWN`, `GEAR_TOGGLE` | — |
| `FLAPS_UP`, `FLAPS_DOWN`, `FLAPS_INCR`, `FLAPS_DECR` | — |
| `FLAPS_SET` | 0–16383 |
| `SPOILERS_ON`, `SPOILERS_OFF`, `SPOILERS_TOGGLE`, `SPOILERS_ARM_TOGGLE` | — |
| `SPOILERS_SET` | 0–16383 |
| `PARKING_BRAKES` (przełącz) | — |
| `PARKING_BRAKE_SET` | 0–1 |

**Światła** (przełączniki)

`TOGGLE_BEACON_LIGHTS`, `STROBES_TOGGLE`, `TOGGLE_NAV_LIGHTS`, `LANDING_LIGHTS_TOGGLE`,
`TOGGLE_TAXI_LIGHTS`, `TOGGLE_LOGO_LIGHTS`, `TOGGLE_WING_LIGHTS`, `PANEL_LIGHTS_TOGGLE`,
`TOGGLE_CABIN_LIGHTS` — bez wartości.

**Trymer**

| name | value |
|---|---|
| `ELEV_TRIM_UP`, `ELEV_TRIM_DN`, `AILERON_TRIM_LEFT`, `AILERON_TRIM_RIGHT`, `RUDDER_TRIM_LEFT`, `RUDDER_TRIM_RIGHT` | — |
| `ELEVATOR_TRIM_SET` | −16383–16383 |

**Silniki** (dźwignie wszystkich silników naraz; w `state` wartości silnika 1 w `controls`)

| name | value |
|---|---|
| `THROTTLE_SET` | 0 (jałowy) – 16383 (pełna moc) |
| `THROTTLE1_SET`, `THROTTLE2_SET`, `THROTTLE3_SET`, `THROTTLE4_SET` | dźwignia jednego silnika, 0 – 16383; położenie w `engines[n].throttlePct` |
| `MIXTURE_SET` | 0 (odcięcie) – 16383 (bogata) |
| `PROPELLER_SET` | 0 (małe obroty) – 16383 (duże obroty) |
| `THROTTLE_CUT`, `THROTTLE_FULL`, `THROTTLE_REVERSE_THRUST_TOGGLE`, `AUTO_THROTTLE_TO_GA` | — |

**Kamera** (zapis SimVar `CAMERA STATE`; bieżąca wartość w `controls.cameraState`)

| name | value |
|---|---|
| `CAMERA_STATE` | 2 kabina, 3 zewnętrzna (pościg), 4 dron; zapas między 2 a 3: zdarzenie `VIEW_MODE` |
| `CAMERA_PITCH_SET` | pochylenie kamery w stopniach od początku rozglądania: −90 (w dół) … 90 (w górę) |
| `CAMERA_YAW_SET` | obrót kamery w stopniach od początku rozglądania: −180 (w lewo) … 180 (w prawo) |

Rozglądanie się działa w bieżącym widoku i nie zmienia `CAMERA STATE`. MSFS 2024 nie przyjmuje zapisu kąta kamery
(`CAMERA GAMEPLAY PITCH YAW` jest tylko do odczytu, `PAN_*` są ignorowane), a `CameraSetRelative6DOF` przełącza w kamerę
6DOF, z której MSFS często nie wraca. Link obraca więc kamerę jak użytkownik: przytrzymany prawy przycisk myszy i ruch
(SendInput), tylko gdy okno MSFS jest na pierwszym planie; kursor stoi wtedy u góry okna, nad szybą. Telefon śle kąty
ok. 33 razy na sekundę; 0/0 albo cisza przez 0,4 s = koniec: mysz wraca o tyle samo i puszcza przycisk. Tempo obrotu:
ustawienie „Czułość rozglądania” w Linku (100 % = 8 zliczeń myszy na stopień). Odmowa (okno MSFS nieaktywne, MSFS
jako administrator) to `error` z kodem `not_allowed`, raz na przytrzymanie. Odczytane kąty kamery (diagnostyka)
są w `controls.cameraPitchDeg` / `controls.cameraYawDeg`.

Odpowiedź: `ack` po przekazaniu do symulatora (nie oznacza, że samolot zareagował — skutek
widać w kolejnych `state`), albo `error`.

### `navdata_request` — dane nawigacyjne z bazy MSFS

Telefon pyta Link o lotniska, procedury i punkty. Źródłem jest baza nawigacyjna MSFS 2024
(SimConnect Facilities API: `SimConnect_AddToFacilityDefinition`, `SimConnect_RequestFacilityData`,
`SimConnect_RequestFacilityData_EX1`), więc dane zgadzają się z tym, co widzi symulator.
Telefon zapisuje odpowiedzi u siebie i nie musi pytać ponownie.

```json
{ "v": 1, "type": "navdata_request", "id": "n1", "kind": "airport", "icao": "EPGD" }
{ "v": 1, "type": "navdata_request", "id": "n2", "kind": "procedures", "icao": "EPWA" }
{ "v": 1, "type": "navdata_request", "id": "n3", "kind": "procedure_legs", "icao": "EPGD",
  "procType": "approach", "name": "ILS 29", "transition": "GD401" }
{ "v": 1, "type": "navdata_request", "id": "n5", "kind": "procedure_legs", "icao": "EPWA",
  "procType": "departure", "name": "LOGDA1K", "runway": "15", "transition": "NEPOX" }
{ "v": 1, "type": "navdata_request", "id": "n4", "kind": "fix", "ident": "WAR", "nearLat": 52.2, "nearLon": 21.0 }
```

| `kind` | Pola | Odpowiedź `data` |
|---|---|---|
| `airport` | `icao` | lotnisko, pasy z ILS, częstotliwości |
| `procedures` | `icao` | listy SID, STAR i podejść (bez odcinków) |
| `procedure_legs` | `icao`, `procType` (`departure` / `arrival` / `approach`), `name`, opcjonalnie `runway`, `transition` | odcinki procedury + `missed` |
| `fix` | `ident`, opcjonalnie `nearLat` + `nearLon` (razem) | wszystkie punkty o tej nazwie |
| `nearby` | `lat`, `lon`, opcjonalnie `radiusNm` (1–250, domyślnie 60) | lotniska i pomoce nawigacyjne w pobliżu |

- `icao`: 2–8, `ident`: 1–8 liter/cyfr; wielkość liter bez znaczenia. `name`, `transition`:
  1–16 znaków, bez rozróżniania wielkości liter. `runway` w dowolnej postaci (`"RW29"`, `"9"`,
  `"11l"`) — serwer normalizuje do `"29"`, `"09"`, `"11L"`.
- Odpowiedź: `navdata` (p. 3) z tym samym `id`, albo `error`:
  `invalid_value` (zły `kind` lub pola), `not_found` (brak lotniska / procedury / pasa / przejścia /
  punktu albo MSFS nie odpowiedział w 10 s), `sim_unavailable` (brak MSFS i trybu symulacji),
  `rate_limited` (ponad **10 zapytań/s** na telefon), `sim_error` (inny błąd SimConnect).
- Zapytania są obsługiwane w tle (do 4 naraz na telefon, kolejne czekają w kolejce), więc
  odpowiedzi mogą przyjść w innej kolejności niż zapytania — paruj je po `id`.
- Link trzyma odpowiedzi w pamięci przez **10 minut** (osobno dla MSFS i trybu symulacji);
  `procedures` i `procedure_legs` jednego lotniska korzystają z jednego pobrania z MSFS.
  Pierwsze zapytanie o procedury dużego lotniska może trwać kilka sekund.
- **Tryb symulacji** zwraca stałe, przykładowe dane dla **EPWA** i **EPGD** (pasy z ILS,
  po 2 SID i 2 STAR, podejścia ILS i RNAV z odcinkami w realistycznych miejscach; nazwy procedur
  i punktów są wymyślone — nie do prawdziwej nawigacji). Inne lotniska → `not_found`.
  Punkty dla `fix`: m.in. `WAR` (VOR 113.45), `GDA` (VOR), `WA`/`GD` (NDB), `SOXER` (dwa wyniki:
  EP i ED — do sprawdzenia sortowania po odległości) oraz wszystkie punkty procedur.

### `axis` — sterowanie przechylaniem telefonu

```json
{ "v": 1, "type": "axis", "aileron": -0.25, "elevator": 0.10, "rudder": 0.0 }
```

- Zakres każdej osi −1…1: `aileron` + = w prawo, `elevator` + = drążek do siebie (nos w górę),
  `rudder` + = w prawo. Pominięta oś = bez zmian; `NaN`/brak liczby = `invalid_value`.
- Serwer **przycina wychylenie** do ±`maxDeflection` (domyślnie 0,8; podawane w `hello_ok`).
- Telefon musi wysyłać `axis` co najmniej 5 razy na sekundę (zalecane 20 Hz) tak długo, jak
  steruje. **Auto-neutral:** jeśli przez 500 ms nie przyjdzie żadne `axis`, serwer ustawia
  wszystkie trzy osie na 0 i przestaje je nadpisywać (oddaje sterowanie joystickowi).
  To samo dzieje się po rozłączeniu telefonu.
- `{ "v": 1, "type": "axis", "release": true }` — natychmiastowe zwolnienie (neutral i koniec).
- Tylko jeden telefon steruje osiami naraz — pierwszy, który zaczął; inne dostają `error`
  `not_allowed` do czasu zwolnienia.
- `axis` nie dostaje `ack` (za duża częstotliwość) — tylko `error`.

W MSFS osie trafiają do zdarzeń `AXIS_AILERONS_SET`, `AXIS_ELEVATOR_SET`, `AXIS_RUDDER_SET`
(±16383).

#### Hamulce: `speedbrake` i `brake`

```json
{ "v": 1, "type": "axis", "speedbrake": 0.5 }
{ "v": 1, "type": "axis", "brake": 0.8 }
```

- Dwie osobne osie 0…1, z **własnym właścicielem** — wiadomość z samymi hamulcami nie przejmuje
  lotek, steru wysokości i kierunku (joystick przy symulatorze dalej działa).
- `speedbrake` — hamulec aerodynamiczny (`AXIS_SPOILER_SET`, 0 = schowany, 1 = pełny). Zostaje
  w ustawionej pozycji jak dźwignia; Link wysyła go raz po każdej zmianie.
- `brake` — hamulce kół, oba naraz (`AXIS_LEFT_BRAKE_SET` i `AXIS_RIGHT_BRAKE_SET`). Działają jak pedały:
  telefon wysyła je (co 50 ms), dopóki są wciśnięte; po 500 ms ciszy albo `release` Link sam je puszcza.
  **Tylko na ziemi** — w powietrzu Link zawsze wysyła puszczone hamulce.
- Inny telefon sterujący hamulcami dostaje `error` `not_allowed`; zakres spoza 0…1 jest przycinany.

## 5. Bezpieczeństwo

- Serwer działa tylko w sieci lokalnej, bez TLS. Kod parowania + token chronią przed
  przypadkowym sterowaniem z innego urządzenia w tej samej sieci, nie przed atakiem w sieci.
- Bez sparowania nie da się ani czytać danych, ani wysyłać komend.
- Serwer nigdy nie wykonuje nazw spoza listy — telefon nie może wysłać dowolnego zdarzenia
  ani zapisać dowolnej zmiennej symulatora.
