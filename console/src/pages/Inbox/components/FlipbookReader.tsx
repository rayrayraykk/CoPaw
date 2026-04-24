import { useState } from "react";
import { Modal, Button } from "antd";
import {
  ChevronLeft,
  ChevronRight,
  X,
  Bookmark,
  Share2,
  Maximize2,
} from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import type { HarvestContent, HarvestThemeConfig } from "../types";
import { getThemeConfig } from "../themes";
import styles from "./FlipbookReader.module.less";

interface FlipbookReaderProps {
  open: boolean;
  content: HarvestContent;
  onClose: () => void;
}

export const FlipbookReader: React.FC<FlipbookReaderProps> = ({
  open,
  content,
  onClose,
}) => {
  const [currentPage, setCurrentPage] = useState(0);
  const theme = getThemeConfig(content.theme);

  if (!theme) return null;

  const totalPages = content.sections.length + 1; // +1 for cover page

  const nextPage = () => {
    if (currentPage < totalPages - 1) {
      setCurrentPage(currentPage + 1);
    }
  };

  const prevPage = () => {
    if (currentPage > 0) {
      setCurrentPage(currentPage - 1);
    }
  };

  const handleKeyPress = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowRight") nextPage();
    if (e.key === "ArrowLeft") prevPage();
  };

  return (
    <Modal
      open={open}
      onCancel={onClose}
      footer={null}
      width="90vw"
      style={{ maxWidth: 1200, top: 20 }}
      closeIcon={<X size={20} />}
      className={styles.readerModal}
    >
      <div
        className={styles.flipbookContainer}
        onKeyDown={handleKeyPress}
        tabIndex={0}
      >
        {/* Header */}
        <div className={styles.readerHeader}>
          <div className={styles.headerLeft}>
            <h2>{content.title}</h2>
            <span className={styles.pageIndicator}>
              {currentPage + 1} / {totalPages}
            </span>
          </div>
          <div className={styles.headerActions}>
            <Button icon={<Bookmark size={16} />} type="text">
              Bookmark
            </Button>
            <Button icon={<Share2 size={16} />} type="text">
              Share
            </Button>
            <Button icon={<Maximize2 size={16} />} type="text">
              Fullscreen
            </Button>
          </div>
        </div>

        {/* Page Content */}
        <div className={styles.pageContainer}>
          <AnimatePresence mode="wait">
            <motion.div
              key={currentPage}
              initial={{ opacity: 0, x: currentPage > 0 ? 50 : -50 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: currentPage > 0 ? -50 : 50 }}
              transition={{ duration: 0.3 }}
              className={styles.page}
              style={{
                background: theme.colors.background,
                color: theme.colors.text,
                fontFamily: theme.fonts.body,
              }}
            >
              {currentPage === 0 ? (
                <CoverPage content={content} theme={theme} />
              ) : (
                <ContentPage
                  section={content.sections[currentPage - 1]}
                  theme={theme}
                />
              )}
            </motion.div>
          </AnimatePresence>
        </div>

        {/* Navigation */}
        <div className={styles.navigation}>
          <Button
            icon={<ChevronLeft size={20} />}
            onClick={prevPage}
            disabled={currentPage === 0}
            size="large"
            className={styles.navButton}
          >
            Previous
          </Button>

          <div className={styles.pageIndicatorCenter}>
            {Array.from({ length: Math.min(totalPages, 10) }).map((_, i) => {
              const pageIndex =
                totalPages <= 10
                  ? i
                  : Math.floor((i * (totalPages - 1)) / 9);
              return (
                <div
                  key={i}
                  className={`${styles.pageDot} ${
                    currentPage === pageIndex ? styles.activeDot : ""
                  }`}
                  onClick={() => setCurrentPage(pageIndex)}
                />
              );
            })}
          </div>

          <Button
            icon={<ChevronRight size={20} />}
            onClick={nextPage}
            disabled={currentPage === totalPages - 1}
            size="large"
            className={styles.navButton}
            iconPosition="end"
          >
            Next
          </Button>
        </div>
      </div>
    </Modal>
  );
};

