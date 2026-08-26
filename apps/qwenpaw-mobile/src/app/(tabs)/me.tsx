import Constants from "expo-constants";
import { router, useFocusEffect } from "expo-router";
import {
  Bot,
  Cloud,
  Info,
  LayoutGrid,
  LogOut,
  LogIn,
  Plus,
  RefreshCw,
  Server,
  ShieldCheck,
  Trash2,
} from "lucide-react-native";
import { useCallback, useState } from "react";
import {
  Alert,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { IosHeader } from "../../components/IosHeader";
import { IosGroup, IosRow } from "../../components/IosList";
import { AgentAvatar } from "../../features/agents/AgentAvatar";
import { workspaceName } from "../../features/workspaces/WorkspaceSwitcher";
import { resolveAgentAppearance } from "../../storage/agentAppearance";
import { connectionKey } from "../../storage/connection";
import {
  clearPlatformSession,
  loadPlatformSession,
  type PlatformSession,
} from "../../storage/platformSession";
import { useAppStore } from "../../store/app";
import { colors, radius, spacing } from "../../theme/tokens";

export default function MeScreen() {
  const connection = useAppStore((state) => state.connection);
  const connections = useAppStore((state) => state.connections);
  const agents = useAppStore((state) => state.agents);
  const connect = useAppStore((state) => state.connect);
  const switchConnection = useAppStore((state) => state.switchConnection);
  const removeConnection = useAppStore((state) => state.removeConnection);
  const appearances = useAppStore((state) => state.agentAppearances);
  const [platformSession, setPlatformSession] = useState<PlatformSession | null>(null);
  const activeAgent = agents.find((agent) => agent.id === connection?.agentId);
  const activeAppearance = resolveAgentAppearance(
    appearances,
    connection,
    activeAgent,
  );

  useFocusEffect(useCallback(() => {
    void loadPlatformSession().then(setPlatformSession);
  }, []));

  const confirmDisconnect = () => {
    if (!connection) return;
    Alert.alert(
      `移除${workspaceName(connection)}？`,
      connections.length > 1
        ? "只会移除这只 QwenPaw，其他配对仍会保留。"
        : "这是最后一只 QwenPaw，移除后需要重新扫码或登录。",
      [
        { text: "取消", style: "cancel" },
        {
          text: "移除",
          style: "destructive",
          onPress: () => void removeConnection(connectionKey(connection)).then(() => {
            if (connections.length === 1) router.replace("/");
          }),
        },
      ],
    );
  };

  const reconnect = () => {
    if (!connection) return;
    void connect(connection).catch((error) => {
      Alert.alert(
        "重新连接失败",
        error instanceof Error ? error.message : "请检查服务器状态。",
      );
    });
  };

  const confirmPlatformLogout = () => {
    Alert.alert(
      "退出 Platform？",
      "不会解除当前 QwenPaw 配对；社区点赞、评论和发布会再次要求登录。",
      [
        { text: "取消", style: "cancel" },
        {
          text: "退出 Platform",
          style: "destructive",
          onPress: () => void clearPlatformSession().then(() => setPlatformSession(null)),
        },
      ],
    );
  };

  return (
    <SafeAreaView edges={["top"]} style={styles.root}>
      <IosHeader title="我的" />
      <ScrollView contentContainerStyle={styles.content}>
        <Pressable
          accessibilityRole="button"
          onPress={() => router.push("/agents")}
          style={({ pressed }) => [styles.profile, pressed && styles.profilePressed]}
        >
          <AgentAvatar
            active
            avatarUri={activeAppearance.avatarUri}
            size={58}
          />
          <View style={styles.profileText}>
            <Text style={styles.profileName}>{activeAppearance.name}</Text>
            <Text numberOfLines={1} style={styles.profileMeta}>
              {connection?.source === "platform" ? "Platform 云端 QwenPaw" : "私人部署的 QwenPaw"}
            </Text>
          </View>
          <View style={styles.online} />
        </Pressable>

        <IosGroup title="设置">
          <IosRow
            icon={LayoutGrid}
            iconTone="ink"
            label="工作台"
            onPress={() => router.push("/workbench")}
            subtitle="Agent、模型、Skills、渠道、任务与系统"
          />
          <IosRow
            icon={ShieldCheck}
            label="安全"
            onPress={() => router.push("/module/security")}
            subtitle="Sandbox、Tool Guard、File Guard 与扫描"
          />
        </IosGroup>

        <IosGroup title="已配对的 QwenPaw">
          {connections.map((item) => {
            const active = connection
              ? connectionKey(item) === connectionKey(connection)
              : false;
            return (
              <IosRow
                key={connectionKey(item)}
                icon={item.source === "platform" ? Cloud : Server}
                label={workspaceName(item)}
                onPress={() => active
                  ? undefined
                  : void switchConnection(connectionKey(item)).catch((error) => {
                    Alert.alert("切换失败", error instanceof Error ? error.message : "请稍后重试。");
                  })}
                subtitle={item.baseUrl}
                trailing={active ? "当前" : "切换"}
              />
            );
          })}
          <IosRow
            icon={Plus}
            label="再配对一只 QwenPaw"
            onPress={() => router.push({ pathname: "/", params: { add: "1" } })}
            subtitle="同时保留私人部署和 Platform 云端"
          />
        </IosGroup>

        <IosGroup title="当前 Agent">
          <IosRow
            icon={Bot}
            iconTone="ink"
            label={activeAppearance.name}
            onPress={() => router.push("/agents")}
            subtitle="头像、昵称与 Agent 切换"
            trailing={activeAgent?.id}
          />
          <IosRow
            icon={RefreshCw}
            label="重新连接"
            onPress={reconnect}
            subtitle="刷新 Agent 和会话数据"
          />
        </IosGroup>

        <IosGroup title="Platform 账号">
          <IosRow
            icon={Cloud}
            label="Platform 账号"
            subtitle={platformSession
              ? platformSession.username || "已安全登录"
              : "社区浏览无需登录，互动时再登录"}
            trailing={platformSession ? "已登录" : "未登录"}
          />
          {platformSession ? (
            <IosRow
              destructive
              icon={LogOut}
              iconTone="ink"
              label="退出 Platform"
              onPress={confirmPlatformLogout}
            />
          ) : (
            <IosRow
              icon={LogIn}
              label="登录 Platform"
              onPress={() => router.push("/community/login")}
              subtitle="启用点赞、评论和发布"
            />
          )}
        </IosGroup>

        <IosGroup title="设备">
          <IosRow
            icon={ShieldCheck}
            label="配对状态"
            subtitle="除非主动移除，否则此设备保持配对"
            trailing="已配对"
          />
        </IosGroup>

        <IosGroup title="关于">
          <IosRow
            icon={Info}
            iconTone="ink"
            label="QwenPaw Mobile"
            trailing={Constants.expoConfig?.version || "1.0.0"}
          />
        </IosGroup>

        <IosGroup>
          <IosRow
            destructive
            icon={Trash2}
            iconTone="ink"
            label="移除这只 QwenPaw"
            onPress={confirmDisconnect}
          />
        </IosGroup>
      </ScrollView>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1, backgroundColor: colors.groupedBackground },
  content: {
    width: "100%",
    maxWidth: 760,
    alignSelf: "center",
    gap: spacing.lg,
    paddingHorizontal: spacing.md,
    paddingBottom: spacing.xxl,
  },
  profile: {
    minHeight: 98,
    flexDirection: "row",
    alignItems: "center",
    gap: spacing.md,
    padding: spacing.md,
    borderRadius: radius.md,
    backgroundColor: colors.surface,
  },
  profilePressed: { backgroundColor: colors.pressed },
  profileText: { flex: 1, minWidth: 0, gap: 4 },
  profileName: { color: colors.ink, fontSize: 20, fontWeight: "600" },
  profileMeta: { color: colors.muted, fontSize: 13 },
  online: { width: 9, height: 9, borderRadius: 5, backgroundColor: colors.accent },
});
