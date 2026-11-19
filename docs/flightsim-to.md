# flightsim.to – OpenTakeoff (wpis i aktualizacja)

Wszystko poniżej po angielsku, gotowe do wklejenia w pola formularza na flightsim.to
(Manage → General / Versioning / Images / FAQ). Zrzuty: `docs/screenshots/flightsim/`.

---

## Title

OpenTakeoff

## Short Description

Free cockpit, moving map and automatic logbook for MSFS 2020/2024 and X-Plane – a full Windows app with cockpit-style instruments, SimBrief OFP, landing analysis and checklists, plus an Android app that turns your phone into a second screen, autopilot panel and yoke. No ads, no accounts, updates itself.

## Compatibility

Microsoft Flight Simulator 2020/24

## Category

Cockpit & Controls (Utilities)

## Keywords (Search Keywords)

opentakeoff, android, phone, tablet, companion app, second screen, efb, pfd, nd, ecam, eicas, autopilot, moving map, logbook, landing rate, landing analysis, simbrief, ofp, checklists, metar, atis, x-plane, obs, stream overlay, free

---

## Description (Visual editor)

> Formatting tip: paste section by section. Lines starting with `###` = H3, `**…**` = bold, `- ` = bullet list.

**Big update: OpenTakeoff is no longer just a phone app.** Since the first upload (2.6.4) the small *OpenTakeoff Link* bridge has grown into a **full Windows program** with cockpit-style instruments, a tiltable navigation map, SimBrief OFP, landing analysis, checklists, statistics and stream overlays. The Android app is still here and pairs with it over your home Wi-Fi. Everything is free, with no ads, no accounts and no API keys, and **both apps update themselves**, so this is the last time you need to download it by hand.

### 🖥️ OpenTakeoff for Windows (MSFS 2020, MSFS 2024, X-Plane 11/12)

- **Cockpit-style instruments**: PFD in six styles (Airbus A320, Boeing 737, Boeing 787, ATR 72-600, Embraer E190, Garmin G1000), analog gauges (C172, R44, glider), ND/MFD in ARC and ROSE with airports, VORs and NDBs, ECAM / EICAS engine and systems pages. The style is picked from the aircraft you fly. Any instrument can open **in its own window** for a second monitor, with "always on top".
- **Navigation map you can tilt and rotate**: VOR roses and radials with intercept heading (wind included), Direct To with ETA and fuel at arrival, fuel range and glide range rings, extended runway centrelines, day/night terminator, **precipitation radar and METAR flags** for every airport in view. Your SimBrief route hangs in the air at its altitude on the tilted map.
- **Flight plan like an FMC**: route from text, SID / STAR / approach pages, waypoint table, vertical profile with T/C and T/D, fuel planning, PDF briefing, export to MSFS (.pln) and X-Plane (.fms).
- **SimBrief OFP in graphics**: weights against limits, block fuel breakdown, route profile with winds and ISA deviation, navlog, METAR/TAF, ATC flight plan and TLR takeoff/landing speeds. One click sends the route, alternate and fuel to the planner.
- **Takeoff and landing calculators** for 16 aircraft types: V1, VR, V2, VREF, VAPP and runway length required against TORA/LDA, with density altitude, runway wind and wet runway.
- **Checklists that tick themselves**: 11 real normal checklists (A320 family, 737 NG/MAX, 777/787, 717, King Air, Caravan, C152, C172S, PA-28, SR22, DA40 NG). Items like parking brake, flaps, gear, lights and transponder are ticked by the simulator. You can add your own as JSON files.
- **Automatic logbook with landing analysis**: every flight is recorded from takeoff to shutdown. The last 30 seconds before touchdown are sampled 5 times a second, so you get the runway seen from above with your track, the side view against a 3° path, VS/G/bank/pitch charts and a plain-language verdict (flare height, float, centreline, touchdown zone). Replay any flight on the map at ×10, ×60 or ×300.
- **Pilot statistics**: pilot rank, logbook totals, activity calendar, time per flight phase, landing score gauge and distribution, records, hangar, departures board, **19 achievements with levels** and two weekly challenges.
- **Airport briefing like an EFB**: runway diagram with wind components, sky profile (clouds, freezing level, visibility), TAF timeline, the last hours of METARs and an interactive METAR decoder.
- **For streamers**: a landing card that pops up on the sim screen after touchdown, a customisable stream bar (22 fields), and both as **OBS browser sources** with a transparent background. They also work with the simulator in exclusive full screen.
- **Screenshots pinned to flights**: F9–F12 or Ctrl+Shift+S works even inside the sim, and every shot remembers position, altitude and nearest airport. Export the whole logbook to Google Earth (KML) with landings and screenshots.
- Keyboard shortcuts (F1), tooltips that explain aviation abbreviations (ZFW, TOD, V2, FL230…), six themes including AMOLED black, English / Polish / Spanish.

### 📱 OpenTakeoff for Android (phone, tablet, foldable)

- **Full cockpit on one screen**: PFD, ND, thrust levers and autopilot. Panels styled after the real Airbus FCU, Boeing MCP, 787, E190, ATR, G1000 and KAP140, with dual concentric knobs and type-in values. Radios, transponder, gear, flaps, spoilers, brakes, trim and lights.
- **Fly with your phone**: gyro mode (hold the phone like a yoke; it only flies while your thumb is on the pad), on-screen stick, or a Bluetooth/USB gamepad. A floating control button stays on every tab.
- **Look around in MSFS**: hold and tilt the phone to turn your head in the cockpit or orbit the aircraft outside.
- **3D moving map** with your flight track coloured by altitude, compass rose on the aircraft, radials tool with intercept heading, airports coloured by flight category and SIGMET areas.
- **About 45 live values** on the Progress page: ETE/ETA, top of descent, wind components, VMO and stall margins, fuel at destination and more.
- **Real-world weather**: interactive METAR, TAF periods, SIGMET, PIREP, winds aloft, runway wind components, **ATIS read aloud**, and FAA charts for US airports.
- **Logbook in your pocket**: flights are recorded in the background with a landing score. It **syncs both ways with the PC** (no duplicates), and you can export GPX or back up to a single file.

