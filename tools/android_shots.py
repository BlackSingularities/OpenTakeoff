"""Zrzuty ekranu aplikacji na Androida w emulatorze (CI, workflow „Zrzuty ekranu”).

    python3 tools/android_shots.py <apk> <folder danych headless> <folder zrzutów> [pl,en]

Emulator już działa (adb widzi jedno urządzenie). Obok działa `headless` (rdzeń OpenTakeoff w trybie
symulacji), osiągalny z emulatora pod 10.0.2.2:47810; kod parowania leży w <folder danych>/code.txt.
Nawigacja po tekstach z plików językowych aplikacji (app/src/main/resources/lang), więc te same kroki
działają w każdym języku. Każdy zrzut: <folder>/<język>/<nr>-<nazwa>.png; brak elementu nie przerywa
przebiegu, tylko trafia do logu.
"""
import json
import os
import re
import subprocess
import sys
import time
import xml.etree.ElementTree as ET

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PKG = "com.freemetar.app"
LANG_DIR = os.path.join(ROOT, "app", "src", "main", "resources", "lang")
LOCALES = {"pl": "pl-PL", "en": "en-GB", "es": "es-ES"}


def log(m):
    print(time.strftime("[%H:%M:%S] ") + m, flush=True)


def adb(*a, check=False, timeout=60):
    r = subprocess.run(["adb", *a], capture_output=True, text=True, timeout=timeout)
    if check and r.returncode:
        raise RuntimeError(f"adb {' '.join(a)}: {r.stderr}")
    return r.stdout


def sh(cmd, timeout=60):
    return adb("shell", cmd, timeout=timeout)


class Ui:
    def __init__(self, lang):
        self.t = json.load(open(os.path.join(LANG_DIR, f"{lang}.json"), encoding="utf-8"))

    def s(self, key):
        return self.t.get(key, key)

    @staticmethod
    def nodes():
        for _ in range(4):
            sh("uiautomator dump /sdcard/ui.xml >/dev/null 2>&1")
            xml = sh("cat /sdcard/ui.xml")
            try:
                root = ET.fromstring(xml[xml.find("<"):]) if "<" in xml else None
            except ET.ParseError:
                root = None
            if root is not None:
                out = []
                for n in root.iter("node"):
                    m = re.match(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]", n.get("bounds", ""))
                    if m:
                        x1, y1, x2, y2 = map(int, m.groups())
                        if x2 > x1 and y2 > y1:
                            out.append({"text": n.get("text", ""), "desc": n.get("content-desc", ""), "cls": n.get("class", ""),
                                        "x1": x1, "y1": y1, "x2": x2, "y2": y2, "enabled": n.get("enabled") == "true"})
                return out
            time.sleep(1)
        return []

    def find(self, label, contains=False, nodes=None, pick=None):
        nodes = nodes if nodes is not None else self.nodes()
        hits = [n for n in nodes if (label in n["text"] or label in n["desc"]) if contains] if contains else \
            [n for n in nodes if n["text"] == label or n["desc"] == label]
        if pick and hits:
            hits.sort(key=pick)
        return hits[0] if hits else None

    def wait(self, label, seconds=30, contains=False):
        end = time.time() + seconds
        while time.time() < end:
            n = self.find(label, contains)
            if n:
                return n
            time.sleep(1.5)
        return None

    @staticmethod
    def tap_node(n):
        sh(f"input tap {(n['x1'] + n['x2']) // 2} {(n['y1'] + n['y2']) // 2}")
        time.sleep(1.2)

    def tap(self, label, contains=False, wait=12, pick=None):
        n = self.wait(label, wait, contains) if pick is None else None
        if pick is not None:
            end = time.time() + wait
            while time.time() < end and not n:
                n = self.find(label, contains, pick=pick)
                if not n:
                    time.sleep(1.5)
        if not n:
            log(f"  ! nie znaleziono „{label}”")
            return False
        self.tap_node(n)
        return True


def screen_size():
    m = re.search(r"(\d+)x(\d+)", sh("wm size"))
    return (int(m.group(1)), int(m.group(2))) if m else (1080, 2400)


def swipe_up(frac=0.5):
    w, h = screen_size()
    x = int(w * frac)
    sh(f"input swipe {x} {int(h * 0.78)} {x} {int(h * 0.32)} 500")
    time.sleep(1.5)


