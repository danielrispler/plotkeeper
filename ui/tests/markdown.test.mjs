// Safe Markdown rendering (#525, decision 21), run on the `tsc` output. Run: `npm test` in apps/studio.
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
  assert.match(html, /<a href="https:\/\/example.com\/a\?b=1&amp;c=2">docs<\/a>/);
  assert.match(html, /<a href="http:\/\/example.org">http:\/\/example.org<\/a>/);
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

test("a file link renders as a link the editor opens, an http(s) link as one the browser opens (#557)", () => {
  const html = markdown("[keys](apps/studio/ui/keys.ts#L12) [abs](file:///Users/d/a.rs) [web](https://e.com) [no](javascript:x())");
  assert.match(html, /<a href="apps\/studio\/ui\/keys.ts#L12" data-file>keys<\/a>/);
  assert.match(html, /<a href="file:\/\/\/Users\/d\/a.rs" data-file>abs<\/a>/);
  assert.match(html, /<a href="https:\/\/e.com">web<\/a>/);
  assert.doesNotMatch(html, /<a[^>]*>no/);
});

test("inline code that is a whole file path links to the editor, keeping its code look (#581)", () => {
  const html = markdown("`src/foo.rs:42` `src/foo.rs` `foo.rs:42` `apps/a.ts#L5`");
  for (const p of ["src/foo.rs:42", "src/foo.rs", "foo.rs:42", "apps/a.ts#L5"])
    assert.ok(html.includes(`<a href="${p}" data-file><code>${p}</code></a>`), p);
});

test("inline code that only looks like a path, and fenced code, stay plain code (#581)", () => {
  const spans = ["origin/main", "std::fs", "@types/node", "apps/studio", "see src/foo.rs:42 here", "https://e.com/a.rs", "localhost:8080", "1/2"];
  const html = markdown(spans.map((s) => `\`${s}\``).join(" ") + "\n\n```\nsrc/foo.rs:42\n```");
  assert.doesNotMatch(html, /<a\b/);
  assert.match(html, /<pre><code>src\/foo.rs:42\n<\/code><\/pre>/);
});

test("inline code stays escaped, linked or not (#581)", () => {
  const html = markdown('`a<b "c" & d` `src/a&b.rs`');
  assert.match(html, /<code>a&lt;b &quot;c&quot; &amp; d<\/code>/);
  assert.doesNotMatch(html, /<a\b/);
});

test("inline code shaped like a host, a module or a ref stays plain code (#581)", () => {
  const spans = ["example.com:443", "api.example.com:8080", "git@github.com:22", "Foo.bar:12", "std.io:3", "example.com/index.html", "origin/main:3"];
  for (const s of spans) assert.doesNotMatch(markdown(`\`${s}\``), /<a\b/, s);
});

test("inline code inside a Markdown link is not linked again (#581)", () => {
  const html = markdown("[`src/a.rs`](https://x.com)");
  assert.equal(html.match(/<a\b/g)?.length, 1, html);
  assert.match(html, /<a href="https:\/\/x.com"><code>src\/a.rs<\/code><\/a>/);
});
