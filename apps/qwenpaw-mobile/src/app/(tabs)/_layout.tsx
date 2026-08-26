import { Tabs } from "expo-router";
import {
  Aperture,
  Bot,
  MessageCircle,
  UserRound,
} from "lucide-react-native";
import { Platform, StyleSheet } from "react-native";

import { colors } from "../../theme/tokens";

export default function TabLayout() {
  return (
    <Tabs
      screenOptions={{
        animation: "fade",
        headerShown: false,
        tabBarActiveTintColor: colors.accent,
        tabBarInactiveTintColor: colors.faint,
        tabBarLabelStyle: styles.label,
        tabBarStyle: styles.bar,
        tabBarItemStyle: styles.item,
        tabBarHideOnKeyboard: true,
      }}
    >
      <Tabs.Screen
        name="chats"
        options={{
          title: "会话",
          tabBarIcon: ({ color, size }) => (
            <MessageCircle color={color} size={size} strokeWidth={2} />
          ),
        }}
      />
      <Tabs.Screen
        name="agents"
        options={{
          title: "智能体",
          tabBarIcon: ({ color, size }) => (
            <Bot color={color} size={size} strokeWidth={2} />
          ),
        }}
      />
      <Tabs.Screen
        name="community"
        options={{
          title: "社区",
          tabBarIcon: ({ color, size }) => (
            <Aperture color={color} size={size} strokeWidth={2} />
          ),
        }}
      />
      <Tabs.Screen
        name="me"
        options={{
          title: "我的",
          tabBarIcon: ({ color, size }) => (
            <UserRound color={color} size={size} strokeWidth={2} />
          ),
        }}
      />
      <Tabs.Screen name="workbench" options={{ href: null }} />
    </Tabs>
  );
}

const styles = StyleSheet.create({
  bar: {
    height: Platform.OS === "ios" ? 84 : 66,
    paddingTop: 7,
    backgroundColor: colors.tabBar,
    borderTopColor: colors.hairline,
    borderTopWidth: StyleSheet.hairlineWidth,
    shadowColor: colors.black,
    shadowOpacity: 0,
    elevation: 0,
  },
  item: { paddingBottom: Platform.OS === "ios" ? 2 : 8 },
  label: { fontSize: 10, fontWeight: "500" },
});
