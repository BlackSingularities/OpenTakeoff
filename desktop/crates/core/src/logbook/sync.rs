//! Synchronizacja dziennika lotów z telefonem (HTTP na porcie Linka, docs/link-protocol.md p. 6).
//!
//! 1. `POST /v1/logbook/sync` – telefon podaje swoje loty (id, czasy, samolot) i loty, które usunął.
//!    Komputer przenosi usunięte do kosza i odpowiada: `want` (przyślij), `offer` (mam dla ciebie),
//!    `remove` (usunięte na komputerze – usuń u siebie). Lot nagrany jednocześnie na obu urządzeniach
//!    (pokrywający się czas) nie jest przesyłany w żadną stronę.
//! 2. `POST /v1/logbook/upload` – archiwum .oab z lotami `want`.
//! 3. `POST /v1/logbook/download` – `{"ids": [...]}` → archiwum .oab z lotami `offer`.

use serde::{Deserialize, Serialize};

use super::archive;
use super::store::{ImportResult, LogbookStore};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncFlight {
    pub id: String,
    pub start: i64,
    pub end: i64,
    pub aircraft: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncRequest {
    pub v: i32,
    pub device: String,
    pub flights: Vec<SyncFlight>,
    pub deleted: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncResponse {
    pub v: i32,
    pub want: Vec<String>,
    pub offer: Vec<String>,
    pub remove: Vec<String>,
    /// Liczba lotów na komputerze (do komunikatu w telefonie).
    pub flights: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DownloadRequest {
    pub ids: Vec<String>,
}

pub trait SyncHandler: Send + Sync {
    fn plan(&self, req: &SyncRequest, device: &str) -> SyncResponse;
    fn upload(&self, bytes: &[u8], device: &str) -> Result<ImportResult, String>;
    fn download(&self, ids: &[String]) -> Vec<u8>;
    fn list(&self) -> Vec<SyncFlight>;
}

impl SyncHandler for LogbookStore {
    fn plan(&self, req: &SyncRequest, device: &str) -> SyncResponse {
        self.sync_plan(req, device)
    }

    fn upload(&self, bytes: &[u8], device: &str) -> Result<ImportResult, String> {
        let flights = archive::decode(bytes)?;
        Ok(self.import(flights, device))
    }

    fn download(&self, ids: &[String]) -> Vec<u8> {
        if ids.is_empty() {
            return archive::encode(&[]);
        }
        self.export(ids)
    }

    fn list(&self) -> Vec<SyncFlight> {
        self.sync_list()
    }
}
