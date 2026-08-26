import { router } from "expo-router";
import { Bot, Cloud, Search, Server } from "lucide-react-native";
import { useMemo, useState } from "react";
import {
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { IosHeader } from "../../components/IosHeader";
import { IosGroup, IosRow } from "../../components/IosList";
import { workbenchSections } from "../../features/workbench/modules";
import { resolveAgentAppearance } from "../../storage/agentAppearance";
import { useAppStore } from "../../store/app";
import { colors, radius, spacing } from "../../theme/tokens";

export default function WorkbenchScreen() {
  const [query, setQuery] = useState("");
  const connection = useAppStore((state) => state.connection);
  const agents = useAppStore((state) => state.agents);
  const appearances = useAppStore((state) => state.agentAppearances);
  const agent = agents.find((item) => item.id === connection?.agentId);
  const appearance = resolveAgentAppearance(appearances, connection, agent);
  const visibleSections = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    if (!normalized) return workbenchSections;
    return workbenchSections.flatMap((section) => {
      const modules = section.modules.filter((module) => (
        [module.title, module.subtitle, ...(module.keywords ?? [])]
          .join(" ")
          .toLocaleLowerCase()
          .includes(normalized)
      ));
      return modules.length ? [{ ...section, modules }] : [];
    });
  }, [query]);

  const openModule = (key: string) => {
    if (key === "sessions") {
      router.push("/chats");
      return;
    }
    router.push({ pathname: "/module/[key]", params: { key } });
  };

  return (
    <SafeAreaView edges={["top"]} style={styles.root}>
      <IosHeader title="工作台" />
      <ScrollView
        contentContainerStyle={styles.content}
        keyboardDismissMode="on-drag"
        keyboardShouldPersistTaps="handled"
      >
        <View style={styles.search}>
          <Search color={colors.faint} size={17} />
          <TextInput
            clearButtonMode="while-editing"
            onChangeText={setQuery}
            placeholder="搜索设置"
            placeholderTextColor={colors.faint}
            style={styles.searchInput}
            value={query}
          />
        </View>

        {connection ? (
          <View style={styles.scopeCard}>
            <View style={styles.scopeIcon}>
              {connection.source === "platform" ? (
                <Cloud color={colors.white} size={23} />
              ) : (
                <Server color={colors.white} size={23} />
              )}
            </View>
            <View style={styles.scopeBody}>
              <Text style={styles.scopeTitle}>
                {connection.source === "platform" ? "Platform 云端" : "本地 / 私人"}
              </Text>
              <Text numberOfLines={1} style={styles.scopeSubtitle}>
                {appearance.name} · 所有设置只作用于当前 QwenPaw
              </Text>
            </View>
            <View style={styles.online} />
          </View>
        ) : null}

        {visibleSections.map((section) => (
          <IosGroup key={section.title} title={section.title}>
            {section.modules.map((module) => (
              <IosRow
                icon={module.icon}
                iconTone={module.iconTone}
                key={module.key}
                label={module.title}
                onPress={() => openModule(module.key)}
                subtitle={module.subtitle}
              />
            ))}
          </IosGroup>
        ))}

        {!visibleSections.length ? (
          <View style={styles.empty}>
            <Bot color={colors.faint} size={26} />
            <Text style={styles.emptyTitle}>没有匹配的设置</Text>
            <Text style={styles.emptyCopy}>换一个关键词试试。</Text>
          </View>
        ) : null}
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
  search: {
    height: 38,
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
    paddingHorizontal: 11,
    borderRadius: radius.sm,
    backgroundColor: colors.searchBackground,
  },
  searchInput: { flex: 1, color: colors.ink, fontSize: 15, paddingVertical: 0 },
  scopeCard: {
    minHeight: 74,
    flexDirection: "row",
    alignItems: "center",
    gap: spacing.sm,
    padding: spacing.md,
    borderRadius: radius.md,
    backgroundColor: colors.surface,
  },
  scopeIcon: {
    width: 44,
    height: 44,
    borderRadius: 13,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: colors.accent,
  },
  scopeBody: { flex: 1, minWidth: 0, gap: 3 },
  scopeTitle: { color: colors.ink, fontSize: 16, fontWeight: "600" },
  scopeSubtitle: { color: colors.muted, fontSize: 12 },
  online: { width: 9, height: 9, borderRadius: 5, backgroundColor: "#34C759" },
  empty: { alignItems: "center", gap: 6, paddingVertical: spacing.xxl },
  emptyTitle: { color: colors.ink, fontSize: 16, fontWeight: "600" },
  emptyCopy: { color: colors.muted, fontSize: 13 },
});
