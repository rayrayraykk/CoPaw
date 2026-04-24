import { useState, useCallback } from "react";
import { createPortal } from "react-dom";
import { Modal, Badge, Button, message } from "antd";
import { X, Download, BookmarkCheck, BookmarkX, ChevronLeft, ChevronRight } from "lucide-react";
import { useSpring, animated, config, useTransition } from "@react-spring/web";
import { useDrag } from "@use-gesture/react";
import type { HarvestInstance } from "../types";
import { generateMockHistory } from "../hooks/useMockHarvestContent";
import styles from "./MagazineStackViewer.module.less";

interface MagazineStackViewerProps {
  open: boolean;
  harvest: HarvestInstance;
  onClose: () => void;
}

export const MagazineStackViewer: React.FC<MagazineStackViewerProps> = ({
  open,
  harvest,
  onClose,
}) => {
  const [magazines, setMagazines] = useState(() => generateMockHistory(harvest.id));
  const [currentIndex, setCurrentIndex] = useState(0);
  const [isFullscreen, setIsFullscreen] = useState(false);

  // 关闭Modal时重置状态
  const handleClose = useCallback(() => {
    setIsFullscreen(false); // 重置全屏状态
    onClose();
  }, [onClose]);

  // 标记为已读
  const markAsRead = useCallback((index: number) => {
    setMagazines((prev: any[]) =>
      prev.map((mag: any, i: number) => i === index ? { ...mag, isRead: true } : mag)
    );
  }, []);

  // 翻到下一页
  const nextPage = useCallback(() => {
    if (currentIndex < magazines.length - 1) {
      markAsRead(currentIndex);
      setCurrentIndex(prev => prev + 1);
    }
  }, [currentIndex, magazines.length, markAsRead]);

  // 翻到上一页
  const prevPage = useCallback(() => {
    if (currentIndex > 0) {
      setCurrentIndex(prev => prev - 1);
    }
  }, [currentIndex]);

  // 跳转到指定页
  const jumpToPage = useCallback((index: number) => {
    markAsRead(currentIndex);
    setCurrentIndex(index);
  }, [currentIndex, markAsRead]);

  // 导出PNG
  const handleExport = useCallback(() => {
    message.info("导出PNG功能将在生产版本中实现（使用Playwright）");
  }, []);

  // 进入全屏预览
  const enterFullscreen = useCallback(() => {
    setIsFullscreen(true);
    markAsRead(currentIndex);
  }, [currentIndex, markAsRead]);

  // 退出全屏预览
  const exitFullscreen = useCallback(() => {
    setIsFullscreen(false);
  }, []);

  // 拖拽手势 - 左右滑动翻页
  const bind = useDrag(
    ({ swipe: [swipeX] }) => {
      if (swipeX > 0) {
        // 向右滑 = 上一页
        prevPage();
      } else if (swipeX < 0) {
        // 向左滑 = 下一页
        nextPage();
      }
    },
    {
      axis: 'x',
      swipe: { velocity: 0.3 },
    }
  );

  // 主内容区域弹簧动画
  const contentSpring = useSpring({
    opacity: 1,
    transform: 'scale(1)',
    config: config.gentle,
  });

  // 页面切换过渡动画
  const transitions = useTransition(currentIndex, {
    keys: currentIndex,
    from: { opacity: 0, transform: 'translate3d(100%,0,0)' },
    enter: { opacity: 1, transform: 'translate3d(0%,0,0)' },
    leave: { opacity: 0, transform: 'translate3d(-50%,0,0)' },
    config: config.default,
  });

  return (
    <Modal
      open={open}
      onCancel={handleClose}
      footer={null}
      width="95vw"
      style={{ maxWidth: 1600, top: 20 }}
      closeIcon={<X size={20} />}
      className={styles.stackModal}
    >
      <div className={styles.viewerContainer}>
        {/* 顶部标题区 */}
        <div className={styles.stackHeader}>
          <div className={styles.stackTitle}>
            <span className={styles.titleText}>{harvest.name}</span>
            <Badge
              count={magazines.filter((m: any) => !m.isRead).length}
              className={styles.unreadBadge}
            />
          </div>
          <div className={styles.instructions}>
            📖 第 {currentIndex + 1} / {magazines.length} 篇 • 点击左右箭头或滑动翻页 • 点击时间轴快速跳转
          </div>
        </div>

        {/* 主阅读区域 */}
        <animated.div className={styles.mainArea} style={contentSpring}>
          <div className={styles.readingArea} {...bind()}>
            {/* 左侧翻页按钮 */}
            {currentIndex > 0 && (
              <button
                className={`${styles.navButton} ${styles.prevButton}`}
                onClick={prevPage}
              >
                <ChevronLeft size={32} />
              </button>
            )}

            {/* 报纸内容 */}
            <div className={styles.magazineDisplay}>
              {transitions((style, index) => (
                <animated.div
                  key={magazines[index].id}
                  className={styles.magazineContent}
                  style={style}
                >
                  {/* 报纸标题卡片 */}
                  <div className={styles.magazineHeader}>
                    <div className={styles.headerTop}>
                      <h2>{magazines[index].title}</h2>
                      <div className={`${styles.stickyNote} ${magazines[index].isRead ? styles.read : styles.unread}`}>
                        {magazines[index].isRead ? (
                          <>
                            <BookmarkCheck size={16} />
                            <span>已读</span>
                          </>
                        ) : (
                          <>
                            <BookmarkX size={16} />
                            <span>未读</span>
                          </>
                        )}
                      </div>
                    </div>
                    <div className={styles.dateInfo}>
                      {magazines[index].date.toLocaleDateString('zh-CN', {
                        year: 'numeric',
                        month: 'long',
                        day: 'numeric',
                        weekday: 'long'
                      })}
                    </div>
                  </div>

                  {/* 报纸内容iframe */}
                  <div
                    className={styles.magazineFrame}
                    onClick={enterFullscreen}
                    title="点击查看完整内容"
                  >
                    <iframe
                      src={magazines[index].coverImage}
                      className={styles.contentIframe}
                      title={magazines[index].title}
                    />
                    {/* 点击提示遮罩 */}
                    <div className={styles.clickOverlay}>
                      <div className={styles.clickHint}>
                        🔍 点击查看完整内容
                      </div>
                    </div>
                  </div>
                </animated.div>
              ))}
            </div>

            {/* 右侧翻页按钮 */}
            {currentIndex < magazines.length - 1 && (
              <button
                className={`${styles.navButton} ${styles.nextButton}`}
                onClick={nextPage}
              >
                <ChevronRight size={32} />
              </button>
            )}
          </div>

          {/* 操作按钮 */}
          <div className={styles.actionBar}>
            <Button
              size="large"
              icon={<Download size={18} />}
              onClick={handleExport}
              type="primary"
            >
              下载当前页PNG
            </Button>
          </div>
        </animated.div>

        {/* 底部时间轴 - 缩略图导航 */}
        <div className={styles.timeline}>
          <div className={styles.timelineTrack}>
            {magazines.map((magazine: any, index: number) => (
              <div
                key={magazine.id}
                className={`${styles.timelineItem} ${
                  index === currentIndex ? styles.active : ''
                } ${magazine.isRead ? styles.read : ''}`}
                onClick={() => jumpToPage(index)}
              >
                {/* 缩略图 */}
                <div className={styles.thumbnailWrapper}>
                  <div className={styles.thumbnail}>
                    <iframe
                      src={magazine.coverImage}
                      className={styles.thumbnailIframe}
                      title={`缩略图-${magazine.title}`}
                    />
                  </div>
                  {/* 已读标记 */}
                  {magazine.isRead && (
                    <div className={styles.readMark}>
                      <BookmarkCheck size={12} />
                    </div>
                  )}
                </div>
                {/* 日期标签 */}
                <div className={styles.timelineLabel}>
                  {magazine.date.toLocaleDateString('zh-CN', {
                    month: 'short',
                    day: 'numeric'
                  })}
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* 全屏预览模态框 - 使用 Portal 渲染到 body */}
      {isFullscreen && createPortal(
        <div
          className={styles.fullscreenModal}
          onClick={exitFullscreen}
        >
          <div className={styles.fullscreenHeader}>
            <div className={styles.fullscreenTitle}>
              <h3>{magazines[currentIndex].title}</h3>
              <span className={styles.fullscreenDate}>
                {magazines[currentIndex].date.toLocaleDateString('zh-CN', {
                  year: 'numeric',
                  month: 'long',
                  day: 'numeric'
                })}
              </span>
            </div>
            <div className={styles.fullscreenActions}>
              <Button
                icon={<Download size={18} />}
                onClick={(e) => {
                  e.stopPropagation();
                  handleExport();
                }}
                size="large"
              >
                下载PNG
              </Button>
              <Button
                icon={<X size={20} />}
                onClick={(e) => {
                  e.stopPropagation();
                  exitFullscreen();
                }}
                size="large"
                type="text"
              >
                关闭
              </Button>
            </div>
          </div>
          <div
            className={styles.fullscreenContent}
            onClick={(e) => e.stopPropagation()}
          >
            <iframe
              src={magazines[currentIndex].coverImage}
              className={styles.fullscreenIframe}
              title={`全屏-${magazines[currentIndex].title}`}
            />
          </div>
        </div>,
        document.body
      )}
    </Modal>
  );
};
