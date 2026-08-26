import { Bot } from "lucide-react-native";
import { Image, StyleSheet, View } from "react-native";

import { colors } from "../../theme/tokens";

export function AgentAvatar({
  active = false,
  avatarUri,
  size = 48,
}: {
  active?: boolean;
  avatarUri?: string;
  size?: number;
}) {
  const radius = Math.round(size * 0.3);
  if (avatarUri) {
    return (
      <Image
        accessibilityLabel="智能体头像"
        source={{ uri: avatarUri }}
        style={{ width: size, height: size, borderRadius: radius }}
      />
    );
  }
  return (
    <View style={[
      styles.fallback,
      { width: size, height: size, borderRadius: radius },
      active && styles.active,
    ]}>
      <Bot
        color={active ? colors.white : colors.accent}
        size={Math.round(size * 0.5)}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  fallback: {
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: colors.accentSoft,
  },
  active: { backgroundColor: colors.accent },
});
