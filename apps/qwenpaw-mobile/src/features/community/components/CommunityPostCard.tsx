import {
  BadgeCheck,
  Eye,
  Heart,
  MessageCircle,
  Send,
} from "lucide-react-native";
import { memo } from "react";
import {
  Image,
  Pressable,
  StyleSheet,
  Text,
  View,
} from "react-native";

import { colors, radius, spacing } from "../../../theme/tokens";
import {
  formatCommunityDate,
  normalizeCommunityText,
} from "../model";
import type { CommunityArticleSummary } from "../types";
import { CommunityAvatar } from "./CommunityAvatar";

export const CommunityPostCard = memo(function CommunityPostCard({
  article,
  onInteract,
  onOpen,
  onShare,
}: {
  article: CommunityArticleSummary;
  onInteract: () => void;
  onOpen: () => void;
  onShare: () => void;
}) {
  const official = article.identity_badge?.code === "official";
  const summary = normalizeCommunityText(article.summary);

  return (
    <View style={styles.post}>
      <View style={styles.authorRow}>
        <CommunityAvatar
          name={article.author_name}
          uri={article.author_avatar_url}
          verified={official}
        />
        <View style={styles.authorCopy}>
          <View style={styles.authorNameRow}>
            <Text numberOfLines={1} style={styles.authorName}>
              {article.author_name || "社区用户"}
            </Text>
            {article.identity_badge ? (
              <BadgeCheck
                color={colors.accent}
                fill={colors.accentSoft}
                size={14}
              />
            ) : null}
          </View>
          <Text numberOfLines={1} style={styles.authorMeta}>
            {formatCommunityDate(article.published_at)} · {article.article_type_label}
          </Text>
        </View>
        {article.is_featured ? (
          <View style={styles.featuredBadge}>
            <Text style={styles.featuredLabel}>精选</Text>
          </View>
        ) : null}
      </View>

      <Pressable
        accessibilityRole="button"
        onPress={onOpen}
        style={({ pressed }) => [styles.body, pressed && styles.pressed]}
      >
        <Text style={styles.title}>{normalizeCommunityText(article.title)}</Text>
        {summary ? (
          <Text numberOfLines={4} style={styles.summary}>{summary}</Text>
        ) : null}
        {article.cover_url ? (
          <Image
            resizeMode="cover"
            source={{ uri: article.cover_url }}
            style={styles.cover}
          />
        ) : null}
        <MetaTags article={article} />
      </Pressable>

      <View style={styles.actions}>
        <PostAction
          active={article.liked}
          icon={Heart}
          label={String(article.like_count)}
          onPress={onInteract}
        />
        <PostAction
          icon={MessageCircle}
          label={String(article.comment_count)}
          onPress={onOpen}
        />
        <PostAction
          icon={Eye}
          label={String(article.view_count)}
          onPress={onOpen}
        />
        <PostAction icon={Send} label="分享" onPress={onShare} />
      </View>
    </View>
  );
});

function MetaTags({ article }: { article: CommunityArticleSummary }) {
  const tags = article.tags.slice(0, 3);
  const related = [
    article.related_skill_ids.length
      ? `${article.related_skill_ids.length} 个 Skill`
      : "",
    article.related_plugin_ids.length
      ? `${article.related_plugin_ids.length} 个 Plugin`
      : "",
  ].filter(Boolean);
  if (!tags.length && !related.length && !article.qa_status) return null;
  return (
    <View style={styles.tags}>
      {article.qa_status ? (
        <Text style={[styles.tag, styles.statusTag]}>
          {article.qa_status === "solved" ? "已解决" : "待回答"}
        </Text>
      ) : null}
      {tags.map((tag) => (
        <Text key={tag} style={styles.tag}>#{normalizeCommunityText(tag)}</Text>
      ))}
      {related.map((label) => (
        <Text key={label} style={[styles.tag, styles.resourceTag]}>{label}</Text>
      ))}
    </View>
  );
}

function PostAction({
  active = false,
  icon: Icon,
  label,
  onPress,
}: {
  active?: boolean;
  icon: typeof Heart;
  label: string;
  onPress: () => void;
}) {
  const color = active ? colors.accentDark : colors.muted;
  return (
    <Pressable
      accessibilityRole="button"
      hitSlop={4}
      onPress={onPress}
      style={({ pressed }) => [styles.action, pressed && styles.pressed]}
    >
      <Icon
        color={color}
        fill={active ? color : "transparent"}
        size={16}
        strokeWidth={1.8}
      />
      <Text style={[styles.actionLabel, active && styles.activeAction]}>
        {label}
      </Text>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  post: {
    paddingHorizontal: spacing.md,
    paddingTop: 15,
    backgroundColor: colors.surface,
  },
  authorRow: { flexDirection: "row", alignItems: "center", gap: spacing.sm },
  authorCopy: { flex: 1, minWidth: 0, gap: 2 },
  authorNameRow: { flexDirection: "row", alignItems: "center", gap: 4 },
  authorName: { flexShrink: 1, color: colors.ink, fontSize: 14, fontWeight: "600" },
  authorMeta: { color: colors.faint, fontSize: 11 },
  featuredBadge: {
    paddingHorizontal: 8,
    paddingVertical: 4,
    borderRadius: radius.pill,
    backgroundColor: colors.accentSoft,
  },
  featuredLabel: { color: colors.accentDark, fontSize: 10, fontWeight: "600" },
  body: { marginLeft: 52, paddingTop: 10 },
  title: { color: colors.ink, fontSize: 16, fontWeight: "600", lineHeight: 23 },
  summary: { marginTop: 7, color: colors.muted, fontSize: 13, lineHeight: 20 },
  cover: {
    width: "100%",
    aspectRatio: 1.75,
    marginTop: 11,
    borderRadius: 13,
    backgroundColor: colors.groupedBackground,
  },
  tags: { flexDirection: "row", flexWrap: "wrap", gap: 6, marginTop: 10 },
  tag: {
    paddingHorizontal: 7,
    paddingVertical: 4,
    overflow: "hidden",
    borderRadius: 7,
    color: colors.accentDark,
    backgroundColor: colors.accentSoft,
    fontSize: 10,
  },
  statusTag: { color: colors.white, backgroundColor: colors.accent },
  resourceTag: { color: colors.ink, backgroundColor: colors.searchBackground },
  actions: {
    minHeight: 42,
    flexDirection: "row",
    marginLeft: 52,
    marginTop: 10,
    borderTopWidth: StyleSheet.hairlineWidth,
    borderTopColor: colors.line,
  },
  action: {
    flex: 1,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: 4,
  },
  actionLabel: { color: colors.muted, fontSize: 10 },
  activeAction: { color: colors.accentDark },
  pressed: { opacity: 0.48 },
});
