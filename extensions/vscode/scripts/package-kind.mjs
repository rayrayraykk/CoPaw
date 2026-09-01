export function parsePackageKind(value) {
  const packageKind = value ?? "release";
  if (packageKind !== "release" && packageKind !== "qa") {
    throw new Error(
      "QWENPAW_VSCODE_PACKAGE_KIND must be release or qa",
    );
  }
  return packageKind;
}
