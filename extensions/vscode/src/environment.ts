export const API_KEY_SECRET = "qwenpaw.apiKey";

interface CoreEnvironmentOptions {
  readonly baseUrl: string;
  readonly mcpConfigPath: string;
  readonly model: string;
  readonly storedApiKey: string | undefined;
}

export function createCoreEnvironment(
  inherited: NodeJS.ProcessEnv,
  options: CoreEnvironmentOptions,
): NodeJS.ProcessEnv {
  const environment: NodeJS.ProcessEnv = {
    ...inherited,
    QWENPAW_BASE_URL: options.baseUrl,
    QWENPAW_MODEL: options.model,
  };
  if (options.storedApiKey) {
    environment.QWENPAW_API_KEY = options.storedApiKey;
  }
  if (options.mcpConfigPath.trim()) {
    environment.QWENPAW_MCP_CONFIG = options.mcpConfigPath.trim();
  }
  return environment;
}
