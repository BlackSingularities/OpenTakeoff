//! Wyjaśnienia skrótów lotniczych w podpowiedziach: po najechaniu na tekst ze skrótem (ZFW, TOC, METAR,
//! V1…) albo kodem ICAO lotniska interfejs pokazuje, co znaczy. Teksty wyjaśnień są po polsku i idą przez
//! `tr`, więc tłumaczenia leżą w plikach .po razem z resztą programu.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use opentakeoff_core::aero::airports;
use opentakeoff_core::lang::tr;

/// Dłuższe teksty (zdania, opisy) mają wyjaśnienia w treści – bez podpowiedzi.
const MAX_LEN: usize = 64;
/// Najwięcej wyjaśnień w jednym dymku.
const MAX_LINES: usize = 5;

/// (skrót, wyjaśnienie, tylko gdy to cały tekst – dla krótkich słów, które bywają też zwykłymi słowami).
const ABBR: &[(&str, &str, bool)] = &[
    ("ZFW", "Zero Fuel Weight – masa samolotu bez paliwa (z ładunkiem i pasażerami)", false),
    ("MZFW", "Maximum Zero Fuel Weight – największa dopuszczalna masa bez paliwa", false),
    ("TOW", "Take-Off Weight – masa startowa", false),
    ("MTOW", "Maximum Take-Off Weight – największa dopuszczalna masa startowa", false),
    ("LDW", "Landing Weight – masa do lądowania", false),
    ("LW", "Landing Weight – masa do lądowania", false),
    ("MLW", "Maximum Landing Weight – największa dopuszczalna masa do lądowania", false),
    ("OEW", "Operating Empty Weight – masa pustego samolotu gotowego do lotu (z załogą i wyposażeniem)", false),
    ("PAX", "Pasażerowie", false),
    ("CG", "Center of Gravity – położenie środka ciężkości", false),
    ("MAC", "Mean Aerodynamic Chord – średnia cięciwa aerodynamiczna; środek ciężkości podaje się w % MAC", false),
    ("BLOK", "Paliwo blokowe – całe paliwo na pokładzie przed uruchomieniem (kołowanie, trasa i rezerwy)", false),
    ("BLOCK", "Block – czas blokowy (od odsunięcia od stanowiska do postoju na stanowisku) albo paliwo blokowe (całe paliwo przed uruchomieniem)", false),
    ("BLOQUE", "Block – czas blokowy (od odsunięcia od stanowiska do postoju na stanowisku) albo paliwo blokowe (całe paliwo przed uruchomieniem)", false),
    ("FF", "Fuel Flow – przepływ paliwa (zużycie na godzinę)", false),
    ("ETE", "Estimated Time En route – szacowany czas lotu", false),
    ("EET", "Estimated Elapsed Time – szacowany czas lotu do punktu", false),
    ("ETA", "Estimated Time of Arrival – szacowana godzina przylotu", false),
    ("ETD", "Estimated Time of Departure – szacowana godzina odlotu", false),
    ("STD", "Scheduled Time of Departure – planowa godzina odlotu", false),
    ("STA", "Scheduled Time of Arrival – planowa godzina przylotu", false),
    ("OUT", "OUT – odsunięcie od stanowiska (off-block)", true),
    ("OFF", "OFF – oderwanie od pasa (start)", true),
    ("ON", "ON – przyziemienie na pasie", true),
    ("IN", "IN – postój na stanowisku (on-block)", true),
    ("UTC", "Uniwersalny czas koordynowany (czas Zulu, Z) – w lotnictwie wszystkie godziny podaje się w UTC", false),
    ("LT", "Local Time – czas lokalny", false),
    ("CI", "Cost Index – kompromis między kosztem paliwa a kosztem czasu (0 – najoszczędniej, więcej – szybciej)", false),
    ("COST INDEX", "Cost Index – kompromis między kosztem paliwa a kosztem czasu (0 – najoszczędniej, więcej – szybciej)", false),
    ("TOC", "Top of Climb – koniec wznoszenia, początek przelotu", false),
    ("T/C", "Top of Climb – koniec wznoszenia, początek przelotu", false),
    ("TOD", "Top of Descent – początek zniżania", false),
    ("T/D", "Top of Descent – początek zniżania", false),
    ("CLB", "Climb – wznoszenie", false),
    ("CRZ", "Cruise – przelot", false),
    ("DES", "Descent – zniżanie", false),
    ("DSC", "Descent – zniżanie", false),
    ("APP", "Approach – podejście do lądowania", false),
    ("APPR", "Approach – podejście do lądowania", false),
    ("T/O", "Take-off – start", false),
    ("LDG", "Landing – lądowanie", false),
    ("TOGA", "Take-Off / Go-Around – pełny ciąg do startu albo odejścia na drugi krąg", false),
    ("FLEX", "Obniżony ciąg startowy (zakładana temperatura) – oszczędza silniki", false),
    ("FL", "Flight Level – poziom lotu w setkach stóp przy ciśnieniu wzorcowym 1013,25 hPa (FL230 = 23 000 ft)", false),
    ("ALT", "Altitude – wysokość", false),
    ("AGL", "Above Ground Level – wysokość nad terenem", false),
    ("MSL", "Mean Sea Level – wysokość nad średnim poziomem morza", false),
    ("AMSL", "Above Mean Sea Level – wysokość nad poziomem morza", false),
    ("ELEV", "Elevation – wysokość lotniska nad poziomem morza", false),
    ("TA", "Transition Altitude – wysokość przejściowa (powyżej: ciśnienie wzorcowe 1013 hPa)", false),
    ("TL", "Transition Level – poziom przejściowy (poniżej: QNH lotniska)", false),
    ("VS", "Vertical Speed – prędkość pionowa (ft/min)", false),
    ("ROC", "Rate of Climb – prędkość wznoszenia", false),
    ("ROD", "Rate of Descent – prędkość zniżania", false),
    ("MDA", "Minimum Descent Altitude – najniższa wysokość zniżania bez kontaktu wzrokowego z pasem", false),
    ("DA", "Decision Altitude – wysokość decyzji na podejściu", false),
    ("DH", "Decision Height – wysokość decyzji nad progiem pasa", false),
    ("GS", "Ground Speed – prędkość względem ziemi (przy ILS: ścieżka schodzenia, glideslope)", false),
    ("TAS", "True Airspeed – prędkość rzeczywista względem powietrza", false),
    ("IAS", "Indicated Airspeed – prędkość przyrządowa (pokazywana przez prędkościomierz)", false),
    ("CAS", "Calibrated Airspeed – prędkość przyrządowa poprawiona o błędy instalacji", false),
    ("MACH", "Liczba Macha – prędkość jako ułamek prędkości dźwięku (M.78 = 78% prędkości dźwięku)", false),
    ("V1", "Prędkość decyzji – powyżej niej start trzeba kontynuować", false),
    ("VR", "Prędkość rotacji – podniesienie przedniego podwozia", false),
    ("V2", "Bezpieczna prędkość wznoszenia po starcie (także z niesprawnym silnikiem)", false),
    ("VREF", "Prędkość odniesienia do lądowania (nad progiem pasa)", false),
    ("VAPP", "Prędkość podejścia (VREF plus poprawka na wiatr)", false),
    ("kt", "Węzły – mile morskie na godzinę (1 kt = 1,852 km/h)", false),
    ("fpm", "Stopy na minutę – prędkość pionowa", false),
    ("ft", "Stopy (1 ft = 0,3048 m)", false),
    ("NM", "Nautical Mile – mila morska (1852 m)", false),
    ("Σ NM", "Łączny dystans od startu w milach morskich", false),
    ("SPD", "Speed – prędkość", false),
    ("HW", "Headwind – składowa wiatru czołowego", false),
    ("TW", "Tailwind – składowa wiatru w ogon", false),
    ("XW", "Crosswind – składowa wiatru bocznego", false),
    ("VRB", "Wiatr o zmiennym kierunku", false),
    ("CALM", "Cisza – wiatr słabszy niż 1 kt", false),
    ("OAT", "Outside Air Temperature – temperatura powietrza na zewnątrz", false),
    ("SAT", "Static Air Temperature – temperatura powietrza (bez nagrzania od prędkości)", false),
    ("TAT", "Total Air Temperature – temperatura powietrza z nagrzaniem od prędkości", false),
    ("ISA", "International Standard Atmosphere – odchyłka temperatury od atmosfery wzorcowej (ISA +5 = o 5 °C cieplej)", false),
    ("QNH", "Ciśnienie zredukowane do poziomu morza – nastawione na wysokościomierzu daje wysokość nad poziomem morza", false),
    ("QFE", "Ciśnienie na poziomie lotniska – wysokościomierz pokazuje wysokość nad lotniskiem", false),
    ("hPa", "Hektopaskale – jednostka ciśnienia", false),
    ("inHg", "Cale słupa rtęci – jednostka ciśnienia (USA)", false),
    ("METAR", "Bieżąca depesza pogodowa z lotniska (zwykle co 30 albo 60 minut)", false),
    ("TAF", "Terminal Aerodrome Forecast – prognoza pogody dla lotniska", false),
    ("SIGMET", "Ostrzeżenie o niebezpiecznych zjawiskach pogodowych", false),
    ("ATIS", "Automatyczna informacja lotniskowa (pogoda, pas w użyciu)", false),
    ("NOTAM", "Notice to Airmen – komunikat o zmianach i utrudnieniach", false),
    ("VFR", "Warunki VFR – lot z widocznością: pułap powyżej 3000 ft i widzialność powyżej 5 mil", false),
    ("MVFR", "Marginalne VFR – pułap 1000–3000 ft albo widzialność 3–5 mil", false),
    ("IFR", "Warunki IFR – lot według przyrządów: pułap 500–1000 ft albo widzialność 1–3 mile", false),
    ("LIFR", "Niskie IFR – pułap poniżej 500 ft albo widzialność poniżej 1 mili", false),
    ("CAVOK", "Ceiling And Visibility OK – widzialność 10 km i więcej, bez chmur poniżej 5000 ft", false),
    ("FEW", "Nieliczne chmury (1–2 oktanty)", false),
    ("SCT", "Rozproszone chmury (3–4 oktanty)", false),
    ("BKN", "Chmury z przerwami (5–7 oktantów) – liczy się jako pułap", false),
    ("OVC", "Całkowite zachmurzenie (8 oktantów)", false),
    ("CB", "Cumulonimbus – chmura burzowa", false),
    ("TCU", "Towering Cumulus – wypiętrzony cumulus", false),
    ("CRS", "Course – kurs odcinka trasy", false),
    ("HDG", "Heading – kurs (kierunek, w który skierowany jest nos samolotu)", false),
    ("TRK", "Track – tor (kierunek rzeczywistego ruchu nad ziemią)", false),
    ("BRG", "Bearing – namiar na punkt", false),
    ("DCT", "Direct – lot bezpośrednio do punktu", false),
    ("SID", "Standard Instrument Departure – standardowa procedura odlotu", false),
    ("STAR", "Standard Terminal Arrival Route – standardowa procedura dolotu", false),
    ("IAF", "Initial Approach Fix – punkt początkowy podejścia", false),
    ("FAF", "Final Approach Fix – punkt podejścia końcowego", false),
    ("VOR", "Radiolatarnia VOR – podaje radial (kierunek od stacji)", false),
    ("NDB", "Radiolatarnia bezkierunkowa – wskazuje kierunek do stacji na ADF", false),
    ("DME", "Distance Measuring Equipment – odległość do stacji w milach morskich", false),
    ("ILS", "Instrument Landing System – system precyzyjnego podejścia (kierunek i ścieżka)", false),
    ("LOC", "Localizer – kierunek podejścia ILS", false),
    ("RNAV", "Area Navigation – nawigacja obszarowa (GPS, FMS)", false),
    ("GPS", "Satelitarny system nawigacji", false),
    ("ADF", "Automatic Direction Finder – radiokompas (wskazuje NDB)", false),
    ("FMS", "Flight Management System – komputer zarządzania lotem", false),
    ("FMC", "Flight Management Computer – komputer zarządzania lotem", false),
    ("MCDU", "Panel z klawiaturą do obsługi FMS (Airbus)", false),
    ("CDI", "Course Deviation Indicator – wskaźnik odchylenia od kursu", false),
    ("OBS", "Omni Bearing Selector – wybór radialu VOR", false),
    ("HSI", "Horizontal Situation Indicator – wskaźnik sytuacji poziomej", false),
    ("VAR", "Deklinacja magnetyczna – różnica między północą geograficzną a magnetyczną", false),
    ("RWY", "Runway – pas startowy", false),
    ("TWY", "Taxiway – droga kołowania", false),
    ("THR", "Threshold – próg pasa", false),
    ("TORA", "Take-Off Run Available – rozporządzalna długość rozbiegu", false),
    ("TODA", "Take-Off Distance Available – rozporządzalna długość startu", false),
    ("ASDA", "Accelerate-Stop Distance Available – długość do przerwania startu", false),
    ("LDA", "Landing Distance Available – rozporządzalna długość lądowania", false),
    ("ICAO", "Organizacja Międzynarodowego Lotnictwa Cywilnego; kod ICAO lotniska ma 4 litery (np. EPWA)", false),
    ("IATA", "Międzynarodowe Zrzeszenie Przewoźników; kod IATA lotniska ma 3 litery (np. WAW)", false),
    ("ATC", "Air Traffic Control – kontrola ruchu lotniczego", false),
    ("FPL", "Plan lotu ATC w formacie ICAO", false),
    ("OFP", "Operational Flight Plan – operacyjny plan lotu (paliwo, masy, trasa, pogoda)", false),
    ("TLR", "Takeoff and Landing Report – raport osiągów startu i lądowania", false),
    ("FLAPS", "Ustawienie klap", false),
    ("XPDR", "Transponder – kod squawk dla kontroli ruchu", false),
    ("SQK", "Squawk – kod transpondera", false),
    ("COM", "Radio do łączności głosowej", false),
    ("NAV", "Radio nawigacyjne (VOR, ILS)", false),
    ("AP", "Autopilot", false),
    ("A/THR", "Autothrust – automatyczny ciąg (Airbus)", false),
    ("A/T", "Autothrottle – automatyczna przepustnica", false),
    ("FD", "Flight Director – wskazówki sterowania na PFD", false),
    ("LNAV", "Lateral Navigation – prowadzenie po trasie z FMS", false),
    ("VNAV", "Vertical Navigation – profil pionowy z FMS", false),
    ("PFD", "Primary Flight Display – główny wyświetlacz lotu", false),
    ("ND", "Navigation Display – wyświetlacz nawigacyjny", false),
    ("ECAM", "System monitorowania samolotu i ostrzeżeń (Airbus)", false),
    ("EICAS", "System wskazań silników i ostrzeżeń załogi (Boeing, Embraer)", false),
    ("APU", "Auxiliary Power Unit – pomocnicza jednostka napędowa (prąd i powietrze na ziemi)", false),
    ("N1", "Obroty wentylatora silnika w % – główny wskaźnik ciągu", false),
    ("N2", "Obroty rdzenia silnika w %", false),
    ("EGT", "Exhaust Gas Temperature – temperatura spalin", false),
    ("ITT", "Inter-Turbine Temperature – temperatura między turbinami", false),
    ("TRQ", "Torque – moment obrotowy silnika turbośmigłowego", false),
    ("RPM", "Obroty na minutę", false),
    ("CHT", "Cylinder Head Temperature – temperatura głowic cylindrów", false),
    ("RA", "Radio Altimeter – radiowysokościomierz (w METAR: deszcz)", false),
    ("MSFS", "Microsoft Flight Simulator", false),
];

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '/'
}

