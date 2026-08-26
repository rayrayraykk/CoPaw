import { router } from "expo-router";
import {
  RefreshCw,
  Sparkles,
  SquarePen,
} from "lucide-react-native";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import {
  ActivityIndicator,
  Alert,
  FlatList,
  Pressable,
  RefreshControl,
  Share,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { getPlatformAccessToken } from "../../api/platform";
import { IosHeader } from "../../components/IosHeader";
import {
  communityArticleUrl,
  getCommunityMeta,
  likeCommunityArticle,
  listCommunityArticles,
} from "../../features/community/api";
import { CommunityPostCard } from "../../features/community/components/CommunityPostCard";
import type {
  CommunityArticleSummary,
  CommunityArticleType,
  CommunitySort,
} from "../../features/community/types";
import { colors, radius, spacing } from "../../theme/tokens";

const PAGE_SIZE = 12;

export default function CommunityScreen() {
  const [articles, setArticles] = useState<CommunityArticleSummary[]>([]);
  const [types, setTypes] = useState<CommunityArticleType[]>([]);
  const [selectedType, setSelectedType] = useState("");
  const [sort, setSort] = useState<CommunitySort>("recommended");
  const [page, setPage] = useState(1);
  const [hasMore, setHasMore] = useState(true);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestVersion = useRef(0);

  useEffect(() => {
    void getCommunityMeta()
      .then((meta) => setTypes(meta.filter_types))
      .catch(() => undefined);
  }, []);

  const loadFirstPage = useCallback(async (refresh = false) => {
    const version = requestVersion.current + 1;
    requestVersion.current = version;
    if (refresh) setRefreshing(true);
    else setLoading(true);
    setError(null);
    try {
      const result = await listCommunityArticles({
        page: 1,
        pageSize: PAGE_SIZE,
        sort,
        type: selectedType || undefined,
      });
      if (version !== requestVersion.current) return;
      setArticles(result.items);
      setPage(1);
      setHasMore(result.items.length < result.total);
    } catch (caught) {
      if (version !== requestVersion.current) return;
      setArticles([]);
      setError(caught instanceof Error ? caught.message : "社区加载失败");
    } finally {
      if (version === requestVersion.current) {
        setLoading(false);
        setRefreshing(false);
      }
    }
  }, [selectedType, sort]);

  useEffect(() => {
    const timer = setTimeout(() => void loadFirstPage(), 0);
    return () => clearTimeout(timer);
  }, [loadFirstPage]);

  const loadMore = useCallback(async () => {
    if (!hasMore || loading || loadingMore || refreshing) return;
    setLoadingMore(true);
    try {
      const nextPage = page + 1;
      const result = await listCommunityArticles({
        page: nextPage,
        pageSize: PAGE_SIZE,
        sort,
        type: selectedType || undefined,
      });
      setArticles((current) => {
        const known = new Set(current.map((article) => article.id));
        return [
          ...current,
          ...result.items.filter((article) => !known.has(article.id)),
        ];
      });
      setPage(nextPage);
      setHasMore(nextPage * PAGE_SIZE < result.total);
    } catch {
      setHasMore(false);
    } finally {
      setLoadingMore(false);
    }
  }, [hasMore, loading, loadingMore, page, refreshing, selectedType, sort]);

  const openArticle = useCallback((articleId: string) => {
    router.push({
      pathname: "/community/[id]",
      params: { id: articleId },
    });
  }, []);

  const shareArticle = useCallback((article: CommunityArticleSummary) => {
    void Share.share({
      message: `${article.title}\n${communityArticleUrl(article.id)}`,
      url: communityArticleUrl(article.id),
    });
  }, []);

  const openComposer = useCallback(async () => {
    const token = await getPlatformAccessToken();
    if (token) {
      router.push("/community/compose");
      return;
    }
    router.push({
      pathname: "/community/login",
      params: { returnTo: "compose" },
    });
  }, []);

  const interactArticle = useCallback(async (
    article: CommunityArticleSummary,
  ) => {
    const token = await getPlatformAccessToken();
    if (!token) {
      Alert.alert(
        "登录后点赞",
        "登录 AgentScope Platform 后，操作会实时同步到社区。",
        [
          { text: "取消", style: "cancel" },
          {
            text: "登录",
            onPress: () => router.push("/community/login"),
          },
        ],
      );
      return;
    }
    try {
      const updated = await likeCommunityArticle(article.id);
      setArticles((current) => current.map((item) => item.id === article.id
        ? {
          ...item,
          liked: updated.liked,
          like_count: updated.like_count,
        }
        : item));
    } catch (caught) {
      Alert.alert(
        "点赞失败",
        caught instanceof Error ? caught.message : "请稍后重试",
      );
    }
  }, []);

  const renderArticle = useCallback(({
    item,
  }: {
    item: CommunityArticleSummary;
  }) => (
    <CommunityPostCard
      article={item}
      onInteract={() => void interactArticle(item)}
      onOpen={() => openArticle(item.id)}
      onShare={() => shareArticle(item)}
    />
  ), [interactArticle, openArticle, shareArticle]);

  return (
    <SafeAreaView edges={["top"]} style={styles.root}>
      <View style={styles.shell}>
        <IosHeader
          actionIcon={SquarePen}
          actionLabel="发布社区文章"
          onAction={() => void openComposer()}
          title="社区"
        />
        <View style={styles.sortTabs}>
          <SortButton
            active={sort === "recommended"}
            label="推荐"
            onPress={() => setSort("recommended")}
          />
          <SortButton
            active={sort === "latest"}
            label="最新"
            onPress={() => setSort("latest")}
          />
        </View>
        <View style={styles.categoryBar}>
          <FlatList
            contentContainerStyle={styles.categoryContent}
            data={[{ code: "", label: "全部" }, ...types]}
            horizontal
            keyExtractor={(item) => item.code || "all"}
            renderItem={({ item }) => (
              <Pressable
                onPress={() => setSelectedType(item.code)}
                style={[
                  styles.category,
                  selectedType === item.code && styles.activeCategory,
                ]}
              >
                <Text style={[
                  styles.categoryLabel,
                  selectedType === item.code && styles.activeCategoryLabel,
                ]}>
                  {item.label}
                </Text>
              </Pressable>
            )}
            showsHorizontalScrollIndicator={false}
          />
        </View>

        {loading ? (
          <View style={styles.center}>
            <ActivityIndicator color={colors.accent} />
            <Text style={styles.stateCopy}>正在读取 Platform 社区</Text>
          </View>
        ) : error ? (
          <View style={styles.center}>
            <View style={styles.stateIcon}>
              <RefreshCw color={colors.accent} size={25} />
            </View>
            <Text style={styles.stateTitle}>暂时无法加载社区</Text>
            <Text style={styles.stateCopy}>{error}</Text>
            <Pressable
              onPress={() => void loadFirstPage()}
              style={styles.retry}
            >
              <Text style={styles.retryLabel}>重新加载</Text>
            </Pressable>
          </View>
        ) : (
          <FlatList
            contentContainerStyle={articles.length ? styles.list : styles.emptyList}
            data={articles}
            ItemSeparatorComponent={() => <View style={styles.separator} />}
            keyExtractor={(item) => item.id}
            ListEmptyComponent={(
              <View style={styles.center}>
                <Sparkles color={colors.accent} size={28} />
                <Text style={styles.stateTitle}>这个分类还没有内容</Text>
              </View>
            )}
            ListFooterComponent={loadingMore ? (
              <ActivityIndicator color={colors.accent} style={styles.footer} />
            ) : null}
            onEndReached={() => void loadMore()}
            onEndReachedThreshold={0.45}
            refreshControl={(
              <RefreshControl
                onRefresh={() => void loadFirstPage(true)}
                refreshing={refreshing}
                tintColor={colors.accent}
              />
            )}
            removeClippedSubviews
            renderItem={renderArticle}
            windowSize={7}
          />
        )}
      </View>
    </SafeAreaView>
  );
}

function SortButton({
  active,
  label,
  onPress,
}: {
  active: boolean;
  label: string;
  onPress: () => void;
}) {
  return (
    <Pressable onPress={onPress} style={styles.sortButton}>
      <Text style={[styles.sortLabel, active && styles.activeSortLabel]}>
        {label}
      </Text>
      {active ? <View style={styles.sortIndicator} /> : null}
    </Pressable>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1, backgroundColor: colors.groupedBackground },
  shell: { flex: 1, width: "100%", maxWidth: 760, alignSelf: "center" },
  sortTabs: {
    height: 42,
    flexDirection: "row",
    justifyContent: "center",
    gap: 42,
    backgroundColor: colors.surface,
  },
  sortButton: {
    minWidth: 52,
    alignItems: "center",
    justifyContent: "center",
  },
  sortLabel: { color: colors.muted, fontSize: 14 },
  activeSortLabel: { color: colors.ink, fontWeight: "600" },
  sortIndicator: {
    position: "absolute",
    bottom: 3,
    width: 18,
    height: 3,
    borderRadius: 2,
    backgroundColor: colors.accent,
  },
  categoryBar: {
    paddingVertical: 9,
    backgroundColor: colors.surface,
    borderTopWidth: StyleSheet.hairlineWidth,
    borderTopColor: colors.line,
  },
  categoryContent: { gap: 7, paddingHorizontal: spacing.md },
  category: {
    height: 29,
    justifyContent: "center",
    paddingHorizontal: 11,
    borderRadius: radius.pill,
    backgroundColor: colors.groupedBackground,
  },
  activeCategory: { backgroundColor: colors.accentSoft },
  categoryLabel: { color: colors.muted, fontSize: 11 },
  activeCategoryLabel: { color: colors.accentDark, fontWeight: "600" },
  list: { paddingBottom: spacing.lg },
  emptyList: { flexGrow: 1 },
  separator: { height: 8, backgroundColor: colors.groupedBackground },
  center: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    paddingHorizontal: spacing.xl,
    paddingBottom: 80,
  },
  stateIcon: {
    width: 54,
    height: 54,
    alignItems: "center",
    justifyContent: "center",
    marginBottom: spacing.md,
    borderRadius: 17,
    backgroundColor: colors.accentSoft,
  },
  stateTitle: {
    marginTop: spacing.sm,
    color: colors.ink,
    fontSize: 17,
    fontWeight: "600",
    textAlign: "center",
  },
  stateCopy: {
    marginTop: spacing.xs,
    color: colors.muted,
    fontSize: 13,
    lineHeight: 19,
    textAlign: "center",
  },
  retry: {
    minWidth: 108,
    height: 40,
    alignItems: "center",
    justifyContent: "center",
    marginTop: spacing.md,
    borderRadius: radius.sm,
    backgroundColor: colors.accent,
  },
  retryLabel: { color: colors.white, fontSize: 13, fontWeight: "600" },
  footer: { paddingVertical: spacing.lg },
});
