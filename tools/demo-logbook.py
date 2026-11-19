"""Pokazowy dziennik lotów OpenTakeoff do zrzutów ekranu: 58 lotów po Europie (maj–listopad 2026) z oceną
lądowań, a ostatnie 14 z gęstym śladem i przebiegiem podejścia do analizy lądowania.

    python tools/demo-logbook.py <folder logbook> [pl|en|es]

Zapisuje flights.oab (zwykły JSON – program i telefon czytają go jak archiwum) i meta.json. Uwagi do lądowań
w wybranym języku, tak jak zapisałby je program."""
import json, math, os, random, sys, uuid, datetime as dt

random.seed(7)
AP = {  # icao: lat, lon, elev_ft, runway, rwy_hdg
    "EPWA": (52.1657, 20.9671, 362, "33", 330), "EPGD": (54.3776, 18.4662, 489, "29", 290),
    "EPKK": (50.0777, 19.7848, 791, "25", 250), "EGLL": (51.4700, -0.4543, 83, "27L", 270),
    "EDDF": (50.0379, 8.5622, 364, "25C", 250), "LFPG": (49.0097, 2.5479, 392, "26R", 266),
    "LEMD": (40.4983, -3.5676, 1998, "32L", 323), "LIRF": (41.8003, 12.2389, 13, "16R", 165),
    "EHAM": (52.3105, 4.7683, -11, "18R", 183), "LOWW": (48.1103, 16.5697, 600, "29", 293),
    "LSZH": (47.4647, 8.5492, 1416, "14", 137), "EKCH": (55.6180, 12.6508, 17, "22L", 223),
    "ENGM": (60.1939, 11.1004, 681, "19R", 194), "LPPT": (38.7813, -9.1359, 374, "02", 23),
    "LGAV": (37.9364, 23.9445, 308, "03R", 36), "LKPR": (50.1008, 14.2600, 1247, "24", 243),
    "EDDM": (48.3538, 11.7861, 1487, "26R", 263), "LEBL": (41.2974, 2.0833, 12, "24R", 246),
    "EIDW": (53.4213, -6.2701, 242, "28L", 280), "LHBP": (47.4369, 19.2556, 495, "31R", 313),
    "LPMA": (32.6979, -16.7745, 192, "05", 50), "LOWI": (47.2602, 11.3439, 1906, "26", 263),
    "EPWR": (51.1027, 16.8858, 404, "29", 291), "EPPO": (52.4210, 16.8263, 308, "28", 281),
    "EPKT": (50.4743, 19.0800, 994, "27", 273), "LXGB": (36.1512, -5.3497, 15, "27", 269),
    "BIKF": (63.985, -22.6056, 171, "19", 190), "LIMC": (45.6306, 8.7231, 768, "35R", 349),
}
JETS = [("Airbus A320neo", 34000, 450), ("Boeing 737-800", 36000, 455), ("Boeing 787-10 Dreamliner", 39000, 488),
        ("Embraer E190", 35000, 440), ("Airbus A321neo", 35000, 450)]
