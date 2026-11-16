//! Sterowanie osiami z telefonu (przechylanie, drążek): przycinanie wychyleń, jeden właściciel naraz
//! i auto-neutral, gdy telefon przestanie przysyłać aktualizacje. Komputer sam niczym nie steruje –
//! tylko przekazuje do symulatora to, co przysyła sparowany telefon.

use parking_lot::Mutex;

pub const DEFAULT_MAX_DEFLECTION: f64 = 0.8;
pub const DEFAULT_TIMEOUT_MS: i64 = 500;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisOutput {
    pub aileron: f64,
    pub elevator: f64,
    pub rudder: f64,
    /// true – jednorazowy neutral po zwolnieniu (sterowanie wraca do joysticka).
    pub is_final: bool,
}

impl AxisOutput {
    pub const NEUTRAL: AxisOutput = AxisOutput { aileron: 0.0, elevator: 0.0, rudder: 0.0, is_final: true };
}

/// Osie pomocnicze 0…1. None = bez zmian, nie wysyłać.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AuxOutput {
    pub speedbrake: Option<f64>,
    pub brake: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateResult {
    Ok,
    Invalid,
    Busy,
}

#[derive(Default)]
struct State {
    owner: Option<String>,
    aileron: f64,
    elevator: f64,
    rudder: f64,
    last_update: i64,
    pending_neutral: bool,
    aux_owner: Option<String>,
    aux_update: i64,
    speedbrake: Option<f64>,
    speedbrake_dirty: bool,
    brake: f64,
    brake_held: bool,
    brake_release_pending: bool,
}

pub struct AxisController {
    pub max_deflection: f64,
    pub timeout_ms: i64,
    st: Mutex<State>,
}

impl Default for AxisController {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_DEFLECTION, DEFAULT_TIMEOUT_MS)
    }
}

fn valid(v: Option<f64>) -> bool {
    v.is_none_or(|x| x.is_finite())
}

impl AxisController {
    pub fn new(max_deflection: f64, timeout_ms: i64) -> Self {
        assert!(max_deflection > 0.0 && max_deflection <= 1.0);
        Self { max_deflection, timeout_ms, st: Mutex::new(State::default()) }
    }

    pub fn owner(&self) -> Option<String> {
        self.st.lock().owner.clone()
    }

    pub fn is_active(&self) -> bool {
        self.st.lock().owner.is_some()
    }

    pub fn update(&self, owner: &str, aileron: Option<f64>, elevator: Option<f64>, rudder: Option<f64>, now: i64) -> UpdateResult {
        if !valid(aileron) || !valid(elevator) || !valid(rudder) {
            return UpdateResult::Invalid;
        }
        let mut s = self.st.lock();
        self.expire(&mut s, now);
        if s.owner.as_deref().is_some_and(|o| o != owner) {
            return UpdateResult::Busy;
        }
        s.owner = Some(owner.to_string());
        s.pending_neutral = false;
        s.last_update = now;
        let m = self.max_deflection;
        if let Some(a) = aileron {
            s.aileron = a.clamp(-m, m);
        }
        if let Some(e) = elevator {
            s.elevator = e.clamp(-m, m);
        }
        if let Some(r) = rudder {
            s.rudder = r.clamp(-m, m);
        }
        UpdateResult::Ok
    }

    /// Natychmiastowe zwolnienie (`axis` z `release`, rozłączenie telefonu).
    pub fn release(&self, owner: &str) {
        let mut s = self.st.lock();
        if s.aux_owner.as_deref() == Some(owner) {
            Self::release_aux(&mut s);
        }
        if s.owner.as_deref() != Some(owner) {
            return;
        }
        Self::reset(&mut s);
    }

    /// Hamulec aerodynamiczny zostaje tam, gdzie go ustawiono; hamulce kół działają, dopóki telefon je przysyła.
    pub fn update_aux(&self, owner: &str, speedbrake: Option<f64>, brake: Option<f64>, now: i64) -> UpdateResult {
        if !valid(speedbrake) || !valid(brake) {
            return UpdateResult::Invalid;
        }
        let mut s = self.st.lock();
        self.expire_aux(&mut s, now);
        if s.aux_owner.as_deref().is_some_and(|o| o != owner) {
            return UpdateResult::Busy;
        }
        s.aux_owner = Some(owner.to_string());
        s.aux_update = now;
        if let Some(sb) = speedbrake {
            s.speedbrake = Some(sb.clamp(0.0, 1.0));
            s.speedbrake_dirty = true;
        }
        if let Some(b) = brake {
            s.brake = b.clamp(0.0, 1.0);
            s.brake_held = s.brake > 0.0;
            s.brake_release_pending = !s.brake_held;
        }
        UpdateResult::Ok
    }