// Cover Page Component
const CoverPage: React.FC<{
  content: HarvestContent;
  theme: HarvestThemeConfig;
}> = ({ content, theme }) => {
  return (
    <div className={styles.coverPage}>
      <div className={styles.coverContent}>
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.2 }}
        >
          <h1
            className={styles.coverTitle}
            style={{
              fontFamily: theme.fonts.heading,
              color: theme.colors.primary,
            }}
          >
            {content.title}
          </h1>
          {content.subtitle && (
            <p className={styles.coverSubtitle}>{content.subtitle}</p>
          )}
        </motion.div>

        {content.coverImage && (
          <motion.div
            className={styles.coverImage}
            initial={{ opacity: 0, scale: 0.95 }}
            animate={{ opacity: 1, scale: 1 }}
            transition={{ delay: 0.4 }}
            style={{
              background: `linear-gradient(135deg, ${theme.colors.primary}33, ${theme.colors.secondary}33)`,
            }}
          >
            <div className={styles.imagePlaceholder}>
              <span style={{ fontSize: 48 }}>📰</span>
            </div>
          </motion.div>
        )}

        <motion.div
          className={styles.coverMeta}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ delay: 0.6 }}
        >
          <div className={styles.metaItem}>
            <span className={styles.metaLabel}>Date</span>
            <span className={styles.metaValue}>
              {content.generatedAt.toLocaleDateString()}
            </span>
          </div>
          <div className={styles.metaItem}>
            <span className={styles.metaLabel}>Reading Time</span>
            <span className={styles.metaValue}>
              {content.metadata.estimatedReadTime} min
            </span>
          </div>
          <div className={styles.metaItem}>
            <span className={styles.metaLabel}>Articles</span>
            <span className={styles.metaValue}>
              {content.metadata.articleCount}
            </span>
          </div>
        </motion.div>

        {content.metadata.keywords.length > 0 && (
          <motion.div
            className={styles.coverKeywords}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ delay: 0.8 }}
          >
            {content.metadata.keywords.map((keyword, i) => (
              <span
                key={i}
                className={styles.keyword}
                style={{
                  borderColor: theme.colors.accent,
                  color: theme.colors.accent,
                }}
              >
                #{keyword}
              </span>
            ))}
          </motion.div>
        )}
      </div>
    </div>
  );
};

// Content Page Component
const ContentPage: React.FC<{
  section: any;
  theme: HarvestThemeConfig;
}> = ({ section, theme }) => {
  return (
    <div className={styles.contentPage}>
      {section.type === "hero" && (
        <div className={styles.heroSection}>
          <h2
            className={styles.sectionTitle}
            style={{
              fontFamily: theme.fonts.heading,
              color: theme.colors.primary,
            }}
          >
            {section.title}
          </h2>
          <div
            className={styles.sectionContent}
            dangerouslySetInnerHTML={{ __html: section.content }}
          />
        </div>
      )}

      {section.type === "article" && (
        <div className={styles.articleSection}>
          <h3
            className={styles.articleTitle}
            style={{
              fontFamily: theme.fonts.heading,
              color: theme.colors.primary,
            }}
          >
            {section.title}
          </h3>

          {section.tldr && (
            <div
              className={styles.tldrBox}
              style={{
                borderLeft: `4px solid ${theme.colors.accent}`,
                background: `${theme.colors.primary}10`,
              }}
            >
              <div className={styles.tldrLabel}>TL;DR</div>
              <p>{section.tldr}</p>
            </div>
          )}

          <div
            className={styles.articleContent}
            dangerouslySetInnerHTML={{ __html: section.content }}
          />

          {section.sources && section.sources.length > 0 && (
            <div className={styles.sources}>
              <h4>Sources:</h4>
              <ul>
                {section.sources.map((source: any, i: number) => (
                  <li key={i}>
                    <a
                      href={source.url}
                      target="_blank"
                      rel="noopener noreferrer"
                      style={{ color: theme.colors.accent }}
                    >
                      {source.title}
                    </a>
                    {source.publishedAt && (
                      <span className={styles.sourceDate}>
                        {" "}
                        - {new Date(source.publishedAt).toLocaleDateString()}
                      </span>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}

      {section.type === "quote" && (
        <div
          className={styles.quoteSection}
          style={{ borderLeft: `4px solid ${theme.colors.accent}` }}
        >
          <blockquote
            style={{
              fontFamily: theme.fonts.heading,
              color: theme.colors.secondary,
            }}
          >
            {section.content}
          </blockquote>
        </div>
      )}

      {section.type === "list" && (
        <div className={styles.listSection}>
          {section.title && (
            <h3
              className={styles.listTitle}
              style={{
                fontFamily: theme.fonts.heading,
                color: theme.colors.primary,
              }}
            >
              {section.title}
            </h3>
          )}
          <div dangerouslySetInnerHTML={{ __html: section.content }} />
        </div>
      )}
    </div>
  );
};
