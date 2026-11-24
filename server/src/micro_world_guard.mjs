// The network guard of a micro-world's host check (spec danielrispler/plotkeeper#378, ticket #399). plotkeeper runs dsh-visual's
// verify.mjs as `node --import <this file> <verify.mjs> <page>`, so this runs first and patches the playwright-core that verify.mjs
// imports: every context of the Chrome it launches aborts each request that is not a local file, and Chrome itself is sent through
// a proxy that leads nowhere, which catches what a route does not see, such as a WebSocket. Model-written script then never reaches
// the network; an aborted request fails verify.mjs's "no page or request errors". When playwright-core is not where verify.mjs
// finds it, this throws, so the check fails rather than run unguarded.

import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

// The playwright-core verify.mjs imports: its ES module re-exports this CommonJS object, so patching it reaches verify.mjs.
const { chromium } = createRequire(pathToFileURL(process.argv[1]).href)("playwright-core");
const local = (url) => /^(file|data|blob|about):/i.test(url);
const launch = chromium.launch.bind(chromium);

chromium.launch = async (options = {}) => {
  const args = [...(options.args ?? []), "--proxy-server=http://127.0.0.1:9", "--proxy-bypass-list=<-loopback>"];
  const browser = await launch({ ...options, args });
  const newContext = browser.newContext.bind(browser);
  browser.newContext = async (...given) => {
    const context = await newContext(...given);
    await context.route("**/*", (route) => (local(route.request().url()) ? route.continue() : route.abort("blockedbyclient")));
    return context;
  };
  return browser;
};
