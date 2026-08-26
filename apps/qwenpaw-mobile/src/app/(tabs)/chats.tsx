import { router, useFocusEffect } from "expo-router";
import {
  Archive,
  ChevronLeft,
  Ellipsis,
  FolderPlus,
  Inbox,
  MessageCircle,
  Pin,
  Plus,
  Search,
  X,
} from "lucide-react-native";
import { memo, useCallback, useEffect, useMemo, useState } from "react";
import {
  ActionSheetIOS,
  ActivityIndicator,
  Alert,
  Modal,
  Platform,
  Pressable,
  RefreshControl,
  SectionList,
  StyleSheet,
  Text,
  TextInput,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import Animated, {
  cancelAnimation,
  useAnimatedStyle,
  useSharedValue,
  withRepeat,
  withTiming,
} from "react-native-reanimated";

import type { ChatGroup, ChatSpec, Connection } from "../../api/types";
import { AgentAvatar } from "../../features/agents/AgentAvatar";
import { ApprovalInboxSheet } from "../../features/chat/ApprovalInboxSheet";
import {
  buildChatSections,
  type ChatSection,
} from "../../features/chats/grouping";
import { WorkspaceBadge } from "../../features/workspaces/WorkspaceSwitcher";
import { resolveAgentAppearance } from "../../storage/agentAppearance";
import {
  type ChatActivity,
  type ChatActivityMap,
  resolveChatActivity,
} from "../../storage/chatActivity";
import { useAppStore } from "../../store/app";
import { colors, radius, spacing } from "../../theme/tokens";

export default function ChatsScreen() {
  const chats = useAppStore((state) => state.chats);
  const archivedChats = useAppStore((state) => state.archivedChats);
  const groups = useAppStore((state) => state.chatGroups);
  const supportsChatGroups = useAppStore((state) => state.supportsChatGroups);
  const chatActivity = useAppStore((state) => state.chatActivity);
  const agents = useAppStore((state) => state.agents);
  const connection = useAppStore((state) => state.connection);
  const appearances = useAppStore((state) => state.agentAppearances);
  const pinnedChatId = useAppStore((state) => state.pinnedChatId);
  const pendingApprovals = useAppStore((state) => state.pendingApprovals);
  const refreshChats = useAppStore((state) => state.refreshChats);
  const refreshArchivedChats = useAppStore((state) => state.refreshArchivedChats);
  const createChat = useAppStore((state) => state.createChat);
  const createChatGroup = useAppStore((state) => state.createChatGroup);
  const renameChatGroup = useAppStore((state) => state.renameChatGroup);
  const deleteChatGroup = useAppStore((state) => state.deleteChatGroup);
  const moveChatToGroup = useAppStore((state) => state.moveChatToGroup);
  const archiveChat = useAppStore((state) => state.archiveChat);
  const unarchiveChat = useAppStore((state) => state.unarchiveChat);
  const deleteChat = useAppStore((state) => state.deleteChat);
  const setPinnedChat = useAppStore((state) => state.setPinnedChat);
  const refreshApprovals = useAppStore((state) => state.refreshApprovals);
  const approveRequest = useAppStore((state) => state.approveRequest);
  const denyRequest = useAppStore((state) => state.denyRequest);
  const [query, setQuery] = useState("");
  const [refreshing, setRefreshing] = useState(false);
  const [creating, setCreating] = useState(false);
  const [showArchived, setShowArchived] = useState(false);
  const [groupEditorOpen, setGroupEditorOpen] = useState(false);
  const [groupName, setGroupName] = useState("");
  const [savingGroup, setSavingGroup] = useState(false);
  const [editingGroup, setEditingGroup] = useState<ChatGroup | null>(null);
  const [approvalInboxOpen, setApprovalInboxOpen] = useState(false);

  const activeAgent = agents.find((agent) => agent.id === connection?.agentId);
  const agentAppearance = resolveAgentAppearance(
    appearances,
    connection,
    activeAgent,
  );
  const sourceChats = showArchived ? archivedChats : chats;
  const filteredChats = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    if (!normalized) return sourceChats;
    return sourceChats.filter((chat) =>
      (chat.name || "新会话").toLocaleLowerCase().includes(normalized));
  }, [query, sourceChats]);
  const sections = useMemo<ChatSection[]>(() => showArchived
    ? [{ key: "archived", title: "已归档", data: filteredChats }]
    : buildChatSections(filteredChats, groups, pinnedChatId), [
      filteredChats,
      groups,
      pinnedChatId,
      showArchived,
    ]);

  const refresh = useCallback(async () => {
    setRefreshing(true);
    const action = showArchived ? refreshArchivedChats : refreshChats;
    await action().catch(() => undefined);
    setRefreshing(false);
  }, [refreshArchivedChats, refreshChats, showArchived]);

  useFocusEffect(useCallback(() => {
    let busy = false;
    const poll = async () => {
      if (busy) return;
      busy = true;
      await Promise.all([
        refreshChats().catch(() => undefined),
        refreshApprovals().catch(() => undefined),
      ]);
      busy = false;
    };
    void poll();
    const timer = setInterval(() => void poll(), 4000);
    return () => clearInterval(timer);
  }, [refreshApprovals, refreshChats]));

  const create = async () => {
    setCreating(true);
    try {
      const chat = await createChat();
      router.push({ pathname: "/chat/[id]", params: { id: chat.id } });
    } finally {
      setCreating(false);
    }
  };

  const saveGroup = async () => {
    const name = groupName.trim();
    if (!name) return;
    const duplicate = groups.some((group) =>
      group.id !== editingGroup?.id &&
      group.name.trim().toLocaleLowerCase() === name.toLocaleLowerCase());
    if (duplicate) {
      Alert.alert("分组名称已存在", "请换一个容易区分的名称。");
      return;
    }
    setSavingGroup(true);
    try {
      if (editingGroup) await renameChatGroup(editingGroup.id, name);
      else await createChatGroup(name);
      setGroupName("");
      setEditingGroup(null);
      setGroupEditorOpen(false);
    } catch (error) {
      Alert.alert(editingGroup ? "无法重命名分组" : "无法创建分组", errorMessage(error));
    } finally {
      setSavingGroup(false);
    }
  };

  const openGroupEditor = (group: ChatGroup | null) => {
    setEditingGroup(group);
    setGroupName(group?.name ?? "");
    setGroupEditorOpen(true);
  };

  const confirmDeleteGroup = (group: ChatGroup) => {
    Alert.alert(
      `删除“${group.name}”？`,
      "分组里的会话不会删除，会自动移到“未分组”。",
      [
        { text: "取消", style: "cancel" },
        {
          text: "删除分组",
          style: "destructive",
          onPress: () => void deleteChatGroup(group.id).catch((error) => {
            Alert.alert("删除失败", errorMessage(error));
          }),
        },
      ],
    );
  };

  const openGroupActions = (group: ChatGroup) => {
    if (group.kind !== "custom") return;
    const edit = () => openGroupEditor(group);
    const remove = () => confirmDeleteGroup(group);
    if (Platform.OS === "ios") {
      ActionSheetIOS.showActionSheetWithOptions({
        options: ["重命名分组", "删除分组", "取消"],
        cancelButtonIndex: 2,
        destructiveButtonIndex: 1,
        title: group.name,
      }, (index) => {
        if (index === 0) edit();
        if (index === 1) remove();
      });
      return;
    }
    Alert.alert(group.name, undefined, [
      { text: "重命名分组", onPress: edit },
      { text: "删除分组", style: "destructive", onPress: remove },
      { text: "取消", style: "cancel" },
    ]);
  };

  const confirmDelete = (chat: ChatSpec) => {
    Alert.alert("删除会话？", "此操作无法撤销。", [
      { text: "取消", style: "cancel" },
      {
        text: "删除",
        style: "destructive",
        onPress: () => void deleteChat(chat.id).catch((error) => {
          Alert.alert("删除失败", errorMessage(error));
        }),
      },
    ]);
  };

  const openGroupPicker = (chat: ChatSpec) => {
    const options = ["未分组", ...groups.map((group) => group.name), "取消"];
    const apply = (index: number) => {
      if (index >= options.length - 1) return;
      const groupId = index === 0 ? null : groups[index - 1].id;
      void moveChatToGroup(chat.id, groupId).catch((error) => {
        Alert.alert("移动失败", errorMessage(error));
      });
    };
    if (Platform.OS === "ios") {
      ActionSheetIOS.showActionSheetWithOptions({
        options,
        cancelButtonIndex: options.length - 1,
        title: "移动到分组",
      }, apply);
      return;
    }
    Alert.alert("移动到分组", undefined, options.map((label, index) => ({
      text: label,
      style: index === options.length - 1 ? "cancel" : "default",
      onPress: () => apply(index),
    })));
  };

  const openChatActions = (chat: ChatSpec) => {
    if (showArchived) {
      const restore = () => void unarchiveChat(chat.id).catch((error) => {
        Alert.alert("恢复失败", errorMessage(error));
      });
      if (Platform.OS === "ios") {
        ActionSheetIOS.showActionSheetWithOptions({
          options: ["恢复到会话", "永久删除", "取消"],
          cancelButtonIndex: 2,
          destructiveButtonIndex: 1,
          title: chat.name || "会话操作",
        }, (index) => {
          if (index === 0) restore();
          if (index === 1) confirmDelete(chat);
        });
      } else {
        Alert.alert(chat.name || "会话操作", undefined, [
          { text: "恢复到会话", onPress: restore },
          { text: "永久删除", style: "destructive", onPress: () => confirmDelete(chat) },
          { text: "取消", style: "cancel" },
        ]);
      }
      return;
    }

    const pinned = chat.id === pinnedChatId;
    const pin = () => void setPinnedChat(pinned ? null : chat.id);
    const move = () => setTimeout(() => openGroupPicker(chat), 240);
    const archiveAction = () => void archiveChat(chat.id).catch((error) => {
      Alert.alert("归档失败", errorMessage(error));
    });
    if (Platform.OS === "ios") {
      if (!supportsChatGroups) {
        ActionSheetIOS.showActionSheetWithOptions({
          options: [
            pinned ? "取消置顶" : "置顶会话",
            "归档会话",
            "删除会话",
            "取消",
          ],
          cancelButtonIndex: 3,
          destructiveButtonIndex: 2,
          title: chat.name || "会话操作",
        }, (index) => {
          if (index === 0) pin();
          if (index === 1) archiveAction();
          if (index === 2) confirmDelete(chat);
        });
        return;
      }
      ActionSheetIOS.showActionSheetWithOptions({
        options: [
          pinned ? "取消置顶" : "置顶会话",
          "移动到分组",
          "归档会话",
          "删除会话",
          "取消",
        ],
        cancelButtonIndex: 4,
        destructiveButtonIndex: 3,
        title: chat.name || "会话操作",
      }, (index) => {
        if (index === 0) pin();
        if (index === 1) move();
        if (index === 2) archiveAction();
        if (index === 3) confirmDelete(chat);
      });
      return;
    }
    Alert.alert(chat.name || "会话操作", undefined, [
      { text: pinned ? "取消置顶" : "置顶会话", onPress: pin },
      ...(supportsChatGroups ? [{ text: "移动到分组", onPress: move }] : []),
      { text: "归档会话", onPress: archiveAction },
      { text: "删除会话", style: "destructive", onPress: () => confirmDelete(chat) },
      { text: "取消", style: "cancel" },
    ]);
  };

  return (
    <SafeAreaView edges={["top"]} style={styles.root}>
      <View style={styles.shell}>
        <View style={styles.header}>
          {showArchived ? (
            <Pressable
              accessibilityLabel="返回会话"
              onPress={() => setShowArchived(false)}
              style={styles.headerButton}
            >
              <ChevronLeft color={colors.ink} size={25} />
            </Pressable>
          ) : <WorkspaceBadge />}
          {showArchived ? <Text style={styles.archiveTitle}>已归档</Text> : null}
          <View style={styles.headerActions}>
            {!showArchived ? (
              <>
                <Pressable
                  accessibilityLabel={`已归档 ${archivedChats.length} 个会话`}
                  onPress={() => setShowArchived(true)}
                  style={styles.headerButton}
                >
                  <Archive color={colors.ink} size={21} />
                  {archivedChats.length ? (
                    <View style={styles.badge}><Text style={styles.badgeText}>{Math.min(99, archivedChats.length)}</Text></View>
                  ) : null}
                </Pressable>
                {supportsChatGroups ? (
                  <Pressable
                    accessibilityLabel="新建分组"
                    onPress={() => openGroupEditor(null)}
                    style={styles.headerButton}
                  >
                    <FolderPlus color={colors.ink} size={21} />
                  </Pressable>
                ) : null}
                <Pressable
                  accessibilityLabel="新建会话"
                  onPress={() => void create()}
                  style={styles.headerButton}
                >
                  {creating ? <ActivityIndicator color={colors.accent} size="small" /> : <Plus color={colors.ink} size={24} />}
                </Pressable>
              </>
            ) : <View style={styles.headerButton} />}
          </View>
        </View>

        <View style={styles.search}>
          <Search color={colors.faint} size={17} />
          <TextInput
            clearButtonMode="while-editing"
            onChangeText={setQuery}
            placeholder={showArchived ? "搜索已归档会话" : "搜索会话"}
            placeholderTextColor={colors.faint}
            returnKeyType="search"
            style={styles.searchInput}
            value={query}
          />
        </View>

        {!showArchived ? (
          <Pressable
            onPress={() => setApprovalInboxOpen(true)}
            style={({ pressed }) => [
              styles.inboxCard,
              pendingApprovals.length > 0 && styles.inboxCardActive,
              pressed && styles.rowPressed,
            ]}
          >
            <View style={styles.inboxIcon}>
              <Inbox color={colors.accentDark} size={19} />
            </View>
            <View style={styles.inboxText}>
              <Text style={styles.inboxTitle}>审批 Inbox</Text>
              <Text style={styles.inboxSubtitle}>
                {pendingApprovals.length
                  ? `${pendingApprovals.length} 项工具操作等待确认`
                  : "所有需要确认的操作都会集中在这里"}
              </Text>
            </View>
            <View style={[
              styles.inboxCount,
              !pendingApprovals.length && styles.inboxCountEmpty,
            ]}>
              <Text style={styles.inboxCountText}>{pendingApprovals.length}</Text>
            </View>
          </Pressable>
        ) : null}

        <SectionList
          contentContainerStyle={sections.length ? styles.list : styles.emptyList}
          sections={sections}
          keyExtractor={(item) => item.id}
          keyboardShouldPersistTaps="handled"
          ListEmptyComponent={sections.length
            ? null
            : <EmptyChats archived={showArchived} filtered={Boolean(query.trim())} />}
          refreshControl={<RefreshControl onRefresh={() => void refresh()} refreshing={refreshing} tintColor={colors.accent} />}
          renderSectionHeader={({ section }) => (
            <View style={styles.sectionHeader}>
              {section.pinned ? <Pin color={colors.accent} fill={colors.accent} size={12} /> : null}
              <Text style={styles.sectionTitle}>{section.title}</Text>
              <Text style={styles.sectionCount}>{section.data.length}</Text>
              {!section.data.length ? <Text style={styles.emptyGroup}>空分组</Text> : null}
              <View style={styles.sectionSpacer} />
              {section.group?.kind === "custom" ? (
                <Pressable
                  accessibilityLabel={`${section.title}分组操作`}
                  hitSlop={8}
                  onPress={() => openGroupActions(section.group!)}
                  style={styles.sectionAction}
                >
                  <Ellipsis color={colors.faint} size={17} />
                </Pressable>
              ) : null}
            </View>
          )}
          renderItem={({ item, section, index }) => (
            <ChatRow
              appearance={agentAppearance}
              activity={chatActivity}
              chat={item}
              connection={connection}
              last={index === section.data.length - 1}
              onActions={() => openChatActions(item)}
              pinned={item.id === pinnedChatId}
            />
          )}
          stickySectionHeadersEnabled={false}
        />

        <Modal animationType="fade" onRequestClose={() => setGroupEditorOpen(false)} transparent visible={groupEditorOpen}>
          <View style={styles.modalMask}>
            <Pressable onPress={() => { setGroupEditorOpen(false); setEditingGroup(null); }} style={StyleSheet.absoluteFill} />
            <View style={styles.modalCard}>
              <View style={styles.modalHeader}>
                <Text style={styles.modalTitle}>{editingGroup ? "重命名分组" : "新建会话分组"}</Text>
                <Pressable accessibilityLabel="关闭" onPress={() => { setGroupEditorOpen(false); setEditingGroup(null); }} style={styles.modalClose}>
                  <X color={colors.ink} size={19} />
                </Pressable>
              </View>
              <TextInput
                autoFocus
                maxLength={40}
                onChangeText={setGroupName}
                onSubmitEditing={() => void saveGroup()}
                placeholder="例如：产品研发"
                placeholderTextColor={colors.faint}
                returnKeyType="done"
                style={styles.groupInput}
                value={groupName}
              />
              <Pressable
                disabled={!groupName.trim() || savingGroup}
                onPress={() => void saveGroup()}
                style={[styles.groupSave, (!groupName.trim() || savingGroup) && styles.disabled]}
              >
                <Text style={styles.groupSaveText}>{savingGroup ? "保存中…" : editingGroup ? "保存" : "创建"}</Text>
              </Pressable>
            </View>
          </View>
        </Modal>
        <ApprovalInboxSheet
          approvals={pendingApprovals}
          chats={[...chats, ...archivedChats]}
          onApprove={approveRequest}
          onClose={() => setApprovalInboxOpen(false)}
          onDeny={denyRequest}
          visible={approvalInboxOpen}
        />
      </View>
    </SafeAreaView>
  );
}