TURBO = [("ATR 72-600", 22000, 275)]
GA = [("Cessna 172 Skyhawk", 4500, 115), ("Diamond DA40 NG", 6500, 130)]
ROUTES = [  # (from, to, kind)
    ("EPWA", "EGLL", "jet"), ("EGLL", "EPWA", "jet"), ("EPWA", "LFPG", "jet"), ("LFPG", "LEMD", "jet"),
    ("LEMD", "LPPT", "jet"), ("LPPT", "LPMA", "jet"), ("LPMA", "LPPT", "jet"), ("EPKK", "EDDF", "jet"),
    ("EDDF", "LIRF", "jet"), ("LIRF", "LGAV", "jet"), ("LGAV", "LOWW", "jet"), ("LOWW", "EPWA", "jet"),
    ("EPWA", "EKCH", "jet"), ("EKCH", "ENGM", "jet"), ("ENGM", "BIKF", "jet"), ("BIKF", "EIDW", "jet"),
    ("EIDW", "EHAM", "jet"), ("EHAM", "LSZH", "jet"), ("LSZH", "LEBL", "jet"), ("LEBL", "LXGB", "jet"),
    ("EDDM", "LOWI", "jet"), ("LOWI", "EDDM", "jet"), ("EPGD", "EPWA", "turbo"), ("EPWA", "EPKK", "turbo"),
    ("EPKK", "LKPR", "turbo"), ("LKPR", "LHBP", "turbo"), ("LHBP", "EPWA", "turbo"), ("EPWR", "EPPO", "turbo"),
    ("EPKK", "EPKT", "ga"), ("EPKT", "EPKK", "ga"), ("EPPO", "EPWR", "ga"), ("EPGD", "EPGD", "ga"),
    ("EPWA", "LIMC", "jet"), ("LIMC", "EPWA", "jet"), ("EPWA", "EPGD", "jet"), ("EPGD", "EKCH", "turbo"),
]
NOTES = {
    "en": {"soft": "Soft touchdown: {v} ft/min.", "firm": "Firm touchdown: {v} ft/min.",
           "tdz": "In the touchdown zone: {ft} ft past the threshold.", "xw": "Crosswind {kt} kt – score softened.",
           "grades": ["Excellent", "Good", "Fair", "Hard"]},
    "pl": {"soft": "Miękkie przyziemienie: {v} ft/min.", "firm": "Wyczuwalne przyziemienie: {v} ft/min.",
           "tdz": "W strefie przyziemienia: {ft} ft za progiem.", "xw": "Wiatr boczny {kt} kt – wynik złagodzony.",
           "grades": ["Doskonałe", "Dobre", "Poprawne", "Twarde"]},
    "es": {"soft": "Toma suave: {v} ft/min.", "firm": "Toma firme: {v} ft/min.",
           "tdz": "En la zona de toma: {ft} ft pasado el umbral.", "xw": "Viento cruzado {kt} kt – nota suavizada.",
           "grades": ["Excelente", "Bueno", "Correcto", "Duro"]},
}
LANG = sys.argv[2] if len(sys.argv) > 2 and sys.argv[2] in NOTES else "en"
NOTE = NOTES[LANG]


def gc(a, b):
    la1, lo1, la2, lo2 = map(math.radians, (a[0], a[1], b[0], b[1]))
    d = 2 * math.asin(math.sqrt(math.sin((la2 - la1) / 2) ** 2 + math.cos(la1) * math.cos(la2) * math.sin((lo2 - lo1) / 2) ** 2))
    return d


def interp(a, b, f):
    d = gc(a, b)
    if d < 1e-9:
        return a[0], a[1]
    la1, lo1, la2, lo2 = map(math.radians, (a[0], a[1], b[0], b[1]))
    A = math.sin((1 - f) * d) / math.sin(d); B = math.sin(f * d) / math.sin(d)
    x = A * math.cos(la1) * math.cos(lo1) + B * math.cos(la2) * math.cos(lo2)
    y = A * math.cos(la1) * math.sin(lo1) + B * math.cos(la2) * math.sin(lo2)
    z = A * math.sin(la1) + B * math.sin(la2)
    return math.degrees(math.atan2(z, math.hypot(x, y))), math.degrees(math.atan2(y, x))


def brg(a, b):
    la1, lo1, la2, lo2 = map(math.radians, (a[0], a[1], b[0], b[1]))
    y = math.sin(lo2 - lo1) * math.cos(la2)
    x = math.cos(la1) * math.sin(la2) - math.sin(la1) * math.cos(la2) * math.cos(lo2 - lo1)
    return (math.degrees(math.atan2(y, x)) + 360) % 360


