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

// A button: the one button component (danielrispler/Falinks#566), its type, pressed state and click.
export function pressable<T extends HTMLElement>(button: T, pressed: boolean, click: (event: MouseEvent) => void): T {
  button.classList.add("btn");
  button.setAttribute("type", "button");
  button.setAttribute("aria-pressed", String(pressed));
  button.addEventListener("click", click);
  return button;
}

// Makes `parent`'s children match `next`, keeping each node that stays, so the focused element and
// the text selection survive an update (danielrispler/Falinks#570). A node stays when its tag and `data-key` match, and
// takes the new one's attributes and text. Give a node with a listener a key that pins what the
// listener does, such as its session's id.
export function patch(parent: Node, next: Node[]) {
  const old = [...parent.childNodes];
  next.forEach((node, i) => {
    const was = old[i];
    if (!was) parent.appendChild(node);
    else if (was.nodeName !== node.nodeName || key(was) !== key(node)) parent.replaceChild(node, was);
    else if (was.nodeType !== 1) {
      if (was.nodeValue !== node.nodeValue) was.nodeValue = node.nodeValue;
    } else {
      const [from, to] = [was as Element, node as Element];
      for (const { name, value } of [...to.attributes]) if (from.getAttribute(name) !== value) from.setAttribute(name, value);
      for (const { name } of [...from.attributes]) if (!to.hasAttribute(name)) from.removeAttribute(name);
      patch(from, [...to.childNodes]);
    }
  });
  for (const extra of old.slice(next.length)) parent.removeChild(extra);
}
const key = (node: Node) => (node.nodeType === 1 ? (node as Element).getAttribute("data-key") : null);

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
