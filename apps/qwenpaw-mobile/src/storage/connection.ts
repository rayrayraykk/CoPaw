import * as SecureStore from "expo-secure-store";

import type { Connection } from "../api/types";

const CONNECTION_KEY = "qwenpaw.mobile.connection.v1";

export async function loadConnection(): Promise<Connection | null> {
  const stored = await SecureStore.getItemAsync(CONNECTION_KEY);
  if (!stored) return null;
  try {
    return JSON.parse(stored) as Connection;
  } catch {
    await SecureStore.deleteItemAsync(CONNECTION_KEY);
    return null;
  }
}

export async function saveConnection(connection: Connection): Promise<void> {
  await SecureStore.setItemAsync(CONNECTION_KEY, JSON.stringify(connection), {
    keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY,
  });
}

export async function clearConnection(): Promise<void> {
  await SecureStore.deleteItemAsync(CONNECTION_KEY);
}
