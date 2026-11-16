//! OpenTakeoff dla Windows – rdzeń bez interfejsu: most do telefonu (protokół Link v1, parowanie, serwer,
//! rozgłaszanie, osie, komendy, dane nawigacyjne, tryb symulacji), dziennik lotów z synchronizacją,
//! baza lotnisk, pogoda i narzędzia, tłumaczenia, ścieżki i aktualizacje.

pub mod aero;
pub mod axes;
pub mod commands;
pub mod discovery;
pub mod lang;
pub mod log;
pub mod logbook;
pub mod navdata;
pub mod pairing;
pub mod paths;
pub mod proto;
pub mod server;
pub mod simnav;
pub mod simulation;
pub mod source;
pub mod touchdown;
pub mod update;
pub mod xplane;

/// Wersja programu (z tagu wydania; lokalnie z Cargo.toml).
pub fn version() -> &'static str {
    option_env!("OPENTAKEOFF_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}
