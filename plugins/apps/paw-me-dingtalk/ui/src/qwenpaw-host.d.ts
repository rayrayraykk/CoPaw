import type * as ReactNS from "react";

declare global {
  interface PawApi {
    get<T>(
      path: string,
      options?: { query?: Record<string, unknown> },
    ): Promise<T>;
    post<T>(
      path: string,
      body?: unknown,
      options?: { query?: Record<string, unknown> },
    ): Promise<T>;
    put<T>(
      path: string,
      body?: unknown,
      options?: { query?: Record<string, unknown> },
    ): Promise<T>;
    patch<T>(
      path: string,
      body?: unknown,
      options?: { query?: Record<string, unknown> },
    ): Promise<T>;
    delete<T>(
      path: string,
      options?: { query?: Record<string, unknown> },
    ): Promise<T>;
  }

  interface PawSdk {
    api: PawApi;
    host: {
      getSelectedAgentId(): string;
      toast(message: string, kind?: string): Promise<void>;
    };
    ui: {
      registerPage(input: {
        path: string;
        label: string;
        component: ReactNS.ComponentType;
      }): { dispose(): void };
    };
  }

  interface QwenPawHost {
    React: typeof ReactNS;
    antd: Record<string, any>;
    fetch?: (path: string, init?: RequestInit) => Promise<Response>;
    getApiUrl: (path: string) => string;
    getApiToken: () => string;
  }

  interface QwenPawGlobal {
    host: QwenPawHost;
    paw?: { forApp(appId: string): PawSdk };
    registerRoutes?: (
      pluginId: string,
      routes: Array<{
        path: string;
        component: ReactNS.ComponentType;
        label: string;
      }>,
    ) => void;
  }

  interface Window {
    QwenPaw: QwenPawGlobal;
  }
}

export {};
