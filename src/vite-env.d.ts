/// <reference types="vite/client" />

declare module "*.vue" {
  import type { DefineComponent } from "vue";
  const component: DefineComponent<Record<string, unknown>, Record<string, unknown>, unknown>;
  export default component;
}

declare global {
  interface Window {
    kosmosApp?: {
      ark: { request<T = unknown>(operation: string, params?: Record<string, unknown>): Promise<T> };
    };
  }
}

export {};
