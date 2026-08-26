import { Stack } from "expo-router";
import { StatusBar } from "expo-status-bar";
import { useEffect } from "react";
import { GestureHandlerRootView } from "react-native-gesture-handler";

import { useAppStore } from "../store/app";
import { colors } from "../theme/tokens";

export default function RootLayout() {
  const bootstrap = useAppStore((state) => state.bootstrap);
  const status = useAppStore((state) => state.status);
  const connection = useAppStore((state) => state.connection);
  const connect = useAppStore((state) => state.connect);

  useEffect(() => {
    void bootstrap();
  }, [bootstrap]);

  useEffect(() => {
    if (status !== "disconnected" || !connection) return;
    const timer = setTimeout(() => {
      void connect(connection).catch(() => undefined);
    }, 5000);
    return () => clearTimeout(timer);
  }, [connect, connection, status]);

  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <StatusBar style="dark" />
      <Stack
        screenOptions={{
          headerShown: false,
          contentStyle: { backgroundColor: colors.canvas },
          animation: "fade",
        }}
      />
    </GestureHandlerRootView>
  );
}
