"""Test mostu z atrapą X-Plane (UDP) i klientem protokołu Link. Uruchomienie: python -m unittest."""

import asyncio
import json
import pathlib
import socket
import struct
import tempfile
import unittest

import websockets

import xplane_link as xl


class FakeXPlane(asyncio.DatagramProtocol):
    def __init__(self):
        self.values = {}
        self.subs = {}
        self.client = None
        self.writes = []
        self.commands = []

    def connection_made(self, transport):
        self.transport = transport

    def datagram_received(self, data, addr):
        self.client = addr
        head = data[:5]
        if head == b"RREF\0":
            freq, idx, path = struct.unpack("<ii400s", data[5:])
            self.subs[idx] = path.split(b"\0")[0].decode()
        elif head == b"DREF\0":
            value, path = struct.unpack("<f500s", data[5:])
            self.writes.append((path.split(b"\0")[0].decode(), value))
        elif head == b"CMND\0":
            self.commands.append(data[5:].rstrip(b"\0").decode())

    def publish(self):
        if not self.client:
            return
        items = list(self.subs.items())
        for i in range(0, len(items), 100):
            body = b"".join(struct.pack("<if", idx, self.values.get(path, 0.0)) for idx, path in items[i:i + 100])
            self.transport.sendto(b"RREF," + body, self.client)


