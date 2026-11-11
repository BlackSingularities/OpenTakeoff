#!/usr/bin/env python3
"""
OpenTakeoff Link dla X-Plane 11/12.

Samodzielny program: z X-Plane rozmawia wbudowanym protokołem UDP (port 49000, bez wtyczek),
a z telefonem protokołem OpenTakeoff Link v1 (docs/link-protocol.md).

Uruchomienie (Python 3.9+):
    pip install "websockets>=10.1"
    python xplane_link.py

Program wypisze 6-cyfrowy kod parowania. Jeśli X-Plane działa na innym komputerze: --xplane-host ADRES.
Nie uruchamiaj go razem z OpenTakeoff Link dla MSFS, bo oba używają portu 47810.
"""

import argparse
import asyncio
import datetime as dt
import hmac
import json
import math
import os
import pathlib
import secrets
import socket
import struct
import time

import websockets

VERSION = "1.0.0"
WS_PORT = 47810
DISCOVERY_PORT = 47811
XPLANE_PORT = 49000
MAX_MESSAGE_BYTES = 16 * 1024
MIN_RATE_HZ, MAX_RATE_HZ, DEFAULT_RATE_HZ = 1, 20, 10
HELLO_TIMEOUT_S = 10
IDLE_TIMEOUT_S = 30
MAX_COMMANDS_PER_SECOND = 50
MAX_FAILURES = 5
BLOCK_S = 60
MAX_DEFLECTION = 0.8
AXIS_TIMEOUT_S = 0.5
SIM_STALE_S = 3.0
RESUBSCRIBE_S = 10
MAX_ENGINES = 4
TITLE_BYTES = 64

FT_PER_M = 3.28084
KT_PER_MS = 1.94384
HPA_PER_INHG = 33.8639

D = {
    "lat": "sim/flightmodel/position/latitude",
    "lon": "sim/flightmodel/position/longitude",
    "elev": "sim/flightmodel/position/elevation",
    "agl": "sim/flightmodel/position/y_agl",
    "alt_ind": "sim/cockpit2/gauges/indicators/altitude_ft_pilot",
    "pitch": "sim/flightmodel/position/theta",
    "bank": "sim/flightmodel/position/phi",
    "hdg_true": "sim/flightmodel/position/psi",
    "hdg_mag": "sim/flightmodel/position/mag_psi",
    "track": "sim/flightmodel/position/hpath",
    "ias": "sim/flightmodel/position/indicated_airspeed",
    "tas": "sim/flightmodel/position/true_airspeed",
    "gs": "sim/flightmodel/position/groundspeed",
    "vs": "sim/flightmodel/position/vh_ind_fpm",
    "aoa": "sim/flightmodel/position/alpha",
    "magvar": "sim/flightmodel/position/magnetic_variation",
    "mach": "sim/flightmodel/misc/machno",
    "g": "sim/flightmodel/forces/g_nrml",
    "on_ground": "sim/flightmodel/failures/onground_any",
    "paused": "sim/time/paused",
    "zulu": "sim/time/zulu_time_sec",
    "radio_alt": "sim/cockpit2/gauges/indicators/radio_altimeter_height_ft_pilot",
    "baro": "sim/cockpit2/gauges/actuators/barometer_setting_in_hg_pilot",
    "slip": "sim/cockpit2/gauges/indicators/slip_deg",
    "turn": "sim/cockpit2/gauges/indicators/turn_rate_heading_deg_pilot",
    "stall": "sim/cockpit2/annunciators/stall_warning",
    "servos": "sim/cockpit2/autopilot/servos_on",
    "fd": "sim/cockpit2/autopilot/flight_director_mode",
    "athr": "sim/cockpit2/autopilot/autothrottle_enabled",
    "ap_hdg": "sim/cockpit2/autopilot/heading_dial_deg_mag_pilot",
    "ap_alt": "sim/cockpit2/autopilot/altitude_dial_ft",
    "ap_vs": "sim/cockpit2/autopilot/vvi_dial_fpm",
    "ap_spd": "sim/cockpit2/autopilot/airspeed_dial_kts",
    "st_hdg": "sim/cockpit2/autopilot/heading_status",
    "st_nav": "sim/cockpit2/autopilot/nav_status",
    "st_apr": "sim/cockpit2/autopilot/approach_status",
    "st_alt": "sim/cockpit2/autopilot/altitude_hold_status",
    "st_vs": "sim/cockpit2/autopilot/vvi_status",
    "st_spd": "sim/cockpit2/autopilot/speed_status",
    "st_gs": "sim/cockpit2/autopilot/glideslope_status",
    "com1": "sim/cockpit2/radios/actuators/com1_frequency_hz_833",
    "com1s": "sim/cockpit2/radios/actuators/com1_standby_frequency_hz_833",
    "com2": "sim/cockpit2/radios/actuators/com2_frequency_hz_833",
    "com2s": "sim/cockpit2/radios/actuators/com2_standby_frequency_hz_833",
    "nav1": "sim/cockpit2/radios/actuators/nav1_frequency_hz",
    "nav1s": "sim/cockpit2/radios/actuators/nav1_standby_frequency_hz",
    "nav2": "sim/cockpit2/radios/actuators/nav2_frequency_hz",
    "nav2s": "sim/cockpit2/radios/actuators/nav2_standby_frequency_hz",
    "xpdr": "sim/cockpit2/radios/actuators/transponder_code",
    "xpdr_mode": "sim/cockpit2/radios/actuators/transponder_mode",
    "gear_handle": "sim/cockpit2/controls/gear_handle_down",
    "gear": "sim/flightmodel2/gear/deploy_ratio[0]",
    "flaps": "sim/flightmodel/controls/flaprqst",
    "flap_detents": "sim/aircraft/controls/acf_flap_detents",
    "speedbrake": "sim/cockpit2/controls/speedbrake_ratio",
    "parking": "sim/cockpit2/controls/parking_brake_ratio",
    "trim": "sim/cockpit2/controls/elevator_trim",
    "mixture": "sim/cockpit2/engine/actuators/mixture_ratio_all",
    "prop": "sim/cockpit2/engine/actuators/prop_ratio_all",
    "beacon": "sim/cockpit2/switches/beacon_on",
    "strobe": "sim/cockpit2/switches/strobe_lights_on",
    "nav_lights": "sim/cockpit2/switches/navigation_lights_on",
    "landing": "sim/cockpit2/switches/landing_lights_on",
    "taxi": "sim/cockpit2/switches/taxi_light_on",
    "fuel": "sim/flightmodel/weight/m_fuel_total",
    "engines": "sim/aircraft/engine/acf_num_engines",
    "en_type": "sim/aircraft/prop/acf_en_type[0]",
    "wind_dir": "sim/weather/wind_direction_degt",
    "wind_spd": "sim/weather/wind_speed_kt",  # mimo nazwy: m/s
    "oat": "sim/cockpit2/temperature/outside_air_temp_degc",
    "qnh": "sim/weather/barometer_sealevel_inhg",
}
ENGINE_REFS = {
    "throttle": "sim/cockpit2/engine/actuators/throttle_ratio",
    "n1": "sim/flightmodel/engine/ENGN_N1_",
    "n2": "sim/flightmodel/engine/ENGN_N2_",
    "rpm": "sim/cockpit2/engine/indicators/engine_speed_rpm",
    "egt": "sim/cockpit2/engine/indicators/EGT_deg_C",
    "ff": "sim/cockpit2/engine/indicators/fuel_flow_kg_sec",
}
for _key, _path in ENGINE_REFS.items():
    for _i in range(MAX_ENGINES):
        D[f"{_key}{_i}"] = f"{_path}[{_i}]"
