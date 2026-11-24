// A fake playwright-core: a browser keeps the args it was launched with, and a context answers a request through the routes
// registered on it, "continued" when none aborts it.
class Context {
  constructor() {
    this.routes = [];
  }
  async route(pattern, handler) {
    this.routes.push(handler);
  }
  async request(url) {
    let answer = "continued";
    for (const handler of this.routes) {
      let done = false;
      await handler({
        request: () => ({ url: () => url }),
        continue: async () => { done = true; },
        abort: async () => { answer = "aborted"; done = true; },
      });
      if (done) break;
    }
    return answer;
  }
}

class Browser {
  constructor(options) {
    this.args = options?.args ?? [];
  }
  async newContext() {
    return new Context();
  }
  async close() {}
}

module.exports = {
  chromium: {
    async launch(options) {
      return new Browser(options);
    },
  },
};