def back():
    sh("input keyevent 4")
    time.sleep(1.2)


def hide_keyboard():
    if "mInputShown=true" in sh("dumpsys input_method"):
        back()


def set_locale(tag):
    """Język systemu (aplikacja domyślnie bierze język telefonu). Obraz google_apis pozwala na adb root."""
    adb("root")
    time.sleep(3)
    adb("wait-for-device")
    sh(f"setprop persist.sys.locale {tag}; setprop ctl.restart zygote")
    time.sleep(8)
    adb("wait-for-device")
    end = time.time() + 240
    while time.time() < end:
        if sh("getprop sys.boot_completed").strip() == "1" and "mCurrentFocus" in sh("dumpsys window | grep mCurrentFocus"):
            break
        time.sleep(3)
    time.sleep(10)
    for k in ("window_animation_scale", "transition_animation_scale", "animator_duration_scale"):
        sh(f"settings put global {k} 0")
    sh("settings put system accelerometer_rotation 0")
    sh("settings put system user_rotation 0")
    sh("svc power stayon true")
    sh("input keyevent 82")
    sh("cmd notification set_dnd priority")
    log(f"język systemu {tag}: {sh('getprop persist.sys.locale').strip()}")


def dismiss(ui):
    """Systemowe „nie odpowiada” po zimnym starcie i ewentualne okna aplikacji (aktualizacja, zgody)."""
    labels = {"Wait", "Czekaj", "Esperar", "OK", "Later", "Później", "Allow", "Zezwól"}
    labels |= {v for k, v in ui.t.items() if k.endswith((".dont_send", ".don_t_send", ".later"))}
    for _ in range(3):
        n = next((n for n in ui.nodes() if n["text"] in labels), None)
        if not n:
            return
        log(f"  zamykam okno: {n['text']}")
        ui.tap_node(n)


