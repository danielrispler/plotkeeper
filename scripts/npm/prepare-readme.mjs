// Prepares `npm/README.md` and `npm/LICENSE` from the repository root before `npm pack ./npm` runs.
// Rewrites relative markdown links and images to absolute GitHub URLs so nothing breaks on npmjs.com.
import { copyFileSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = "danielrispler/plotkeeper";
const REF = "main";

const IMAGE_RE = /!\[([^\]]*)\]\(([^)\s]+)([^)]*)\)/g;
const LINK_RE = /(?<!!)\[((?:!\[[^\]]*\]\([^)]*\)|[^\]])*)\]\(([^)\s]+)([^)]*)\)/g;

const isRelativeTarget = (target) => !/^([a-z]+:|#|\/\/)/i.test(target);
const normalizeRelativePath = (target) => target.replace(/^(?:\.\/|\/)+/, "");

// Splits a line into alternating [prose, code, prose, ...] segments, matching CommonMark multi-backtick code spans.
const splitInlineCode = (line) => {
  const parts = [];
  let cursor = 0;
  let proseStart = 0;
  while (cursor < line.length) {
    if (line[cursor] !== "`") {
      cursor += 1;
      continue;
    }
    let ticks = 0;
    while (line[cursor + ticks] === "`") ticks += 1;
    let search = cursor + ticks;
    let close = -1;
    while (search < line.length) {
      if (line[search] !== "`") {
        search += 1;
        continue;
      }
      let run = 0;
      while (line[search + run] === "`") run += 1;
      if (run === ticks) {
        close = search + run;
        break;
      }
      search += run;
    }
    if (close !== -1) {
      parts.push(line.slice(proseStart, cursor), line.slice(cursor, close));
      cursor = close;
      proseStart = close;
    } else {
      cursor += ticks;
    }
  }
  parts.push(line.slice(proseStart));
  return parts;
};

// Transforms every non-code prose segment outside fenced code blocks with `onProse`.
const mapProse = (markdown, onProse) => {
  let fence = null;
  return markdown
    .split("\n")
    .map((line) => {
      if (fence) {
        const close = /^\s*(`{3,}|~{3,})\s*$/.exec(line);
        if (close && close[1][0] === fence.marker && close[1].length >= fence.length) fence = null;
        return line;
      }
      const open = /^\s*(`{3,}|~{3,})(.*)$/.exec(line);
      if (open && (open[1][0] === "~" || !open[2].includes("`"))) {
        fence = { marker: open[1][0], length: open[1].length };
        return line;
      }
      return splitInlineCode(line)
        .map((part, index) => (index % 2 === 1 ? part : onProse(part)))
        .join("");
    })
    .join("\n");
};

export function findRelativeTargets(markdown) {
  const targets = [];
  mapProse(markdown, (segment) => {
    for (const match of segment.matchAll(IMAGE_RE)) {
      if (isRelativeTarget(match[2])) targets.push(match[2]);
    }
    for (const match of segment.matchAll(LINK_RE)) {
      if (isRelativeTarget(match[2])) targets.push(match[2]);
    }
    return segment;
  });
  return targets;
}

export function rewriteReadmeForNpm(markdown, { repo = REPO, ref = REF } = {}) {
  const rawBase = `https://raw.githubusercontent.com/${repo}/${ref}/`;
  const blobBase = `https://github.com/${repo}/blob/${ref}/`;
  return mapProse(markdown, (segment) => {
    const withImages = segment.replace(IMAGE_RE, (match, alt, target, rest) =>
      isRelativeTarget(target) ? `![${alt}](${rawBase}${normalizeRelativePath(target)}${rest})` : match,
    );
    return withImages.replace(LINK_RE, (match, label, target, rest) =>
      isRelativeTarget(target) ? `[${label}](${blobBase}${normalizeRelativePath(target)}${rest})` : match,
    );
  });
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv[2] === "--check") {
    const file = process.argv[3];
    if (!file) {
      console.error("usage: node scripts/npm/prepare-readme.mjs --check <file>");
      process.exit(2);
    }
    const leftover = findRelativeTargets(readFileSync(file, "utf8"));
    if (leftover.length > 0) {
      console.error(`prepare-readme: relative links remain in ${file}: ${leftover.join(", ")}`);
      process.exit(1);
    }
  } else {
    const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
    const source = readFileSync(resolve(root, "README.md"), "utf8");
    const rewritten = rewriteReadmeForNpm(source);
    const leftover = findRelativeTargets(rewritten);
    if (leftover.length > 0) {
      console.error(`prepare-readme: relative links remain in npm/README.md: ${leftover.join(", ")}`);
      process.exit(1);
    }
    writeFileSync(resolve(root, "npm/README.md"), rewritten, "utf8");
    copyFileSync(resolve(root, "LICENSE"), resolve(root, "npm/LICENSE"));
  }
}