### 🔄 Updates are built in

- **The Android app** checks for a new version at every start and installs it with one tap. Your logbook and settings stay.
- **The Windows program** checks at startup and every 6 hours, shows what's new, and installs in the background: *Install now*, *Later* or *Skip this version*.
- New versions are still posted here too, if you prefer to download them manually.

### 📦 What's in the ZIP

- `OpenTakeoff-Setup-x.y.z.exe`: the Windows program (installer, about 20 MB)
- `OpenTakeoff-x.y.z.apk`: the Android app (Android 8.0 or newer)
- `README.txt`: installation steps, pairing and updates

### Installation

1. **Windows:** run `OpenTakeoff-Setup-x.y.z.exe` and follow the installer. If Windows SmartScreen shows a warning, choose *More info → Run anyway*. The installer opens the firewall for **private networks** only. Make sure your home network is set to *Private* in Windows.
2. Start OpenTakeoff. It connects to MSFS 2020/2024 or X-Plane 11/12 by itself; the start order doesn't matter. No sim at hand? Pick **Simulation** in the top bar to see a demo flight (EPWA → EPGD).
3. **Android (optional):** copy the APK to your phone, open it and allow installing from this source (needed once, the app isn't on Google Play yet).
4. **Pair:** in the app open **Flight**, pick your PC from the list (or type its IP address) and enter the 6-digit code shown on the *Phones* tab of the Windows program. The phone remembers the pairing.
5. Fly. The cockpit, map and logbook come alive, and flights sync between the phone and the PC.

**Upgrading from 2.6.4 / OpenTakeoff Link:** install the Windows program and you can delete `OpenTakeoff-Link.exe`. Phones already paired with Link connect to the new program without a new code. Install the new APK over the old app and your logbook stays.

### Good to know

- 💸 **Completely free**: no ads, no accounts, no subscriptions, no API keys. If you enjoy it, you can [buy me a coffee](https://buycoffee.to/adamredzikowski) ☕. The link is also in the Android app under *Settings → About*.
- 🔒 **Private**: flights and logbook stay on your PC and phone; sim data travels only over your own network.
- 🌍 **Open data**: weather from aviationweather.gov (NOAA), airports from OurAirports, winds aloft from Open-Meteo, maps from OpenStreetMap / OpenFreeMap, radar from RainViewer, charts from the FAA.
- ⚠️ **Android only** for the phone app. There is no iOS version.
- 🚀 **Actively developed.** Bug reports and ideas are very welcome in the comments, or straight from the app (*Report a bug or a suggestion*).

> For flight simulation use only. Not for real-world navigation.

*Design and development: [redzikowski.com](https://redzikowski.com)*

---

## FAQ (Add FAQ)

**Q: Do I need the phone app?**
A: No. The Windows program works on its own (instruments, map, logbook, OFP, checklists, overlays). The Android app adds a cockpit, controls and the logbook in your pocket.

**Q: Is there an iOS version?**
A: Not at the moment. The phone app is Android only (8.0 or newer).

**Q: Does it work with MSFS 2020 and X-Plane?**
A: Yes: MSFS 2020, MSFS 2024 (Steam and Microsoft Store) and X-Plane 11/12. X-Plane connects over its built-in UDP protocol, no plugins needed.

**Q: The phone doesn't find my PC.**
A: Both devices must be on the same Wi-Fi/home network (not a guest network), and the network must be set to *Private* in Windows. You can also type the PC's IP address in the app; it is shown on the *Phones* tab.

**Q: Do I have to come back here for every update?**
A: No. Both apps check for new versions themselves and install them in place, keeping your logbook and settings.

**Q: Does it cost anything or collect my data?**
A: It's free, with no ads and no accounts. Flights stay on your devices. If you'd like to support development: buycoffee.to/adamredzikowski.

**Q: Can I use it for streaming?**
A: Yes. The landing card and the stream bar can be shown on the sim screen or added to OBS as browser sources with a transparent background.

---

## Version History / Push Update – notes for 3.10.2

Major update. Since 2.6.4 OpenTakeoff Link has been replaced by a full Windows program.

- New Windows program (MSFS 2020/2024, X-Plane 11/12): cockpit-style PFD/ND/ECAM/EICAS, a navigation map you can tilt and rotate with radials, Direct To, weather radar and METARs, an FMC-style flight planner, SimBrief OFP, takeoff/landing calculators, self-ticking checklists, a logbook with landing analysis and replay, pilot statistics and achievements, an EFB-style airport briefing, stream overlays and OBS pages, instruments in separate windows, keyboard shortcuts.
- Logbook sync between the phone and the PC, both ways, no duplicates.
- Spanish (Spain) language in both apps, alongside English and Polish.
- Built-in updates for the Android app and the Windows program.
- Android: new cockpit panels (737, 787, E190, ATR), larger Progress page, background recording, landing scores, look-around in every view.
- 3.10.2: fixed A320 altitude window above 10,000 ft, ribbon and dialog layout fixes, complete English/Spanish interface on Windows.

Update type: **Major Update** (notifies subscribers and bumps the version).
