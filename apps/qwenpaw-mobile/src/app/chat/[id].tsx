import * as DocumentPicker from "expo-document-picker";
import { router, useLocalSearchParams } from "expo-router";
import {
  ArrowLeft,
  FileUp,
  Send,
  Square,
  Trash2,
  X,
} from "lucide-react-native";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  ActivityIndicator,
  FlatList,
  KeyboardAvoidingView,
  Platform,
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
} from "react-native";
import Markdown from "react-native-markdown-display";
import { SafeAreaView } from "react-native-safe-area-context";

import { QwenPawClient } from "../../api/client";
import type { ContentItem, DisplayMessage } from "../../api/types";
import { useAppStore } from "../../store/app";
import { colors, radius, spacing } from "../../theme/tokens";

interface PendingAttachment {
  name: string;
  content: ContentItem;
}

export default function ChatScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const connection = useAppStore((state) => state.connection);
  const chats = useAppStore((state) => state.chats);
  const messages = useAppStore((state) => state.messages[id] ?? []);
  const activeAbort = useAppStore((state) => state.activeAbort);
  const loadChat = useAppStore((state) => state.loadChat);
  const send = useAppStore((state) => state.send);
  const stop = useAppStore((state) => state.stop);
  const deleteChat = useAppStore((state) => state.deleteChat);
  const [text, setText] = useState("");
  const [attachments, setAttachments] = useState<PendingAttachment[]>([]);
  const [uploading, setUploading] = useState(false);
  const listRef = useRef<FlatList<DisplayMessage>>(null);
  const chat = useMemo(() => chats.find((item) => item.id === id), [chats, id]);

  useEffect(() => {
    if (id) void loadChat(id);
  }, [id, loadChat]);

  useEffect(() => {
    if (messages.length) {
      requestAnimationFrame(() => listRef.current?.scrollToEnd({ animated: true }));
    }
  }, [messages]);

  const pickAttachment = async () => {
    if (!connection) return;
    const result = await DocumentPicker.getDocumentAsync({ multiple: false });
    if (result.canceled) return;
    setUploading(true);
    try {
      const asset = result.assets[0];
      const uploaded = await new QwenPawClient(connection).upload(asset);
      const storedName = uploaded.url.split(/[\\/]/).pop() ?? uploaded.file_name;
      let content: ContentItem;
      if (asset.mimeType?.startsWith("image/")) {
        content = { type: "image", image_url: storedName };
      } else if (asset.mimeType?.startsWith("video/")) {
        content = { type: "video", video_url: storedName };
      } else if (asset.mimeType?.startsWith("audio/")) {
        content = { type: "audio", data: storedName };
      } else {
        content = {
          type: "file",
          file_url: storedName,
          file_name: uploaded.file_name,
        };
      }
      setAttachments((current) => [...current, {
        name: uploaded.file_name,
        content,
      }]);
    } finally {
      setUploading(false);
    }
  };

  const submit = async () => {
    const value = text.trim();
    if (!chat || (!value && !attachments.length) || activeAbort) return;
    setText("");
    const content = attachments.map((item) => item.content);
    setAttachments([]);
    await send(chat, value || "Please review the attached file.", content);
  };

  if (!chat) {
    return <View style={styles.loading}><ActivityIndicator color={colors.accentDark} /></View>;
  }

  return (
    <SafeAreaView style={styles.root} edges={["top", "left", "right"]}>
      <KeyboardAvoidingView behavior={Platform.OS === "ios" ? "padding" : undefined} style={styles.flex}>
        <View style={styles.shell}>
          <View style={styles.header}>
            <Pressable accessibilityLabel="Back" onPress={() => router.back()} style={styles.iconButton}>
              <ArrowLeft size={21} color={colors.ink} />
            </Pressable>
            <View style={styles.headerText}>
              <Text numberOfLines={1} style={styles.title}>{chat.name || "New Chat"}</Text>
              <Text style={styles.status}>{activeAbort ? "QwenPaw is responding" : "Private workspace"}</Text>
            </View>
            <Pressable
              accessibilityLabel="Delete chat"
              onPress={() => void deleteChat(chat.id).then(() => router.back())}
              style={styles.iconButton}
            >
              <Trash2 size={19} color={colors.muted} />
            </Pressable>
          </View>

          <FlatList
            contentContainerStyle={messages.length ? styles.messages : styles.emptyMessages}
            data={messages}
            keyExtractor={(item) => item.id}
            ref={listRef}
            renderItem={({ item }) => <MessageBubble message={item} />}
            ListEmptyComponent={
              <View style={styles.empty}>
                <Text style={styles.emptyTitle}>What are we working on?</Text>
                <Text style={styles.emptyCopy}>Ask, plan, build, or continue where you left off.</Text>
              </View>
            }
          />

          <View style={styles.composerArea}>
            {attachments.length ? (
              <View style={styles.attachments}>
                {attachments.map((attachment) => (
                  <View key={attachment.name} style={styles.attachmentChip}>
                    <Text numberOfLines={1} style={styles.attachmentName}>{attachment.name}</Text>
                    <Pressable onPress={() => setAttachments((items) => items.filter((item) => item.name !== attachment.name))}>
                      <X size={15} color={colors.muted} />
                    </Pressable>
                  </View>
                ))}
              </View>
            ) : null}
            <View style={styles.composer}>
              <Pressable accessibilityLabel="Attach file" onPress={() => void pickAttachment()} style={styles.attachButton}>
                {uploading ? <ActivityIndicator size="small" color={colors.muted} /> : <FileUp size={20} color={colors.muted} />}
              </Pressable>
              <TextInput
                multiline
                onChangeText={setText}
                placeholder="Message QwenPaw"
                placeholderTextColor={colors.faint}
                style={styles.input}
                value={text}
              />
              {activeAbort ? (
                <Pressable accessibilityLabel="Stop" onPress={() => void stop(chat.id)} style={styles.sendButton}>
                  <Square size={17} color={colors.white} fill={colors.white} />
                </Pressable>
              ) : (
                <Pressable accessibilityLabel="Send" disabled={!text.trim() && !attachments.length} onPress={() => void submit()} style={({ pressed }) => [styles.sendButton, pressed && styles.pressed, !text.trim() && !attachments.length && styles.disabled]}>
                  <Send size={18} color={colors.white} />
                </Pressable>
              )}
            </View>
          </View>
        </View>
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
}

