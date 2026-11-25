// A fake playwright-core: a browser keeps the args it was launched with, and a context answers a request through the routes
// registered on it, "continued" when none aborts it.
class Context {
  constructor() {
    this.routes = [];
  }
  route(_pattern, handler) {
    this.routes.push(handler);
    return Promise.resolve();
  }
  async request(url) {
    let answer = "continued";
    for (const handler of this.routes) {
      let done = false;
      await handler({
        request: () => ({ url: () => url }),
        continue: () => {
          done = true;
          return Promise.resolve();
        },
        abort: () => {
          answer = "aborted";
          done = true;
          return Promise.resolve();
        },
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
  newContext() {
    return Promise.resolve(new Context());
  }
  async close() {}
}

module.exports = {
  chromium: {
    launch(options) {
      return Promise.resolve(new Browser(options));
    },
  },
};
