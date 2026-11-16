# Checklisty w OpenTakeoff na Windows

Karta **Checklisty** pokazuje listy kontrolne dopasowane do samolotu w symulatorze. Pozycję odhaczasz
kliknięciem albo przyciskiem **Odhacz**. Pozycje ze znacznikiem **SIM** odhacza symulator, gdy stan
samolotu się zgadza (hamulec postojowy, klapy, podwozie, światła, silniki…). Pozycja odznaczona
ręcznie nie wraca, dopóki nie zaczniesz listy od nowa.

> Checklisty służą tylko do symulacji. Linie lotnicze i szkoły mają własne wersje. W prawdziwym
> lataniu obowiązuje zatwierdzona dokumentacja samolotu (FCOM, QRH, POH).

## Wbudowane zestawy

| Plik | Samolot | Źródło |
|---|---|---|
| `a320.json` | Airbus A318–A321, A330, A340 | Airbus FCOM, Normal Checklist (PRO-NOR-CL) |
| `b737.json` | Boeing 737 NG / MAX | Boeing 737 FCOM / QRH, Normal Checklist |
| `b787.json` | Boeing 777 / 787 (także 747-8) | Boeing 777/787 FCOM, Normal Checklist (ECL) |
| `b717.json` | Boeing 717 | checklista TFDi Design wzorowana na checklistach linii lotniczych (tylko symulacja) |
| `b200.json` | Beechcraft King Air B200 / 350 | skrócona checklista B200 według POH, rozdział 4 |
| `c208.json` | Cessna 208 / 208B Caravan | checklista normalna według PFM |
| `c152.json` | Cessna 152 | checklista szkoły lotniczej oparta na POH, sekcja 4 |
| `c172.json` | Cessna 172S Skyhawk (G1000) | checklista Nav III szkoły lotniczej oparta na POH, sekcja 4 |
| `pa28.json` | Piper PA-28 Archer / Warrior | checklista Archer III szkoły lotniczej oparta na POH, sekcja 4 |
| `sr22.json` | Cirrus SR20 / SR22 | checklista operatora SR22 oparta na POH, sekcja 4 |
| `da40ng.json` | Diamond DA40 NG | checklista normalna operatora (LN-FTQ / LN-FTU, wyd. 3) |

Pliki leżą w `desktop/app/checklists/`. Każdy plik `*.json` z tego folderu jest wkompilowany w program
samoczynnie (build.rs), więc nowy wbudowany zestaw to tylko nowy plik w folderze. Kolejność plików
(alfabetyczna) decyduje, który zestaw wygrywa, gdy pasuje kilka wzorców `match`.

## Własne checklisty

Własne zestawy to pliki `*.json` w folderze `checklists` w danych programu (przycisk **Folder** na
wstążce karty). Po zmianie plików kliknij **Wczytaj**.

- Zestaw z tym samym `id` co wbudowany **zastępuje** wbudowany (np. własna wersja A320 linii lotniczej).
- Zestaw z innym `id` **dochodzi** do listy.
- **Kopia do edycji** zapisuje bieżący zestaw w folderze jako wzór. Zmień w kopii `id`, `name` i
  `match`, żeby zrobić zestaw dla innego samolotu.
- Błędy w plikach (zły JSON, nieznana zmienna w warunku) widać pod listą checklist i w logu programu.

Plik zapisuj w UTF-8. Notatnik w Windows 10/11 robi to domyślnie.

### Wybór zestawu dla samolotu

Gdy na wstążce jest wybrane **Auto**, program bierze pierwszy zestaw, którego wzorzec `match` pasuje
do nazwy samolotu z symulatora (tytuł, model i typ ATC). Własne zestawy sprawdza przed wbudowanymi.
Gdy żaden wzorzec nie pasuje, program bierze pierwszy zestaw, którego `styles` zawiera styl kabiny
rozpoznany jak na karcie Przyrządy. Zestaw można też wybrać ręcznie z tego samego menu.

Zmiana samolotu w symulatorze zaczyna wszystkie listy od nowa. Ręcznie robi to przycisk **Nowy lot**.

## Format pliku

