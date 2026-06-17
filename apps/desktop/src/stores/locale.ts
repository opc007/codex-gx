import { useSyncExternalStore } from "react";
import type { Dict, Locale } from "../i18n";
import { DEFAULT_LOCALE, SUPPORTED_LOCALES, DICTS } from "../i18n";

const STORAGE_KEY = "codex_gx_locale";

function detect(): Locale {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved && (SUPPORTED_LOCALES as string[]).includes(saved)) {
      return saved as Locale;
    }
  } catch {
    // ignore
  }
  const nav = typeof navigator !== "undefined" ? navigator.language : "zh";
  return nav.toLowerCase().startsWith("en") ? "en" : DEFAULT_LOCALE;
}

let current: Locale = detect();
const listeners = new Set<() => void>();

function subscribe(cb: () => void) {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

function getSnapshot() {
  return current;
}

export function setLocale(l: Locale) {
  if (l === current) return;
  current = l;
  try {
    localStorage.setItem(STORAGE_KEY, l);
  } catch {
    // ignore
  }
  listeners.forEach((cb) => cb());
}

export function useLocale(): Locale {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

export function useDict(): Dict {
  const l = useLocale();
  return DICTS[l] ?? DICTS[DEFAULT_LOCALE];
}