    pub fn tick_aux(&self, now: i64) -> Option<AuxOutput> {
        let mut s = self.st.lock();
        self.expire_aux(&mut s, now);
        let sb = if s.speedbrake_dirty { s.speedbrake } else { None };
        s.speedbrake_dirty = false;
        let br = if s.brake_held {
            Some(s.brake)
        } else if s.brake_release_pending {
            s.brake_release_pending = false;
            Some(0.0)
        } else {
            None
        };
        if sb.is_none() && br.is_none() {
            None
        } else {
            Some(AuxOutput { speedbrake: sb, brake: br })
        }
    }

    /// Wychylenia do wysłania, jednorazowy neutral po zwolnieniu albo None, gdy osie są wolne.
    pub fn tick(&self, now: i64) -> Option<AxisOutput> {
        let mut s = self.st.lock();
        self.expire(&mut s, now);
        if s.owner.is_some() {
            return Some(AxisOutput { aileron: s.aileron, elevator: s.elevator, rudder: s.rudder, is_final: false });
        }
        if s.pending_neutral {
            s.pending_neutral = false;
            return Some(AxisOutput::NEUTRAL);
        }
        None
    }

    /// Bieżący hamulec aerodynamiczny i hamulce kół bez zdejmowania flag (X-Plane w trybie
    /// automatycznym, gdzie osie czyta też źródło MSFS): (speedbrake, hamulce 0–1).
    pub fn peek_aux(&self, now: i64) -> (Option<f64>, f64) {
        let mut s = self.st.lock();
        self.expire_aux(&mut s, now);
        (if s.aux_owner.is_some() { s.speedbrake } else { None }, if s.brake_held { s.brake } else { 0.0 })
    }

    /// Bieżące wychylenia bez zdejmowania flagi neutralu (symulacja, podgląd).
    pub fn peek(&self, now: i64) -> AxisOutput {
        let mut s = self.st.lock();
        self.expire(&mut s, now);
        if s.owner.is_some() {
            AxisOutput { aileron: s.aileron, elevator: s.elevator, rudder: s.rudder, is_final: false }
        } else {
            AxisOutput::NEUTRAL
        }
    }

    fn expire(&self, s: &mut State, now: i64) {
        if s.owner.is_some() && now - s.last_update > self.timeout_ms {
            Self::reset(s);
        }
    }

    fn expire_aux(&self, s: &mut State, now: i64) {
        if s.aux_owner.is_some() && now - s.aux_update > self.timeout_ms {
            Self::release_aux(s);
        }
    }

    fn reset(s: &mut State) {
        s.owner = None;
        s.aileron = 0.0;
        s.elevator = 0.0;
        s.rudder = 0.0;
        s.pending_neutral = true;
    }

    fn release_aux(s: &mut State) {
        s.aux_owner = None;
        if s.brake_held || s.brake > 0.0 {
            s.brake_release_pending = true;
        }
        s.brake_held = false;
        s.brake = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_and_owner() {
        let a = AxisController::default();
        assert_eq!(a.update("p1", Some(1.0), Some(-1.0), None, 0), UpdateResult::Ok);
        let o = a.tick(10).unwrap();
        assert_eq!(o.aileron, 0.8);
        assert_eq!(o.elevator, -0.8);
        assert_eq!(a.update("p2", Some(0.1), None, None, 20), UpdateResult::Busy);
        assert_eq!(a.update("p1", Some(f64::NAN), None, None, 20), UpdateResult::Invalid);
    }

    #[test]
    fn auto_neutral() {
        let a = AxisController::default();
        a.update("p1", Some(0.5), None, None, 0);
        assert!(!a.tick(400).unwrap().is_final);
        assert_eq!(a.tick(600), Some(AxisOutput::NEUTRAL));
        assert_eq!(a.tick(650), None);
        assert_eq!(a.update("p2", Some(0.1), None, None, 700), UpdateResult::Ok);
    }

    #[test]
    fn release_by_owner_only() {
        let a = AxisController::default();
        a.update("p1", Some(0.5), None, None, 0);
        a.release("p2");
        assert!(a.is_active());
        a.release("p1");
        assert!(!a.is_active());
        assert_eq!(a.tick(1), Some(AxisOutput::NEUTRAL));
    }

    #[test]
    fn brakes() {
        let a = AxisController::default();
        assert_eq!(a.update_aux("p1", Some(0.5), Some(0.7), 0), UpdateResult::Ok);
        assert_eq!(a.tick_aux(10), Some(AuxOutput { speedbrake: Some(0.5), brake: Some(0.7) }));
        assert_eq!(a.tick_aux(20), Some(AuxOutput { speedbrake: None, brake: Some(0.7) }));
        // Cisza > 500 ms – hamulce kół puszczone raz, spoilery zostają
        assert_eq!(a.tick_aux(700), Some(AuxOutput { speedbrake: None, brake: Some(0.0) }));
        assert_eq!(a.tick_aux(710), None);
        assert!(!a.is_active());
        assert_eq!(a.update_aux("p2", None, Some(2.0), 800), UpdateResult::Ok);
        assert_eq!(a.tick_aux(810).unwrap().brake, Some(1.0));
    }
}
