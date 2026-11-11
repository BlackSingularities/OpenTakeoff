# OpenTakeoff Link — szybki start

OpenTakeoff Link to mały program na Windows, który łączy **MSFS 2024** z aplikacją na telefonie
w tej samej sieci Wi-Fi. Telefon pokazuje samolot na mapie, PFD i panele, a także może sterować
autopilotem, radiami, transponderem, podwoziem, klapami, światłami i lotem (przechylaniem telefonu).

## 1. Pobranie i uruchomienie

1. Pobierz `OpenTakeoff-Link.exe` z najnowszego wydania na GitHubie (ten sam release co APK).
2. Uruchom plik — nie wymaga instalacji ani .NET. Przy pierwszym uruchomieniu Windows SmartScreen
   może ostrzec o nieznanym wydawcy: **Więcej informacji → Uruchom mimo to**.
3. Kolejność nie ma znaczenia: program sam połączy się z MSFS, gdy symulator wystartuje
   (i ponownie po jego restarcie). Stan widać w sekcji **Symulator** — zielona kropka = połączono.
4. Zamknięcie okna chowa program do zasobnika obok zegara. Całkowicie zamkniesz go z menu ikony
   (**Zakończ**).

Aktualizacje są automatyczne: program 10 s po starcie (i potem co 6 godzin) sprawdza najnowsze
wydanie, pobiera nowy `OpenTakeoff-Link.exe` i podmienia nim plik, z którego go uruchomiono — skrót
na pulpicie dalej działa. Podmiana następuje dopiero, gdy żaden telefon nie jest połączony, więc
nie przerwie lotu; wcześniej można ją wymusić przyciskiem **Zainstaluj teraz** w oknie. Ręcznie:
menu ikony → **Sprawdź aktualizacje**. Jeśli plik leży w folderze bez prawa zapisu (np. Program
Files), program tylko podsuwa stronę wydania.

Opcje uruchomienia (np. w skrócie na pulpicie): `--minimized` (start w zasobniku),
`--simulate` / `--no-simulate` (wymuszenie trybu symulacji).

## 2. Zapora Windows

Przy pierwszym uruchomieniu Windows zapyta o dostęp do sieci. Zaznacz **Sieci prywatne** i kliknij
**Zezwalaj**. Sieć domowa musi być ustawiona jako prywatna (Ustawienia → Sieć i Internet →
Wi-Fi/Ethernet → Typ profilu sieci: **Prywatna**).

Jeśli okno zapory się nie pojawiło albo telefon nie widzi komputera, dodaj regułę ręcznie
(PowerShell **jako administrator**; broadcast UDP 47811 wychodzi z komputera i nie wymaga reguły):

```powershell
New-NetFirewallRule -DisplayName "OpenTakeoff Link (WebSocket)" -Direction Inbound -Protocol TCP -LocalPort 47810 -Action Allow -Profile Private
```

Sprawdzenie z telefonu: otwórz w przeglądarce `http://<IP komputera>:47810/` — powinien pojawić się
napis „OpenTakeoff Link dziala”. Adres IP komputera jest w oknie programu (sekcja **Parowanie**).

## 3. Parowanie telefonu

1. Telefon i komputer muszą być w tej samej sieci (ten sam router; nie sieć „gościnna”).
2. W aplikacji wybierz połączenie z MSFS. Komputer pojawi się na liście sam (program rozgłasza się
   co 2 s). Jeśli nie — wpisz ręcznie adres IP z okna programu.
3. Wpisz **6-cyfrowy kod** widoczny w oknie programu. Telefon zapamięta parowanie — kolejne
   połączenia nie wymagają kodu.
4. Połączony telefon pojawi się na liście **Połączone telefony**.

Przycisk **Nowy kod** zmienia kod (już sparowane telefony działają dalej).
**Unieważnij sparowania** odłącza wszystkie telefony — każdy będzie musiał podać nowy kod.
Po 5 błędnych kodach z jednego telefonu program blokuje go na minutę.

## 4. Tryb symulacji

