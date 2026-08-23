/// <reference types="vite/client" />

declare module "*.vue" {
  import type { DefineComponent } from "vue";
  const component: DefineComponent<Record<string, unknown>, Record<string, unknown>, unknown>;
  export default component;
}

declare global {
  interface Window {
    kepler?: {
      ark: { request<T = unknown>(operation: string, params?: Record<string, unknown>): Promise<T> };
      dictation: {
        toggle(): Promise<void>;
        cancel(): Promise<void>;
        pillFinished(): Promise<void>;
        onCommand(cb: (command: { kind: "start" | "stop" | "cancel" }) => void): () => void;
      };
    };
  }
}

export {};
