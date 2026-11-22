// The pointer reaches every control (#220), so its hover, a disabled one's reason among them, can show. Every
// CSS rule that turns pointer events off is listed here with why it hides no hover; a new one fails until it is.
// Each snap page checks the controls themselves (scripts/snap/hover.mjs). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";

const reasons = {
  ".map-links": "the Map view's lines run under the plates and are no control; a plate and a link's mark take their own pointer",
  ".plate-fog": "the mist behind a plate holds no control, and the plate over it takes the pointer",
  ".toasts": "the stack's empty space lets clicks through to the terminal; each toast turns pointer events back on",
};

test("every CSS rule that turns pointer events off is listed with why it hides no hover", () => {
  const css = fs.readFileSync(new URL("../style.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const off = [...css.matchAll(/([^{}]+)\{([^}]*)\}/g)]
    .filter(([, , body]) => /pointer-events:\s*none/.test(body))
    .flatMap(([, selector]) => selector.split(",").map((s) => s.trim().replace(/\s+/g, " ")));
  assert.deepEqual(off.filter((selector) => !(selector in reasons)), []);
});