class BridgeTest(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        loop = asyncio.get_running_loop()
        self.sim = FakeXPlane()
        transport, _ = await loop.create_datagram_endpoint(lambda: self.sim, local_addr=("127.0.0.1", 0))
        self.addCleanup(transport.close)
        port = transport.get_extra_info("sockname")[1]
        self.discovery = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.discovery.bind(("127.0.0.1", 0))
        self.discovery.settimeout(3)
        self.addCleanup(self.discovery.close)
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.link = xl.XPlaneLink("127.0.0.1", port, 0, self.discovery.getsockname()[1],
                                  pathlib.Path(self.tmp.name) / "tokens.json", ["127.0.0.1"])
        self.sim.values.update({
            xl.D["lat"]: 52.25, xl.D["lon"]: 21.0, xl.D["elev"]: 1000.0, xl.D["ias"]: 120.0, xl.D["vs"]: -600.0,
            xl.D["com1"]: 118500.0, xl.D["nav1"]: 11030.0, xl.D["xpdr"]: 7000.0, xl.D["xpdr_mode"]: 3.0,
            xl.D["engines"]: 2.0, xl.D["flap_detents"]: 4.0, xl.D["flaps"]: 0.5, xl.D["en_type"]: 5.0,
        })
        for i, ch in enumerate(b"Airbus A320"):
            self.sim.values[f"{xl.TITLE_REF}[{i}]"] = float(ch)
        await self.link.start()
        self.pump = asyncio.create_task(self.publish_loop())
        self.ws = None

    async def asyncTearDown(self):
        self.pump.cancel()
        if self.ws:
            await self.ws.close()
        await self.link.stop()

    async def publish_loop(self):
        while True:
            self.sim.publish()
            await asyncio.sleep(0.05)

    async def open(self):
        self.ws = await websockets.connect(f"ws://127.0.0.1:{self.link.ws_port}/")
        return self.ws

    async def until(self, ws, kind):
        while True:
            msg = json.loads(await asyncio.wait_for(ws.recv(), 5))
            if msg["type"] == kind:
                return msg

    async def paired(self):
        ws = await self.open()
        await ws.send(json.dumps({"v": 1, "type": "hello", "id": "1", "code": self.link.pairing.code, "client": {"name": "Test"}, "rateHz": 20}))
        return ws, await self.until(ws, "hello_ok")

    async def command(self, ws, name, value=None):
        await ws.send(json.dumps({"v": 1, "type": "command", "id": name, "name": name, "value": value}))
        while True:
            msg = json.loads(await asyncio.wait_for(ws.recv(), 5))
            if msg["type"] in ("ack", "error") and msg.get("id") == name:
                return msg

    async def test_announce_matches_app_discovery(self):
        data = await asyncio.to_thread(self.discovery.recv, 4096)
        msg = json.loads(data)
        self.assertEqual((msg["v"], msg["type"], msg["port"]), (1, "announce", self.link.ws_port))

    async def test_wrong_code_is_rejected_and_state_is_withheld(self):
        ws = await self.open()
        await ws.send(json.dumps({"v": 1, "type": "hello", "id": "1", "code": "x", "client": {"name": "T"}}))
        self.assertEqual((await self.until(ws, "error"))["code"], "pairing_failed")
        await ws.send(json.dumps({"v": 1, "type": "command", "id": "2", "name": "GEAR_UP"}))
        self.assertEqual((await self.until(ws, "error"))["code"], "not_paired")

    async def test_pairing_token_reconnects(self):
        ws, ok = await self.paired()
        await ws.close()
        ws = await self.open()
        await ws.send(json.dumps({"v": 1, "type": "hello", "id": "1", "token": ok["token"], "client": {"name": "T"}}))
        self.assertEqual((await self.until(ws, "hello_ok"))["token"], ok["token"])

    async def test_state_converts_units(self):
        ws, _ = await self.paired()
        # Pierwsze klatki mogą wyprzedzić koniec subskrypcji (nazwa samolotu idzie na końcu)
        for _ in range(60):
            data = (await self.until(ws, "state"))["data"]
            if data["aircraft"]["title"] != "X-Plane":
                break
        self.assertEqual(data["source"], "xplane")
        self.assertEqual(data["aircraft"]["title"], "Airbus A320")
        self.assertEqual(data["aircraft"]["category"], "airliner")
        self.assertAlmostEqual(data["position"]["altMslFt"], 3280.8, 1)
        self.assertEqual(data["radios"]["com1"]["activeMhz"], 118.5)
        self.assertEqual(data["radios"]["nav1"]["activeMhz"], 110.3)
        self.assertEqual(data["radios"]["transponder"], {"code": "7000", "mode": "alt", "ident": False})
        self.assertEqual((data["controls"]["flapsIndex"], data["controls"]["flapsPositions"]), (2, 5))
        self.assertEqual(len(data["engines"]), 2)

    async def test_commands_reach_xplane(self):
        ws, _ = await self.paired()
        await asyncio.sleep(0.3)
        self.assertEqual((await self.command(ws, "GEAR_UP"))["type"], "ack")
        await self.command(ws, "COM_STBY_RADIO_SET_HZ", 121_500_000)
        await self.command(ws, "NAV1_STBY_SET_HZ", 110_300_000)
        await self.command(ws, "XPNDR_MODE", 4)
        await asyncio.sleep(0.1)
        self.assertIn("sim/flight_controls/landing_gear_up", self.sim.commands)
        self.assertIn((xl.D["com1s"], 121500.0), self.sim.writes)
        self.assertIn((xl.D["nav1s"], 11030.0), self.sim.writes)
        self.assertIn((xl.D["xpdr_mode"], 3.0), self.sim.writes)

    async def test_command_validation(self):
        ws, _ = await self.paired()
        await asyncio.sleep(0.3)
        self.assertEqual((await self.command(ws, "RM_RF"))["code"], "not_allowed")
        self.assertEqual((await self.command(ws, "XPNDR_SET", 7800))["code"], "invalid_value")
        self.assertEqual((await self.command(ws, "AP_N1_HOLD"))["code"], "sim_error")

    async def test_axes_clamp_and_release_on_silence(self):
        ws, _ = await self.paired()
        await asyncio.sleep(0.3)
        await ws.send(json.dumps({"v": 1, "type": "axis", "aileron": 1.0, "elevator": 0.1, "rudder": 0}))
        await asyncio.sleep(0.2)
        self.assertIn((xl.OVERRIDE_JOYSTICK, 1.0), self.sim.writes)
        rolls = [v for p, v in self.sim.writes if p == xl.YOKE_ROLL]
        self.assertAlmostEqual(max(rolls), 0.8, 5)
        await asyncio.sleep(0.8)
        self.assertEqual(self.sim.writes[-1], (xl.OVERRIDE_JOYSTICK, 0.0))

    async def test_touchdown_is_latched(self):
        ws, _ = await self.paired()
        await asyncio.sleep(0.5)
        self.sim.values[xl.D["on_ground"]] = 1.0
        for _ in range(100):
            data = (await self.until(ws, "state"))["data"]
            if "touchdown" in data:
                self.assertEqual((data["touchdown"]["seq"], data["touchdown"]["vsFpm"]), (1, -600))
                return
        self.fail("brak przyziemienia")

    async def test_status_follows_xplane(self):
        ws, _ = await self.paired()
        await asyncio.sleep(0.3)
        self.pump.cancel()
        msg = await asyncio.wait_for(self.until(ws, "status"), 6)
        while msg["source"] != "none":
            msg = await asyncio.wait_for(self.until(ws, "status"), 6)
        self.assertFalse(msg["simConnected"])


if __name__ == "__main__":
    unittest.main()
