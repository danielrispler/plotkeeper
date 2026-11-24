// As the real one: the ES module re-exports the CommonJS object.
import playwright from "./index.js";
export const chromium = playwright.chromium;
export default playwright;
