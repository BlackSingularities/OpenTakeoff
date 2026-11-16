//! Dziennik lotów na dysku: `flights.oab` (ten sam format co w telefonie), `trash.oab` (kosz),
//! `meta.json` (dane tylko z komputera: notatki, tagi, poprawione lotniska, ulubione, pochodzenie wpisu
//! i znaczniki usuniętych lotów do synchronizacji) oraz `draft.oab` (trwający lot).
//!
//! Zapis odbywa się w tle (z opóźnieniem 0,4 s), żeby edycja w oknie nie czekała na kompresję archiwum.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::{Condvar, Mutex};
use serde::{Deserialize, Serialize};

use super::archive;
use super::model::FlightLog;
use super::sync::{SyncFlight, SyncRequest, SyncResponse};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct FlightMeta {
    pub notes: String,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aircraft: Option<String>,
    pub favorite: bool,
    /// Skąd wpis: "pc" (nagrany tu), "import", albo nazwa telefonu.
    pub source: String,
    pub added_at: i64,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct MetaFile {
    flights: HashMap<String, FlightMeta>,
    /// Loty usunięte na komputerze lub w telefonie (id → czas) – synchronizacja ich nie przywraca.
    tombstones: HashMap<String, i64>,
}

struct Data {
    flights: Vec<Arc<FlightLog>>,
    trash: Vec<Arc<FlightLog>>,
    meta: MetaFile,
}

struct Saver {
    dirty: Mutex<bool>,
    wake: Condvar,
}

pub struct LogbookStore {
    dir: PathBuf,
    data: Mutex<Data>,
    revision: AtomicU64,
    saver: Arc<Saver>,
}

/// Wynik importu: dodane, pominięte (już są albo to ten sam lot nagrany drugi raz), odrzucone (w koszu).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImportResult {
    pub added: usize,
    pub skipped: usize,
}

fn read_archive(p: &Path) -> Vec<Arc<FlightLog>> {
    std::fs::read(p).ok().and_then(|b| archive::decode(&b).ok()).unwrap_or_default().into_iter().map(Arc::new).collect()
}

