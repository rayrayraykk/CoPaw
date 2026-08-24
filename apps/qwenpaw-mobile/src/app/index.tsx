import { LinearGradient } from "expo-linear-gradient";
import { router } from "expo-router";
import { ArrowRight, QrCode, Server, Sparkles } from "lucide-react-native";
import { useEffect, useState } from "react";
import {
  ActivityIndicator,
  KeyboardAvoidingView,
  Platform,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import {
  discoverPlatformQwenPaw,
  loginQwenPaw,
} from "../api/client";
import { normalizeBaseUrl } from "../api/pairing";
import { Field } from "../components/Field";
import { PrimaryButton } from "../components/PrimaryButton";
import { useAppStore } from "../store/app";
import { colors, radius, spacing } from "../theme/tokens";

type Mode = "pair" | "direct" | "platform";

export default function ConnectScreen() {
  const status = useAppStore((state) => state.status);
  const connect = useAppStore((state) => state.connect);
  const storeError = useAppStore((state) => state.error);
  const [mode, setMode] = useState<Mode>("pair");
  const [baseUrl, setBaseUrl] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [platformAccount, setPlatformAccount] = useState("");
  const [platformPassword, setPlatformPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (status === "ready") router.replace("/chats");
  }, [status]);

  if (status === "booting") {
    return (
      <View style={styles.loading}>
        <ActivityIndicator color={colors.accentDark} />
      </View>
    );
  }

  const submitDirect = async (resolvedUrl?: string) => {
    setBusy(true);
    setError(null);
    try {
      const url = normalizeBaseUrl(resolvedUrl ?? baseUrl);
      const connection = await loginQwenPaw(url, username, password);
      await connect(connection);
      router.replace("/chats");
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Connection failed.");
    } finally {
      setBusy(false);
    }
  };

  const submitPlatform = async () => {
    setBusy(true);
    setError(null);
    try {
      const url = await discoverPlatformQwenPaw(
        platformAccount,
        platformPassword,
      );
      setBaseUrl(url);
      await submitDirect(url);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Platform login failed.");
      setBusy(false);
    }
  };

  return (
    <LinearGradient colors={["#F8F5EF", colors.canvas]} style={styles.root}>
      <SafeAreaView style={styles.safe}>
        <KeyboardAvoidingView
          behavior={Platform.OS === "ios" ? "padding" : undefined}
          style={styles.flex}
        >
          <ScrollView
            contentContainerStyle={styles.content}
            keyboardShouldPersistTaps="handled"
          >
            <View style={styles.brandRow}>
              <View style={styles.mark}><Sparkles size={18} color={colors.white} /></View>
              <Text style={styles.brand}>QwenPaw</Text>
            </View>
            <View style={styles.intro}>
              <Text style={styles.eyebrow}>PRIVATE AI, WITH YOU</Text>
              <Text style={styles.title}>Your QwenPaw,{"\n"}within reach.</Text>
              <Text style={styles.subtitle}>
                Link your private workspace and continue every conversation
                across Android and iOS.
              </Text>
            </View>

            <View style={styles.card}>
              <View style={styles.modes}>
                <ModeButton active={mode === "pair"} label="Scan" onPress={() => setMode("pair")} />
                <ModeButton active={mode === "direct"} label="Direct" onPress={() => setMode("direct")} />
                <ModeButton active={mode === "platform"} label="Platform" onPress={() => setMode("platform")} />
              </View>

              {mode === "pair" ? (
                <View style={styles.modeBody}>
                  <View style={styles.scanArt}>
                    <QrCode size={48} color={colors.accentDark} strokeWidth={1.4} />
                  </View>
                  <Text style={styles.cardTitle}>Pair in one scan</Text>
                  <Text style={styles.cardCopy}>
                    Open QwenPaw Console, choose Pair mobile, then scan its
                    short-lived code. No password is copied to your phone.
                  </Text>
                  <PrimaryButton
                    label="Scan pairing code"
                    icon={QrCode}
                    onPress={() => router.push("/scan")}
                  />
                </View>
              ) : null}

              {mode === "direct" ? (
                <View style={styles.modeBody}>
                  <Field
                    autoCapitalize="none"
                    autoCorrect={false}
                    keyboardType="url"
                    label="QwenPaw address"
                    onChangeText={setBaseUrl}
                    placeholder="https://paw.example.com"
                    value={baseUrl}
                  />
                  <Field
                    autoCapitalize="none"
                    label="Username"
                    onChangeText={setUsername}
                    placeholder="Your QwenPaw username"
                    value={username}
                  />
                  <Field
                    label="Password"
                    onChangeText={setPassword}
                    placeholder="Your QwenPaw password"
                    secureTextEntry
                    value={password}
                  />
                  <PrimaryButton
                    disabled={!baseUrl}
                    icon={ArrowRight}
                    label="Connect securely"
                    loading={busy}
                    onPress={() => void submitDirect()}
                  />
                </View>
              ) : null}

              {mode === "platform" ? (
                <View style={styles.modeBody}>
                  <Field
                    autoCapitalize="none"
                    label="AgentScope account"
                    onChangeText={setPlatformAccount}
                    placeholder="Email or account"
                    value={platformAccount}
                  />
                  <Field
                    label="Platform password"
                    onChangeText={setPlatformPassword}
                    placeholder="Password"
                    secureTextEntry
                    value={platformPassword}
                  />
                  <View style={styles.divider} />
                  <Text style={styles.helper}>
                    If QwenPaw web login is enabled, enter those credentials below.
                  </Text>
                  <Field
                    autoCapitalize="none"
                    label="QwenPaw username"
                    onChangeText={setUsername}
                    placeholder="Optional when web login is off"
                    value={username}
                  />
                  <Field
                    label="QwenPaw password"
                    onChangeText={setPassword}
                    placeholder="Optional when web login is off"
                    secureTextEntry
                    value={password}
                  />
                  <PrimaryButton
                    disabled={!platformAccount || !platformPassword}
                    icon={Server}
                    label="Find my deployment"
                    loading={busy}
                    onPress={() => void submitPlatform()}
                  />
                </View>
              ) : null}

              {error || storeError ? (
                <Text style={styles.error}>{error || storeError}</Text>
              ) : null}
            </View>
          </ScrollView>
        </KeyboardAvoidingView>
      </SafeAreaView>
    </LinearGradient>
  );
}

