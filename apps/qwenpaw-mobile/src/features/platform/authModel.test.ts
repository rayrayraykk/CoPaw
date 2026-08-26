import assert from "node:assert/strict";
import test from "node:test";

import {
  isValidPlatformEmail,
  isValidPlatformPassword,
  platformRegistrationError,
} from "./authModel";

test("validates Platform registration email and password rules", () => {
  assert.equal(isValidPlatformEmail("user@example.com"), true);
  assert.equal(isValidPlatformEmail("user"), false);
  assert.equal(isValidPlatformPassword("a1234567"), true);
  assert.equal(isValidPlatformPassword("12345678"), false);
  assert.equal(isValidPlatformPassword("a123"), false);
});

test("returns the first actionable registration error", () => {
  assert.equal(platformRegistrationError({
    account: "user@example.com",
    confirmPassword: "a1234567",
    password: "a1234567",
    verifyCode: "123456",
  }), null);
  assert.equal(platformRegistrationError({
    account: "user@example.com",
    confirmPassword: "b1234567",
    password: "a1234567",
    verifyCode: "123456",
  }), "两次输入的密码不一致");
});
