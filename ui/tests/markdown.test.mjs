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
  assert.doesNotMatch(html, /<a href="(?!https?:)/);
  assert.doesNotMatch(html, /<[^>]*\son\w+=/i);
  assert.match(html, /&lt;script&gt;alert\(1\)&lt;\/script&gt;/);
  assert.match(html, /&lt;img src=x onerror=&quot;alert\(1\)&quot;&gt;/);
  assert.match(html, /click/);
  assert.match(html, /file/);
  assert.doesNotMatch(html, /<a[^>]*>click/);
});
