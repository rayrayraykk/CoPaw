import { Check, Plus, Search } from "lucide-react-native";
import { memo, useCallback, useMemo, useState } from "react";
import {
  ActivityIndicator,
  Alert,
  FlatList,
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import type { AgentSummary } from "../../api/types";
import { IosHeader } from "../../components/IosHeader";
import { AgentAvatar } from "../../features/agents/AgentAvatar";
import { AgentProfileSheet } from "../../features/agents/AgentProfileSheet";
import { QwenPawClient } from "../../api/client";
import { DynamicConfigSheet } from "../../features/workbench/DynamicConfigSheet";
import {
  agentAppearanceKey,
  resolveAgentAppearance,
} from "../../storage/agentAppearance";
import { useAppStore } from "../../store/app";
import { colors, radius, spacing } from "../../theme/tokens";

export default function AgentsScreen() {
  const agents = useAppStore((state) => state.agents);
  const activeAgentId = useAppStore((state) => state.connection?.agentId);
  const selectAgent = useAppStore((state) => state.selectAgent);
  const connection = useAppStore((state) => state.connection);
  const appearances = useAppStore((state) => state.agentAppearances);
  const setAgentAppearance = useAppStore((state) => state.setAgentAppearance);
  const reconnect = useAppStore((state) => state.connect);
  const [query, setQuery] = useState("");
  const [switchingId, setSwitchingId] = useState<string | null>(null);
  const [editingAgent, setEditingAgent] = useState<AgentSummary | null>(null);
  const [manager, setManager] = useState<
    { mode: "create" } | { mode: "copy"; agent: AgentSummary } | null
  >(null);

  const visibleAgents = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    if (!normalized) return agents;
    return agents.filter((agent) => (
      `${resolveAgentAppearance(appearances, connection, agent).name} ` +
      agent.description
    ).toLocaleLowerCase().includes(normalized));
  }, [agents, appearances, connection, query]);

  const switchAgent = useCallback(async (agentId: string) => {
    if (agentId === activeAgentId || switchingId) return;
    setSwitchingId(agentId);
    try {
      await selectAgent(agentId);
    } catch (error) {
      Alert.alert(
        "无法切换 Agent",
        error instanceof Error ? error.message : "请稍后重试。",
      );
    } finally {
      setSwitchingId(null);
    }
  }, [activeAgentId, selectAgent, switchingId]);

  const reload = useCallback(async () => {
    if (connection) await reconnect(connection);
  }, [connection, reconnect]);

  const manageAgent = useCallback((agent: AgentSummary) => {
    if (!connection) return;
    const client = new QwenPawClient(connection);
    const enabled = agent.enabled !== false;
    const pinned = agent.pinned === true;
    Alert.alert(agent.name || agent.id, "管理 QwenPaw Agent", [
      { text: "取消", style: "cancel" },
      { text: "复制", onPress: () => setManager({ mode: "copy", agent }) },
      {
        text: pinned ? "取消置顶" : "置顶",
        onPress: () => void client.mutateModule(
          `/agents/${encodeURIComponent(agent.id)}/pin`,
          "PATCH",
          { pinned: !pinned },
        ).then(reload).catch((reason) => Alert.alert("操作失败", errorMessage(reason))),
      },
      {
        text: enabled ? "停用" : "启用",
        onPress: () => void client.mutateModule(
          `/agents/${encodeURIComponent(agent.id)}/toggle`,
          "PATCH",
          { enabled: !enabled },
        ).then(reload).catch((reason) => Alert.alert("操作失败", errorMessage(reason))),
      },
      ...(agent.id === "default" || agent.id === activeAgentId ? [] : [{
        text: "删除",
        style: "destructive" as const,
        onPress: () => Alert.alert("删除 Agent？", "会删除对应 workspace 和配置，此操作无法撤销。", [
          { text: "取消", style: "cancel" },
          {
            text: "删除",
            style: "destructive",
            onPress: () => void client.mutateModule(
              `/agents/${encodeURIComponent(agent.id)}`,
              "DELETE",
            ).then(reload).catch((reason) => Alert.alert("删除失败", errorMessage(reason))),
          },
        ]),
      }]),
    ]);
  }, [activeAgentId, connection, reload]);

  return (
    <SafeAreaView edges={["top"]} style={styles.root}>
      <View style={styles.shell}>
        <IosHeader
          actionIcon={Plus}
          actionLabel="创建 Agent"
          onAction={() => setManager({ mode: "create" })}
          title="智能体"
        />
        <Text style={styles.helper}>
          选择当前智能体。新会话和工作台配置会跟随这里的选择。
        </Text>
        <View style={styles.search}>
        <Search color={colors.faint} size={17} />
        <TextInput
          clearButtonMode="while-editing"
          onChangeText={setQuery}
          placeholder="搜索智能体"
          placeholderTextColor={colors.faint}
          style={styles.searchInput}
          value={query}
        />
        </View>
        <Text style={styles.sectionTitle}>可用智能体</Text>
        <FlatList
        contentContainerStyle={styles.list}
        data={visibleAgents}
        ItemSeparatorComponent={() => <View style={styles.separator} />}
        keyExtractor={(item) => item.id}
        renderItem={({ item }) => (
          <AgentRow
            active={item.id === activeAgentId}
            agent={item}
            loading={item.id === switchingId}
            appearance={resolveAgentAppearance(appearances, connection, item)}
            onEdit={() => setEditingAgent(item)}
            onManage={() => manageAgent(item)}
            onPress={switchAgent}
          />
        )}
        />
        {editingAgent ? (
          <AgentProfileSheet
            agent={editingAgent}
            appearance={connection
              ? appearances[agentAppearanceKey(connection.baseUrl, editingAgent.id)]
              : undefined}
            key={editingAgent.id}
            onClose={() => setEditingAgent(null)}
            onSave={(appearance) => setAgentAppearance(editingAgent.id, appearance)}
          />
        ) : null}
        {manager && connection ? (
          <DynamicConfigSheet
            fields={manager.mode === "create" ? [
              { name: "name", label: "名称", type: "text", required: true },
              { name: "id", label: "Agent ID", type: "text", placeholder: "可选，留空自动生成" },
              { name: "description", label: "说明", type: "textarea" },
              { name: "workspace_dir", label: "Workspace 路径", type: "text", placeholder: "可选" },
            ] : [
              { name: "name", label: "新 Agent 名称", type: "text", required: true },
              { name: "copy_md_files", label: "复制提示与记忆文件", type: "switch" },
              { name: "copy_skills", label: "复制 Skills", type: "switch" },
              { name: "copy_jobs", label: "复制 Cron Jobs", type: "switch" },
            ]}
            onClose={() => setManager(null)}
            onSave={async (values) => {
              const client = new QwenPawClient(connection);
              if (manager.mode === "create") {
                await client.mutateModule("/agents", "POST", {
                  name: String(values.name || "").trim(),
                  ...(String(values.id || "").trim()
                    ? { id: String(values.id).trim() }
                    : {}),
                  ...(String(values.description || "").trim()
                    ? { description: String(values.description).trim() }
                    : {}),
                  ...(String(values.workspace_dir || "").trim()
                    ? { workspace_dir: String(values.workspace_dir).trim() }
                    : {}),
                  language: "zh-CN",
                  backend: "qwenpaw",
                });
              } else {
                await client.mutateModule(
                  `/agents/${encodeURIComponent(manager.agent.id)}/copy`,
                  "POST",
                  {
                    name: String(values.name || "").trim(),
                    copy_agent_json: true,
                    copy_md_files: values.copy_md_files === true,
                    copy_skills: values.copy_skills === true,
                    copy_jobs: values.copy_jobs === true,
                  },
                );
              }
              await reload();
            }}
            title={manager.mode === "create" ? "创建 Agent" : `复制 ${manager.agent.name}`}
            values={manager.mode === "create" ? {} : {
              name: `${manager.agent.name || manager.agent.id} Copy`,
              copy_md_files: true,
              copy_skills: false,
              copy_jobs: false,
            }}
          />
        ) : null}
      </View>
    </SafeAreaView>
  );
}

