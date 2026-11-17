# OpenTakeoff – plan rozwoju

Stan na listopad 2026 (wersja 3.7.0). Szczegóły każdego wydania są w [CHANGELOG.md](../CHANGELOG.md).

## Co już jest

### Android
- Pogoda (METAR, TAF, SIGMET, PIREP), briefing lotniska, charty FAA, mapa 2D/3D ze śladem lotu.
- Kokpit z OpenTakeoff na Windows: PFD, ND, silniki, autopilot z pokrętłami, radio, systemy, sterowanie
  żyroskopem, drążkiem albo kontrolerem, kamera MSFS.
- Dziennik lotów z oceną lądowania, eksportem GPX i kopią `.oab`, synchronizacja z komputerem.
- Zapowiedzi głosowe (`Callouts.kt`) i checklisty z plików w `assets/checklists` (A320, 737-800, C172).

### Windows (Rust + Slint, folder `desktop/`)
- Most MSFS 2020/2024 i X-Plane 11/12 ↔ telefon (SimConnect, UDP bez wtyczek), start razem z symulatorem.
- Przyrządy w stylach kabin (A320, 737, 787, ATR, E190, G1000, zegary analogowe) z ekranami silników.
- Mapa nawigacyjna z radialami, Direct To, nakładkami, radarem opadów i METAR-ami; widok 3D z kamerami.
- Plan lotu z SID/STAR/podejściami, eksport do MSFS i X-Plane; SimBrief.
- Dziennik lotów, analiza lądowania, odtwarzanie w 3D, statystyki z osiągnięciami, eksport KML/GPX/CSV.
- Nakładki: karta lądowania, pasek do transmisji, okno zawsze na wierzchu, strony dla OBS; zrzuty ekranu.
- Checklisty (3.7.0): 11 zestawów, odhaczanie przez symulator, własne pliki JSON ([opis](checklists.md)).

## Plan

Kolejność to propozycja – priorytety ustala autor.

### Najbliżej
1. **Callouty i checklisty na głos (Windows).** Wysokości 1000/500/minimums/100…10, „V1”, „Rotate”,
   „Retard”, STALL i OVERSPEED przez syntezator mowy Windows (SAPI); czytanie pozycji checklisty.
2. **Ruch VATSIM/IVAO.** Samoloty w pobliżu na mapie, ND i w 3D; stacje ATC z częstotliwościami, ATIS
   w briefingu (publiczne dane, bez kont).
3. **ECAM/EICAS.** Strony ELEC, HYD, FUEL i DOOR (belki ILS na PFD – zrobione).

### Checklisty
- Checklista w nakładce na ekranie symulatora (spacja odhacza – zrobione).
- Wybór listy według fazy lotu (po starcie silników „After start”, po oderwaniu „After takeoff”).
- Jeden format plików dla Windows i Androida; odhaczanie zsynchronizowane z telefonem.
- Kolejne zestawy, gdy znajdą się publiczne źródła: ATR 72, Dash 8 Q400, CRJ, E-Jets, PC-12, TBM.

### Plan lotu i briefing
- Zrobione: paliwo bez SimBriefa, TOC/TOD, propozycje zapasowych, briefing do PDF, OFP SimBrief
  z wagami, paliwem, navlogiem i profilem wiatru.
- NOTAM-y w programie – gdy znajdzie się darmowe źródło bez klucza (dziś tylko wyszukiwarka FAA
  w przeglądarce).
- Diagram pasów i mapa trasy w briefingu PDF.

### Wygoda
- Zrobione: przyrządy w osobnych oknach, skróty klawiaturowe z oknem pomocy (F1), przewodnik.
- Discord Rich Presence; import dziennika z Volanty i SimToolkitPro.

### Jakość kodu
- Testy aplikacji Windows (`-p opentakeoff`) uruchamiane w CI – dopisane w listopadzie 2026; warto dopisać
  testy synchronizacji dziennika, protokołu z telefonem i oceny lądowania.
- Panele karty Narzędzia (`tools.slint`) do rozdzielenia na osobne komponenty – wymaga przeniesienia
  stanu pól z korzenia karty i funkcji `fill()`.

## Wydania

Tag `vX.Y.Z` uruchamia [release.yml](../.github/workflows/release.yml): testy, APK, instalator Windows
i wydanie na GitHubie. Ręczne sprawdzenie kompilacji wersji na Windows na dowolnej gałęzi:
workflow „Sprawdzenie (Windows)” ([check.yml](../.github/workflows/check.yml)).
