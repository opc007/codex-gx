import { useMemo } from "react";
import { useLocale, useDict as _useDict, setLocale as _setLocale } from "../stores/locale";
import type { Dict, Locale } from "./types";
import { DEFAULT_LOCALE, SUPPORTED_LOCALES, LOCALE_LABELS } from "./types";
import { DICTS } from "./dicts";
export { DICTS } from "./dicts";

export { SUPPORTED_LOCALES, DEFAULT_LOCALE, LOCALE_LABELS };
export type { Locale, Dict } from "./types";

export function useTranslation(): Dict {
  return _useDict();
}

export function useLocaleSwitcher() {
  const locale = useLocale();
  return { locale, setLocale: _setLocale };
}

export function setLocale(l: Locale) {
  _setLocale(l);
}

/** 非 hook：用 useMemo 缓存（其他地方需要时） */
export function getDictCached(locale: Locale): Dict {
  return useMemo(() => DICTS[locale] ?? DICTS[DEFAULT_LOCALE], [locale]);
}
