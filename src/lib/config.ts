// 設定ファイル ~/.config/flashcap/config.json の読み書き (実体は src-tauri/src/config.rs)

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type ConfigValues = Record<string, unknown>;

export function loadConfig(): Promise<ConfigValues> {
  return invoke<ConfigValues>("config_get_all");
}

export function setConfig(key: string, value: unknown): Promise<void> {
  return invoke("config_set", { key, value });
}

/** どのウインドウが書いても、全ウインドウに書いたキーが届く */
export function onConfigChange(callback: (key: string) => void): Promise<UnlistenFn> {
  return listen<string>("config-changed", (e) => callback(e.payload));
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

const lastWritten = new Map<string, string>();
const pending = new Map<string, { timer: ReturnType<typeof setTimeout>; value: unknown }>();

/** 読み込んだ値を「書き込み済み」として覚える。読んだ直後の同じ値を書き戻さないため */
export function rememberConfig(key: string, value: unknown) {
  lastWritten.set(key, JSON.stringify(value));
}

/**
 * まとめて書く。カラーピッカーのドラッグやテキスト入力は 1 操作で何十回も変わるので、
 * そのたびにファイルを書き直さない。直前に書いた値と同じなら書かない。
 */
export function setConfigDebounced(key: string, value: unknown, delayMs = 300) {
  const previous = pending.get(key);
  if (previous) clearTimeout(previous.timer);
  pending.delete(key);
  if (lastWritten.get(key) === JSON.stringify(value)) return;
  const timer = setTimeout(() => {
    pending.delete(key);
    write(key, value);
  }, delayMs);
  pending.set(key, { timer, value });
}

/** 待っている書き込みをすぐに出す (ウインドウを閉じる直前など) */
export function flushConfig() {
  for (const [key, { timer, value }] of pending) {
    clearTimeout(timer);
    write(key, value);
  }
  pending.clear();
}

function write(key: string, value: unknown) {
  const json = JSON.stringify(value);
  setConfig(key, value)
    .then(() => lastWritten.set(key, json))
    .catch((e) => console.error(`Failed to save "${key}" to the config file:`, e));
}
