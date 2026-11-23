// Safe Markdown rendering (danielrispler/Falinks#525, decision 21), run on the `tsc` output. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";

// The window loads marked as a plain script from `vendor/`, so it is a global there too.
globalThis.marked = createRequire(import.meta.url)("marked");
const { markdown } = await import("../../dist/markdown.js");

test("a table, a list, code and an http(s) link render", () => {
  const html = markdown(
    "| a | b |\n|---|---|\n| 1 | 2 |\n\n- one\n- two\n\n`x <= y`\n\n```\nlet a = 1;\n```\n\n[docs](https://example.com/a?b=1&c=2) and <http://example.org>",
  );
  assert.match(html, /<table>[\s\S]*<th>a<\/th>[\s\S]*<td>2<\/td>/);
  assert.match(html, /<ul>\s*<li>one<\/li>\s*<li>two<\/li>/);
  assert.match(html, /<code>x &lt;= y<\/code>/);
  assert.match(html, /<pre><code>let a = 1;\n<\/code><\/pre>/);
  assert.match(html, /<a href="https:\/\/example.com\/a\?b=1&amp;c=2" target="_blank" rel="noopener noreferrer" title="Open https:\/\/example.com\/a\?b=1&amp;c=2 in a new tab">docs<\/a>/);
  assert.match(html, /<a href="http:\/\/example.org" target="_blank" rel="noopener noreferrer" title="Open http:\/\/example.org in a new tab">http:\/\/example.org<\/a>/);
});

test("raw HTML, a javascript: link and an event handler come out as inert text", () => {
  const html = markdown(
    '<script>alert(1)</script>\n\nhi <img src=x onerror="alert(1)"> <b onclick="x()">b</b>\n\n' +
      '[click](javascript:alert(1)) [file](file:///etc/passwd) <javascript:alert(1)> ![p](https://e.com/p.png "t\\" onload=\\"x")\n\n' +
      '[q](https://e.com/"onmouseover="x)',
  );
  assert.doesNotMatch(html, /<(script|img|b)\b/);
  assert.doesNotMatch(html, /<a href="(?!https?:|file:)/);
  assert.doesNotMatch(html, /<[^>]*\son\w+=/i);
  assert.match(html, /&lt;script&gt;alert\(1\)&lt;\/script&gt;/);
  assert.match(html, /&lt;img src=x onerror=&quot;alert\(1\)&quot;&gt;/);
  assert.match(html, /click/);
  assert.match(html, /file/);
  assert.doesNotMatch(html, /<a[^>]*>click/);
});

test("an http(s) link opens in a new tab; a file link, which the page cannot open, is its text (danielrispler/Falinks#557)", () => {
  const html = markdown("[keys](apps/studio/ui/keys.ts#L12) [abs](file:///Users/d/a.rs) [web](https://e.com) [no](javascript:x())");
  assert.match(html, /<a href="https:\/\/e.com" target="_blank" rel="noopener noreferrer" title="Open https:\/\/e.com in a new tab">web<\/a>/);
  assert.doesNotMatch(html, /<a[^>]*>(keys|abs|no)/);
  assert.match(html, /keys abs/);
});

test("inline code stays escaped, linked or not (danielrispler/Falinks#581)", () => {
  const html = markdown('`a<b "c" & d` `src/a&b.rs`');
  assert.match(html, /<code>a&lt;b &quot;c&quot; &amp; d<\/code>/);
  assert.doesNotMatch(html, /<a\b/);
});

test("inline code is never a link, a path or not", () => {
  const spans = ["src/foo.rs:42", "src/foo.rs", "example.com:443", "origin/main:3", "@types/node"];
  for (const s of spans) assert.equal(markdown(`\`${s}\``).trim(), `<p><code>${s}</code></p>`, s);
  assert.match(markdown("```\nsrc/foo.rs:42\n```"), /<pre><code>src\/foo.rs:42\n<\/code><\/pre>/);
});

test("inline code inside a Markdown link stays in the one link (danielrispler/Falinks#581)", () => {
  const html = markdown("[`src/a.rs`](https://x.com)");
  assert.equal(html.match(/<a\b/g)?.length, 1, html);
  assert.match(html, /<a href="https:\/\/x.com" target="_blank" rel="noopener noreferrer" title="Open https:\/\/x.com in a new tab"><code>src\/a.rs<\/code><\/a>/);
});
