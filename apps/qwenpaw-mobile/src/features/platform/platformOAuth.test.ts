import assert from "node:assert/strict";
import test from "node:test";

import {
  base64Url,
  buildPlatformAuthorizeUrl,
  parsePlatformOAuthCallback,
} from "./platformOAuth";

test("Platform authorize URL uses PKCE and a loopback callback", () => {
  const value = buildPlatformAuthorizeUrl({
    codeChallenge: "challenge",
    redirectUri: "http://127.0.0.1:43210/callback/qwenpaw-mobile",
    state: "state",
  });
  const url = new URL(value);
  assert.equal(url.pathname, "/cli/login");
  assert.equal(url.searchParams.get("code_challenge_method"), "S256");
  assert.equal(
    url.searchParams.get("redirect_uri"),
    "http://127.0.0.1:43210/callback/qwenpaw-mobile",
  );
});

test("OAuth callback rejects state mismatch", () => {
  assert.throws(
    () => parsePlatformOAuthCallback(
      "qwenpaw://platform-auth?code=code&state=other",
      "expected",
    ),
    /状态校验失败/,
  );
});

test("OAuth callback returns a verified code", () => {
  assert.equal(
    parsePlatformOAuthCallback(
      "qwenpaw://platform-auth?code=code&state=expected",
      "expected",
    ),
    "code",
  );
});

test("base64 URL encoding removes unsafe characters", () => {
  assert.equal(base64Url("ab+c/d=="), "ab-c_d");
});
