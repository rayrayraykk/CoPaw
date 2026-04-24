import { Card, Button, Badge, Tooltip } from "antd";
import {
  Zap,
  BookOpen,
  Settings,
  Clock,
  Sparkles,
  CheckCircle2,
  AlertCircle,
  PauseCircle,
  Trophy,
} from "lucide-react";
import { motion } from "framer-motion";
import { CircularProgressbar, buildStyles } from "react-circular-progressbar";
import "react-circular-progressbar/dist/styles.css";
import type { HarvestInstance } from "../types";
import { useHarvestCountdown } from "../hooks/useHarvestCountdown";
import styles from "./HarvestCard.module.less";

interface HarvestCardProps {
  harvest: HarvestInstance;
  onTrigger: (id: string) => void;
  onViewAll: (id: string) => void;
  onSettings: (id: string) => void;
}

export const HarvestCard: React.FC<HarvestCardProps> = ({
  harvest,
  onTrigger,
  onViewAll,
  onSettings,
}) => {
  const countdown = useHarvestCountdown(harvest.schedule.nextRun);

  const getStatusIcon = () => {
    switch (harvest.status) {
      case "active":
        return <CheckCircle2 size={16} className={styles.statusIconActive} />;
      case "paused":
        return <PauseCircle size={16} className={styles.statusIconPaused} />;
      case "error":
        return <AlertCircle size={16} className={styles.statusIconError} />;
    }
  };

  const getStatusText = () => {
    switch (harvest.status) {
      case "active":
        return countdown.isOverdue ? "Ready to Harvest" : "Growing";
      case "paused":
        return "Paused";
      case "error":
        return "Error";
    }
  };

  const formatTime = () => {
    if (countdown.isOverdue) {
      return "Ready";
    }
    return `${String(countdown.hours).padStart(2, "0")}:${String(countdown.minutes).padStart(2, "0")}:${String(countdown.seconds).padStart(2, "0")}`;
  };

  const formatLastGenerated = () => {
    if (!harvest.lastGenerated) return "Never";

    const diff = Date.now() - harvest.lastGenerated.timestamp.getTime();
    const hours = Math.floor(diff / (1000 * 60 * 60));
    const days = Math.floor(hours / 24);

    if (days > 0) return `${days} days ago`;
    if (hours > 0) return `${hours} hours ago`;
    return "Just now";
  };

  return (
    <motion.div
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
      whileHover={{ y: -4, boxShadow: "0 8px 24px rgba(0,0,0,0.12)" }}
      transition={{ duration: 0.2 }}
    >
      <Card
        className={`${styles.harvestCard} ${countdown.isOverdue ? styles.harvestCardReady : ""}`}
        hoverable
      >
        <div className={styles.cardHeader}>
          <div className={styles.titleRow}>
            <span className={styles.emoji}>{harvest.emoji}</span>
            <h3 className={styles.title}>{harvest.name}</h3>
          </div>
          {getStatusIcon()}
        </div>

        <div className={styles.cardBody}>
          <div className={styles.countdownSection}>
            <div className={styles.progressCircle}>
              <CircularProgressbar
                value={countdown.percentage}
                text={formatTime()}
                styles={buildStyles({
                  textSize: "14px",
                  pathTransitionDuration: 0.5,
                  pathColor:
                    countdown.isOverdue
                      ? "url(#gradient-ready)"
                      : "url(#gradient)",
                  textColor: countdown.isOverdue ? "#FFD700" : "#FF7F16",
                  trailColor: "#f0f0f0",
                })}
              />
              <svg style={{ height: 0 }}>
                <defs>
                  <linearGradient id="gradient" x1="0%" y1="0%" x2="100%">
                    <stop offset="0%" stopColor="#FF7F16" />
                    <stop offset="100%" stopColor="#FFD700" />
                  </linearGradient>
                  <linearGradient id="gradient-ready" x1="0%" y1="0%" x2="100%">
                    <stop offset="0%" stopColor="#FFD700" />
                    <stop offset="100%" stopColor="#FFA500" />
                  </linearGradient>
                </defs>
              </svg>
            </div>
            <div className={styles.countdownInfo}>
              <div className={styles.statusBadge}>
                {countdown.isOverdue ? (
                  <Badge status="success" text={getStatusText()} />
                ) : (
                  <Badge status="processing" text={getStatusText()} />
                )}
              </div>
            </div>
          </div>

          <div className={styles.statsSection}>
            <div className={styles.statItem}>
              <Sparkles size={14} />
              <span>Harvested {harvest.stats.totalGenerated} times</span>
            </div>
            <div className={styles.statItem}>
              <CheckCircle2 size={14} />
              <span>Success Rate {harvest.stats.successRate}%</span>
            </div>
            {harvest.stats.consecutiveDays > 0 && (
              <div className={styles.statItem}>
                <Trophy size={14} />
                <span>{harvest.stats.consecutiveDays} days streak</span>
              </div>
            )}
          </div>

          <div className={styles.lastHarvest}>
            <Clock size={14} />
            <span>Last harvest: {formatLastGenerated()}</span>
          </div>

          {harvest.lastGenerated?.previewUrl && (
            <div className={styles.previewSection}>
              <div className={styles.previewTitle}>Latest Output:</div>
              <div className={styles.previewContent}>
                📦 Tech Trends Analysis Report
              </div>
            </div>
          )}
        </div>

        <div className={styles.cardActions}>
          <Tooltip title="Harvest Now">
            <Button
              type="primary"
              icon={<Zap size={16} />}
              onClick={() => onTrigger(harvest.id)}
              className={styles.triggerButton}
            >
              Harvest Now
            </Button>
          </Tooltip>
          <Tooltip title="View History">
            <Button
              icon={<BookOpen size={16} />}
              onClick={() => onViewAll(harvest.id)}
            >
              View All
            </Button>
          </Tooltip>
          <Tooltip title="Settings">
            <Button
              icon={<Settings size={16} />}
              onClick={() => onSettings(harvest.id)}
            />
          </Tooltip>
        </div>
      </Card>
    </motion.div>
  );
};