const ChatRow = memo(function ChatRow({
  appearance,
  activity,
  chat,
  connection,
  last,
  onActions,
  pinned,
}: {
  appearance: { name: string; avatarUri?: string };
  activity: ChatActivityMap;
  chat: ChatSpec;
  connection: Connection | null;
  last: boolean;
  onActions: () => void;
  pinned: boolean;
}) {
  const chatActivity = resolveChatActivity(connection, chat, activity);
  return (
    <Pressable
      delayLongPress={320}
      onLongPress={onActions}
      onPress={() => router.push({ pathname: "/chat/[id]", params: { id: chat.id } })}
      style={({ pressed }) => [styles.row, pressed && styles.rowPressed]}
    >
      <View style={styles.avatarWrap}>
        <AgentAvatar active={pinned} avatarUri={appearance.avatarUri} size={48} />
        <ActivityDot activity={chatActivity} />
      </View>
      <View style={[styles.rowBody, !last && styles.rowDivider]}>
        <View style={styles.rowTop}>
          <View style={styles.rowTitleWrap}>
            <Text numberOfLines={1} style={styles.rowTitle}>{chat.name || "新会话"}</Text>
            {pinned ? <Pin color={colors.accent} fill={colors.accent} size={12} /> : null}
          </View>
          <Text style={styles.time}>{formatTime(chat.updated_at)}</Text>
          <Pressable
            accessibilityLabel="会话操作"
            hitSlop={7}
            onPress={(event) => { event.stopPropagation(); onActions(); }}
            style={styles.more}
          >
            <Ellipsis color={colors.faint} size={19} />
          </Pressable>
        </View>
        <Text numberOfLines={1} style={styles.preview}>
          {activityLabel(chatActivity, appearance.name)}
        </Text>
      </View>
    </Pressable>
  );
});

