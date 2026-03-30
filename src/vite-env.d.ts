/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_LOCALE?: "zh" | "en";
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
