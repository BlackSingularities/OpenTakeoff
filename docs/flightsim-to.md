# OpenTakeoff — your MSFS cockpit, map and logbook on an Android phone

**Turn the phone in your pocket into a second screen, an autopilot panel, a moving map and a yoke for Microsoft Flight Simulator 2024.**

OpenTakeoff is a free Android app for flight-sim pilots. It pairs with MSFS over your home Wi-Fi through a small companion program for Windows, and it gives you a full glass cockpit, a 3D moving map with your flight track, live weather and airport briefings, a fly-by-phone control mode and an automatic logbook with landing scores. No accounts, no API keys, no ads and no subscriptions.

> ⚠️ **Android only.** There is no iOS version.
> 📦 **Not on Google Play yet.** This download is a ZIP with both parts: the Android app (`.apk`) and the companion program for Windows (`.exe`).
> 🔄 **Download once, then it updates itself.** After the first install, both the app and the companion program tell you when a new version is out and update from inside, so you don't have to come back here for every release.
> 🚀 **Actively developed.** New features and fixes arrive regularly, and your feedback shapes what comes next.

---

## What you get

### ✈️ A complete cockpit on your phone or tablet
Everything you need to fly fits on **one screen with no scrolling**: PFD, navigation display, thrust levers and the floating control button. Autopilot, radios, systems, engines and flight progress open from an icon bar in place of the ND, so the PFD and levers always stay in view. Phones work in portrait and landscape. Tablets and foldables get a permanent side or bottom panel.

- **PFD**: attitude, speed and altitude tapes with selected values, 10-second speed trend, Mach, vertical speed scale, flight path vector, height above terrain, wind and the autopilot mode annunciator (FMA).
- **ND / MFD** in ARC and ROSE modes: your active route, the next waypoint with distance, bearing and time, plus airports, VORs (with frequencies), NDBs and waypoints **read straight from your own MSFS navigation database**, with a vertical situation display (VSD) underneath.
- **Engines**: EICAS/ECAM-style display (N1 or RPM, EGT, fuel flow, fuel) and a drag-to-set throttle quadrant. Pistons and turboprops get mixture and prop levers. Jets get IDLE / CL / FLX / TOGA detents, TO/GA and reverse thrust.
- **Aircraft-specific panels**, each styled after the real thing:
  - **Airbus A320**: FCU with SPD / HDG / ALT / V/S, push/pull knobs, RMP radio, ATC panel
  - **Boeing 737**: MCP with IAS/MACH, HEADING with bank-angle ring, ALTITUDE, V/S wheel, LNAV, VOR LOC, APP, CMD A
  - **Boeing 787**: MCP with HDG/TRK, V/S/FPA, A/T, TCP-style radios, 787 EICAS
  - **Embraer E190**: guidance panel, MCDU-style RADIO page, E-Jet EICAS
  - **ATR 72, Garmin G1000, Cessna 172 (KAP140 style), Robinson R44 and gliders**
  The right profile is picked automatically from the aircraft you load, or you choose it yourself.
- **Real-feeling knobs**: dual concentric knobs (outer ring for big steps, inner for small), −/+ buttons that repeat while held, and a value window you can tap to type an exact heading, altitude or speed.
- **Radios and transponder**: COM, NAV, standby swap, squawk, modes and IDENT.
- **Systems**: gear, flaps, spoilers and speed brake, parking and wheel brakes, trim and all exterior lights.
- **Look around in MSFS**: hold the look button and tilt the phone (or slide your finger). In the cockpit you turn the pilot's head; in the external, drone and other views the camera circles your aircraft. Let go and you're back in the view you started from.

### 🎮 Fly the aircraft with your phone
- **Gyro mode** (default): hold the phone like a yoke. It only flies while your thumb is on the pad. Let go and control goes straight back to your hardware.
- **Thumb stick**, or a **Bluetooth/USB-C gamepad or joystick** connected to the phone.
- **Floating control button**, available on every tab. Read the airport briefing or watch the map and keep the controls under your thumb. It has an optional built-in trim and stays in the same spot on screen everywhere in the app.

