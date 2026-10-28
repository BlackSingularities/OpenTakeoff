## 2.5.3

- Nowa strona aplikacji z opisem funkcji.
- Jednolita informacja o autorze (Adam Rędzikowski · redzikowski.com) i przycisk „Postaw kawę”.
- Usunięte odnośniki do repozytorium z interfejsu.

## 2.5.2 — Simpler About section

- About: no update-status badge next to the version and no GitHub link; the version, updates, "Buy me a coffee"
  and the author link remain.

## 2.5.1 — Look around in every view, cleaner About, English landing reports

- Camera: the cockpit / external / drone buttons are gone (MSFS 2024 often ignored them); one look-around button
  remains and works in every view — in the cockpit the pilot's head turns, in external, drone and other views the
  camera circles the aircraft. After letting go the camera returns to the view it started from, also when MSFS gets
  stuck in its free camera (it leaves through the drone view if needed). A short tap explains how it works.
- Settings: "About" and "Updates" are one section — version with update status, the update action, and clear links
  to "Buy me a coffee", the author and GitHub releases.
- Landing grade and notes are shown in the current app language, also for flights recorded before switching it.
- Google Drive sync removed from the app (it was never enabled); sync messages no longer appear anywhere.

## 2.5.0 — Heading rose and radials on the map, much richer Progress, background recording

- Map: heading rose on the aircraft (compass button) — magnetic card with 5°/10°/30° ticks, heading line with an HDG
  readout, green track marker, cyan autopilot HDG bug and a magenta pointer to the reference point; it keeps the same
  size on screen at every zoom.
- Map: radials from any point (target button, a long press on the map, or "Radials" on an airport card). The point snaps
  to the nearest airport, navaid or flight-plan waypoint and gets its own compass rose; the card shows the radial the
  aircraft is on (QDR), the course to the point (QDM), distance and time. Pick a radial by tapping the map, with
  ±1/±10 or "current radial", as IN or OUT: the radial is drawn with direction arrows and the card gives the course on
  the radial, cross-track distance and the heading to fly with the intercept angle and wind correction. Without the
  simulator the radial is measured from the map centre (+).
- Map: the 3D track is a smooth ramp — altitude between track points is interpolated in small steps and neighbouring
  segments share edges, so there are no stairs or gaps in turns; the geometry is built off the UI thread.
- Progress tab: flight phase and route progress bar, then grouped Route (next waypoint with ETE/ETA, bearing and DTK,
  cross-track, waypoint count, arrival, top of descent with time, 3° descent rate), Flight (time since takeoff, distance
  flown, average speed, max altitude/GS/V/S, sim time, last touchdown), Speed (IAS/Mach, TAS, head/tailwind and
  crosswind, V/S and FPA, margins to VMO and VS1), Altitude (MSL, RA/AGL, terrain, AP altitude with time to reach,
  pressure and density altitude, minimums), Direction (heading, track, drift, variation, HDG bug turn, pitch/bank,
  turn rate, g and AoA), Atmosphere (wind, OAT with ISA deviation, TAT, QNH), Fuel (on board, used, flow per engine,
  endurance and range, fuel and reserve at destination, kg/NM, average burn) and Navigation (position, nearest
  airport with bearing and time, NAV1/NAV2, COM1, transponder).
- Flight recording no longer stops when the app goes to the home screen or the screen turns off: while connected to
  the simulator a foreground service keeps the Link connection, Wi-Fi and CPU awake, with a notification showing
  whether the track is being recorded.
- The floating control button stays in the same place on screen in every tab, card and view: one position for the
  whole app (a fraction of the window), shared live between views; older per-view positions start from the flight view's.

## 2.4.0 — Boeing 787 cockpit panels

- 787 MCP: F/D, A/P, A/T ARM, CLB/CON, A/T, IAS ( ) MACH with the IAS/MACH changeover, LNAV, FLCH, HDG ( ) TRK with the
  bank-limit ring (tap the knob for HDG SEL) and HOLD, V/S ( ) FPA wheel and V/S, ALTITUDE with HOLD, LOC and APP, and the
  A/P DISENGAGE bar; one row on tablets and foldables, three tiers on phones.
- TCP-style radio: VHF L/R, NAV RAD L/R and XPDR pages, values typed into the scratchpad and inserted next to the standby
  line, transfer keys, transponder modes and IDENT.
- Systems: exterior lights, gear lever with the EICAS-style GEAR box, flaps UP–30, speed brake with ARMED, parking brake
  with the wheel brake axis, stab trim.
- 787 EICAS: TAT, N1 and EGT dials, N2 and FF, gear, flaps, stab trim and fuel, above the thrust levers.

## 2.3.3 — Smaller engine displays

- Engines tab: the EICAS / ECAM engine display is the same size as the ND on wide screens (centred), instead of filling
  the full width; phones are unchanged.

## 2.3.2 — Aircraft scales with the map

- The aircraft on the map (3D and the ground symbol) is smaller when zoomed out and grows with the map when zoomed in,
  close to its real size at an airport, instead of covering the apron.

## 2.3.1 — MSFS 2024 camera fixes, narrower 737 and E190 panels

- Looking around (hold a camera button) now pivots from the pilot's eye instead of placing the camera in front of the
  aircraft, works from the cockpit view, and returns to the normal cockpit once after release instead of flipping views
  every time the view passes through centre.
- Camera buttons: tapping the current view does nothing; leaving the look-around camera goes straight to the chosen view,
  VIEW_MODE is only used between cockpit and external views, and the Link waits longer for MSFS 2024 to switch.
