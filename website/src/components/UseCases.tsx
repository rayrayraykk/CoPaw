import {
  Share2,
  Lightbulb,
  CheckSquare,
  BookOpen,
  LayoutDashboard,
  Sparkles,
  type LucideIcon,
} from "lucide-react";
import { motion } from "motion/react";
import { t, type Lang } from "../i18n";

const CATEGORIES: Array<{
  key:
    | "social"
    | "creative"
    | "productivity"
    | "research"
    | "assistant"
    | "explore";
  icon: LucideIcon;
  items: number;
}> = [
  { key: "social", icon: Share2, items: 3 },
  { key: "creative", icon: Lightbulb, items: 2 },
  { key: "productivity", icon: CheckSquare, items: 3 },
  { key: "research", icon: BookOpen, items: 2 },
  { key: "assistant", icon: LayoutDashboard, items: 1 },
  { key: "explore", icon: Sparkles, items: 1 },
];

interface UseCasesProps {
  lang: Lang;
  delay?: number;
}

export function UseCases({ lang, delay = 0 }: UseCasesProps) {
  return (
    <motion.section
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.5, delay }}
      className="usecases-section"
      style={{
        margin: "0 auto",
        maxWidth: "var(--container)",
        padding: "var(--space-8) var(--space-4)",
      }}
    >
      <h2
        style={{
          margin: "0 0 var(--space-6)",
          fontSize: "2rem",
          fontWeight: 600,
          color: "var(--text)",
          textAlign: "center",
        }}
      >
        {t(lang, "usecases.title")}
      </h2>
      <div className="usecases-grid">
        {CATEGORIES.map(({ key, icon: Icon, items }) => (
          <div key={key} className="usecases-card">
            <div className="usecases-card-header">
              <Icon
                size={22}
                strokeWidth={1.5}
                style={{ flexShrink: 0, color: "var(--text)" }}
                aria-hidden
              />
              <span className="usecases-card-title">
                {t(lang, `usecases.category.${key}`)}
              </span>
            </div>
            <ul className="usecases-list">
              {Array.from({ length: items }, (_, i) => i + 1).map((i) => (
                <li key={i}>{t(lang, `usecases.${key}.${i}`)}</li>
              ))}
            </ul>
          </div>
        ))}
      </div>
      {t(lang, "usecases.sub") ? (
        <p className="usecases-sub">{t(lang, "usecases.sub")}</p>
      ) : null}
    </motion.section>
  );
}