### 🗺️ 3D moving map
- Tilted 3D map that follows your aircraft and looks along your flight path, with 3D buildings when you zoom in.
- **Your flight track in 3D**: a line coloured by altitude, hanging at flight level (height exaggerated so climbs and descents read clearly), joined to the ground track by a translucent curtain, with altitude labels along the way.
- Airports coloured by flight category (VFR / MVFR / IFR / LIFR), SIGMET areas, the current leg from the MSFS GPS, and nearby airports, VORs, NDBs and waypoints from your MSFS database.
- **Heading rose on the aircraft**: a magnetic compass rose drawn around your aircraft on the map. It shows a heading line with a readout, a track marker, your autopilot heading bug and a pointer to the reference point.
- **Radials tool, inbound and outbound**: long-press anywhere, or pick an airport, VOR, NDB or waypoint, and it gets its own compass rose. You instantly see which radial you're on (QDR), the course to the point (QDM), the distance and the time. Choose any radial, IN or OUT, by tapping the map or with ±1/±10. The app shows the course on the radial, how far off it you are, and **the heading to fly to intercept it, wind correction included**. Perfect for holds, DME arcs, procedure turns and "join the 270 radial inbound" moments.
- Open any past flight from the logbook and replay its full 3D track.

### 📊 Flight progress at a glance
A dedicated **Progress** panel with the flight phase, a route progress bar and around **45 live values** in clear groups:
- **Route**: next waypoint with ETE/ETA, bearing and desired track, cross-track error, arrival time, top of descent (distance and time), 3° descent rate
- **Flight**: time since takeoff, distance flown, average speed, max altitude, groundspeed and vertical speed
- **Speed**: IAS/Mach, TAS, headwind/tailwind and crosswind components, flight path angle, margins to VMO and stall speed
- **Altitude**: MSL, radio altitude, terrain, autopilot altitude with time to reach it, pressure and density altitude, minimums
- **Direction**: heading, track, drift, magnetic variation, turn needed to the HDG bug, pitch/bank, turn rate, g-load and AoA
- **Atmosphere**: wind, OAT with ISA deviation, TAT, QNH
- **Fuel**: on board, used, flow per engine, endurance, range, **fuel and reserve at destination**, kg per NM
- **Navigation**: position, nearest airport with bearing and time, NAV1/NAV2, COM1, transponder

### 🌦️ Real-world weather and airport briefings
- Live **METAR** with an interactive decoder: tap any group to see what it means.
- **TAF** split into periods with flight categories, plus **SIGMETs, PIREPs and winds aloft**.
- Runways with **headwind and crosswind components**, frequencies, ILS and nearby airports.
- **ATIS read aloud** from the latest METAR.
- FAA charts for US airports with a built-in PDF viewer.
- "Nearby" home screen: the airports closest to your aircraft while connected, or to the phone otherwise.

### 📒 Automatic logbook with landing scores
- **Every flight is recorded automatically** from takeoff to shutdown: position, altitude, IAS, groundspeed, vertical speed and heading, at a rate you choose (4 per second up to every 5 s).
- **Keeps recording in the background.** Switch apps, go to the home screen or turn the screen off and the track keeps going. A notification shows that recording is active.
- **Landing score** for every touchdown: vertical speed, g-force, position against the runway centreline and threshold, and bounces. Touch-and-goes and stops along the way are scored separately.
- Altitude, speed and vertical speed charts, monthly grouping, statistics and your landing trend.
- Smart handling of sim restarts: a teleport or an aircraft change closes the flight and starts a new one. Flights are saved even if the sim stops or the phone closes the app.
- Export the whole logbook as one compact backup file, import it on another phone, or export a single flight to **GPX**.

