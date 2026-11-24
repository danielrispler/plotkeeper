// The contrast pass (#455 of spec #452), run after the script of a fixture with `contrast`: every visible text, input border and
// scroll container is measured against what is actually behind it, with WCAG's relative luminance. The page is drawn once with
// every glyph made transparent (`window.__snapShot`, Chrome's own screenshot), so the pixels under a text's box are its backgrounds
// as composited, translucent layers and the blurred fog mist included; the text's colour, at its own and its ancestors' opacity, is
// laid over each pixel. Text fails below 4.5:1, large text (24 px, or 18.66 px bold) and input borders below 3:1, a scroll
// container whose thumb is below 3:1 against its background or whose scrollbar is not thin. Text on a fogged plate gets a 0.1
// margin, as the blurred mist is sampled. Disabled controls are exempt, as WCAG has them, and so is what a playing PR brief dims on
// purpose (`--brief-dim`) behind its card. A failure logs a `snap:fail contrast:` line.
export const contrastAudit = `
  const words = (text) => (text ?? "").replace(/\\s+/g, " ").trim();
  const named = (node) => node.tagName.toLowerCase() + (node.id ? "#" + node.id : "") + (typeof node.className === "string" && node.className ? "." + node.className.trim().split(/\\s+/).join(".") : "");
  const parse = (colour) => {
    const rgb = /^rgba?\\(([\\d.]+),\\s*([\\d.]+),\\s*([\\d.]+)(?:,\\s*([\\d.]+))?\\)$/.exec(colour);
    if (rgb) return [+rgb[1], +rgb[2], +rgb[3], rgb[4] === undefined ? 1 : +rgb[4]];
    const srgb = /^color\\(srgb ([\\d.]+) ([\\d.]+) ([\\d.]+)(?: \\/ ([\\d.]+))?\\)$/.exec(colour);
    if (srgb) return [srgb[1] * 255, srgb[2] * 255, srgb[3] * 255, srgb[4] === undefined ? 1 : +srgb[4]];
    return null;
  };
  const channel = (c) => ((c /= 255) <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  const luminance = ([r, g, b]) => 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
  const ratio = (a, b) => {
    const [x, y] = [luminance(a), luminance(b)];
    return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
  };
  const over = ([r, g, b], alpha, under) => [r * alpha + under[0] * (1 - alpha), g * alpha + under[1] * (1 - alpha), b * alpha + under[2] * (1 - alpha)];
  const opacityOf = (node) => {
    let alpha = 1;
    for (let at = node; at; at = at.parentElement) alpha *= Number(getComputedStyle(at).opacity);
    return alpha;
  };
  // A box cut to the window and to every ancestor that clips its overflow, so an ellipsised name is measured where it is drawn.
  const clipped = (node, box) => {
    let [left, top, right, bottom] = [box.left, box.top, box.right, box.bottom];
    for (let at = node; at && at !== document.documentElement; at = at.parentElement) {
      const style = getComputedStyle(at);
      if (style.overflowX === "visible" && style.overflowY === "visible") continue;
      const edge = at.getBoundingClientRect();
      [left, top, right, bottom] = [Math.max(left, edge.left), Math.max(top, edge.top), Math.min(right, edge.right), Math.min(bottom, edge.bottom)];
    }
    [left, top, right, bottom] = [Math.max(left, 0), Math.max(top, 0), Math.min(right, innerWidth), Math.min(bottom, innerHeight)];
    return right - left >= 2 && bottom - top >= 2 ? { left, top, right, bottom } : null;
  };
  // Drawn where it is: the topmost element at a point is the node, inside it or its ancestor. \`shown\` keeps the box's 4 px columns
  // that are, at its middle height, so a text half under an overlay is measured on its own half only; null when none is.
  const onTop = (node, x, y) => {
    const hit = document.elementFromPoint(x, y);
    return !!hit && (hit === node || node.contains(hit) || hit.contains(node));
  };
  const shown = (node, box) => {
    const columns = new Set();
    for (let column = 0; box.left + column * 4 < box.right; column++)
      if (onTop(node, Math.min(box.left + column * 4 + 2, box.right - 1), (box.top + box.bottom) / 2)) columns.add(column);
    return columns.size ? { ...box, columns } : null;
  };
  const exempt = (node) =>
    !!node.closest("script, style, noscript, svg, .xterm, :disabled, [aria-disabled=true]") ||
    !!node.closest("body.brief-open .map-panel, body.brief-open .map-side, body.brief-open #map-tray, .map-world.briefing .plate:not(.lit), .map-world.briefing .actor:not(.lit)") ||
    !node.checkVisibility({ visibilityProperty: true });

  // The page settled: Chrome's virtual time stalls animations, so a panel's fade-in is ended, as it would be by now; endless ones, such
  // as the mist's drift, stay where they are.
  for (const animation of document.getAnimations()) if (animation.effect?.getComputedTiming().iterations !== Infinity) animation.finish();

  // What to measure, read before the glyphs are hidden.
  const texts = [];
  const add = (node, colour, box, label) => {
    const ink = parse(colour);
    if (!ink) return log("snap:fail contrast: an unreadable colour " + colour + " on " + named(node));
    const alpha = ink[3] * opacityOf(node);
    if (alpha < 0.05) return;
    const style = getComputedStyle(node);
    const size = parseFloat(style.fontSize);
    const large = size >= 24 || (size >= 18.66 && Number(style.fontWeight) >= 700);
    const fogged = !!node.closest(".plate.fog-thin, .plate.fog-thick");
    texts.push({ node, ink, alpha, box, label, need: (large ? 3 : 4.5) - (fogged ? 0.1 : 0) });
  };
  // A rect of a node as cut by its clipping ancestors, when any of it is left and shown.
  const drawn = (node, rect) => {
    const cut = clipped(node, rect);
    return cut && shown(node, cut);
  };
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  for (let text = walker.nextNode(); text; text = walker.nextNode()) {
    const node = text.parentElement;
    if (!node || !words(text.data) || exempt(node)) continue;
    const range = document.createRange();
    range.selectNodeContents(text);
    const colour = getComputedStyle(node).color;
    const label = words(text.data).slice(0, 40);
    for (const rect of range.getClientRects()) {
      const box = drawn(node, rect);
      if (box) add(node, colour, box, label);
    }
  }
  const fields = [...document.querySelectorAll("input, textarea, select")].filter((field) => !exempt(field) && !["checkbox", "radio", "range", "hidden", "color", "file"].includes(field.type));
  const borders = [];
  for (const field of fields) {
    const rect = field.getBoundingClientRect();
    const box = drawn(field, rect);
    if (!box) continue;
    const style = getComputedStyle(field);
    const inner = { columns: null, left: box.left + parseFloat(style.borderLeftWidth) + 2, top: box.top + parseFloat(style.borderTopWidth) + 2, right: box.right - parseFloat(style.borderRightWidth) - 2, bottom: box.bottom - parseFloat(style.borderBottomWidth) - 2 };
    if (parseFloat(style.borderTopWidth) > 0) borders.push({ field, border: parse(style.borderTopColor), at: inner });
    if (field.tagName === "SELECT") continue;
    if (field.value) add(field, style.color, inner, words(field.value).slice(0, 40));
    else if (field.placeholder) add(field, getComputedStyle(field, "::placeholder").color, inner, "placeholder " + words(field.placeholder).slice(0, 40));
  }

  // The page with its glyphs transparent, as pixels.
  const ink = document.createElement("style");
  ink.textContent = "*, *::before, *::after, *::placeholder, *::marker { color: transparent !important; -webkit-text-fill-color: transparent !important; text-shadow: none !important; text-decoration-color: transparent !important; caret-color: transparent !important; transition: none !important; }";
  document.head.append(ink);
  const shot = await window.__snapShot();
  // The glyphs come back at once, not by a transition from transparent, which Chrome's own screenshot could catch.
  const still = document.createElement("style");
  still.textContent = "*, *::before, *::after { transition: none !important; }";
  document.head.append(still);
  ink.remove();
  document.body.getBoundingClientRect();
  still.remove();
  const scale = shot.width / innerWidth;
  const pixel = (x, y) => {
    const i = (Math.min(shot.height - 1, Math.round(y * scale)) * shot.width + Math.min(shot.width - 1, Math.round(x * scale))) * 4;
    return [shot.data[i], shot.data[i + 1], shot.data[i + 2]];
  };
  // The lowest ratio of a colour laid over each pixel of a box, a pixel in from its edges, in its shown columns.
  const lowest = (colour, alpha, box) => {
    let low = Infinity;
    for (let y = Math.ceil((box.top + 1) * scale); y < (box.bottom - 1) * scale; y++)
      for (let x = Math.ceil((box.left + 1) * scale); x < (box.right - 1) * scale; x++) {
        if (box.columns && !box.columns.has(Math.floor((x / scale - box.left) / 4))) continue;
        const under = pixel(x / scale, y / scale);
        low = Math.min(low, ratio(over(colour, alpha, under), under));
      }
    return low;
  };

  let worst = null;
  for (const text of texts) {
    const low = lowest(text.ink, text.alpha, text.box);
    if (low === Infinity) continue;
    if (!worst || low - text.need < worst.low - worst.need) worst = { ...text, low };
    if (low < text.need) log("snap:fail contrast: " + low.toFixed(2) + " < " + text.need + " on " + named(text.node) + " " + JSON.stringify(text.label));
  }
  for (const { field, border, at } of borders) {
    if (!border) {
      log("snap:fail contrast: an unreadable border colour on " + named(field));
      continue;
    }
    const under = pixel((at.left + at.right) / 2, (at.top + at.bottom) / 2);
    const low = ratio(over(border, border[3], under), under);
    if (low < 3) log("snap:fail contrast: the border " + low.toFixed(2) + " < 3 on " + named(field));
  }
  // Every scroll container: a thin scrollbar, and a thumb at 3:1 against the background at its inner edge.
  for (const node of document.querySelectorAll("*")) {
    const style = getComputedStyle(node);
    if (![style.overflowX, style.overflowY].some((flow) => flow === "auto" || flow === "scroll")) continue;
    if (node === document.documentElement || node === document.body || node.closest(".xterm")) continue;
    if (style.scrollbarWidth !== "thin") log("snap:fail contrast: the scrollbar is " + style.scrollbarWidth + ", not thin, on " + named(node));
    const box = node.getClientRects().length && clipped(node, node.getBoundingClientRect());
    if (!box || style.visibility !== "visible") continue;
    const thumb = parse(style.scrollbarColor.split(/\\s(?=rgb|color)/)[0]);
    if (!thumb) {
      log("snap:fail contrast: no scrollbar colour (" + style.scrollbarColor + ") on " + named(node));
      continue;
    }
    const under = pixel(box.left + 1, (box.top + box.bottom) / 2);
    const low = ratio(over(thumb, thumb[3], under), under);
    if (low < 3) log("snap:fail contrast: the scrollbar thumb " + low.toFixed(2) + " < 3 on " + named(node));
  }
  log("contrast: " + texts.length + " texts, " + borders.length + " borders" + (worst ? "; the closest " + worst.low.toFixed(2) + " (needs " + worst.need + ") on " + named(worst.node) + " " + JSON.stringify(worst.label) : ""));
`;
