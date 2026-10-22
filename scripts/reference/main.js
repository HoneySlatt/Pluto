// Captures the upstream web client served by a fixture server.
// usage: electron scripts/reference <url> <out.png> [waitMs] [js] [width] [height]
// SCALE=1.5 zooms the page by that factor in a window enlarged to match, like a
// fractional display scale; CLICKS stay in page points.
// The optional js runs in the page after waitMs (10s limit); its result is
// printed. CLICKS="x,y;x,y" clicks those viewport points afterwards.
const { app, BrowserWindow } = require("electron");
const fs = require("fs");
const os = require("os");
const path = require("path");

const [url, out, wait = "4000", js = "", w = "1440", h = "900"] = process.argv
  .slice(2)
  .filter((arg) => !arg.startsWith("--"));
const scale = +(process.env.SCALE || 1);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
app.commandLine.appendSwitch("force-device-scale-factor", "1");
app.setPath("userData", path.join(os.tmpdir(), "t3-egui-reference-profile"));
app.whenReady().then(async () => {
  const win = new BrowserWindow({
    width: Math.round(w * scale),
    height: Math.round(h * scale),
    show: false,
    webPreferences: { offscreen: true },
  });
  win.webContents.on("did-finish-load", () => win.webContents.setZoomFactor(scale));
  await win.loadURL(url);
  await sleep(+wait);
  if (js) {
    const value = await Promise.race([
      win.webContents.executeJavaScript(js).catch((e) => `error: ${e}`),
      sleep(10000).then(() => "error: script timed out"),
    ]);
    if (value !== undefined) console.log(typeof value === "string" ? value : JSON.stringify(value, null, 1));
    await sleep(1500);
  }
  // CLICKS points prefixed with "r" (e.g. "r60,150") are right clicks and
  // points prefixed with "m" only move the pointer there (hover); "c" and "s"
  // hold Ctrl or Shift for the click.
  for (const point of (process.env.CLICKS || "").split(";").filter(Boolean)) {
    const button = point.startsWith("r") ? "right" : "left";
    const modifiers = point.startsWith("c") ? ["control"] : point.startsWith("s") ? ["shift"] : [];
    const [x, y] = point.replace(/^[rmcs]/, "").split(",").map((v) => Math.round(v * scale));
    win.webContents.sendInputEvent({ type: "mouseMove", x, y });
    if (point.startsWith("m")) {
      await sleep(800);
      continue;
    }
    win.webContents.sendInputEvent({ type: "mouseDown", x, y, button, clickCount: 1, modifiers });
    win.webContents.sendInputEvent({ type: "mouseUp", x, y, button, clickCount: 1, modifiers });
    await sleep(800);
  }
  // DRAG="x1,y1,x2,y2" presses at the first point and moves to the second
  // without releasing, for a mid-drag capture.
  if (process.env.DRAG) {
    const [x1, y1, x2, y2] = process.env.DRAG.split(",").map((v) => Math.round(v * scale));
    win.webContents.sendInputEvent({ type: "mouseMove", x: x1, y: y1 });
    win.webContents.sendInputEvent({ type: "mouseDown", x: x1, y: y1, button: "left", clickCount: 1 });
    for (let step = 1; step <= 12; step += 1) {
      const x = Math.round(x1 + ((x2 - x1) * step) / 12);
      const y = Math.round(y1 + ((y2 - y1) * step) / 12);
      win.webContents.sendInputEvent({ type: "mouseMove", x, y, button: "left", modifiers: ["leftbuttondown"] });
      await sleep(40);
    }
    await sleep(800);
  }
  // KEYS="Ctrl+K;Escape" sends key chords (modifiers joined with +).
  for (const chord of (process.env.KEYS || "").split(";").filter(Boolean)) {
    const parts = chord.split("+");
    const keyCode = parts.pop();
    const modifiers = parts.map((m) => m.toLowerCase());
    win.webContents.sendInputEvent({ type: "keyDown", keyCode, modifiers });
    win.webContents.sendInputEvent({ type: "char", keyCode, modifiers });
    win.webContents.sendInputEvent({ type: "keyUp", keyCode, modifiers });
    await sleep(800);
  }
  // POSTJS runs after CLICKS and KEYS; its result is printed.
  if (process.env.POSTJS) {
    const value = await Promise.race([
      win.webContents.executeJavaScript(process.env.POSTJS).catch((e) => `error: ${e}`),
      sleep(10000).then(() => "error: script timed out"),
    ]);
    console.log(typeof value === "string" ? value : JSON.stringify(value, null, 1));
  }
  fs.writeFileSync(out, (await win.webContents.capturePage()).toPNG());
  app.quit();
});