- 737 MCP fits in one row from 680 dp (a denser layout with narrower keys and digits), the E190 guidance panel from 520 dp,
  so foldables and tablets get the single-row cockpit layout.

## 2.3.0 — E190 cockpit panels, map wall, Tools screen polish

- E190 (Primus Epic): guidance panel (FD, NAV, APP, HDG SEL with PUSH SYNC, AP, YD, A/T, SPEED with FLCH, ALT SEL with ALT,
  V/S wheel), MCDU-style RADIO page with COM/NAV and XPDR plus a 0–7 keypad, external lights, gear, flaps 0–5/FULL,
  speed brake axis, EMER/PRKG BRAKE with the wheel brake axis, pitch trim, and an E-Jet EICAS (N1, ITT, N2, FF, fuel, gear, flaps, trim).
- Map: a translucent wall joins the ground track with the 3D line at flight altitude; the compass moved to the bottom-right corner.

- Tools look like the rest of the app: 10 dp panels, active tool marked with a lighter background, accent outline and dot
  (no inverted block), tablet list inside a panel, same margins and content width as Settings, monospaced number fields,
  divided result rows in the unit converter and METAR decoder.

## 2.2.0 (not released: build failed; shipped in 2.3.0) — Boeing 737 cockpit panels, cockpit-style FCU and MCP, brakes in Systems

- 737: MCP (A/T ARM and F/D switches, IAS/MACH, HEADING, ALTITUDE, VERT SPEED wheel, N1, SPEED, LVL CHG, HDG SEL, LNAV,
  VOR LOC, APP, ALT HLD, V/S, CMD A, DISENGAGE); VHF/NAV radio panel; ATC panel with per-digit setting and STBY / ALT RPTG OFF / XPNDR;
  overhead lights, gear with three lights, flaps UP–40, stab trim; EICAS with N1/EGT/N2 dials, FF and fuel quantity.
- A320 FCU redrawn like the real panel: dark plate, black display strip with orange digits, and each knob shaped like its
  counterpart (SPD, HDG with the triangle, ALT with the 100/1000 step selector, V/S); tablets get one row as in the cockpit,
  phones two tiers (SPD/HDG, then ALT/V/S).
- 737 MCP redrawn the same way: windows with IAS/MACH, HDG/TRK and V/S/FPA captions, distinct IAS, HEADING (bank-angle ring)
  and ALTITUDE knobs, a vertical V/S wheel rolled with a finger, A/P ENGAGE and DISENGAGE; one row on wide tablets, three tiers on phones.
- Speed brake and wheel brake axes moved from Controls to Systems in every profile: arm key next to the speed brake axis,
  small parking brake key next to the wheel brake axis.

## 2.1.0 — One profile per display set, A320 cockpit panels

- Nine aircraft profiles, one per display set (A320, 737, 787, ATR 72, E190, G1000, C172, R44, glider); each has its own
  autopilot layout, radios, systems, lights and engine levers. Saved AIRBUS/BOEING choices carry over to A320/737.
- A320: FCU with SPD/HDG/ALT/V/S windows, knobs and PUSH/PULL; RMP; ATC panel with 0–7 keypad and STBY/AUTO/ON;
  EXT LT, gear, flaps CONF, speed brake and pitch trim; E/WD engine display. Only controls the simulator accepts are shown.
- Engines page: one thrust lever per engine (new Link commands `THROTTLE1_SET`…`THROTTLE4_SET`, lever position in `engines[n].throttlePct`);
  the shared lever next to the PFD/MFD still sets all engines.
- The long profile bar above the autopilot panel is gone; the profile is picked from the small round button next to checklists.

## 2.0.0 — Aircraft-matched displays (from OpenPFD)

- Versioning: releases are now cut from `vX.Y.Z` tags instead of every push to `main`; the app and Link carry the tag version.
  Apps on 1.10.x cannot detect 2.0.0 automatically — install it once by hand from the release page.

- PFD and ND/MFD now match the aircraft: Airbus A320, Boeing 737, 787, ATR 72, Embraer E190, Garmin G1000 (with SVT terrain),
  analog six-pack (3 per row), R44 and glider panels. The aircraft profile picked in the cockpit menu drives the displays too.
- Displays use full simulator data from the new Link `avionics` block: baro/STD, FMA modes, MCP/FCU bugs, flight director,
  ILS/VOR/GPS deviation, DME, markers, minimums, speed limits, gyro instrument readings, radios and transponder.
- Separate speed brake and wheel brake axes (wheel brakes on the ground only; L2/R2 on a gamepad).
- Cockpit: header removed; recording, camera, checklists and the aircraft menu sit next to the panel tabs.
- Camera switching falls back to VIEW_MODE when MSFS ignores CAMERA STATE.
- Live flight strip above other tabs (tap to open the cockpit); collapsible tab rail on tablets.
- 3D map: thin flight-path line at altitude instead of the curtain, and the aircraft drawn at its altitude.
- Logbook: every landing in a flight (touch-and-go, multi-airport) is scored separately.
- Removed route planning (plan editor, map planning, typed routes, saved routes, import/export) and SimBrief;
  the flight plan now comes from the simulator GPS (next/previous waypoint positions added to the Link `avionics.gps` block).
- Map: compact button group, simpler top bar; the bottom value bar is gone (the live flight strip shows the same).

## next CI release — Transparent application icon

- Use the transparent application mark in launcher, splash and existing branding surfaces.
- Preserve alpha in legacy and adaptive Android icons; refresh themed icon resources.

