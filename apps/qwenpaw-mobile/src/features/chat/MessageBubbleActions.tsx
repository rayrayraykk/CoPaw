import * as Clipboard from "expo-clipboard";
import { Check, Copy, X } from "lucide-react-native";
import type { ReactNode } from "react";
import { useState } from "react";
import {
  ActionSheetIOS,
  Alert,
  Modal,
  Platform,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  type StyleProp,
  View,
  type ViewStyle,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { colors, radius, spacing } from "../../theme/tokens";

export function MessageBubbleActions({
  children,
  style,
  text,
}: {
  children: ReactNode;
  style?: StyleProp<ViewStyle>;
  text: string;
}) {
  const [selecting, setSelecting] = useState(false);
  const [copied, setCopied] = useState(false);

  if (!text) return <View style={style}>{children}</View>;

  const copy = async () => {
    await Clipboard.setStringAsync(text);
    setCopied(true);
  };

  const apply = (index: number) => {
    if (index === 0) void copy();
    if (index === 1) {
      setCopied(false);
      setSelecting(true);
    }
  };

  const showActions = () => {
    const options = ["复制", "选择文本", "取消"];
    if (Platform.OS === "ios") {
      ActionSheetIOS.showActionSheetWithOptions({
        options,
        cancelButtonIndex: 2,
        title: "消息操作",
      }, apply);
      return;
    }
    Alert.alert("消息操作", undefined, options.map((label, index) => ({
      text: label,
      style: index === 2 ? "cancel" : "default",
      onPress: () => apply(index),
    })));
  };

  return (
    <>
      <Pressable
        accessibilityHint="长按可复制或选择文本"
        delayLongPress={320}
        onLongPress={showActions}
        style={({ pressed }) => [style, pressed && styles.pressed]}
      >
        {children}
      </Pressable>
      <Modal
        animationType="slide"
        onRequestClose={() => setSelecting(false)}
        presentationStyle="pageSheet"
        visible={selecting}
      >
        <SafeAreaView edges={["bottom"]} style={styles.modalRoot}>
          <View style={styles.header}>
            <View style={styles.headerAction} />
            <Text style={styles.headerTitle}>选择文本</Text>
            <Pressable
              accessibilityLabel="关闭"
              hitSlop={8}
              onPress={() => setSelecting(false)}
              style={styles.headerAction}
            >
              <X color={colors.ink} size={21} />
            </Pressable>
          </View>
          <ScrollView
            contentContainerStyle={styles.selectionContent}
            showsVerticalScrollIndicator={false}
          >
            <Text selectable selectionColor={colors.accent} style={styles.selectionText}>
              {text}
            </Text>
          </ScrollView>
          <View style={styles.footer}>
            <Pressable
              onPress={() => void copy()}
              style={({ pressed }) => [styles.copyButton, pressed && styles.pressed]}
            >
              {copied ? (
                <Check color={colors.white} size={18} />
              ) : (
                <Copy color={colors.white} size={18} />
              )}
              <Text style={styles.copyButtonText}>
                {copied ? "已复制" : "复制全文"}
              </Text>
            </Pressable>
          </View>
        </SafeAreaView>
      </Modal>
    </>
  );
}

const styles = StyleSheet.create({
  pressed: { opacity: 0.82 },
  modalRoot: { flex: 1, backgroundColor: colors.canvas },
  header: {
    height: 54,
    flexDirection: "row",
    alignItems: "center",
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: colors.hairline,
  },
  headerAction: {
    width: 54,
    height: 54,
    alignItems: "center",
    justifyContent: "center",
  },
  headerTitle: {
    flex: 1,
    color: colors.ink,
    fontSize: 17,
    fontWeight: "600",
    textAlign: "center",
  },
  selectionContent: { padding: spacing.lg, paddingBottom: 120 },
  selectionText: {
    padding: spacing.md,
    borderWidth: 1,
    borderColor: colors.line,
    borderRadius: radius.lg,
    backgroundColor: colors.surfaceStrong,
    color: colors.ink,
    fontSize: 17,
    lineHeight: 27,
  },
  footer: {
    position: "absolute",
    right: 0,
    bottom: 0,
    left: 0,
    padding: spacing.md,
    backgroundColor: colors.canvas,
    borderTopWidth: StyleSheet.hairlineWidth,
    borderTopColor: colors.hairline,
  },
  copyButton: {
    minHeight: 50,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: 8,
    borderRadius: radius.md,
    backgroundColor: colors.accent,
  },
  copyButtonText: { color: colors.white, fontSize: 15, fontWeight: "700" },
});
