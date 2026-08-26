import {
  AudioLines,
  Download,
  FileText,
  Film,
  Share2,
} from "lucide-react-native";
import { useRef, useState } from "react";
import {
  ActionSheetIOS,
  ActivityIndicator,
  Alert,
  Image,
  Modal,
  Platform,
  Pressable,
  StyleSheet,
  Text,
  View,
} from "react-native";
import Markdown from "react-native-markdown-display";
import { SafeAreaView } from "react-native-safe-area-context";

import { mediaSource } from "../../api/client";
import type { Connection, DisplayPart } from "../../api/types";
import { colors, radius, spacing } from "../../theme/tokens";
import { saveImageToLibrary, shareMedia } from "./mediaActions";

type MediaPart = Exclude<DisplayPart, { type: "text" }>;

export function MessageParts({
  connection,
  compact = false,
  parts,
  user = false,
}: {
  connection: Connection;
  compact?: boolean;
  parts: DisplayPart[];
  user?: boolean;
}) {
  const [preview, setPreview] = useState<MediaPart | null>(null);
  const [busy, setBusy] = useState(false);
  const longPressed = useRef(false);

  const runAction = async (action: "save" | "share", part: MediaPart) => {
    setBusy(true);
    try {
      if (action === "save" && part.type === "image") {
        await saveImageToLibrary(connection, part);
        Alert.alert("已保存", "图片已保存到照片。");
      } else {
        await shareMedia(connection, part);
      }
    } catch (error) {
      Alert.alert(
        action === "save" ? "保存失败" : "分享失败",
        error instanceof Error ? error.message : "请稍后重试。",
      );
    } finally {
      setBusy(false);
    }
  };

  const showActions = (part: MediaPart) => {
    const options = part.type === "image"
      ? ["保存到照片", "分享", "取消"]
      : ["分享文件", "取消"];
    const apply = (index: number) => {
      if (part.type === "image" && index === 0) void runAction("save", part);
      if (part.type === "image" && index === 1) void runAction("share", part);
      if (part.type !== "image" && index === 0) void runAction("share", part);
    };
    if (Platform.OS === "ios") {
      ActionSheetIOS.showActionSheetWithOptions({
        options,
        cancelButtonIndex: options.length - 1,
        title: part.name || "媒体操作",
      }, apply);
      return;
    }
    Alert.alert(part.name || "媒体操作", undefined, options.map((label, index) => ({
      text: label,
      style: index === options.length - 1 ? "cancel" : "default",
      onPress: () => apply(index),
    })));
  };
  return (
    <>
      <View style={styles.parts}>
        {parts.map((part, index) => {
          if (part.type === "text") {
            return user ? (
              <Text key={`text-${index}`} style={styles.userText}>{part.text}</Text>
            ) : (
              <Markdown key={`text-${index}`} style={markdownStyles}>{part.text}</Markdown>
            );
          }
          if (part.type === "image") {
            return (
              <Pressable
                delayLongPress={320}
                key={`${part.type}-${part.url}`}
                onLongPress={(event) => {
                  event.stopPropagation();
                  longPressed.current = true;
                  showActions(part);
                }}
                onPress={(event) => {
                  event.stopPropagation();
                  if (longPressed.current) {
                    longPressed.current = false;
                    return;
                  }
                  setPreview(part);
                }}
                style={[styles.imageFrame, compact && styles.imageFrameCompact]}
              >
                <Image
                  resizeMode="cover"
                  source={mediaSource(connection, part.url)}
                  style={styles.image}
                />
              </Pressable>
            );
          }
          return (
            <Pressable
              delayLongPress={320}
              key={`${part.type}-${part.url}`}
              onLongPress={(event) => {
                event.stopPropagation();
                longPressed.current = true;
                showActions(part);
              }}
              onPress={(event) => {
                event.stopPropagation();
                if (longPressed.current) {
                  longPressed.current = false;
                  return;
                }
                void runAction("share", part);
              }}
              style={[styles.fileCard, user && styles.userFileCard]}
            >
              <View style={[styles.fileIcon, user && styles.userFileIcon]}>
                {part.type === "video" ? (
                  <Film color={user ? colors.white : colors.accentDark} size={19} />
                ) : part.type === "audio" ? (
                  <AudioLines color={user ? colors.white : colors.accentDark} size={19} />
                ) : (
                  <FileText color={user ? colors.white : colors.accentDark} size={19} />
                )}
              </View>
              <View style={styles.fileBody}>
                <Text numberOfLines={1} style={[styles.fileName, user && styles.userFileText]}>
                  {part.name || mediaLabel(part.type)}
                </Text>
                <Text style={[styles.fileMeta, user && styles.userFileMeta]}>
                  {mediaLabel(part.type)} · 点击打开
                </Text>
              </View>
            </Pressable>
          );
        })}
      </View>
      <Modal animationType="fade" onRequestClose={() => setPreview(null)} visible={Boolean(preview)}>
        <Pressable
          accessibilityLabel="轻点关闭图片，长按显示保存选项"
          delayLongPress={420}
          onLongPress={() => {
            if (!preview) return;
            longPressed.current = true;
            showActions(preview);
          }}
          onPress={() => {
            if (longPressed.current) {
              longPressed.current = false;
              return;
            }
            setPreview(null);
          }}
          style={styles.previewRoot}
        >
          <SafeAreaView pointerEvents="none" style={styles.previewSafeArea}>
            <View style={styles.previewHint}>
              <Download color={colors.white} size={15} />
              <Text style={styles.previewHintText}>轻点退出 · 长按保存或分享</Text>
              <Share2 color={colors.white} size={15} />
            </View>
            {preview?.type === "image" ? (
              <Image
                resizeMode="contain"
                source={mediaSource(connection, preview.url)}
                style={styles.fullImage}
              />
            ) : null}
            {busy ? (
              <View style={styles.busy}>
                <ActivityIndicator color={colors.white} />
              </View>
            ) : null}
          </SafeAreaView>
        </Pressable>
      </Modal>
    </>
  );
}