/// Pozycja `key` w `text` jako osobnego słowa (nie fragmentu dłuższego).
fn find_word(text: &str, key: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = text[from..].find(key) {
        let at = from + i;
        let before = text[..at].chars().next_back();
        let after = text[at + key.len()..].chars().next();
        if !before.is_some_and(is_word) && !after.is_some_and(is_word) {
            return Some(at);
        }
        from = at + key.len().max(1);
        while from < text.len() && !text.is_char_boundary(from) {
            from += 1;
        }
    }
    None
}

/// Znalezione wyjaśnienie: pozycja i długość w tekście, skrót, treść.
struct Hit {
    at: usize,
    len: usize,
    key: String,
    text: String,
}

fn overlaps(found: &[Hit], at: usize, len: usize) -> bool {
    found.iter().any(|f| at < f.at + f.len && f.at < at + len)
}

fn entry(key: &str) -> Option<&'static str> {
    ABBR.iter().find(|a| a.0 == key).map(|a| tr(a.1))
}

/// Czteroliterowe słowa z tekstów interfejsu (np. „WAGI”, „CZAS” w nagłówkach) – nie są kodami lotnisk,
/// nawet jeśli gdzieś na świecie istnieje lotnisko o takim kodzie. Brane ze wszystkich plików .po, tylko
/// słowa pisane tam małymi literami (kody ICAO w przykładach są wielkimi).
fn ui_words() -> &'static HashSet<String> {
    static W: OnceLock<HashSet<String>> = OnceLock::new();
    W.get_or_init(|| {
        let mut s = HashSet::new();
        for (_, po) in opentakeoff_core::lang::LANGS {
            for w in po.split(|c: char| !c.is_alphabetic()) {
                if w.chars().count() == 4 && w.chars().any(|c| c.is_lowercase()) {
                    s.insert(w.to_uppercase());
                }
            }
        }
        s
    })
}

