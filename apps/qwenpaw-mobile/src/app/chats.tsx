import { router } from "expo-router";
import {
  Bot,
  ChevronRight,
  LogOut,
  MessageSquare,
  Plus,
  RefreshCw,
} from "lucide-react-native";
import { useCallback, useEffect, useState } from "react";
import {
  ActivityIndicator,
  FlatList,
  Pressable,
  RefreshControl,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import type { ChatSpec } from "../api/types";
import { useAppStore } from "../store/app";
import { colors, radius, spacing } from "../theme/tokens";

export default function ChatsScreen() {
  const connection = useAppStore((state) => state.connection);
  const agents = useAppStore((state) => state.agents);
  const chats = useAppStore((state) => state.chats);
  const selectAgent = useAppStore((state) => state.selectAgent);
  const refreshChats = useAppStore((state) => state.refreshChats);
  const createChat = useAppStore((state) => state.createChat);
  const disconnect = useAppStore((state) => state.disconnect);
  const [refreshing, setRefreshing] = useState(false);
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    if (!connection) router.replace("/");
  }, [connection]);

  const refresh = useCallback(async () => {
    setRefreshing(true);
    await refreshChats().catch(() => undefined);
    setRefreshing(false);
  }, [refreshChats]);

  const create = async () => {
    setCreating(true);
    try {
      const chat = await createChat();
      router.push({ pathname: "/chat/[id]", params: { id: chat.id } });
    } finally {
      setCreating(false);
    }
  };

  return (
    <SafeAreaView style={styles.root}>
      <View style={styles.shell}>
        <View style={styles.header}>
          <View>
            <Text style={styles.eyebrow}>QWENPAW MOBILE</Text>
            <Text style={styles.title}>Conversations</Text>
          </View>
          <View style={styles.headerActions}>
            <Pressable accessibilityLabel="Refresh" onPress={() => void refresh()} style={styles.iconButton}>
              <RefreshCw size={19} color={colors.ink} />
            </Pressable>
            <Pressable accessibilityLabel="Disconnect" onPress={() => void disconnect()} style={styles.iconButton}>
              <LogOut size={19} color={colors.ink} />
            </Pressable>
          </View>
        </View>

        <View style={styles.connectionCard}>
          <View style={styles.connectionIcon}><Bot size={21} color={colors.accentDark} /></View>
          <View style={styles.connectionText}>
            <Text style={styles.connectionTitle} numberOfLines={1}>{connection?.baseUrl}</Text>
            <Text style={styles.connectionMeta}>Connected securely</Text>
          </View>
          <View style={styles.onlineDot} />
        </View>

        <Text style={styles.sectionLabel}>Agent</Text>
        <FlatList
          contentContainerStyle={styles.agentList}
          data={agents}
          horizontal
          keyExtractor={(item) => item.id}
          renderItem={({ item }) => {
            const active = item.id === connection?.agentId;
            return (
              <Pressable
                onPress={() => void selectAgent(item.id)}
                style={[styles.agentChip, active && styles.agentChipActive]}
              >
                <Text style={[styles.agentName, active && styles.agentNameActive]}>{item.name}</Text>
              </Pressable>
            );
          }}
          showsHorizontalScrollIndicator={false}
        />

        <View style={styles.listHeader}>
          <Text style={styles.sectionLabel}>Recent</Text>
          <Pressable onPress={() => void create()} style={styles.newButton}>
            {creating ? <ActivityIndicator size="small" color={colors.white} /> : <Plus size={18} color={colors.white} />}
            <Text style={styles.newButtonText}>New chat</Text>
          </Pressable>
        </View>

        <FlatList
          contentContainerStyle={chats.length ? styles.chatList : styles.emptyList}
          data={chats}
          keyExtractor={(item) => item.id}
          refreshControl={<RefreshControl refreshing={refreshing} onRefresh={() => void refresh()} tintColor={colors.accentDark} />}
          renderItem={({ item }) => <ChatRow chat={item} />}
          ItemSeparatorComponent={() => <View style={styles.separator} />}
          ListEmptyComponent={
            <View style={styles.empty}>
              <MessageSquare size={30} color={colors.accentDark} />
              <Text style={styles.emptyTitle}>A quiet beginning</Text>
              <Text style={styles.emptyCopy}>Start a conversation with your QwenPaw.</Text>
            </View>
          }
        />
      </View>
    </SafeAreaView>
  );
}

