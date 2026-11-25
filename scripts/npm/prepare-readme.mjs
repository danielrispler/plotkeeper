// Prepares `npm/README.md` and `npm/LICENSE` from the repository root before `npm pack ./npm` runs.
// Rewrites relative markdown links and images to absolute GitHub URLs so nothing breaks on npmjs.com.
import { copyFileSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = "danielrispler/plotkeeper";
const REF = "main";

const isRelativeTarget = (target) => !/^([a-z]+:|#|\/\/)/i.test(target);
const stripLeadingDotSlash = (target) => target.replace(/^\.\//, "");

const rewriteProse = (segment, { repo, ref }) => {
  const rawBase = `https://raw.githubusercontent.com/${repo}/${ref}/`;
  const blobBase = `https://github.com/${repo}/blob/${ref}/`;
  const withImages = segment.replace(/!\[([^\]]*)\]\(([^)\s]+)([^)]*)\)/g, (match, alt, target, rest) =>
    isRelativeTarget(target) ? `![${alt}](${rawBase}${stripLeadingDotSlash(target)}${rest})` : match,
  );
  return withImages.replace(
    /(?<!!)\[((?:!\[[^\]]*\]\([^)]*\)|[^\]])*)\]\(([^)\s]+)([^)]*)\)/g,
    (match, label, target, rest) =>
      isRelativeTarget(target) ? `[${label}](${blobBase}${stripLeadingDotSlash(target)}${rest})` : match,
  );
};

const rewriteLine = (line, options) =>
  line
    .split(/(`+[^`]*`+)/g)
    .map((part, index) => (index % 2 === 1 ? part : rewriteProse(part, options)))
    .join("");

export function rewriteReadmeForNpm(markdown, { repo = REPO, ref = REF } = {}) {
  let fence = null;
  return markdown
    .split("\n")
    .map((line) => {
      const open = /^\s*(`{3,}|~{3,})/.exec(line);
      if (open) {
        const marker = open[1][0];
        if (!fence) fence = { marker, length: open[1].length };
        else if (fence.marker === marker && open[1].length >= fence.length) fence = null;
        return line;
      }
      return fence ? line : rewriteLine(line, { repo, ref });
    })
    .join("\n");
}

const findRelativeTargets = (markdown) => {
  const targets = [];
  let fence = null;
  for (const line of markdown.split("\n")) {
    const open = /^\s*(`{3,}|~{3,})/.exec(line);
    if (open) {
      const marker = open[1][0];
      if (!fence) fence = { marker, length: open[1].length };
      else if (fence.marker === marker && open[1].length >= fence.length) fence = null;
      continue;
    }
    if (fence) continue;
    const parts = line.split(/(`+[^`]*`+)/g);
    for (let i = 0; i < parts.length; i += 2) {
      for (const match of parts[i].matchAll(/!?\[[^\]]*\]\(([^)\s]+)\)/g)) {
        if (isRelativeTarget(match[1])) targets.push(match[1]);
      }
    }
  }
  return targets;
};

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
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