fn explain_uncached(text: &str) -> String {
    let t = text.trim();
    if t.is_empty() || t.chars().count() > MAX_LEN {
        return String::new();
    }
    let mut found: Vec<Hit> = Vec::new();
    // dłuższe skróty najpierw – „T/C” nie rozpada się na krótsze, „MZFW” nie daje „ZFW”
    let mut keys: Vec<&(&str, &str, bool)> = ABBR.iter().collect();
    keys.sort_by_key(|k| std::cmp::Reverse(k.0.len()));
    for (k, e, exact) in keys {
        let at = if *exact {
            if t == *k { text.find(k) } else { None }
        } else {
            find_word(text, k)
        };
        let Some(at) = at else { continue };
        let e = tr(e);
        if !overlaps(&found, at, k.len()) && !found.iter().any(|f| f.text == e) {
            found.push(Hit { at, len: k.len(), key: k.to_string(), text: e.to_string() });
        }
    }
    // słowa: poziomy lotu (FL230), godziny UTC (15:30Z) i kody ICAO lotnisk (EPWA)
    let mut words: Vec<(usize, &str)> = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices().chain(std::iter::once((text.len(), ' '))) {
        match (c.is_alphanumeric(), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                words.push((s, &text[s..i]));
                start = None;
            }
            _ => {}
        }
    }
    for (at, w) in words {
        if overlaps(&found, at, w.len()) {
            continue;
        }
        let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
        if w.len() > 2 && w.starts_with("FL") && digits(&w[2..]) {
            if let Some(e) = entry("FL") {
                found.push(Hit { at, len: w.len(), key: w.to_string(), text: e.to_string() });
            }
        } else if w.len() >= 3 && w.ends_with('Z') && digits(&w[..w.len() - 1]) {
            if !found.iter().any(|f| f.key == "UTC" || f.key == "Z") {
                if let Some(e) = entry("UTC") {
                    found.push(Hit { at, len: w.len(), key: "Z".to_string(), text: e.to_string() });
                }
            }
        } else if w.len() == 4 && w.chars().all(|c| c.is_ascii_uppercase()) && !ui_words().contains(w) {
            if let Some(a) = airports::db().and_then(|db| db.get(w).cloned()) {
                let place = if a.city.is_empty() || a.name.contains(&a.city) { String::new() } else { format!(", {}", a.city) };
                let country = if a.country.is_empty() { String::new() } else { format!(" ({})", a.country) };
                found.push(Hit { at, len: w.len(), key: w.to_string(), text: format!("{}{}{}", a.name, place, country) });
            }
        }
    }
    found.sort_by_key(|f| f.at);
    found.truncate(MAX_LINES);
    found.iter().map(|f| format!("{}: {}", f.key, f.text)).collect::<Vec<_>>().join("\n")
}