def landing(apt, kind, t_ms, profile):
    vs = int(random.choice([-62, -85, -104, -118, -132, -146, -171, -188, -212, -240, -96, -77, -128, -310]))
    if kind == "ga":
        vs = int(vs * 0.8)
    along = random.randint(900, 1700) if kind != "ga" else random.randint(350, 900)
    cross = random.randint(-14, 14)
    xw = random.choice([2, 4, 6, 9, 12, 15, 5, 3])
    score = max(38, min(99, int(100 - abs(vs) * 0.12 - abs(cross) * 0.4 - (8 if abs(vs) > 280 else 0) + random.randint(-3, 3) + (4 if xw > 10 else 0))))
    g = NOTE["grades"]
    grade = g[0] if score >= 90 else g[1] if score >= 75 else g[2] if score >= 55 else g[3]
    notes = [(NOTE["soft"] if abs(vs) < 160 else NOTE["firm"]).format(v=abs(vs)), NOTE["tdz"].format(ft=along)]
    if xw > 10:
        notes.append(NOTE["xw"].format(kt=xw))
    rep = {"score": score, "grade": grade, "vsFpm": vs, "gForce": round(1.0 + abs(vs) / 900 + random.random() * 0.05, 2),
           "bankDeg": round(random.uniform(-1.8, 1.8), 1), "pitchDeg": round(random.uniform(2.5, 5.5) if kind != "ga" else random.uniform(5, 9), 1),
           "iasKt": {"jet": random.randint(128, 146), "turbo": random.randint(104, 114), "ga": random.randint(56, 64)}[kind],
           "runway": AP[apt][3], "alongFt": along, "crossFt": cross, "crosswindKt": xw, "bounces": 0, "notes": notes,
           "airport": apt, "at": t_ms, "runwayLengthFt": random.choice([9186, 10663, 11483, 12795, 8202, 6568]) if kind != "ga" else 8202,
           "profile": []}
    if profile:
        ias0 = rep["iasKt"]
        for dms in range(-30000, 6001, 200):
            s = dms / 1000
            if s < 0:
                agl = max(0, int(-s * (16.5 if kind != "ga" else 9) - (0 if s < -6 else (6 + s) ** 2 * 1.6)))
                v = int(-760 if s < -6 else -760 + (760 + vs) * (1 - (-s / 6)) ** 0.7) if kind != "ga" else int(-480 if s < -6 else -480 + (480 + vs) * (1 + s / 6))
                ias = ias0 + int(-s * 0.25)
                pitch = 2.6 if s < -6 else 2.6 + (6 + s) * 0.45
                x = int(along + s * ias0 * 1.6878)
                gnd = False
            else:
                agl, v, ias, pitch, x, gnd = 0, 0, int(ias0 - s * 4.5), max(0.0, rep["pitchDeg"] - s * 1.4), int(along + s * (ias0 - s * 2) * 1.6878), True
            rep["profile"].append({"dt": dms, "agl": agl, "vs": v, "ias": ias, "gs": max(0, ias - 6), "g": round(1.0 if s < 0 else (rep["gForce"] if dms == 0 else 1.0), 2),
                                   "bank": round(math.sin(s * 0.7) * (2.5 if s < -8 else 0.8), 1), "pitch": round(pitch, 1), "x": x,
                                   "y": int(cross + math.sin(s * 0.3) * (12 if s < -5 else 2) * (1 if s < 0 else 0.2)), "gnd": gnd})
    return rep