TITLE_REF = "sim/aircraft/view/acf_ui_name"

OVERRIDE_JOYSTICK = "sim/operation/override/override_joystick"
YOKE_ROLL = "sim/joystick/yoke_roll_ratio"
YOKE_PITCH = "sim/joystick/yoke_pitch_ratio"
YOKE_YAW = "sim/joystick/yoke_heading_ratio"
LEFT_BRAKE = "sim/cockpit2/controls/left_brake_ratio"
RIGHT_BRAKE = "sim/cockpit2/controls/right_brake_ratio"

# Komendy bez wartości → komenda X-Plane
COMMANDS = {
    "AP_MASTER": "sim/autopilot/servos_toggle",
    "AUTOPILOT_ON": "sim/autopilot/servos_on",
    "AUTOPILOT_OFF": "sim/autopilot/servos_off_any",
    "TOGGLE_FLIGHT_DIRECTOR": "sim/autopilot/fdir_toggle",
    "AUTO_THROTTLE_ARM": "sim/autopilot/autothrottle_toggle",
    "YAW_DAMPER_TOGGLE": "sim/systems/yaw_damper_toggle",
    "AP_HDG_HOLD": "sim/autopilot/heading",
    "AP_NAV1_HOLD": "sim/autopilot/NAV",
    "AP_APR_HOLD": "sim/autopilot/approach",
    "AP_LOC_HOLD": "sim/autopilot/NAV",
    "AP_BC_HOLD": "sim/autopilot/back_course",
    "AP_ALT_HOLD": "sim/autopilot/altitude_hold",
    "AP_VS_HOLD": "sim/autopilot/vertical_speed",
    "FLIGHT_LEVEL_CHANGE": "sim/autopilot/level_change",
    "AP_PANEL_SPEED_HOLD": "sim/autopilot/autothrottle_toggle",
    "AP_AIRSPEED_HOLD": "sim/autopilot/level_change",
    "AP_WING_LEVELER": "sim/autopilot/wing_leveler",
    "HEADING_BUG_INC": "sim/autopilot/heading_up",
    "HEADING_BUG_DEC": "sim/autopilot/heading_down",
    "AP_ALT_VAR_INC": "sim/autopilot/altitude_up",
    "AP_ALT_VAR_DEC": "sim/autopilot/altitude_down",
    "AP_VS_VAR_INC": "sim/autopilot/vertical_speed_up",
    "AP_VS_VAR_DEC": "sim/autopilot/vertical_speed_down",
    "AP_SPD_VAR_INC": "sim/autopilot/airspeed_up",
    "AP_SPD_VAR_DEC": "sim/autopilot/airspeed_down",
    "COM_STBY_RADIO_SWAP": "sim/radios/com1_standy_flip",
    "COM2_RADIO_SWAP": "sim/radios/com2_standy_flip",
    "NAV1_RADIO_SWAP": "sim/radios/nav1_standy_flip",
    "NAV2_RADIO_SWAP": "sim/radios/nav2_standy_flip",
    "XPNDR_IDENT_ON": "sim/transponder/transponder_ident",
    "GEAR_UP": "sim/flight_controls/landing_gear_up",
    "GEAR_DOWN": "sim/flight_controls/landing_gear_down",
    "GEAR_TOGGLE": "sim/flight_controls/landing_gear_toggle",
    "FLAPS_INCR": "sim/flight_controls/flaps_down",
    "FLAPS_DECR": "sim/flight_controls/flaps_up",
    "TOGGLE_BEACON_LIGHTS": "sim/lights/beacon_lights_toggle",
    "STROBES_TOGGLE": "sim/lights/strobe_lights_toggle",
    "TOGGLE_NAV_LIGHTS": "sim/lights/nav_lights_toggle",
    "LANDING_LIGHTS_TOGGLE": "sim/lights/landing_lights_toggle",
    "TOGGLE_TAXI_LIGHTS": "sim/lights/taxi_lights_toggle",
    "ELEV_TRIM_UP": "sim/flight_controls/pitch_trim_up",
    "ELEV_TRIM_DN": "sim/flight_controls/pitch_trim_down",
    "AILERON_TRIM_LEFT": "sim/flight_controls/aileron_trim_left",
    "AILERON_TRIM_RIGHT": "sim/flight_controls/aileron_trim_right",
    "RUDDER_TRIM_LEFT": "sim/flight_controls/rudder_trim_left",
    "RUDDER_TRIM_RIGHT": "sim/flight_controls/rudder_trim_right",
    "THROTTLE_REVERSE_THRUST_TOGGLE": "sim/engines/thrust_reverse_toggle",
    "AUTO_THROTTLE_TO_GA": "sim/engines/TOGA_power",
}

