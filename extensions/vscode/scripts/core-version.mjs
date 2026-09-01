export function verifyCoreVersionResult(version, result) {
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error("QwenPaw Core --version failed");
  }
  const expected = `qwenpaw-core ${version}`;
  if (result.stdout.trim() !== expected) {
    throw new Error(
      `QwenPaw Core version mismatch: expected ${expected}, received ${result.stdout.trim() || "empty output"}`,
    );
  }
}