function ChatRow({ chat }: { chat: ChatSpec }) {
  const date = chat.updated_at
    ? new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" }).format(new Date(chat.updated_at))
    : "";
  return (
    <Pressable
      onPress={() => router.push({ pathname: "/chat/[id]", params: { id: chat.id } })}
      style={({ pressed }) => [styles.chatRow, pressed && styles.rowPressed]}
    >
      <View style={styles.chatIcon}><MessageSquare size={19} color={colors.accentDark} /></View>
      <View style={styles.chatText}>
        <Text numberOfLines={1} style={styles.chatTitle}>{chat.name || "New Chat"}</Text>
        <Text style={styles.chatMeta}>{chat.status === "running" ? "Responding" : date}</Text>
      </View>
      <ChevronRight size={18} color={colors.faint} />
    </Pressable>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1, backgroundColor: colors.canvas },
  shell: { flex: 1, width: "100%", maxWidth: 760, alignSelf: "center", paddingHorizontal: spacing.lg },
  header: { flexDirection: "row", alignItems: "flex-end", justifyContent: "space-between", paddingTop: spacing.lg, paddingBottom: spacing.lg },
  eyebrow: { color: colors.accentDark, fontSize: 10, letterSpacing: 1.8, fontWeight: "700", marginBottom: spacing.xs },
  title: { color: colors.ink, fontSize: 36, fontWeight: "600", letterSpacing: -1.5 },
  headerActions: { flexDirection: "row", gap: spacing.sm },
  iconButton: { width: 44, height: 44, borderRadius: 15, backgroundColor: colors.surfaceStrong, borderWidth: 1, borderColor: colors.line, alignItems: "center", justifyContent: "center" },
  connectionCard: { minHeight: 76, flexDirection: "row", alignItems: "center", gap: spacing.md, backgroundColor: colors.surface, borderRadius: radius.lg, borderWidth: 1, borderColor: colors.line, padding: spacing.md, marginBottom: spacing.lg },
  connectionIcon: { width: 42, height: 42, borderRadius: 14, backgroundColor: colors.accentSoft, alignItems: "center", justifyContent: "center" },
  connectionText: { flex: 1, minWidth: 0 },
  connectionTitle: { color: colors.ink, fontSize: 14, fontWeight: "600" },
  connectionMeta: { color: colors.muted, fontSize: 12, marginTop: 3 },
  onlineDot: { width: 9, height: 9, borderRadius: 5, backgroundColor: colors.accent },
  sectionLabel: { color: colors.muted, fontSize: 11, fontWeight: "700", letterSpacing: 1.2, textTransform: "uppercase" },
  agentList: { gap: spacing.sm, paddingVertical: spacing.sm, paddingBottom: spacing.lg },
  agentChip: { height: 38, borderRadius: radius.pill, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface, paddingHorizontal: spacing.md, justifyContent: "center" },
  agentChipActive: { backgroundColor: colors.black, borderColor: colors.black },
  agentName: { color: colors.muted, fontSize: 13, fontWeight: "600" },
  agentNameActive: { color: colors.white },
  listHeader: { flexDirection: "row", alignItems: "center", justifyContent: "space-between", marginBottom: spacing.sm },
  newButton: { height: 40, borderRadius: radius.pill, backgroundColor: colors.black, paddingHorizontal: spacing.md, flexDirection: "row", alignItems: "center", gap: spacing.xs },
  newButtonText: { color: colors.white, fontSize: 13, fontWeight: "600" },
  chatList: { paddingBottom: spacing.xxl },
  emptyList: { flexGrow: 1, justifyContent: "center", paddingBottom: 100 },
  chatRow: { minHeight: 76, flexDirection: "row", alignItems: "center", gap: spacing.md, paddingVertical: spacing.sm },
  chatIcon: { width: 42, height: 42, borderRadius: 14, backgroundColor: colors.accentSoft, alignItems: "center", justifyContent: "center" },
  chatText: { flex: 1 },
  chatTitle: { color: colors.ink, fontSize: 16, fontWeight: "600", letterSpacing: -0.2 },
  chatMeta: { color: colors.faint, fontSize: 12, marginTop: 4 },
  separator: { height: 1, backgroundColor: colors.line, marginLeft: 58 },
  rowPressed: { opacity: 0.55 },
  empty: { alignItems: "center", gap: spacing.sm },
  emptyTitle: { color: colors.ink, fontSize: 20, fontWeight: "600", marginTop: spacing.sm },
  emptyCopy: { color: colors.muted, fontSize: 14 },
});