COM_MIN, COM_MAX = 118_000_000, 136_990_000
NAV_MIN, NAV_MAX = 108_000_000, 117_950_000


def _ratio(v):
    return v / 16383.0


def _khz(v):
    return v / 1000.0


def _ten_khz(v):
    return v / 10000.0


def _same(v):
    return float(v)


# Komendy z wartością → (min, max, zmienna X-Plane, przeliczenie)
SETTERS = {
    "HEADING_BUG_SET": (0, 359, D["ap_hdg"], _same),
    "AP_ALT_VAR_SET_ENGLISH": (0, 50_000, D["ap_alt"], _same),
    "AP_VS_VAR_SET_ENGLISH": (-8_000, 8_000, D["ap_vs"], _same),
    "AP_SPD_VAR_SET": (0, 500, D["ap_spd"], _same),
    "COM_RADIO_SET_HZ": (COM_MIN, COM_MAX, D["com1"], _khz),
    "COM_STBY_RADIO_SET_HZ": (COM_MIN, COM_MAX, D["com1s"], _khz),
    "COM2_RADIO_SET_HZ": (COM_MIN, COM_MAX, D["com2"], _khz),
    "COM2_STBY_RADIO_SET_HZ": (COM_MIN, COM_MAX, D["com2s"], _khz),
    "NAV1_RADIO_SET_HZ": (NAV_MIN, NAV_MAX, D["nav1"], _ten_khz),
    "NAV1_STBY_SET_HZ": (NAV_MIN, NAV_MAX, D["nav1s"], _ten_khz),
    "NAV2_RADIO_SET_HZ": (NAV_MIN, NAV_MAX, D["nav2"], _ten_khz),
    "NAV2_STBY_SET_HZ": (NAV_MIN, NAV_MAX, D["nav2s"], _ten_khz),
    "FLAPS_SET": (0, 16383, D["flaps"], _ratio),
    "SPOILERS_SET": (0, 16383, D["speedbrake"], _ratio),
    "PARKING_BRAKE_SET": (0, 1, D["parking"], _same),
    "ELEVATOR_TRIM_SET": (-16383, 16383, D["trim"], _ratio),
    "THROTTLE_SET": (0, 16383, "sim/cockpit2/engine/actuators/throttle_ratio_all", _ratio),
    "THROTTLE1_SET": (0, 16383, D["throttle0"], _ratio),
    "THROTTLE2_SET": (0, 16383, D["throttle1"], _ratio),
    "THROTTLE3_SET": (0, 16383, D["throttle2"], _ratio),
    "THROTTLE4_SET": (0, 16383, D["throttle3"], _ratio),
    "MIXTURE_SET": (0, 16383, D["mixture"], _ratio),
    "PROPELLER_SET": (0, 16383, D["prop"], _ratio),
}

# Na liście protokołu, ale bez odpowiednika w X-Plane
UNSUPPORTED = {
    "AP_N1_HOLD", "AP_PANEL_MACH_HOLD", "AP_MACH_VAR_SET", "AP_BANK_HOLD",
    "TOGGLE_LOGO_LIGHTS", "TOGGLE_WING_LIGHTS", "PANEL_LIGHTS_TOGGLE", "TOGGLE_CABIN_LIGHTS",
}
LOOK_COMMANDS = {"CAMERA_PITCH_SET", "CAMERA_YAW_SET"}
CAMERA_COMMANDS = {2: "sim/view/3d_cockpit_cmnd_look", 3: "sim/view/chase", 4: "sim/view/free_camera"}
# Numeracja protokołu: off, standby, test, on, alt, ground. X-Plane: off, standby, on, alt, test, ground.
XPDR_TO_XPLANE = {0: 0, 1: 1, 2: 4, 3: 2, 4: 3, 5: 5}
XPDR_NAMES = {0: "off", 1: "standby", 2: "on", 3: "alt", 4: "test", 5: "ground", 6: "alt", 7: "alt"}

BIZJET_WORDS = ("CITATION", "LEARJET", "PHENOM", "GULFSTREAM", "CHALLENGER", "FALCON", "HONDAJET", "CJ4", "SF50", "PC-24")
AIRLINER_WORDS = ("A300", "A310", "A318", "A319", "A320", "A321", "A330", "A340", "A350", "A380", "737", "747", "757",
                  "767", "777", "787", "CRJ", "E170", "E175", "E190", "E195", "MD-11", "MD-80", "MD-82", "DC-10", "ATR",
                  "DASH 8", "DHC-8", "Q400", "CONCORDE")
HELICOPTER_WORDS = ("HELICOPTER", "R22", "R44", "R66", "S-76", "EC135", "BELL ")


