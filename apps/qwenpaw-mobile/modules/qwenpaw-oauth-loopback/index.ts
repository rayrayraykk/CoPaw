import { requireNativeModule } from "expo-modules-core";
import { Platform } from "react-native";

interface OAuthLoopbackNativeModule {
  startAsync(): Promise<number>;
  stopAsync(): Promise<void>;
}

const nativeModule = Platform.OS === "ios"
  ? requireNativeModule<OAuthLoopbackNativeModule>("QwenPawOAuthLoopback")
  : null;

export async function startOAuthLoopback(): Promise<number> {
  if (!nativeModule) {
    throw new Error("当前设备暂不支持 Platform OAuth 回跳");
  }
  return nativeModule.startAsync();
}

export async function stopOAuthLoopback(): Promise<void> {
  await nativeModule?.stopAsync();
}
