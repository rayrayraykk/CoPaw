import { Card, Button, Tag, Avatar } from "antd";
import {
  MessageCircle,
  Hash,
  Send,
  MessageSquare,
  Mail,
  Eye,
  CheckCheck,
} from "lucide-react";
import { motion } from "framer-motion";
import type { PushMessage } from "../types";
import styles from "./PushMessageCard.module.less";

interface PushMessageCardProps {
  message: PushMessage;
  onMarkAsRead: (id: string) => void;
  onView: (id: string) => void;
}

const CHANNEL_ICONS = {
  wechat: MessageCircle,
  slack: Hash,
  telegram: Send,
  discord: MessageSquare,
  email: Mail,
};

const CHANNEL_COLORS = {
  wechat: "#07C160",
  slack: "#4A154B",
  telegram: "#0088CC",
  discord: "#5865F2",
  email: "#EA4335",
};

export const PushMessageCard: React.FC<PushMessageCardProps> = ({
  message,
  onMarkAsRead,
  onView,
}) => {
  const IconComponent = CHANNEL_ICONS[message.channelType];
  const channelColor = CHANNEL_COLORS[message.channelType];

  const formatTime = () => {
    const diff = Date.now() - message.createdAt.getTime();
    const hours = Math.floor(diff / (1000 * 60 * 60));
    const minutes = Math.floor(diff / (1000 * 60));

    if (hours >= 24) {
      const days = Math.floor(hours / 24);
      return `${days} days ago`;
    }
    if (hours > 0) return `${hours} hours ago`;
    if (minutes > 0) return `${minutes} minutes ago`;
    return "Just now";
  };

  const getPriorityTag = () => {
    if (!message.metadata?.priority || message.metadata.priority === "normal")
      return null;

    const colors = {
      low: "default",
      high: "warning",
      urgent: "error",
    };

    return (
      <Tag color={colors[message.metadata.priority]}>
        {message.metadata.priority.toUpperCase()}
      </Tag>
    );
  };

  return (
    <motion.div
      initial={{ opacity: 0, x: -20 }}
      animate={{ opacity: 1, x: 0 }}
      transition={{ duration: 0.2 }}
    >
      <Card
        className={`${styles.messageCard} ${!message.read ? styles.unread : ""}`}
        hoverable
      >
        <div className={styles.cardHeader}>
          <div className={styles.channelInfo}>
            <Avatar
              size={40}
              style={{ backgroundColor: channelColor }}
              icon={<IconComponent size={20} />}
            />
            <div className={styles.channelDetails}>
              <div className={styles.channelName}>{message.channelName}</div>
              <div className={styles.senderInfo}>
                From: {message.sender.username}
              </div>
            </div>
          </div>
          <div className={styles.headerRight}>
            {!message.read && <div className={styles.unreadDot} />}
            {getPriorityTag()}
          </div>
        </div>

        <div className={styles.cardBody}>
          <h4 className={styles.messageTitle}>{message.title}</h4>
          <p className={styles.messageContent}>{message.content}</p>
        </div>

        <div className={styles.cardFooter}>
          <span className={styles.timestamp}>{formatTime()}</span>
          <div className={styles.actions}>
            <Button
              type="link"
              icon={<Eye size={16} />}
              onClick={() => onView(message.id)}
            >
              View Details
            </Button>
            {!message.read && (
              <Button
                type="primary"
                ghost
                icon={<CheckCheck size={16} />}
                onClick={() => onMarkAsRead(message.id)}
              >
                Mark as Read
              </Button>
            )}
          </div>
        </div>
      </Card>
    </motion.div>
  );
};
