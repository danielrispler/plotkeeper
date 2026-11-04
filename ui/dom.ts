// The window's small parts: the shell's calls, and the elements every region builds.

import type { Step } from "./types.js";

declare global {
  interface Window {
    __TAURI__: {
      core: { invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> };
      event: { listen<T>(event: string, handler: (event: { payload: T }) => void): Promise<() => void> };
    };
  }
}

export const invoke = <T>(command: string, args?: Record<string, unknown>) =>
  window.__TAURI__.core.invoke<T>(command, args);
export const listen = <T>(event: string, handler: (payload: T) => void) =>
  window.__TAURI__.event.listen<T>(event, (e) => handler(e.payload));

export const element = (id: string) => document.getElementById(id)!;

export function text(tag: string, content: string, className = ""): HTMLElement {
  const node = document.createElement(tag);
  node.className = className;
  node.textContent = content;
  return node;
}

export const stepText = (step: Step | null) => (step ? `${step.tool} ${step.detail ?? ""}` : "No step yet");

// The time from `from` to `to`, such as "3m".
export function age(from: number, to = Date.now()): string {
  const minutes = Math.floor((to - from) / 60_000);
  return minutes < 60 ? `${minutes}m` : `${Math.floor(minutes / 60)}h`;
}

// A button's type, pressed state and click.
export function pressable<T extends HTMLElement>(button: T, pressed: boolean, click: () => void): T {
  button.setAttribute("type", "button");
  button.setAttribute("aria-pressed", String(pressed));
  button.addEventListener("click", click);
  return button;
}

// Code, left to right, in its own isolate.
export function code(text: string): HTMLElement {
  const node = document.createElement("code");
  node.dir = "ltr";
  node.textContent = text;
  return node;
}

// The writer's text, whose code is in backticks. It sets its own direction, so a card stored in Hebrew
// still reads right to left; each code span is a left-to-right isolate, so `a <= b` is not mirrored.
export function prose(tag: string, text: string): HTMLElement {
  const node = document.createElement(tag);
  node.dir = "auto";
  node.append(...text.split("`").map((part, i) => (i % 2 ? code(part) : part)));
  return node;
}