class Run:
    def __init__(self, lang, out, data):
        self.lang, self.ui, self.data = lang, Ui(lang), data
        self.out = os.path.join(out, lang)
        os.makedirs(self.out, exist_ok=True)
        self.n = 0

    def shot(self, name):
        self.n += 1
        f = os.path.join(self.out, f"{self.n:02d}-{name}.png")
        with open(f, "wb") as fh:
            fh.write(subprocess.run(["adb", "exec-out", "screencap", "-p"], capture_output=True, timeout=60).stdout)
        log(f"  zrzut {os.path.basename(f)} ({os.path.getsize(f) // 1024} kB)")

    def crashlog(self, name):
        """Stos wywołań awarii aplikacji (bufor crash) – do pliku obok zrzutów."""
        txt = adb("logcat", "-d", "-b", "crash", timeout=60)
        if txt.strip():
            open(os.path.join(self.out, f"crash-{name}.txt"), "w", encoding="utf-8").write(txt)
            log(f"  ! awaria aplikacji – zapisano crash-{name}.txt")
            adb("logcat", "-c", "-b", "crash")

    def tab(self, key):
        ui = self.ui
        label = ui.s(key)
        # Dolny pasek nawigacji: najniższe dopasowanie
        n = ui.find(label, pick=lambda n: -n["y1"])
        if not n:
            log(f"  ! brak zakładki „{label}”")
            return False
        ui.tap_node(n)
        time.sleep(2.5)
        return True

    def launch(self):
        sh(f"am force-stop {PKG}")
        sh(f"am start -W -n {PKG}/.MainActivity")
        ui = self.ui
        # Ekran powitalny przy pierwszym uruchomieniu
        for _ in range(3):
            if not ui.wait(ui.s("welcome.get_started"), 8):
                break
            ui.tap(ui.s("welcome.get_started"))
            time.sleep(3)
        # Pierwsze uruchomienie pobiera bazę lotnisk
        ui.wait(ui.s("home.airport_icao_iata_or_city"), 120, contains=True)
        time.sleep(4)
        dismiss(ui)

    def connect(self):
        ui = self.ui
        self.tab("common.flight")
        time.sleep(2)
        field = ui.wait(ui.s("live.pc_ip_address"), 20, contains=True)
        for _ in range(4):
            if field:
                break
            swipe_up()
            field = ui.find(ui.s("live.pc_ip_address"), contains=True)
        if not field:
            log("  ! brak pola adresu IP")
            return False
        ui.tap_node(field)
        sh("input text 10.0.2.2")
        time.sleep(1)
        hide_keyboard()
        # Przycisk „Połącz” w wierszu pola adresu (najbliższy w pionie)
        fy = (field["y1"] + field["y2"]) / 2
        ui.tap(ui.s("live.connect"), pick=lambda n: abs((n["y1"] + n["y2"]) / 2 - fy))
        code = open(os.path.join(self.data, "code.txt")).read().strip()
        time.sleep(4)
        edit = next((n for n in ui.nodes() if n["cls"].endswith("EditText") and n["y1"] < screen_size()[1] * 0.8), None)
        if ui.find(ui.s("live.pair")) and edit:
            ui.tap_node(edit)
            sh(f"input text {code}")
            time.sleep(1)
            hide_keyboard()
            ui.tap(ui.s("live.pair"))
        else:
            log("  (bez okna parowania – telefon już sparowany albo okno się nie pokazało)")
        time.sleep(6)
        ok = ui.wait(ui.s("live.connected_simulation_mode"), 40, contains=True)
        log("  połączono z symulacją" if ok else "  ! brak potwierdzenia połączenia")
        return bool(ok)

    def go(self):
        ui = self.ui
        log(f"== {self.lang}")
        set_locale(LOCALES[self.lang])
        self.launch()
        self.shot("lotniska")
        swipe_up()
        self.shot("lotniska-2")

        # Briefing lotniska
        self.launch()
        if ui.tap(ui.s("home.airport_icao_iata_or_city"), contains=True):
            sh("input text EPGD")
            time.sleep(2)
            sh("input keyevent 66")
            time.sleep(10)
            hide_keyboard()
            self.shot("briefing")
            swipe_up()
            self.shot("briefing-2")
            swipe_up()
            self.shot("briefing-3")
            back()
        self.crashlog("po-briefingu")

        # Połączenie z rdzeniem w trybie symulacji: kokpit, synchronizacja dziennika
        self.launch()
        self.connect()
        time.sleep(30)  # lot pokazowy nabiera wysokości, dziennik się synchronizuje
        dismiss(ui)
        self.tab("common.flight")
        time.sleep(3)
        self.shot("kokpit")
        swipe_up()
        self.shot("kokpit-2")
        sh("settings put system user_rotation 1")
        time.sleep(5)
        self.shot("kokpit-poziomo")
        sh("settings put system user_rotation 0")
        time.sleep(4)

        self.tab("common.map")
        time.sleep(10)
        self.shot("mapa")

        self.tab("main_activity.logbook")
        time.sleep(4)
        self.shot("dziennik")
        n = next((n for n in ui.nodes() if "→" in n["text"] and n["y1"] > 300), None)
        if n:
            ui.tap_node(n)
            time.sleep(5)
            self.shot("lot")
            swipe_up()
            self.shot("lot-2")
            swipe_up()
            self.shot("lot-3")
            back()

        self.tab("common.tools")
        time.sleep(2)
        self.shot("narzedzia")

        self.tab("common.settings")
        time.sleep(2)
        self.shot("ustawienia")
        coffee = ui.s("about_section.buy_me_a_coffee")
        for _ in range(8):
            if ui.find(coffee):
                break
            swipe_up()
        self.shot("o-programie")
        self.crashlog("koniec")


def main():
    apk, data, out = sys.argv[1], sys.argv[2], sys.argv[3]
    langs = (sys.argv[4] if len(sys.argv) > 4 else "pl,en").split(",")
    adb("wait-for-device")
    log(adb("install", "-r", "-g", apk, timeout=240).strip())
    for perm in ("android.permission.POST_NOTIFICATIONS", "android.permission.ACCESS_FINE_LOCATION", "android.permission.ACCESS_COARSE_LOCATION"):
        sh(f"pm grant {PKG} {perm}")
    adb("emu", "geo", "fix", "20.967", "52.166")  # telefon przy EPWA – „W pobliżu” na ekranie lotnisk
    for lang in langs:
        try:
            Run(lang, out, data).go()
        except Exception as e:  # jeden język nie psuje drugiego
            log(f"! przebieg {lang} przerwany: {e}")


if __name__ == "__main__":
    main()
