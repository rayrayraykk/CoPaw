import type * as ReactNS from "react";

declare global {
  interface QwenPawHost {
    React: typeof ReactNS;
    getApiUrl: (path: string) => string;
    getApiToken: () => string;
    fetch?: (path: string, init?: RequestInit) => Promise<Response>;
  }

  interface QwenPawGlobal {
    host: QwenPawHost;
    registerRoutes?: (
      pluginId: string,
      routes: Array<{
        path: string;
        component: unknown;
        label: string;
        icon?: string;
        priority?: number;
      }>,
    ) => void;
  }

  interface Window {
    QwenPaw: QwenPawGlobal;
  }
}

export {};