thread_local! {
    static CACHE: RefCell<HashMap<(String, bool), String>> = RefCell::new(HashMap::new());
}

/// Podpowiedź dla tekstu: wyjaśnienia skrótów i nazwy lotnisk w kolejności wystąpienia, albo pusty tekst.
pub fn explain(text: &str) -> String {
    let key = (text.to_string(), airports::db().is_some());
    if let Some(v) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return v;
    }
    let v = explain_uncached(text);
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 20_000 {
            c.clear();
        }
        c.insert(key, v.clone());
    });
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_abbreviations_as_words() {
        assert!(explain_uncached("ZFW").starts_with("ZFW: "));
        assert!(explain_uncached("V1 137 kt · VR 143 kt").contains("VR: "));
        assert!(explain_uncached("FL230").starts_with("FL230: "));
        assert!(explain_uncached("16:22Z").starts_with("Z: "));
        // nie w środku słowa; krótkie słowa tylko jako cały tekst
        assert_eq!(explain_uncached("START"), "");
        assert_eq!(explain_uncached("ON TIME"), "");
        assert!(explain_uncached("ON").starts_with("ON: "));
        // T/C to jeden skrót, MZFW nie daje dodatkowo ZFW
        assert_eq!(explain_uncached("T/C").lines().count(), 1);
        assert_eq!(explain_uncached("MZFW").lines().count(), 1);
        assert_eq!(explain_uncached("Zwykły opis bez skrótów"), "");
    }
}
