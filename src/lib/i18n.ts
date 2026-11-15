import enUS from "./locales/en-US";
import ptBR from "./locales/pt-BR";
import esES from "./locales/es-ES";

export type Dict = Record<string, string>;

const dicts: Record<string, Dict> = {
  "en-US": enUS,
  "pt-BR": ptBR,
  "es-ES": esES,
};

export const LANGUAGES = [
  { id: "default", label: "Default" },
  { id: "en-US", label: "English (United States)" },
  { id: "pt-BR", label: "Português (Brasil)" },
  { id: "es-ES", label: "Español (España)" },
];

export function makeT(language: string) {
  const dict = dicts[language] ?? dicts["en-US"];
  const fallback = dicts["en-US"];
  return (key: string): string => dict[key] ?? fallback[key] ?? key;
}

export type TFn = ReturnType<typeof makeT>;
