//! Źródło stanu samolotu i odbiorca komend telefonu: MSFS (SimConnect) albo tryb symulacji.

use std::sync::Arc;

use crate::axes::AxisController;
use crate::commands::Validated;
use crate::navdata::NavDataProvider;
use crate::proto::FlightState;

#[derive(Clone, Debug, PartialEq)]
pub struct CommandResult {
    pub ok: bool,
    pub code: Option<&'static str>,
    pub message: Option<String>,
}

impl CommandResult {
    pub fn success() -> Self {
        Self { ok: true, code: None, message: None }
    }
    pub fn fail(code: &'static str, message: impl Into<String>) -> Self {
        Self { ok: false, code: Some(code), message: Some(message.into()) }
    }
}

pub trait FlightSource: Send + Sync {
    /// "msfs" albo "simulation".
    fn kind(&self) -> &'static str;
    /// Czy są aktualne dane (MSFS: połączono i przyszła pierwsza paczka).
    fn is_connected(&self) -> bool;
    fn status_text(&self) -> String;
    /// Ostatnia migawka albo None.
    fn snapshot(&self) -> Option<FlightState>;
    /// Wykonanie komendy z telefonu (może blokować do kilku sekund – wołane z wątku sesji).
    fn execute(&self, cmd: &Validated) -> CommandResult;
    fn axes(&self) -> Arc<AxisController>;
    fn navdata(&self) -> Option<Arc<dyn NavDataProvider>>;
    fn start(&self);
    fn stop(&self);
    /// Nazwa symulatora do wyświetlenia (np. „MSFS 2024”).
    fn sim_name(&self) -> String {
        String::new()
    }
}

/// Czas monotoniczny w ms (do osi, zatrzasku przyziemień i limitów).
pub fn tick_ms() -> i64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as i64
}

/// Czas w ms od epoki Unix.
pub fn unix_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