def flight(fr, to, kind, start, recent):
    ac, cruise, tas = random.choice({"jet": JETS, "turbo": TURBO, "ga": GA}[kind])
    a, b = AP[fr], AP[to]
    local = fr == to
    dist = gc(a, b) * 3440.065 if not local else 48
    cruise = min(cruise, max(9000 if kind != "ga" else 3500, int(dist * 140) // 1000 * 1000)) if kind != "ga" else cruise
    climb_fpm = {"jet": 2300, "turbo": 1400, "ga": 650}[kind]
    desc_fpm = {"jet": 1800, "turbo": 1200, "ga": 550}[kind]
    climb_min = (cruise - a[2]) / climb_fpm
    desc_min = (cruise - b[2]) / desc_fpm
    gs_c = tas * 0.78
    climb_nm = climb_min / 60 * gs_c; desc_nm = desc_min / 60 * gs_c
    cruise_nm = max(5, dist - climb_nm - desc_nm)
    total_min = climb_min + desc_min + cruise_nm / tas * 60 + (14 if kind != "ga" else 6)
    step = 8 if recent else 20
    n = int(total_min * 60 / step)
    taxi = 7 * 60 if kind != "ga" else 3 * 60
    track = []
    for i in range(n + 1):
        ts = i * step
        if local:  # krąg po okolicy
            f = ts / (total_min * 60)
            ang = f * 2 * math.pi
            lat = a[0] + 0.28 * math.sin(ang); lon = a[1] + 0.45 * (1 - math.cos(ang))
            alt = a[2] + int((cruise - a[2]) * min(1, math.sin(math.pi * f) * 1.6)) if 0.03 < f < 0.97 else a[2]
            gs = tas if 0.03 < f < 0.97 else 12
            hdg = int((math.degrees(math.atan2(0.45 * math.sin(ang), 0.28 * math.cos(ang))) + 360) % 360)
            vs = 0
        else:
            air = ts - taxi
            fly = total_min * 60 - 2 * taxi
            if air < 0 or air > fly:
                f = 0 if air < 0 else 1
                alt, gs, vs = (a[2] if air < 0 else b[2]), 14, 0
            else:
                tm = air / 60
                if tm < climb_min:
                    alt = a[2] + tm * climb_fpm; vs = climb_fpm; nm = tm / 60 * gs_c; gs = gs_c * (0.55 + 0.45 * tm / climb_min)
                elif tm > fly / 60 - desc_min:
                    r = fly / 60 - tm; alt = b[2] + r * desc_fpm; vs = -desc_fpm; nm = dist - r / 60 * gs_c; gs = gs_c * (0.5 + 0.5 * r / desc_min)
                else:
                    alt = cruise + random.randint(-20, 20); vs = 0; nm = climb_nm + (tm - climb_min) / 60 * tas; gs = tas + random.randint(-25, 25)
                f = max(0, min(1, nm / dist))
            lat, lon = interp(a, b, f)
            hdg = int(brg((lat, lon), b) if f < 0.999 else b[4])
        p = {"t": start + ts * 1000, "lat": round(lat, 5), "lon": round(lon, 5), "altFt": int(alt), "gsKt": int(gs),
             "vsFpm": int(vs), "iasKt": int(gs * (1 - alt / 140000) if gs > 30 else 0), "hdg": hdg}
        if recent:
            p.update(aglFt=max(0, int(alt - (a[2] if f < 0.5 else b[2]))), onGround=gs < 30, bankDeg=round(random.uniform(-3, 3), 1),
                     pitchDeg=round(2.5 if vs == 0 else (8 if vs > 0 else -1.5), 1), gLoad=1.0)
        track.append(p)
    end = track[-1]["t"]
    land_t = end - (taxi - 60) * 1000
    lands = [landing(to, kind, land_t, recent)]
    if kind == "ga" and local:
        lands.insert(0, landing(to, kind, land_t - 1500 * 1000, False))
    return {"id": str(uuid.uuid4()), "start": start, "end": end, "aircraft": ac, "from": fr, "to": to,
            "maxAltFt": max(p["altFt"] for p in track), "distanceNm": int(dist), "track": track,
            "category": "Airplane", "landings": lands, "markers": [], "landing": lands[-1]}


def main(out):
    flights = []
    t = dt.datetime(2026, 5, 24, 9, 0, tzinfo=dt.timezone.utc)
    order = ROUTES * 2
    random.shuffle(order)
    order = order[:58]
    for i, (fr, to, kind) in enumerate(order):
        t += dt.timedelta(days=random.choice([1, 2, 3, 3, 4, 5]), hours=random.randint(-6, 6))
        hr = random.choice([6, 8, 10, 13, 16, 19, 21, 23])
        start = t.replace(hour=hr, minute=random.randint(0, 59))
        if start > dt.datetime(2026, 11, 18, tzinfo=dt.timezone.utc):
            break
        flights.append(flight(fr, to, kind, int(start.timestamp() * 1000), i >= len(order) - 14))
    import os
    os.makedirs(os.path.dirname(os.path.abspath(out)), exist_ok=True)
    json.dump({"flights": flights}, open(out, "w", encoding="utf-8"))
    meta = {"flights": {f["id"]: {"notes": "", "tags": [], "favorite": i % 9 == 0, "source": "pc", "addedAt": f["end"]} for i, f in enumerate(flights)}, "tombstones": {}}
    json.dump(meta, open(out.replace("flights.oab", "meta.json"), "w", encoding="utf-8"))
    print(len(flights), "flights", sum(f["distanceNm"] for f in flights), "nm")


main(os.path.join(sys.argv[1], "flights.oab"))
