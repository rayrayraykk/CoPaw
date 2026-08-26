import { router } from "expo-router";
import {
  Check,
  ChevronDown,
  Cloud,
  Plus,
  Server,
  X,
} from "lucide-react-native";
import { useState } from "react";
import {
  ActivityIndicator,
  Alert,
  Modal,
  Pressable,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import type { Connection } from "../../api/types";
import { connectionKey } from "../../storage/connection";
import { useAppStore } from "../../store/app";
import { colors, radius, spacing } from "../../theme/tokens";

export function WorkspaceBadge() {
  const connection = useAppStore((state) => state.connection);
  const [visible, setVisible] = useState(false);
  if (!connection) return null;
  return (
    <>
      <Pressable
        accessibilityLabel="切换 QwenPaw"
        onPress={() => setVisible(true)}
        style={({ pressed }) => [styles.badge, pressed && styles.pressed]}
      >
        <Text style={styles.badgeTitle}>QwenPaw</Text>
        <View style={styles.badgeMeta}>
          <Text style={styles.badgeSource}>{workspaceName(connection)}</Text>
          <ChevronDown color={colors.faint} size={12} strokeWidth={2.4} />
        </View>
      </Pressable>
      <WorkspaceSwitcher
        onClose={() => setVisible(false)}
        visible={visible}
      />
    </>
  );
}

export function WorkspaceSwitcher({
  onClose,
  visible,
}: {
  onClose: () => void;
  visible: boolean;
}) {
  const connection = useAppStore((state) => state.connection);
  const connections = useAppStore((state) => state.connections);
  const status = useAppStore((state) => state.status);
  const switchConnection = useAppStore((state) => state.switchConnection);
  const [switchingKey, setSwitchingKey] = useState<string | null>(null);
  const activeKey = connection ? connectionKey(connection) : null;

  const select = async (target: Connection) => {
    const key = connectionKey(target);
    if (key === activeKey) {
      onClose();
      return;
    }
    try {
      setSwitchingKey(key);
      await switchConnection(key);
      onClose();
    } catch (error) {
      Alert.alert(
        "切换失败",
        error instanceof Error ? error.message : "当前 QwenPaw 仍保持连接。",
      );
    } finally {
      setSwitchingKey(null);
    }
  };

  const add = () => {
    onClose();
    router.push({ pathname: "/", params: { add: "1" } });
  };

  return (
    <Modal
      animationType="slide"
      onRequestClose={onClose}
      transparent
      visible={visible}
    >
      <View style={styles.mask}>
        <Pressable onPress={onClose} style={StyleSheet.absoluteFill} />
        <SafeAreaView edges={["bottom"]} style={styles.sheet}>
          <View style={styles.sheetHandle} />
          <View style={styles.sheetHeader}>
            <View>
              <Text style={styles.sheetTitle}>切换 QwenPaw</Text>
              <Text style={styles.sheetCopy}>切换不会退出另一个连接</Text>
            </View>
            <Pressable
              accessibilityLabel="关闭"
              onPress={onClose}
              style={styles.close}
            >
              <X color={colors.ink} size={20} />
            </Pressable>
          </View>
          <View style={styles.workspaceList}>
            {connections.map((item) => {
              const key = connectionKey(item);
              const active = key === activeKey;
              const Icon = item.source === "platform" ? Cloud : Server;
              return (
                <Pressable
                  key={key}
                  disabled={status === "connecting"}
                  onPress={() => void select(item)}
                  style={({ pressed }) => [
                    styles.workspace,
                    active && styles.workspaceActive,
                    pressed && styles.pressed,
                  ]}
                >
                  <View style={[styles.workspaceIcon, active && styles.workspaceIconActive]}>
                    <Icon color={active ? colors.white : colors.accent} size={20} />
                  </View>
                  <View style={styles.workspaceBody}>
                    <Text style={styles.workspaceName}>{workspaceName(item)}</Text>
                    <Text numberOfLines={1} style={styles.workspaceUrl}>{item.baseUrl}</Text>
                  </View>
                  {status === "connecting" && switchingKey === key ? (
                    <ActivityIndicator color={colors.accent} size="small" />
                  ) : active ? (
                    <View style={styles.check}><Check color={colors.white} size={15} strokeWidth={2.6} /></View>
                  ) : null}
                </Pressable>
              );
            })}
          </View>
          <Pressable onPress={add} style={({ pressed }) => [styles.add, pressed && styles.pressed]}>
            <View style={styles.addIcon}><Plus color={colors.accent} size={20} /></View>
            <View style={styles.workspaceBody}>
              <Text style={styles.addTitle}>再配对一只 QwenPaw</Text>
              <Text style={styles.workspaceUrl}>私人部署或 Platform 云端 QwenPaw</Text>
            </View>
          </Pressable>
        </SafeAreaView>
      </View>
    </Modal>
  );
}

export function workspaceName(connection: Connection): string {
  return connection.source === "platform" ? "Platform 云端" : "本地 / 私人";
}

const styles = StyleSheet.create({
  badge: { flex: 1, minWidth: 0, marginLeft: spacing.sm, justifyContent: "center" },
  badgeTitle: { color: colors.ink, fontSize: 21, fontWeight: "700", letterSpacing: -0.45 },
  badgeMeta: { flexDirection: "row", alignItems: "center", gap: 2, marginTop: 1 },
  badgeSource: { color: colors.muted, fontSize: 10, fontWeight: "600" },
  mask: { flex: 1, justifyContent: "flex-end", backgroundColor: "rgba(20, 15, 12, 0.28)" },
  sheet: { paddingHorizontal: spacing.md, paddingBottom: spacing.md, borderTopLeftRadius: 26, borderTopRightRadius: 26, backgroundColor: colors.groupedBackground },
  sheetHandle: { width: 36, height: 5, alignSelf: "center", marginTop: 8, marginBottom: 10, borderRadius: 3, backgroundColor: colors.line },
  sheetHeader: { minHeight: 58, flexDirection: "row", alignItems: "center", justifyContent: "space-between" },
  sheetTitle: { color: colors.ink, fontSize: 20, fontWeight: "700" },
  sheetCopy: { color: colors.muted, fontSize: 11, marginTop: 3 },
  close: { width: 38, height: 38, alignItems: "center", justifyContent: "center", borderRadius: 19, backgroundColor: colors.searchBackground },
  workspaceList: { overflow: "hidden", marginTop: spacing.sm, borderRadius: radius.md, backgroundColor: colors.surface },
  workspace: { minHeight: 72, flexDirection: "row", alignItems: "center", gap: 12, paddingHorizontal: spacing.md, borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.hairline },
  workspaceActive: { backgroundColor: colors.accentSoft },
  workspaceIcon: { width: 42, height: 42, alignItems: "center", justifyContent: "center", borderRadius: 13, backgroundColor: colors.accentSoft },
  workspaceIconActive: { backgroundColor: colors.accent },
  workspaceBody: { flex: 1, minWidth: 0, gap: 3 },
  workspaceName: { color: colors.ink, fontSize: 15, fontWeight: "600" },
  workspaceUrl: { color: colors.muted, fontSize: 11 },
  check: { width: 26, height: 26, alignItems: "center", justifyContent: "center", borderRadius: 13, backgroundColor: colors.accent },
  add: { minHeight: 68, flexDirection: "row", alignItems: "center", gap: 12, marginTop: spacing.sm, paddingHorizontal: spacing.md, borderRadius: radius.md, backgroundColor: colors.surface },
  addIcon: { width: 42, height: 42, alignItems: "center", justifyContent: "center", borderRadius: 13, borderWidth: 1, borderColor: colors.accentSoft, backgroundColor: colors.surfaceStrong },
  addTitle: { color: colors.accentDark, fontSize: 14, fontWeight: "700" },
  pressed: { opacity: 0.68 },
});