const AgentRow = memo(function AgentRow({
  active,
  agent,
  loading,
  appearance,
  onEdit,
  onManage,
  onPress,
}: {
  active: boolean;
  agent: AgentSummary;
  loading: boolean;
  appearance: { name: string; avatarUri?: string };
  onEdit: () => void;
  onManage: () => void;
  onPress: (agentId: string) => void;
}) {
  return (
    <Pressable
      accessibilityRole="button"
      onLongPress={onManage}
      onPress={() => onPress(agent.id)}
      style={({ pressed }) => [styles.row, pressed && styles.pressed]}
    >
      <Pressable
        accessibilityLabel={`编辑 ${appearance.name} 头像和昵称`}
        onPress={(event) => {
          event.stopPropagation();
          onEdit();
        }}
      >
        <AgentAvatar
          active={active}
          avatarUri={appearance.avatarUri}
          size={46}
        />
      </Pressable>
      <View style={styles.rowBody}>
        <Text numberOfLines={1} style={styles.name}>{appearance.name}</Text>
        <Text numberOfLines={1} style={styles.description}>
          {agent.description || "QwenPaw Agent"}
        </Text>
      </View>
      {loading ? (
        <ActivityIndicator color={colors.accent} size="small" />
      ) : active ? (
        <View style={styles.check}>
          <Check color={colors.white} size={14} strokeWidth={3} />
        </View>
      ) : null}
    </Pressable>
  );
});

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Agent 操作失败";
}

const styles = StyleSheet.create({
  root: { flex: 1, backgroundColor: colors.groupedBackground },
  shell: { flex: 1, width: "100%", maxWidth: 760, alignSelf: "center" },
  helper: {
    color: colors.muted,
    fontSize: 13,
    lineHeight: 19,
    paddingHorizontal: spacing.md,
    paddingBottom: spacing.sm,
  },
  search: {
    height: 36,
    flexDirection: "row",
    alignItems: "center",
    gap: 7,
    marginHorizontal: spacing.md,
    marginBottom: spacing.lg,
    paddingHorizontal: 10,
    borderRadius: radius.sm,
    backgroundColor: colors.searchBackground,
  },
  searchInput: { flex: 1, color: colors.ink, fontSize: 15, paddingVertical: 0 },
  sectionTitle: {
    color: colors.muted,
    fontSize: 13,
    marginHorizontal: spacing.md,
    marginBottom: 7,
    textTransform: "uppercase",
  },
  list: { backgroundColor: colors.surface },
  row: {
    minHeight: 72,
    flexDirection: "row",
    alignItems: "center",
    gap: 12,
    paddingHorizontal: spacing.md,
    backgroundColor: colors.surface,
  },
  pressed: { backgroundColor: colors.pressed },
  rowBody: { flex: 1, minWidth: 0, gap: 4 },
  name: { color: colors.ink, fontSize: 16, fontWeight: "600" },
  description: { color: colors.muted, fontSize: 12 },
  check: {
    width: 24,
    height: 24,
    borderRadius: 12,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: colors.accent,
  },
  separator: {
    height: StyleSheet.hairlineWidth,
    marginLeft: 74,
    backgroundColor: colors.hairline,
  },
});