function MessageBubble({ message }: { message: DisplayMessage }) {
  const user = message.role === "user";
  return (
    <View style={[styles.bubbleRow, user && styles.userRow]}>
      <View style={[styles.bubble, user ? styles.userBubble : styles.assistantBubble]}>
        {user ? (
          <Text style={styles.userText}>{message.text}</Text>
        ) : message.text ? (
          <Markdown style={markdownStyles}>{message.text}</Markdown>
        ) : (
          <ActivityIndicator size="small" color={colors.accentDark} />
        )}
      </View>
    </View>
  );
}

const markdownStyles = {
  body: { color: colors.ink, fontSize: 15, lineHeight: 23 },
  paragraph: { marginTop: 0, marginBottom: 10 },
  code_inline: { backgroundColor: "#EAE6DF", color: colors.ink, borderRadius: 5, paddingHorizontal: 4 },
  fence: { backgroundColor: colors.black, color: "#E7ECE7", borderColor: colors.black, borderRadius: 12, padding: 12 },
  link: { color: colors.accentDark },
};

const styles = StyleSheet.create({
  root: { flex: 1, backgroundColor: colors.canvas },
  flex: { flex: 1 },
  shell: { flex: 1, width: "100%", maxWidth: 840, alignSelf: "center" },
  loading: { flex: 1, alignItems: "center", justifyContent: "center", backgroundColor: colors.canvas },
  header: { minHeight: 72, flexDirection: "row", alignItems: "center", gap: spacing.md, paddingHorizontal: spacing.lg, borderBottomWidth: 1, borderBottomColor: colors.line },
  iconButton: { width: 42, height: 42, borderRadius: 14, backgroundColor: colors.surfaceStrong, borderWidth: 1, borderColor: colors.line, alignItems: "center", justifyContent: "center" },
  headerText: { flex: 1, minWidth: 0 },
  title: { color: colors.ink, fontSize: 16, fontWeight: "600" },
  status: { color: colors.muted, fontSize: 11, marginTop: 3 },
  messages: { padding: spacing.lg, gap: spacing.md },
  emptyMessages: { flexGrow: 1, justifyContent: "center", padding: spacing.xl },
  empty: { alignItems: "center", gap: spacing.sm },
  emptyTitle: { color: colors.ink, fontSize: 26, fontWeight: "600", letterSpacing: -0.8, textAlign: "center" },
  emptyCopy: { color: colors.muted, fontSize: 14, textAlign: "center" },
  bubbleRow: { alignItems: "flex-start" },
  userRow: { alignItems: "flex-end" },
  bubble: { maxWidth: "88%", borderRadius: radius.lg, paddingHorizontal: spacing.md, paddingVertical: spacing.sm },
  userBubble: { backgroundColor: colors.black, borderBottomRightRadius: 7 },
  assistantBubble: { backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line, borderBottomLeftRadius: 7 },
  userText: { color: colors.white, fontSize: 15, lineHeight: 22 },
  composerArea: { paddingHorizontal: spacing.md, paddingTop: spacing.sm, paddingBottom: Platform.OS === "ios" ? spacing.sm : spacing.md, borderTopWidth: 1, borderTopColor: colors.line, backgroundColor: colors.canvas },
  composer: { minHeight: 58, maxHeight: 150, flexDirection: "row", alignItems: "flex-end", gap: spacing.sm, borderRadius: radius.lg, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surfaceStrong, padding: 7 },
  input: { flex: 1, minHeight: 42, maxHeight: 130, color: colors.ink, fontSize: 15, lineHeight: 21, paddingHorizontal: 4, paddingTop: 10, paddingBottom: 9 },
  attachButton: { width: 42, height: 42, alignItems: "center", justifyContent: "center" },
  sendButton: { width: 42, height: 42, borderRadius: 14, backgroundColor: colors.black, alignItems: "center", justifyContent: "center" },
  pressed: { opacity: 0.75 },
  disabled: { opacity: 0.3 },
  attachments: { flexDirection: "row", flexWrap: "wrap", gap: spacing.xs, paddingBottom: spacing.sm },
  attachmentChip: { maxWidth: 220, height: 34, flexDirection: "row", alignItems: "center", gap: spacing.xs, borderRadius: radius.pill, backgroundColor: colors.accentSoft, paddingHorizontal: spacing.sm },
  attachmentName: { flexShrink: 1, color: colors.accentDark, fontSize: 12, fontWeight: "600" },
});