function mediaLabel(type: DisplayPart["type"]): string {
  if (type === "video") return "视频";
  if (type === "audio") return "音频";
  if (type === "image") return "图片";
  return "文件";
}

const markdownStyles = {
  body: { color: colors.ink, fontSize: 15, lineHeight: 24 },
  paragraph: { marginTop: 0, marginBottom: 10 },
  code_inline: { backgroundColor: "#EAE6DF", color: colors.ink, borderRadius: 5, paddingHorizontal: 4 },
  fence: { backgroundColor: colors.black, color: "#E7ECE7", borderColor: colors.black, borderRadius: 12, padding: 12 },
  link: { color: colors.accentDark },
};

const styles = StyleSheet.create({
  parts: { gap: spacing.sm },
  userText: { color: colors.white, fontSize: 15, lineHeight: 22 },
  imageFrame: { width: "100%", aspectRatio: 1.28, overflow: "hidden", borderRadius: radius.md, backgroundColor: colors.pressed },
  imageFrameCompact: { maxHeight: 180, aspectRatio: 1.8 },
  image: { width: "100%", height: "100%" },
  fileCard: { minHeight: 62, flexDirection: "row", alignItems: "center", gap: 10, padding: 10, borderRadius: radius.md, backgroundColor: colors.accentSoft },
  userFileCard: { backgroundColor: "rgba(255,255,255,0.16)" },
  fileIcon: { width: 40, height: 40, alignItems: "center", justifyContent: "center", borderRadius: 12, backgroundColor: colors.surfaceStrong },
  userFileIcon: { backgroundColor: "rgba(255,255,255,0.15)" },
  fileBody: { flex: 1, minWidth: 0, gap: 3 },
  fileName: { color: colors.ink, fontSize: 13, fontWeight: "600" },
  fileMeta: { color: colors.muted, fontSize: 10 },
  userFileText: { color: colors.white },
  userFileMeta: { color: "rgba(255,255,255,0.72)" },
  previewRoot: { flex: 1, backgroundColor: colors.black },
  previewSafeArea: { flex: 1 },
  previewHint: { position: "absolute", zIndex: 2, top: 14, alignSelf: "center", flexDirection: "row", alignItems: "center", gap: 8, paddingHorizontal: 12, paddingVertical: 8, borderRadius: 18, backgroundColor: "rgba(20,20,20,0.72)" },
  previewHintText: { color: colors.white, fontSize: 11, fontWeight: "600" },
  fullImage: { flex: 1, width: "100%" },
  busy: { position: "absolute", inset: 0, alignItems: "center", justifyContent: "center", backgroundColor: "rgba(0,0,0,0.42)" },
});