def engine_type_name(en_type):
    # acf_en_type: 0–1 tłokowy, 3 elektryczny, 5 i 7 odrzutowy, 9–10 turbośmigłowy
    if en_type in (0, 1, 3):
        return "piston"
    if en_type in (9, 10):
        return "turboprop"
    if en_type in (5, 7):
        return "jet"
    return "none"


def classify(engine_type, engine_count, title):
    text = title.upper()
    if any(w in text for w in HELICOPTER_WORDS):
        return "helicopter"
    airliner = any(w in text for w in AIRLINER_WORDS)
    biz = any(w in text for w in BIZJET_WORDS)
    if engine_type == "jet":
        if biz and not airliner:
            return "bizjet"
        return "airliner" if airliner or engine_count >= 3 else "bizjet"
    if engine_type == "turboprop":
        return "airliner" if airliner else "turboprop"
    return "ga"


class XPlaneUdp(asyncio.DatagramProtocol):
    """Subskrypcje zmiennych (RREF) oraz zapis zmiennych i komendy (DREF, CMND)."""

    def __init__(self, host, port):
        self.addr = (host, port)
        self.transport = None
        self.values = {}
        self.last_data = 0.0
        self.paths = []

    def connection_made(self, transport):
        self.transport = transport

    def datagram_received(self, data, addr):
        if data[:4] != b"RREF":
            return
        for off in range(5, len(data) - 7, 8):
            idx, value = struct.unpack_from("<if", data, off)
            if 0 <= idx < len(self.paths):
                self.values[self.paths[idx]] = value
        self.last_data = time.monotonic()

    def error_received(self, exc):
        pass  # ICMP „port niedostępny”, gdy X-Plane jeszcze nie działa

    @property
    def connected(self):
        return time.monotonic() - self.last_data < SIM_STALE_S

    async def subscribe(self):
        """Paczkami, bo seria kilkuset datagramów przepełnia bufor UDP i część ginie."""
        self.paths = list(D.values()) + [f"{TITLE_REF}[{i}]" for i in range(TITLE_BYTES)]
        title_start = len(D)
        for i, path in enumerate(self.paths):
            freq = 1 if i >= title_start else MAX_RATE_HZ
            self._send(b"RREF\0" + struct.pack("<ii400s", freq, i, path.encode()))
            if i % 16 == 15:
                await asyncio.sleep(0.01)

    def unsubscribe(self):
        for i, path in enumerate(self.paths):
            self._send(b"RREF\0" + struct.pack("<ii400s", 0, i, path.encode()))

    def write(self, path, value):
        self._send(b"DREF\0" + struct.pack("<f500s", float(value), path.encode()))

    def command(self, path):
        self._send(b"CMND\0" + path.encode() + b"\0")

    def _send(self, data):
        if self.transport is not None:
            self.transport.sendto(data, self.addr)

    def title(self):
        raw = bytearray()
        for i in range(TITLE_BYTES):
            b = int(self.values.get(f"{TITLE_REF}[{i}]", 0)) & 0xFF
            if b == 0:
                break
            raw.append(b)
        return raw.decode("utf-8", "replace").strip()


class Pairing:
    def __init__(self, store_path):
        self.store_path = store_path
        self.code = f"{secrets.randbelow(1_000_000):06d}"
        self.failures = {}
        self.tokens = set()
        try:
            self.tokens = set(json.loads(store_path.read_text(encoding="utf-8")))
        except (OSError, ValueError, TypeError):
            pass

    def try_pair(self, ip, code, token, now):
        """Zwraca (token, zablokowany)."""
        count, blocked_until = self.failures.get(ip, (0, 0.0))
        if blocked_until > now:
            return None, True
        if isinstance(token, str) and token in self.tokens:
            self.failures.pop(ip, None)
            return token, False
        if isinstance(code, str) and code and hmac.compare_digest(code.encode(), self.code.encode()):
            self.failures.pop(ip, None)
            new_token = secrets.token_urlsafe(24)
            self.tokens.add(new_token)
            self._save()
            return new_token, False
        count += 1
        self.failures[ip] = (0, now + BLOCK_S) if count >= MAX_FAILURES else (count, 0.0)
        return None, False

    def _save(self):
        try:
            self.store_path.parent.mkdir(parents=True, exist_ok=True)
            self.store_path.write_text(json.dumps(sorted(self.tokens)), encoding="utf-8")
            os.chmod(self.store_path, 0o600)
        except OSError:
            pass


class TouchdownLatch:
    """Zatrzaskuje dane każdego przyziemienia; przez 0,6 s po kontakcie zbiera szczyt przeciążenia."""

    MIN_AIRBORNE_S = 0.25
    PEAK_WINDOW_S = 0.6

    def __init__(self):
        self.seq = 0
        self.current = None
        self.airborne_since = None
        self.last_on_ground = None
        self.last_air = None
        self.window_end = 0.0

    def update(self, now, on_ground, sample):
        if not on_ground:
            if self.airborne_since is None:
                self.airborne_since = now
            self.last_air = sample
        elif self.last_on_ground is False and self.airborne_since is not None \
                and now - self.airborne_since >= self.MIN_AIRBORNE_S and self.last_air is not None:
            air = self.last_air
            self.seq += 1
            self.current = {
                "seq": self.seq, "t": int(time.time() * 1000), "vsFpm": round(air["vsFpm"]),
                "gForce": round(max(air["gForce"], sample["gForce"]), 2),
                "pitchDeg": round(air["pitchDeg"], 1), "bankDeg": round(air["bankDeg"], 1),
                "headingTrueDeg": round(sample["headingTrueDeg"], 1), "lat": sample["lat"], "lon": sample["lon"],
                "iasKt": round(sample["iasKt"], 1), "gsKt": round(sample["gsKt"], 1),
            }
            self.window_end = now + self.PEAK_WINDOW_S
        elif on_ground and self.current is not None and now <= self.window_end:
            self.current["gForce"] = round(max(self.current["gForce"], sample["gForce"]), 2)
        if on_ground:
            self.airborne_since = None
        self.last_on_ground = on_ground
        return self.current