function ModeButton({
  active,
  label,
  onPress,
}: {
  active: boolean;
  label: string;
  onPress: () => void;
}) {
  return (
    <Pressable onPress={onPress} style={[styles.mode, active && styles.modeActive]}>
      <Text style={[styles.modeLabel, active && styles.modeLabelActive]}>{label}</Text>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1 },
  safe: { flex: 1 },
  flex: { flex: 1 },
  loading: { flex: 1, alignItems: "center", justifyContent: "center", backgroundColor: colors.canvas },
  content: { width: "100%", maxWidth: 560, alignSelf: "center", padding: spacing.lg, paddingBottom: spacing.xxl },
  brandRow: { flexDirection: "row", alignItems: "center", gap: spacing.sm },
  mark: { width: 36, height: 36, borderRadius: 12, backgroundColor: colors.black, alignItems: "center", justifyContent: "center" },
  brand: { fontSize: 18, color: colors.ink, fontWeight: "700", letterSpacing: -0.5 },
  intro: { paddingVertical: spacing.xxl, gap: spacing.md },
  eyebrow: { fontSize: 11, color: colors.accentDark, letterSpacing: 2, fontWeight: "700" },
  title: { fontSize: 46, lineHeight: 49, color: colors.ink, letterSpacing: -2.4, fontWeight: "600" },
  subtitle: { fontSize: 17, lineHeight: 26, color: colors.muted, maxWidth: 440 },
  card: { backgroundColor: colors.surface, borderRadius: radius.lg, borderWidth: 1, borderColor: colors.line, padding: spacing.sm, shadowColor: colors.black, shadowOpacity: 0.06, shadowRadius: 30, shadowOffset: { width: 0, height: 12 }, elevation: 3 },
  modes: { flexDirection: "row", padding: 4, borderRadius: radius.md, backgroundColor: "#ECE8E1" },
  mode: { flex: 1, minHeight: 38, borderRadius: 12, alignItems: "center", justifyContent: "center" },
  modeActive: { backgroundColor: colors.surfaceStrong },
  modeLabel: { color: colors.muted, fontSize: 13, fontWeight: "600" },
  modeLabelActive: { color: colors.ink },
  modeBody: { padding: spacing.md, paddingTop: spacing.lg, gap: spacing.md },
  scanArt: { width: 92, height: 92, borderRadius: 28, backgroundColor: colors.accentSoft, alignItems: "center", justifyContent: "center", marginBottom: spacing.sm },
  cardTitle: { color: colors.ink, fontSize: 25, fontWeight: "600", letterSpacing: -0.8 },
  cardCopy: { color: colors.muted, fontSize: 15, lineHeight: 23, marginBottom: spacing.sm },
  helper: { color: colors.muted, fontSize: 13, lineHeight: 19 },
  divider: { height: 1, backgroundColor: colors.line, marginVertical: spacing.xs },
  error: { color: colors.danger, fontSize: 13, lineHeight: 19, paddingHorizontal: spacing.md, paddingBottom: spacing.md },
});