function ActivityDot({ activity }: { activity: ChatActivity }) {
  const pulse = useSharedValue(1);
  useEffect(() => {
    if (activity === "running") {
      pulse.value = withRepeat(withTiming(1.8, { duration: 900 }), -1, true);
      return () => cancelAnimation(pulse);
    }
    cancelAnimation(pulse);
    pulse.value = 1;
    return undefined;
  }, [activity, pulse]);
  const pulseStyle = useAnimatedStyle(() => ({
    opacity: Math.max(0, 1.25 - pulse.value * 0.55),
    transform: [{ scale: pulse.value }],
  }));
  const color = activity === "running"
    ? "#2F80ED"
    : activity === "unread"
      ? "#34C759"
      : "#A9A5A1";
  return (
    <View style={styles.activitySlot}>
      {activity === "running" ? (
        <Animated.View style={[styles.activityPulse, { backgroundColor: color }, pulseStyle]} />
      ) : null}
      <View style={[
        styles.activityDot,
        { backgroundColor: activity === "idle" ? colors.surface : color },
        activity === "idle" && styles.activityIdle,
      ]} />
    </View>
  );
}

function activityLabel(activity: ChatActivity, agentName: string): string {
  if (activity === "running") return `${agentName} 正在回复…`;
  if (activity === "unread") return "新回复 · 未读";
  if (activity === "read") return "已读";
  return "尚未开始";
}

