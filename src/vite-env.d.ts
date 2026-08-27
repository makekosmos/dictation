/// <reference types="vite/client" />

import type { DictationBridge } from "./lib/dictationApi";

declare module "*.vue" {
  import type { DefineComponent } from "vue";
  const component: DefineComponent<Record<string, unknown>, Record<string, unknown>, unknown>;
  export default component;
}

declare global {
  interface Window {
    kosmosApp?: {
      ark: DictationBridge;
    };
  }
}

export {};
