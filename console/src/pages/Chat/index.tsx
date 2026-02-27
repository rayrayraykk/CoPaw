import {
  AgentScopeRuntimeWebUI,
  IAgentScopeRuntimeWebUIOptions,
} from "@agentscope-ai/chat";
import { useMemo } from "react";
import sessionApi from "./sessionApi";
import { useLocalStorageState } from "ahooks";
import defaultConfig, { DefaultConfig } from "./OptionsPanel/defaultConfig";
import Weather from "./Weather";
import { getApiUrl, getApiToken } from "../../api/config";
import "./index.module.less";

interface CustomWindow extends Window {
  currentSessionId?: string;
  currentUserId?: string;
  currentChannel?: string;
}

declare const window: CustomWindow;

type OptionsConfig = DefaultConfig;

export default function ChatPage() {
  const [optionsConfig] = useLocalStorageState<OptionsConfig>(
    "agent-scope-runtime-webui-options",
    {
      defaultValue: defaultConfig,
      listenStorageChange: true,
    },
  );

  const options = useMemo(() => {
    const customFetch = async (data: {
      input: any[];
      biz_params?: any;
    }): Promise<Response> => {
      const { input, biz_params } = data;

      const lastMessage = input[input.length - 1];
      const session = lastMessage?.session || {};

      const session_id = window.currentSessionId || session?.session_id || "";
      const user_id = window.currentUserId || session?.user_id || "default";
      const channel = window.currentChannel || session?.channel || "console";

      const requestBody = {
        input: input.slice(-1),
        session_id,
        user_id,
        channel,
        stream: true,
        ...biz_params,
      };

      const headers: HeadersInit = {
        "Content-Type": "application/json",
      };

      const token = getApiToken();
      if (token) {
        (headers as Record<string, string>).Authorization = `Bearer ${token}`;
      }

      const url = optionsConfig?.api?.baseURL || getApiUrl("/agent/process");
      return fetch(url, {
        method: "POST",
        headers,
        body: JSON.stringify(requestBody),
      });
    };

    return {
      ...optionsConfig,
      session: {
        multiple: true,
        api: sessionApi,
      },
      theme: {
        ...optionsConfig.theme,
      },
      api: {
        ...optionsConfig.api,
        fetch: customFetch,
        cancel(data: { session_id: string }) {
          console.log(data);
        },
      },
      customToolRenderConfig: {
        "weather search mock": Weather,
      },
    } as unknown as IAgentScopeRuntimeWebUIOptions;
  }, [optionsConfig]);

  return (
    <div style={{ height: "100%", width: "100%" }}>
      <AgentScopeRuntimeWebUI options={options} />
    </div>
  );
}