function EmptyChats({ archived, filtered }: { archived: boolean; filtered: boolean }) {
  const title = filtered ? "没有匹配的会话" : archived ? "还没有归档会话" : "开始第一次对话";
  const copy = filtered ? "换个关键词再试试。" : archived ? "长按会话即可将它归档。" : "点击右上角加号创建会话。";
  return (
    <View style={styles.empty}>
      <View style={styles.emptyIcon}>
        {archived ? <Archive color={colors.accent} size={27} /> : <MessageCircle color={colors.accent} size={28} strokeWidth={1.8} />}
      </View>
      <Text style={styles.emptyTitle}>{title}</Text>
      <Text style={styles.emptyCopy}>{copy}</Text>
    </View>
  );
}

function formatTime(value?: string | null): string {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  const today = new Date();
  if (date.toDateString() === today.toDateString()) {
    return new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit" }).format(date);
  }
  return new Intl.DateTimeFormat(undefined, { month: "numeric", day: "numeric" }).format(date);
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "请稍后重试。";
}

const styles = StyleSheet.create({
  root: { flex: 1, backgroundColor: colors.groupedBackground },
  shell: { flex: 1, width: "100%", maxWidth: 760, alignSelf: "center" },
  header: { height: 52, flexDirection: "row", alignItems: "center", paddingHorizontal: spacing.sm },
  title: { flex: 1, marginLeft: spacing.sm, color: colors.ink, fontSize: 21, fontWeight: "700", letterSpacing: -0.45 },
  archiveTitle: { flex: 1, color: colors.ink, textAlign: "center", fontSize: 17, fontWeight: "700" },
  headerActions: { flexDirection: "row", alignItems: "center" },
  headerButton: { width: 40, height: 40, alignItems: "center", justifyContent: "center" },
  badge: { position: "absolute", right: 0, top: 1, minWidth: 16, height: 16, paddingHorizontal: 3, alignItems: "center", justifyContent: "center", borderRadius: 8, backgroundColor: colors.accent },
  badgeText: { color: colors.white, fontSize: 9, fontWeight: "700" },
  search: { height: 36, flexDirection: "row", alignItems: "center", gap: 7, marginHorizontal: spacing.md, marginBottom: spacing.sm, paddingHorizontal: 10, borderRadius: radius.sm, backgroundColor: colors.searchBackground },
  searchInput: { flex: 1, color: colors.ink, fontSize: 15, paddingVertical: 0 },
  inboxCard: {
    minHeight: 62,
    flexDirection: "row",
    alignItems: "center",
    gap: spacing.sm,
    marginHorizontal: spacing.md,
    marginBottom: spacing.xs,
    borderWidth: 1,
    borderColor: colors.line,
    borderRadius: radius.md,
    backgroundColor: colors.surface,
    paddingHorizontal: spacing.sm,
  },
  inboxCardActive: { borderColor: "#F0BE96", backgroundColor: "#FFFBF7" },
  inboxIcon: {
    width: 38,
    height: 38,
    alignItems: "center",
    justifyContent: "center",
    borderRadius: 12,
    backgroundColor: colors.accentSoft,
  },
  inboxText: { flex: 1, minWidth: 0 },
  inboxTitle: { color: colors.ink, fontSize: 14, fontWeight: "700" },
  inboxSubtitle: { marginTop: 3, color: colors.muted, fontSize: 11 },
  inboxCount: {
    minWidth: 25,
    height: 25,
    alignItems: "center",
    justifyContent: "center",
    borderRadius: 13,
    backgroundColor: colors.accent,
    paddingHorizontal: 6,
  },
  inboxCountEmpty: { backgroundColor: colors.searchBackground },
  inboxCountText: { color: colors.white, fontSize: 11, fontWeight: "800" },
  list: { paddingBottom: spacing.xl },
  emptyList: { flexGrow: 1, justifyContent: "center", paddingBottom: 90 },
  sectionHeader: { minHeight: 35, flexDirection: "row", alignItems: "center", gap: 6, paddingHorizontal: spacing.md, paddingTop: 8, backgroundColor: colors.groupedBackground },
  sectionTitle: { color: colors.muted, fontSize: 12, fontWeight: "600" },
  sectionCount: { color: colors.faint, fontSize: 11 },
  emptyGroup: { paddingHorizontal: 6, paddingVertical: 2, borderRadius: 5, color: colors.faint, backgroundColor: colors.searchBackground, fontSize: 9, fontWeight: "600" },
  sectionSpacer: { flex: 1 },
  sectionAction: { width: 30, height: 30, alignItems: "center", justifyContent: "center" },
  row: { minHeight: 74, flexDirection: "row", alignItems: "center", gap: 12, paddingLeft: spacing.md, backgroundColor: colors.surface },
  rowPressed: { backgroundColor: colors.pressed },
  avatarWrap: { position: "relative" },
  activitySlot: { position: "absolute", right: -2, bottom: -2, width: 15, height: 15, alignItems: "center", justifyContent: "center", borderRadius: 8, backgroundColor: colors.surface },
  activityPulse: { position: "absolute", width: 8, height: 8, borderRadius: 4 },
  activityDot: { width: 8, height: 8, borderRadius: 4, borderWidth: 1.5, borderColor: colors.surface },
  activityIdle: { borderColor: "#A9A5A1" },
  rowBody: { flex: 1, minWidth: 0, alignSelf: "stretch", justifyContent: "center", gap: 5, paddingRight: 7 },
  rowDivider: { borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.hairline },
  rowTop: { flexDirection: "row", alignItems: "center", gap: 6 },
  rowTitleWrap: { flex: 1, minWidth: 0, flexDirection: "row", alignItems: "center", gap: 5 },
  rowTitle: { flexShrink: 1, color: colors.ink, fontSize: 16, fontWeight: "500" },
  time: { color: colors.faint, fontSize: 11 },
  more: { width: 32, height: 32, alignItems: "center", justifyContent: "center" },
  preview: { color: colors.muted, fontSize: 13 },
  empty: { alignItems: "center", paddingHorizontal: spacing.xl },
  emptyIcon: { width: 58, height: 58, borderRadius: 18, backgroundColor: colors.accentSoft, alignItems: "center", justifyContent: "center", marginBottom: spacing.md },
  emptyTitle: { color: colors.ink, fontSize: 18, fontWeight: "600" },
  emptyCopy: { color: colors.muted, fontSize: 14, marginTop: spacing.xs },
  modalMask: { flex: 1, alignItems: "center", justifyContent: "center", padding: spacing.lg, backgroundColor: "rgba(20, 15, 12, 0.32)" },
  modalCard: { width: "100%", maxWidth: 420, padding: spacing.lg, borderRadius: radius.lg, backgroundColor: colors.surfaceStrong },
  modalHeader: { flexDirection: "row", alignItems: "center", marginBottom: spacing.md },
  modalTitle: { flex: 1, color: colors.ink, fontSize: 19, fontWeight: "700" },
  modalClose: { width: 36, height: 36, alignItems: "center", justifyContent: "center" },
  groupInput: { height: 50, paddingHorizontal: spacing.md, borderWidth: 1, borderColor: colors.line, borderRadius: radius.md, color: colors.ink, backgroundColor: colors.surface, fontSize: 16 },
  groupSave: { height: 48, alignItems: "center", justifyContent: "center", marginTop: spacing.md, borderRadius: radius.md, backgroundColor: colors.accent },
  groupSaveText: { color: colors.white, fontSize: 15, fontWeight: "700" },
  disabled: { opacity: 0.35 },
});