### 🧰 And more
- Voice callouts, cockpit checklists (A320, 737-800, C172) and flight-sim tools: wind calculator, density altitude, descent planner, fuel, unit converter, METAR/TAF decoder and radio phraseology.
- AMOLED (true black), graphite and bright themes.
- **English and Polish** interface.

---

## How it works: OpenTakeoff Link (companion app for Windows)

The phone doesn't talk to MSFS directly. A small **companion program, OpenTakeoff Link**, runs on the PC next to the simulator and bridges MSFS (via SimConnect) to the phone over your **local network**. Nothing goes through the internet.

- **Portable, no installation, no .NET needed**: one `OpenTakeoff-Link.exe`. Start it before or after MSFS. It connects when the sim starts and reconnects after a sim restart.
- **Automatic discovery**: the phone finds your PC on the Wi-Fi by itself, so you don't have to type an IP address.
- **Secure pairing with a 6-digit code**: enter it once and the phone remembers the PC.
- **Updates itself**: Link checks for new versions and swaps itself out, but only when no phone is connected, so it never interrupts a flight.
- **Lives in the system tray** while you fly.
- **Simulation mode**: a built-in demo flight (EPWA → EPGD) lets you try the whole app without starting MSFS.
- Reads procedures, ILS frequencies, airports and navaids from **your own MSFS navigation database**, including AIRAC updates installed in the sim.

---

## Installation

**You need:** an Android phone or tablet (Android 8.0 or newer), a Windows PC with Microsoft Flight Simulator 2024, and both on the **same Wi-Fi / home network** (not a guest network).

1. **Download the ZIP from this page and unpack it.** Inside you'll find:
   - `OpenTakeoff-x.y.z.apk`: the Android app
   - `OpenTakeoff-Link.exe`: the companion program for your Windows PC
   - `README.txt`: these steps in short
2. **Install the app on your phone.** Copy the APK to the phone (USB cable, Google Drive, e-mail, anything works), open it and allow installing from this source when Android asks. This is needed only once because the app isn't on Google Play yet.
3. **Run `OpenTakeoff-Link.exe` on your PC.** Put it in any folder you like (for example next to your other sim tools) and start it, no installation needed. If Windows SmartScreen warns about an unknown publisher, choose *More info → Run anyway*. When the firewall asks, allow access on **private networks**, and make sure your home network is set to *Private* in Windows.
4. **Pair.** In the app open **Flight → connect to MSFS**, pick your PC from the list and enter the 6-digit code shown in the Link window.
5. **Fly.** Load a flight in MSFS and the cockpit, map and logbook come alive.

### 🔄 Updates: you only download once
- **The Android app** checks for a new version every time it starts. When one is out, it shows a notice with a one-tap **Install** button, downloads the update and installs it over the current version. Your logbook and settings stay as they are. You can also check by hand in *Settings → About*.
- **OpenTakeoff Link** checks at startup and every few hours. It downloads the new version and swaps itself out in the same place, but only when no phone is connected, so it never interrupts a flight. Your desktop shortcut keeps working.
- Every new version is also posted here on flightsim.to if you prefer to download it manually.

---

## Good to know

- 💸 **Completely free**: no ads, no accounts, no subscriptions, no API keys.
- 🔒 **Private**: your flights and logbook stay on your phone. Sim data travels only across your own network.
- 🌍 **Open data**: weather from aviationweather.gov (NOAA), airports from OurAirports, winds aloft from Open-Meteo, maps from OpenFreeMap / OpenStreetMap, charts from the FAA.
- 🚀 **Actively developed**: OpenTakeoff keeps growing, with more aircraft panels, map tools and logbook features on the way. Bug reports and feature ideas are very welcome in the comments here or on GitHub.

> **For flight simulation use only.** Not for real-world navigation.

*Design and development: [redzikowski.com](https://redzikowski.com)*