class Session:
    def __init__(self, ws, ip):
        self.ws = ws
        self.ip = ip
        self.name = ip
        self.paired = False
        self.rate_hz = DEFAULT_RATE_HZ
        self.credit = 0
        self.window_start = 0.0
        self.window_count = 0
        self.warned_look = False

    def take_command(self, now):
        if now - self.window_start >= 1.0:
            self.window_start = now
            self.window_count = 0
        self.window_count += 1
        return self.window_count <= MAX_COMMANDS_PER_SECOND


class XPlaneLink:
    def __init__(self, xplane_host="127.0.0.1", xplane_port=XPLANE_PORT, ws_port=WS_PORT,
                 discovery_port=DISCOVERY_PORT, store_path=None, discovery_targets=None):
        self.xp = XPlaneUdp(xplane_host, xplane_port)
        self.ws_port = ws_port
        self.discovery_port = discovery_port
        self.discovery_targets = discovery_targets
        self.pairing = Pairing(store_path or pathlib.Path.home() / ".opentakeoff" / "xplane-tokens.json")
        self.sessions = set()
        self.touchdown = TouchdownLatch()
        self.seq = 0
        self.axis_owner = None
        self.axis_time = 0.0
        self.axis = [0.0, 0.0, 0.0]
        self.axes_engaged = False
        self.aux_owner = None
        self.aux_time = 0.0
        self.brake = 0.0
        self.was_connected = False
        self.server = None
        self.tasks = []

    def v(self, key, default=0.0):
        return self.xp.values.get(D[key], default)

    @property
    def source(self):
        return "xplane" if self.xp.connected else "none"

    async def start(self):
        loop = asyncio.get_running_loop()
        await loop.create_datagram_endpoint(lambda: self.xp, local_addr=("0.0.0.0", 0))
        self.server = await websockets.serve(self.handle, "0.0.0.0", self.ws_port, max_size=MAX_MESSAGE_BYTES, ping_interval=15)
        self.ws_port = self.server.sockets[0].getsockname()[1]
        self.tasks = [asyncio.create_task(self.subscribe_loop()), asyncio.create_task(self.tick_loop()),
                      asyncio.create_task(self.discovery_loop())]

    async def stop(self):
        for t in self.tasks:
            t.cancel()
        await asyncio.gather(*self.tasks, return_exceptions=True)
        self.release_all()
        self.xp.unsubscribe()
        self.server.close()
        await self.server.wait_closed()
        if self.xp.transport:
            self.xp.transport.close()

    # ---------- X-Plane ----------

    async def subscribe_loop(self):
        # Ponawiamy też przy działającym połączeniu: UDP może zgubić pojedynczą subskrypcję
        since = RESUBSCRIBE_S
        while True:
            if not self.xp.connected or since >= RESUBSCRIBE_S:
                await self.xp.subscribe()
                since = 0
            await asyncio.sleep(2)
            since += 2

    async def tick_loop(self):
        step = 1.0 / MAX_RATE_HZ
        while True:
            await asyncio.sleep(step)
            now = time.monotonic()
            connected = self.xp.connected
            if connected != self.was_connected:
                self.was_connected = connected
                print("Połączono z X-Plane" if connected else "Czekam na X-Plane…")
                await self.broadcast_status()
            self.tick_axes(now, connected)
            if not connected:
                continue
            state = self.build_state(now)
            due = []
            for s in self.sessions:
                if not s.paired:
                    continue
                s.credit += s.rate_hz
                if s.credit >= MAX_RATE_HZ:
                    s.credit -= MAX_RATE_HZ
                    due.append(s)
            if due:
                self.seq += 1
                message = {"v": 1, "type": "state", "seq": self.seq, "t": int(time.time() * 1000), "data": state}
                await asyncio.gather(*[self.send(s, message) for s in due], return_exceptions=True)

    def tick_axes(self, now, connected):
        if self.axis_owner is not None and now - self.axis_time > AXIS_TIMEOUT_S:
            self.release_axes()
        if self.aux_owner is not None and now - self.aux_time > AXIS_TIMEOUT_S:
            self.release_aux()
        if not connected:
            return
        if self.axis_owner is not None:
            self.xp.write(OVERRIDE_JOYSTICK, 1)
            self.xp.write(YOKE_ROLL, self.axis[0])
            self.xp.write(YOKE_PITCH, self.axis[1])
            self.xp.write(YOKE_YAW, self.axis[2])
            self.axes_engaged = True
        if self.brake > 0:
            self.xp.write(LEFT_BRAKE, self.brake)
            self.xp.write(RIGHT_BRAKE, self.brake)

    def release_axes(self):
        self.axis_owner = None
        self.axis = [0.0, 0.0, 0.0]
        if self.axes_engaged:
            self.xp.write(YOKE_ROLL, 0)
            self.xp.write(YOKE_PITCH, 0)
            self.xp.write(YOKE_YAW, 0)
            self.xp.write(OVERRIDE_JOYSTICK, 0)
            self.axes_engaged = False

    def release_aux(self):
        self.aux_owner = None
        if self.brake > 0:
            self.xp.write(LEFT_BRAKE, 0)
            self.xp.write(RIGHT_BRAKE, 0)
        self.brake = 0.0

    def release_all(self):
        self.release_axes()
        self.release_aux()

    def build_state(self, now):
        v = self.v
        title = self.xp.title()
        on_ground = v("on_ground") >= 0.5
        lat, lon = v("lat"), v("lon")
        ias, gs = v("ias"), v("gs") * KT_PER_MS
        vs, g = v("vs"), v("g", 1.0)
        pitch, bank, hdg = v("pitch"), v("bank"), v("hdg_true")
        touchdown = self.touchdown.update(now, on_ground, {
            "vsFpm": vs, "gForce": g, "pitchDeg": pitch, "bankDeg": bank, "headingTrueDeg": hdg,
            "lat": lat, "lon": lon, "iasKt": ias, "gsKt": gs,
        })

        count = max(1, min(MAX_ENGINES, int(v("engines", 1))))
        engines = [{
            "n1Pct": round(v(f"n1{i}"), 1), "n2Pct": round(v(f"n2{i}"), 1), "rpm": round(v(f"rpm{i}")),
            "egtC": round(v(f"egt{i}")), "fuelFlowKgH": round(v(f"ff{i}") * 3600, 1),
            "throttlePct": round(v(f"throttle{i}") * 100, 1),
        } for i in range(count)]

        engine_type = engine_type_name(int(v("en_type")))
        detents = int(v("flap_detents"))
        flaps = v("flaps")
        speedbrake = v("speedbrake")
        midnight = dt.datetime.now(dt.timezone.utc).replace(hour=0, minute=0, second=0, microsecond=0)
        sim_time = midnight + dt.timedelta(seconds=int(v("zulu")))

        def radio(active, standby, scale):
            return {"activeMhz": round(v(active) / scale, 3), "standbyMhz": round(v(standby) / scale, 3)}

        state = {
            "source": "xplane",
            "aircraft": {"title": title or "X-Plane", "onGround": on_ground, "paused": v("paused") >= 0.5,
                         "engineType": engine_type, "category": classify(engine_type, count, title)},
            "simTimeUtc": sim_time.strftime("%Y-%m-%dT%H:%M:%SZ"),
            "position": {"lat": lat, "lon": lon, "altMslFt": round(v("elev") * FT_PER_M, 1),
                         "altAglFt": round(v("agl") * FT_PER_M, 1), "altIndFt": round(v("alt_ind"), 1)},
            "speed": {"iasKt": round(ias, 1), "tasKt": round(v("tas") * KT_PER_MS, 1), "gsKt": round(gs, 1),
                      "mach": round(v("mach"), 3), "vsFpm": round(vs)},
            "attitude": {"pitchDeg": round(pitch, 2), "bankDeg": round(bank, 2), "hdgTrueDeg": round(hdg, 1),
                         "hdgMagDeg": round(v("hdg_mag"), 1), "trackTrueDeg": round(v("track"), 1)},
            "autopilot": {
                "master": v("servos") >= 0.5, "fd": v("fd") >= 0.5, "athr": v("athr") >= 0.5,
                "hdgDeg": round(v("ap_hdg")), "altFt": round(v("ap_alt")), "vsFpm": round(v("ap_vs")), "spdKt": round(v("ap_spd")),
                "modes": {"hdg": v("st_hdg") >= 1, "nav": v("st_nav") >= 1, "apr": v("st_apr") >= 1, "loc": False,
                          "alt": v("st_alt") >= 2, "vs": v("st_vs") >= 2, "flc": v("st_spd") >= 2},
            },
            "radios": {
                "com1": radio("com1", "com1s", 1000), "com2": radio("com2", "com2s", 1000),
                "nav1": radio("nav1", "nav1s", 100), "nav2": radio("nav2", "nav2s", 100),
                "transponder": {"code": f"{int(v('xpdr')):04d}", "mode": XPDR_NAMES.get(int(v("xpdr_mode")), "off"), "ident": False},
            },
            "controls": {
                "gearDown": v("gear_handle") >= 0.5, "gearPct": round(v("gear") * 100, 1),
                "flapsIndex": round(flaps * detents), "flapsPositions": detents + 1, "flapsPct": round(flaps * 100, 1),
                "spoilersPct": round(max(speedbrake, 0.0) * 100, 1), "spoilersArmed": speedbrake < -0.25,
                "parkingBrake": v("parking") >= 0.5, "elevatorTrimPct": round(v("trim") * 100, 1),
                "throttlePct": round(v("throttle0") * 100, 1), "mixturePct": round(v("mixture") * 100, 1), "propPct": round(v("prop") * 100, 1),
            },
            "lights": {"beacon": v("beacon") >= 0.5, "strobe": v("strobe") >= 0.5, "nav": v("nav_lights") >= 0.5,
                       "landing": v("landing") >= 0.5, "taxi": v("taxi") >= 0.5},
            "fuel": {"totalKg": round(v("fuel"), 1)},
            "engines": engines,
            "environment": {"windDirDeg": round(v("wind_dir")), "windSpdKt": round(v("wind_spd") * KT_PER_MS, 1),
                            "oatC": round(v("oat"), 1), "qnhHpa": round(v("qnh") * HPA_PER_INHG, 1)},
            "axes": {"active": self.axis_owner is not None},
            "avionics": {
                "baroHpa": round(v("baro") * HPA_PER_INHG, 1), "radioAltFt": round(v("radio_alt"), 1),
                "slipBall": round(max(-1.0, min(1.0, v("slip") / 8.0)), 2), "turnRateDps": round(v("turn"), 2),
                "gForce": round(g, 2), "aoaDeg": round(v("aoa"), 1), "magVarDeg": round(v("magvar"), 1),
                "stallWarning": v("stall") >= 0.5,
                "ap": {"gsArmed": v("st_gs") == 1, "gsActive": v("st_gs") >= 2},
            },
        }
        if touchdown is not None:
            state["touchdown"] = touchdown
        return state

    # ---------- telefon ----------

    async def send(self, session, message):
        await asyncio.wait_for(session.ws.send(json.dumps(message, ensure_ascii=False, allow_nan=False)), 5)

    async def error(self, session, msg_id, code, text):
        await self.send(session, {"v": 1, "type": "error", "id": msg_id, "code": code, "message": text})

    def status_message(self):
        connected = self.xp.connected
        return {"v": 1, "type": "status", "source": self.source, "simConnected": connected,
                "message": "Połączono z X-Plane" if connected else "Czekam na X-Plane (Settings → Network: włącz UDP)"}

    async def broadcast_status(self):
        message = self.status_message()
        await asyncio.gather(*[self.send(s, message) for s in self.sessions if s.paired], return_exceptions=True)

    async def handle(self, ws):
        session = Session(ws, ws.remote_address[0])
        self.sessions.add(session)
        started = time.monotonic()
        try:
            while True:
                timeout = IDLE_TIMEOUT_S if session.paired else HELLO_TIMEOUT_S - (time.monotonic() - started)
                if timeout <= 0:
                    await ws.close(1008, "hello timeout")
                    return
                try:
                    text = await asyncio.wait_for(ws.recv(), timeout)
                except asyncio.TimeoutError:
                    await ws.close(1008, "timeout")
                    return
                if isinstance(text, str):
                    await self.on_message(session, text)
        except websockets.exceptions.ConnectionClosed:
            pass
        finally:
            if self.axis_owner is session:
                self.release_axes()
            if self.aux_owner is session:
                self.release_aux()
            self.sessions.discard(session)

    async def on_message(self, session, text):
        try:
            msg = json.loads(text)
        except ValueError:
            await self.error(session, None, "bad_json", "Niepoprawny JSON")
            return
        if not isinstance(msg, dict):
            await self.error(session, None, "bad_json", "Oczekiwano obiektu JSON")
            return
        msg_id = msg.get("id")
        if msg.get("v") != 1:
            await self.error(session, msg_id, "unsupported_version", "Obsługiwana wersja protokołu: 1")
            return
        kind = msg.get("type")
        if kind == "ping":
            await self.send(session, {"v": 1, "type": "pong", "id": msg_id, "t": msg.get("t"), "serverT": int(time.time() * 1000)})
        elif kind == "hello":
            await self.on_hello(session, msg)
        elif not session.paired:
            await self.error(session, msg_id, "not_paired", "Najpierw wyślij hello z kodem parowania")
        elif kind == "command":
            await self.on_command(session, msg)
        elif kind == "axis":
            await self.on_axis(session, msg)
        elif kind == "navdata_request":
            await self.error(session, msg_id, "sim_unavailable", "Dane nawigacyjne są dostępne tylko z MSFS")
        else:
            await self.error(session, msg_id, "unknown_type", f"Nieznany typ wiadomości: {kind}")

    async def on_hello(self, session, msg):
        token, blocked = self.pairing.try_pair(session.ip, msg.get("code"), msg.get("token"), time.monotonic())
        if token is None:
            await self.error(session, msg.get("id"), "rate_limited" if blocked else "pairing_failed",
                             "Za dużo prób — odczekaj minutę" if blocked else "Zły kod parowania lub nieważny token")
            return
        client = msg.get("client")
        if isinstance(client, dict) and isinstance(client.get("name"), str) and client["name"].strip():
            session.name = client["name"].strip()[:40]
        rate = msg.get("rateHz")
        session.rate_hz = max(MIN_RATE_HZ, min(MAX_RATE_HZ, rate)) if isinstance(rate, int) else DEFAULT_RATE_HZ
        session.paired = True
        print(f"Sparowano: {session.name} ({session.ip})")
        await self.send(session, {
            "v": 1, "type": "hello_ok", "id": msg.get("id"), "token": token, "server": "OpenTakeoff Link (X-Plane)",
            "version": VERSION, "source": self.source, "rateHz": session.rate_hz,
            "limits": {"maxDeflection": MAX_DEFLECTION, "axisTimeoutMs": int(AXIS_TIMEOUT_S * 1000)}, "features": ["touchdown", "aircraft_category"],
        })
        await self.send(session, self.status_message())

    @staticmethod
    def integer(value, low, high):
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            return None
        if not math.isfinite(value) or value != math.floor(value) or not low <= value <= high:
            return None
        return int(value)

    async def on_command(self, session, msg):
        msg_id, name, value = msg.get("id"), msg.get("name"), msg.get("value")
        if name in LOOK_COMMANDS:
            # Rozglądanie to strumień komend, więc o braku obsługi informujemy tylko raz
            if not session.warned_look:
                session.warned_look = True
                await self.error(session, msg_id, "sim_error", "Rozglądanie się działa tylko w MSFS")
            return
        if not session.take_command(time.monotonic()):
            await self.error(session, msg_id, "rate_limited", f"Maksymalnie {MAX_COMMANDS_PER_SECOND} komend na sekundę")
            return
        action = self.resolve_command(name, value)
        if isinstance(action, tuple):
            await self.error(session, msg_id, *action)
            return
        if not self.xp.connected:
            await self.error(session, msg_id, "sim_unavailable", "Brak połączenia z X-Plane")
            return
        action()
        await self.send(session, {"v": 1, "type": "ack", "id": msg_id, "name": name})

    def resolve_command(self, name, value):
        """Zwraca funkcję wykonującą komendę albo krotkę (kod błędu, opis)."""
        xp = self.xp
        if name in COMMANDS:
            return lambda: xp.command(COMMANDS[name])
        if name in SETTERS:
            low, high, path, convert = SETTERS[name]
            n = self.integer(value, low, high)
            if n is None:
                return "invalid_value", f"{name} wymaga wartości całkowitej {low}…{high}"
            return lambda: xp.write(path, convert(n))
        if name in UNSUPPORTED:
            return "sim_error", f"X-Plane nie obsługuje komendy {name}"
        if name == "XPNDR_SET":
            n = self.integer(value, 0, 7777)
            if n is None or any(c > "7" for c in f"{n:04d}"):
                return "invalid_value", "Kod transpondera musi mieć cyfry 0–7"
            return lambda: xp.write(D["xpdr"], n)
        if name == "XPNDR_MODE":
            n = self.integer(value, 0, 5)
            if n is None:
                return "invalid_value", "XPNDR_MODE wymaga wartości 0…5"
            return lambda: xp.write(D["xpdr_mode"], XPDR_TO_XPLANE[n])
        if name == "CAMERA_STATE":
            n = self.integer(value, 2, 4)
            if n is None:
                return "invalid_value", "CAMERA_STATE wymaga wartości 2…4"
            return lambda: xp.command(CAMERA_COMMANDS[n])
        fixed = {
            "FLAPS_UP": (D["flaps"], 0), "FLAPS_DOWN": (D["flaps"], 1), "SPOILERS_ON": (D["speedbrake"], 1), "SPOILERS_OFF": (D["speedbrake"], 0),
            "THROTTLE_CUT": (SETTERS["THROTTLE_SET"][2], 0), "THROTTLE_FULL": (SETTERS["THROTTLE_SET"][2], 1),
        }
        if name in fixed:
            path, target = fixed[name]
            return lambda: xp.write(path, target)
        if name == "SPOILERS_TOGGLE":
            return lambda: xp.write(D["speedbrake"], 0 if self.v("speedbrake") > 0 else 1)
        if name == "SPOILERS_ARM_TOGGLE":
            return lambda: xp.write(D["speedbrake"], 0 if self.v("speedbrake") < -0.25 else -0.5)
        if name == "PARKING_BRAKES":
            return lambda: xp.write(D["parking"], 0 if self.v("parking") >= 0.5 else 1)
        return "not_allowed", f"Komenda poza listą dozwolonych: {name}"

    async def on_axis(self, session, msg):
        msg_id = msg.get("id")
        if msg.get("release") is True:
            if self.axis_owner is session:
                self.release_axes()
            if self.aux_owner is session:
                self.release_aux()
            return
        values = {}
        for key, low, high in (("aileron", -1, 1), ("elevator", -1, 1), ("rudder", -1, 1), ("speedbrake", 0, 1), ("brake", 0, 1)):
            raw = msg.get(key)
            if raw is None:
                continue
            if isinstance(raw, bool) or not isinstance(raw, (int, float)) or not math.isfinite(raw):
                await self.error(session, msg_id, "invalid_value", f"{key} musi być liczbą")
                return
            values[key] = max(low, min(high, raw))
        now = time.monotonic()
        if "speedbrake" in values or "brake" in values:
            if self.aux_owner not in (None, session):
                await self.error(session, msg_id, "not_allowed", "Hamulcami steruje już inny telefon")
            else:
                self.aux_owner, self.aux_time = session, now
                if "speedbrake" in values:
                    self.xp.write(D["speedbrake"], values["speedbrake"])
                if "brake" in values:
                    if values["brake"] == 0 and self.brake > 0:
                        self.xp.write(LEFT_BRAKE, 0)
                        self.xp.write(RIGHT_BRAKE, 0)
                    self.brake = values["brake"]
        if not {"aileron", "elevator", "rudder"} & values.keys():
            return
        if self.axis_owner not in (None, session):
            await self.error(session, msg_id, "not_allowed", "Osiami steruje już inny telefon")
            return
        self.axis_owner, self.axis_time = session, now
        for i, key in enumerate(("aileron", "elevator", "rudder")):
            if key in values:
                self.axis[i] = max(-MAX_DEFLECTION, min(MAX_DEFLECTION, values[key]))

    # ---------- wykrywanie w sieci ----------

    @staticmethod
    def local_ip():
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
            try:
                s.connect(("10.255.255.255", 1))
                return s.getsockname()[0]
            except OSError:
                return "127.0.0.1"

    async def discovery_loop(self):
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
            sock.setblocking(False)
            while True:
                ip = self.local_ip()
                targets = self.discovery_targets or ["255.255.255.255", ip.rsplit(".", 1)[0] + ".255"]
                payload = json.dumps({"v": 1, "type": "announce", "name": "OpenTakeoff Link (X-Plane)", "version": VERSION,
                                      "host": socket.gethostname(), "port": self.ws_port, "ips": [ip], "source": self.source}).encode()
                for target in targets:
                    try:
                        sock.sendto(payload, (target, self.discovery_port))
                    except OSError:
                        pass
                await asyncio.sleep(2)


async def run(args):
    link = XPlaneLink(args.xplane_host, args.xplane_port, args.port)
    try:
        await link.start()
    except OSError as e:
        print(f"Nie można otworzyć portu {args.port}: {e}. Czy działa już OpenTakeoff Link?")
        return
    print(f"OpenTakeoff Link dla X-Plane, port {link.ws_port}")
    print(f"Kod parowania: {link.pairing.code}")
    print(f"Czekam na X-Plane pod adresem {args.xplane_host}:{args.xplane_port}…")
    try:
        await asyncio.Future()
    finally:
        await link.stop()


def main():
    parser = argparse.ArgumentParser(description="OpenTakeoff Link dla X-Plane")
    parser.add_argument("--xplane-host", default="127.0.0.1", help="adres komputera z X-Plane")
    parser.add_argument("--xplane-port", type=int, default=XPLANE_PORT)
    parser.add_argument("--port", type=int, default=WS_PORT, help="port dla telefonu")
    try:
        asyncio.run(run(parser.parse_args()))
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
