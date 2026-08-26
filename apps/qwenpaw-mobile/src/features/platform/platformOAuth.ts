export const PLATFORM_CLI_CLIENT_ID = "agentscope-platform-cli";
export const PLATFORM_CLI_SCOPE = "qwenpaw";
const PLATFORM_ORIGIN = "https://platform.agentscope.io";

export function buildPlatformAuthorizeUrl({
  codeChallenge,
  redirectUri,
  state,
}: {
  codeChallenge: string;
  redirectUri: string;
  state: string;
}): string {
  const query = new URLSearchParams({
    client_id: PLATFORM_CLI_CLIENT_ID,
    redirect_uri: redirectUri,
    response_type: "code",
    state,
    code_challenge: codeChallenge,
    code_challenge_method: "S256",
    scope: PLATFORM_CLI_SCOPE,
  });
  return `${PLATFORM_ORIGIN}/cli/login?${query.toString()}`;
}

export function parsePlatformOAuthCallback(
  value: string,
  expectedState: string,
): string {
  const callback = new URL(value);
  const error = callback.searchParams.get("error");
  if (error) {
    throw new Error(callback.searchParams.get("error_description") || error);
  }
  const state = callback.searchParams.get("state");
  if (!state || state !== expectedState) {
    throw new Error("Platform 登录状态校验失败，请重新登录");
  }
  const code = callback.searchParams.get("code");
  if (!code) throw new Error("Platform 登录没有返回授权码");
  return code;
}

export function base64Url(value: string): string {
  return value.replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}