impl LogbookStore {
    pub fn open(dir: PathBuf) -> Arc<Self> {
        let _ = std::fs::create_dir_all(&dir);
        let mut flights = read_archive(&dir.join("flights.oab"));
        flights.sort_by(|a, b| b.start.cmp(&a.start));
        let trash = read_archive(&dir.join("trash.oab"));
        let meta = std::fs::read_to_string(dir.join("meta.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let store = Arc::new(Self {
            dir,
            data: Mutex::new(Data { flights, trash, meta }),
            revision: AtomicU64::new(1),
            saver: Arc::new(Saver { dirty: Mutex::new(false), wake: Condvar::new() }),
        });
        let weak = Arc::downgrade(&store);
        let saver = store.saver.clone();
        std::thread::Builder::new()
            .name("logbook-save".into())
            .spawn(move || loop {
                {
                    let mut d = saver.dirty.lock();
                    while !*d {
                        if saver.wake.wait_for(&mut d, Duration::from_secs(5)).timed_out() && weak.strong_count() == 0 {
                            return;
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(400));
                *saver.dirty.lock() = false;
                match weak.upgrade() {
                    Some(s) => s.save_now(),
                    None => return,
                }
            })
            .ok();
        store
    }

    /// Numer zmiany – widoki odświeżają się, gdy rośnie.
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn changed(&self) {
        self.revision.fetch_add(1, Ordering::SeqCst);
        *self.saver.dirty.lock() = true;
        self.saver.wake.notify_one();
    }

    /// Natychmiastowy zapis (przy zamykaniu programu).
    pub fn save_now(&self) {
        let (flights, trash, meta) = {
            let d = self.data.lock();
            let f: Vec<FlightLog> = d.flights.iter().map(|f| (**f).clone()).collect();
            let t: Vec<FlightLog> = d.trash.iter().map(|f| (**f).clone()).collect();
            (f, t, serde_json::to_vec_pretty(&d.meta).unwrap_or_default())
        };
        let r1 = crate::paths::write_atomic(&self.dir.join("flights.oab"), &archive::encode(&flights));
        let r2 = crate::paths::write_atomic(&self.dir.join("trash.oab"), &archive::encode(&trash));
        let r3 = crate::paths::write_atomic(&self.dir.join("meta.json"), &meta);
        if let Some(e) = [r1, r2, r3].into_iter().find_map(|r| r.err()) {
            crate::log::error(&format!("Dziennik lotów: zapis nie powiódł się: {e}"));
        }
    }

    pub fn flights(&self) -> Vec<Arc<FlightLog>> {
        self.data.lock().flights.clone()
    }

    pub fn trash(&self) -> Vec<Arc<FlightLog>> {
        self.data.lock().trash.clone()
    }

    pub fn len(&self) -> usize {
        self.data.lock().flights.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, id: &str) -> Option<Arc<FlightLog>> {
        let d = self.data.lock();
        d.flights.iter().chain(d.trash.iter()).find(|f| f.id == id).cloned()
    }

    pub fn meta(&self, id: &str) -> FlightMeta {
        self.data.lock().meta.flights.get(id).cloned().unwrap_or_default()
    }

    pub fn all_meta(&self) -> HashMap<String, FlightMeta> {
        self.data.lock().meta.flights.clone()
    }

    /// Lot z poprawkami z komputera (lotniska, samolot).
    pub fn effective(&self, f: &FlightLog) -> FlightLog {
        let m = self.meta(&f.id);
        let mut out = f.clone();
        if m.from.is_some() {
            out.from = m.from;
        }
        if m.to.is_some() {
            out.to = m.to;
        }
        if m.aircraft.is_some() {
            out.aircraft = m.aircraft;
        }
        out
    }

    pub fn update_meta(&self, id: &str, edit: impl FnOnce(&mut FlightMeta)) {
        {
            let mut d = self.data.lock();
            let m = d.meta.flights.entry(id.to_string()).or_default();
            edit(m);
        }
        self.changed();
    }

    /// Nowy albo zmieniony lot (ten sam id zastępuje poprzedni wpis).
    pub fn put(&self, f: FlightLog, source: &str) {
        {
            let mut d = self.data.lock();
            d.flights.retain(|x| x.id != f.id);
            let m = d.meta.flights.entry(f.id.clone()).or_default();
            if m.source.is_empty() {
                m.source = source.to_string();
                m.added_at = crate::source::unix_ms();
            }
            let pos = d.flights.iter().position(|x| x.start < f.start).unwrap_or(d.flights.len());
            d.flights.insert(pos, Arc::new(f));
        }
        self.changed();
    }

    /// Zmiana samego lotu (np. znaczniki) bez zmiany pochodzenia.
    pub fn replace(&self, f: FlightLog) {
        {
            let mut d = self.data.lock();
            if let Some(slot) = d.flights.iter_mut().find(|x| x.id == f.id) {
                *slot = Arc::new(f);
            }
        }
        self.changed();
    }

    /// Do kosza – z kosza można przywrócić; synchronizacja nie przywraca usuniętych lotów.
    pub fn delete(&self, ids: &[String]) -> usize {
        let n;
        {
            let mut d = self.data.lock();
            let now = crate::source::unix_ms();
            let (gone, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut d.flights).into_iter().partition(|f| ids.contains(&f.id));
            d.flights = keep;
            n = gone.len();
            for f in gone {
                d.meta.tombstones.insert(f.id.clone(), now);
                d.trash.retain(|t| t.id != f.id);
                d.trash.push(f);
            }
        }
        if n > 0 {
            self.changed();
        }
        n
    }

    pub fn restore(&self, ids: &[String]) -> usize {
        let n;
        {
            let mut d = self.data.lock();
            let (back, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut d.trash).into_iter().partition(|f| ids.contains(&f.id));
            d.trash = keep;
            n = back.len();
            for f in back {
                d.meta.tombstones.remove(&f.id);
                d.flights.retain(|x| x.id != f.id);
                d.flights.push(f);
            }
            d.flights.sort_by(|a, b| b.start.cmp(&a.start));
        }
        if n > 0 {
            self.changed();
        }
        n
    }

    /// Trwałe usunięcie z kosza (znacznik usunięcia zostaje – telefon nie przyśle lotu ponownie).
    pub fn purge(&self, ids: &[String]) -> usize {
        let n;
        {
            let mut d = self.data.lock();
            let before = d.trash.len();
            d.trash.retain(|f| !ids.contains(&f.id));
            n = before - d.trash.len();
            for id in ids {
                d.meta.flights.remove(id);
            }
        }
        if n > 0 {
            self.changed();
        }
        n
    }

    pub fn empty_trash(&self) -> usize {
        let ids: Vec<String> = self.data.lock().trash.iter().map(|f| f.id.clone()).collect();
        self.purge(&ids)
    }

    /// Import z dedupikacją: pomija loty o znanym id, usunięte i nagrane drugi raz (pokrywający się czas).
    pub fn import(&self, incoming: Vec<FlightLog>, source: &str) -> ImportResult {
        let mut res = ImportResult::default();
        {
            let mut d = self.data.lock();
            let now = crate::source::unix_ms();
            for f in incoming {
                let known = d.flights.iter().any(|x| x.id == f.id || x.same_flight(f.start, f.end, f.aircraft.as_deref()))
                    || d.meta.tombstones.contains_key(&f.id)
                    || d.trash.iter().any(|x| x.id == f.id);
                if known || f.track.len() < 2 {
                    res.skipped += 1;
                    continue;
                }
                let m = d.meta.flights.entry(f.id.clone()).or_default();
                m.source = source.to_string();
                m.added_at = now;
                d.flights.push(Arc::new(f));
                res.added += 1;
            }
            d.flights.sort_by(|a, b| b.start.cmp(&a.start));
        }
        if res.added > 0 {
            self.changed();
        }
        res
    }

    pub fn export(&self, ids: &[String]) -> Vec<u8> {
        let d = self.data.lock();
        let list: Vec<FlightLog> = d.flights.iter().filter(|f| ids.is_empty() || ids.contains(&f.id)).map(|f| (**f).clone()).collect();
        drop(d);
        archive::encode(&list)
    }

    // ---------- szkic trwającego lotu ----------

    pub fn save_draft(&self, f: &FlightLog) {
        let _ = crate::paths::write_atomic(&self.dir.join("draft.oab"), &archive::encode(std::slice::from_ref(f)));
    }

    pub fn take_draft(&self) -> Option<FlightLog> {
        let p = self.dir.join("draft.oab");
        let f = std::fs::read(&p).ok().and_then(|b| archive::decode(&b).ok()).and_then(|v| v.into_iter().next());
        let _ = std::fs::remove_file(p);
        f
    }

    pub fn clear_draft(&self) {
        let _ = std::fs::remove_file(self.dir.join("draft.oab"));
    }

    // ---------- synchronizacja z telefonem ----------

    /// Plan synchronizacji z listy lotów telefonu (p. 6 protokołu).
    pub fn sync_plan(&self, req: &SyncRequest, device: &str) -> SyncResponse {
        let phone_deleted: HashSet<&str> = req.deleted.iter().map(|s| s.as_str()).collect();
        // Loty usunięte w telefonie – do kosza na komputerze (z kosza można je przywrócić)
        let to_trash: Vec<String> = {
            let d = self.data.lock();
            d.flights.iter().filter(|f| phone_deleted.contains(f.id.as_str())).map(|f| f.id.clone()).collect()
        };
        if !to_trash.is_empty() {
            crate::log::info(&format!("{device}: {} lot(y) usunięte w telefonie – przeniesione do kosza", to_trash.len()));
            self.delete(&to_trash);
        }
        let d = self.data.lock();
        let phone_ids: HashSet<&str> = req.flights.iter().map(|f| f.id.as_str()).collect();
        let live: HashSet<&str> = d.flights.iter().map(|f| f.id.as_str()).collect();
        let trash: HashSet<&str> = d.trash.iter().map(|f| f.id.as_str()).collect();
        let want = req
            .flights
            .iter()
            .filter(|p| {
                !live.contains(p.id.as_str())
                    && !trash.contains(p.id.as_str())
                    && !d.meta.tombstones.contains_key(&p.id)
                    && !d.flights.iter().any(|l| l.same_flight(p.start, p.end, p.aircraft.as_deref()))
            })
            .map(|p| p.id.clone())
            .collect();
        let offer = d
            .flights
            .iter()
            .filter(|l| {
                !phone_ids.contains(l.id.as_str())
                    && !phone_deleted.contains(l.id.as_str())
                    && !req.flights.iter().any(|p| l.same_flight(p.start, p.end, p.aircraft.as_deref()))
            })
            .map(|l| l.id.clone())
            .collect();
        let remove = req.flights.iter().filter(|p| d.meta.tombstones.contains_key(&p.id) && !live.contains(p.id.as_str())).map(|p| p.id.clone()).collect();
        SyncResponse { v: 1, want, offer, remove, flights: d.flights.len() }
    }

    /// Lista lotów do porównania (bez torów).
    pub fn sync_list(&self) -> Vec<SyncFlight> {
        self.data.lock().flights.iter().map(|f| SyncFlight { id: f.id.clone(), start: f.start, end: f.end, aircraft: f.aircraft.clone() }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logbook::model::TrackPoint;

    fn flight(id: &str, start: i64, end: i64) -> FlightLog {
        let track = vec![TrackPoint { t: start, lat: 52.0, lon: 21.0, ..Default::default() }, TrackPoint { t: end, lat: 52.1, lon: 21.1, ..Default::default() }];
        FlightLog { id: id.into(), start, end, aircraft: Some("C172".into()), track, ..Default::default() }
    }

    fn temp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("otk-test-{}", crate::logbook::model::new_id()));
        let _ = std::fs::create_dir_all(&d);
        d
    }

    #[test]
    fn put_delete_restore_persist() {
        let dir = temp();
        {
            let s = LogbookStore::open(dir.clone());
            s.put(flight("a", 1000, 400_000), "pc");
            s.put(flight("b", 500_000, 900_000), "pc");
            assert_eq!(s.flights()[0].id, "b");
            assert_eq!(s.delete(&["a".into()]), 1);
            assert_eq!(s.trash().len(), 1);
            s.restore(&["a".into()]);
            assert_eq!(s.len(), 2);
            s.update_meta("a", |m| m.notes = "test".into());
            s.save_now();
        }
        let s = LogbookStore::open(dir.clone());
        assert_eq!(s.len(), 2);
        assert_eq!(s.meta("a").notes, "test");
        assert_eq!(s.meta("a").source, "pc");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn import_dedupes() {
        let dir = temp();
        let s = LogbookStore::open(dir.clone());
        s.put(flight("pc1", 0, 3_600_000), "pc");
        let r = s.import(vec![flight("ph1", 60_000, 3_650_000), flight("ph2", 5_000_000, 6_000_000), flight("pc1", 0, 1)], "Pixel");
        assert_eq!(r, ImportResult { added: 1, skipped: 2 });
        s.delete(&["ph2".into()]);
        assert_eq!(s.import(vec![flight("ph2", 5_000_000, 6_000_000)], "Pixel").added, 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sync_plan_both_ways() {
        let dir = temp();
        let s = LogbookStore::open(dir.clone());
        s.put(flight("pc-only", 10_000_000, 11_000_000), "pc");
        s.put(flight("shared", 20_000_000, 21_000_000), "pc");
        s.put(flight("pc-dup", 30_000_000, 33_000_000), "pc");
        s.put(flight("gone", 40_000_000, 41_000_000), "pc");
        s.put(flight("phone-deleted", 50_000_000, 51_000_000), "pc");
        s.delete(&["gone".into()]);
        let req = SyncRequest {
            v: 1,
            device: "Pixel".into(),
            flights: vec![
                SyncFlight { id: "shared".into(), start: 20_000_000, end: 21_000_000, aircraft: None },
                SyncFlight { id: "phone-only".into(), start: 60_000_000, end: 61_000_000, aircraft: None },
                SyncFlight { id: "phone-dup".into(), start: 30_100_000, end: 33_100_000, aircraft: Some("C172".into()) },
                SyncFlight { id: "gone".into(), start: 40_000_000, end: 41_000_000, aircraft: None },
            ],
            deleted: vec!["phone-deleted".into()],
        };
        let plan = s.sync_plan(&req, "Pixel");
        assert_eq!(plan.want, vec!["phone-only".to_string()]);
        assert_eq!(plan.offer, vec!["pc-only".to_string()]);
        assert_eq!(plan.remove, vec!["gone".to_string()]);
        assert!(s.get("phone-deleted").is_some_and(|_| s.trash().iter().any(|f| f.id == "phone-deleted")));
        let _ = std::fs::remove_dir_all(dir);
    }
}
