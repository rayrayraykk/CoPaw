import { Card, Button, Tag, Tooltip } from "antd";
import { Terminal, FileText, Settings, Check, X } from "lucide-react";
import { motion } from "framer-motion";
import type { ApprovalItem } from "../types";
import styles from "./ApprovalCard.module.less";

interface ApprovalCardProps {
  approval: ApprovalItem;
  onApprove: (id: string) => void;
  onReject: (id: string) => void;
}

const TYPE_ICONS = {
  tool_call: Terminal,
  config_change: Settings,
  file_access: FileText,
};

const TYPE_LABELS = {
  tool_call: "Tool Call",
  config_change: "Config Change",
  file_access: "File Access",
};

const PRIORITY_COLORS = {
  low: "default",
  normal: "processing",
  high: "warning",
  urgent: "error",
};

export const ApprovalCard: React.FC<ApprovalCardProps> = ({
  approval,
  onApprove,
  onReject,
}) => {
  const IconComponent = TYPE_ICONS[approval.type];

  const formatTime = () => {
    const diff = Date.now() - approval.requestedAt.getTime();
    const minutes = Math.floor(diff / (1000 * 60));

    if (minutes < 1) return "Just now";
    if (minutes < 60) return `${minutes} minutes ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours} hours ago`;
    const days = Math.floor(hours / 24);
    return `${days} days ago`;
  };

  return (
    <motion.div
      initial={{ opacity: 0, scale: 0.95 }}
      animate={{ opacity: 1, scale: 1 }}
      transition={{ duration: 0.2 }}
    >
      <Card
        className={`${styles.approvalCard} ${styles[`priority-${approval.priority}`]}`}
        hoverable
      >
        <div className={styles.cardHeader}>
          <div className={styles.typeInfo}>
            <div
              className={styles.iconWrapper}
              style={{
                background:
                  approval.priority === "urgent"
                    ? "#fff2f0"
                    : approval.priority === "high"
                      ? "#fff7e6"
                      : "#f0f5ff",
              }}
            >
              <IconComponent
                size={20}
                color={
                  approval.priority === "urgent"
                    ? "#ff4d4f"
                    : approval.priority === "high"
                      ? "#fa8c16"
                      : "#1890ff"
                }
              />
            </div>
            <div>
              <div className={styles.typeLabel}>
                {TYPE_LABELS[approval.type]}
              </div>
              <div className={styles.requestedBy}>by {approval.requestedBy}</div>
            </div>
          </div>
          <Tag color={PRIORITY_COLORS[approval.priority]}>
            {approval.priority.toUpperCase()}
          </Tag>
        </div>

        <div className={styles.cardBody}>
          <h4 className={styles.title}>{approval.title}</h4>
          <p className={styles.description}>{approval.description}</p>
        </div>

        <div className={styles.cardFooter}>
          <span className={styles.timestamp}>{formatTime()}</span>
          <div className={styles.actions}>
            <Tooltip title="Reject">
              <Button
                danger
                icon={<X size={16} />}
                onClick={() => onReject(approval.id)}
              >
                Reject
              </Button>
            </Tooltip>
            <Tooltip title="Approve">
              <Button
                type="primary"
                icon={<Check size={16} />}
                onClick={() => onApprove(approval.id)}
              >
                Approve
              </Button>
            </Tooltip>
          </div>
        </div>
      </Card>
    </motion.div>
  );
};