Przełącznik **Tryb symulacji** uruchamia lot testowy bez MSFS: start z EPWA (pas 33), wznoszenie
na FL240, przelot, zniżanie i lądowanie w EPGD (pas 29), po czym lot zaczyna się od nowa.
Symulacja przyjmuje komendy z telefonu (autopilot, radia, transponder, światła, podwozie, klapy,
osie), więc da się sprawdzić aplikację bez uruchamiania symulatora. Hamulec postojowy włączony
z telefonu wstrzymuje start. Po lądowaniu w EPGD telefon dostaje dane przyziemienia
(−180 ft/min, 1,2 g) do oceny lądowania.

W trybie symulacji Link podaje też przykładowe dane nawigacyjne dla **EPWA** i **EPGD** (pasy z ILS,
częstotliwości, po dwa SID i STAR, podejścia ILS i RNAV) — wystarczą do sprawdzenia edytora planu
lotu w aplikacji. Nazwy procedur w symulacji są wymyślone.

## 5. Dane nawigacyjne z MSFS

Edytor planu lotu w aplikacji (SID, STAR, podejścia, częstotliwości ILS) pobiera dane z bazy
nawigacyjnej **Twojego MSFS 2024** przez Link — tej samej, której używa symulator, łącznie
z aktualizacjami AIRAC zainstalowanymi w MSFS. Działa to tylko przy połączonym MSFS (zielona kropka
w sekcji **Symulator**); bez symulatora telefon korzysta z danych zapisanych wcześniej.
Telefon zapamiętuje pobrane lotniska i procedury, a Link trzyma je w pamięci przez 10 minut.
Pierwsze pobranie procedur dużego lotniska może trwać kilka sekund.

## 6. Ocena lądowania i profile samolotów

Link wykrywa każde przyziemienie (także odbicie) i wysyła telefonowi prędkość opadania,
przeciążenie, pochylenie, przechylenie i miejsce kontaktu z pasem. Rozpoznaje też typ samolotu
(liniowiec, odrzutowiec biznesowy, turbośmigłowy, GA, śmigłowiec), żeby aplikacja pokazała
pasujący panel autopilota (np. N1, Mach i bank hold w liniowcach).

## 7. Sterowanie przechylaniem telefonu

Telefon wysyła wychylenia lotek, steru wysokości i kierunku. Program ogranicza je do 80% pełnego
wychylenia, a gdy przez pół sekundy nie dostanie aktualizacji (np. zerwane Wi-Fi), sam ustawia
stery w neutrum i oddaje sterowanie joystickowi. Naraz osiami steruje tylko jeden telefon.

## 8. X-Plane 11 i 12

Dla X-Plane zamiast programu Windows uruchom most `bridge-xplane/xplane_link.py` (Python 3.9+). Nie potrzebuje wtyczek.

1. Zainstaluj zależność: `pip install "websockets>=10.1"`.
2. Uruchom X-Plane, a potem `python xplane_link.py`. Jeśli X-Plane działa na innym komputerze, dodaj `--xplane-host ADRES`.
3. Sparuj telefon kodem wypisanym w oknie, jak w punkcie 3.

Most podaje pozycję, przyrządy, autopilota, radia, silniki i przyziemienia oraz przyjmuje komendy i sterowanie osiami. Nie ma danych nawigacyjnych z bazy (punkt 5), planu lotu GPS, odbiorników NAV ani rozglądania się. Nie uruchamiaj go razem z programem dla MSFS, bo oba używają portu 47810.

## Problemy

| Objaw | Co zrobić |
|---|---|
| „Czekam na MSFS…” mimo działającego symulatora | Poczekaj do załadowania menu MSFS; SimConnect startuje razem z symulatorem. Uruchom program bez uprawnień administratora (albo oba jako administrator). |
| Telefon nie widzi komputera | Profil sieci „Prywatna”, reguła zapory z punktu 2, wyłączona izolacja klientów Wi-Fi w routerze; ewentualnie wpisz IP ręcznie. |
| „Zły kod parowania” | Kod zmienia się przy każdym uruchomieniu programu — przepisz aktualny z okna. |
| Port 47810 zajęty | Inna kopia programu działa w zasobniku albo inny program używa portu. |
| Brak procedur / „nie znaleziono” w edytorze planu | MSFS musi być połączony i w pełni załadowany. W trybie symulacji dostępne są tylko EPWA i EPGD. |

Szczegóły techniczne: [link-protocol.md](link-protocol.md).