```json
{
  "id": "tbm930",
  "name": "Daher TBM 930",
  "match": ["tbm ?9[0-9]0", "daher"],
  "styles": ["G1000"],
  "source": { "pl": "POH TBM 930, sekcja 4", "en": "TBM 930 POH, section 4" },
  "note": "Dowolna uwaga pod źródłem (opcjonalnie)",
  "lists": [
    {
      "name": { "pl": "Przed uruchomieniem", "en": "Before start" },
      "items": [
        { "item": "Parking brake", "resp": "SET", "auto": "parking_brake" },
        { "item": "Beacon", "resp": "ON", "auto": "beacon" },
        { "item": "Fuel quantity", "resp": "CHECK", "note": { "pl": "Obie strony", "en": "Both sides" } }
      ]
    }
  ]
}
```

| Pole | Znaczenie |
|---|---|
| `id` | Identyfikator zestawu (bez spacji). Pusty – nazwa pliku. |
| `name` | Nazwa w menu i nagłówku. |
| `match` | Wyrażenia regularne dla nazwy samolotu, bez rozróżniania wielkości liter. Wystarczy, że pasuje jedno. |
| `styles` | Style kabiny jako zapas: `A320`, `B737`, `B787`, `ATR72`, `E190`, `G1000`, `C172`, `R44`, `GLIDER`. |
| `source`, `note` | Źródło checklisty i uwaga – pokazywane pod listą. |
| `lists` | Listy w kolejności lotu. Każda ma `name` i `items`. |
| `item`, `resp` | Wyzwanie i odpowiedź, jak na karcie w kabinie. |
| `note` | Drobny dopisek pod pozycją (opcjonalnie). |
| `auto` | Warunek, przy którym symulator odhacza pozycję (opcjonalnie). |

Teksty (`name`, `source`, `note`, `item`, `resp`) mogą być zwykłym tekstem albo obiektem z językami:
`{"pl": "…", "en": "…", "es": "…"}`. Program pokazuje język interfejsu, a gdy go brak – angielski,
potem polski.

### Warunki `auto`

Warunek to jedna zmienna albo kilka połączonych:

- `beacon` – prawda, gdy wartość jest różna od zera,
- `!parking_brake` – zaprzeczenie,
- `flaps>0`, `throttle>=95`, `flaps==0`, `xpdr_code!=7000`, `agl<500` – porównanie z liczbą,
- `engines_running>=engines` – porównanie z inną zmienną,
- `gear_down & spoilers_armed` – wszystkie naraz (`&`),
- `landing_lights | taxi_lights` – którykolwiek (`|`); `&` wiąże mocniej niż `|`.

Gdy symulator nie podaje wartości (np. danych awioniki), pozycja czeka i nie jest odhaczana.

| Zmienna | Wartość |
|---|---|
| `on_ground` | samolot na ziemi (0/1) |
| `parking_brake` | hamulec postojowy (0/1) |
| `gear_down` | podwozie wypuszczone (0/1) |
| `flaps` | pozycja klap (indeks: 0 – schowane) |
| `flaps_pct` | wychylenie klap (%) |
| `spoilers_armed`, `spoilers_pct` | spoilery uzbrojone (0/1), wychylenie (%) |
| `throttle`, `mixture`, `prop` | dźwignie mocy, mieszanki i śmigła (%) |
| `trim` | trym steru wysokości (%) |
| `beacon`, `strobe`, `nav_lights`, `landing_lights`, `taxi_lights`, `logo_lights`, `wing_lights`, `panel_lights`, `cabin_lights` | światła (0/1) |
| `engines` | liczba silników |
| `engines_running` | liczba pracujących silników |
| `ap`, `fd`, `athr` | autopilot, dyrektor lotu, automat ciągu (0/1) |
| `ap_alt`, `ap_hdg`, `ap_spd` | nastawy autopilota (ft, °, kt) |
| `xpdr_on`, `xpdr_alt` | transponder włączony / w trybie ALT (0/1) |
| `xpdr_code` | kod transpondera (np. 7000) |
| `ias`, `gs`, `vs` | prędkość przyrządowa i nad ziemią (kt), pionowa (ft/min) |
| `alt`, `agl` | wysokość wskazywana i nad terenem (ft) |
| `fuel_kg` | paliwo (kg) |
| `baro_std`, `baro_hpa` | wysokościomierz na STD (0/1), nastawa (hPa) |
| `altimeter_set` | wysokościomierz na bieżące QNH (±1 hPa) i nie na STD (0/1) |

Brakuje zmiennej? Dodaje się ją w `desktop/app/src/checklists.rs` (funkcja `var` i lista `VARS`).
