import { useState } from "react";
import { Tabs, Empty, Button, Badge, message } from "antd";
import { PackageOpen, Bell, Sparkles, Plus } from "lucide-react";
import { PageHeader } from "@/components/PageHeader";
import { ApprovalCard } from "./components/ApprovalCard";
import { PushMessageCard } from "./components/PushMessageCard";
import { HarvestCard } from "./components/HarvestCard";
import { FlipbookReader } from "./components/FlipbookReader";
import { CreateHarvestModal } from "./components/CreateHarvestModal";
import { useInboxData } from "./hooks/useInboxData";
import { useMockHarvestContent } from "./hooks/useMockHarvestContent";
import type { HarvestContent } from "./types";
import styles from "./index.module.less";

type TabKey = "approvals" | "messages" | "harvests";

function InboxPage() {
  const [activeTab, setActiveTab] = useState<TabKey>("harvests");
  const [readerOpen, setReaderOpen] = useState(false);
  const [currentContent, setCurrentContent] = useState<HarvestContent | null>(
    null,
  );
  const [createModalOpen, setCreateModalOpen] = useState(false);

  const {
    summary,
    approvals,
    pushMessages,
    harvests,
    markMessageAsRead,
    approveRequest,
    rejectRequest,
    triggerHarvest,
  } = useInboxData();

  const handleViewMessage = (messageId: string) => {
    console.log("View message:", messageId);
    message.info("Message detail view - Coming soon!");
  };

  const handleViewHarvest = (harvestId: string) => {
    const content = useMockHarvestContent(harvestId);
    if (content) {
      setCurrentContent(content);
      setReaderOpen(true);
    } else {
      message.warning("No harvest content available yet");
    }
  };

  const handleHarvestSettings = (harvestId: string) => {
    console.log("Open harvest settings:", harvestId);
    message.info("Harvest settings - Coming soon!");
  };

  const handleCreateHarvest = () => {
    setCreateModalOpen(true);
  };

  const handleCreateSubmit = (values: any) => {
    console.log("Creating harvest with values:", values);
    message.success(
      `Harvest "${values.name}" created successfully! 🎉`,
    );
  };

  const tabItems = [
    {
      key: "approvals",
      label: (
        <span className={styles.tabLabel}>
          <PackageOpen size={16} />
          Approvals
          {summary.approvals.urgent > 0 && (
            <Badge count={summary.approvals.urgent} />
          )}
        </span>
      ),
      children: (
        <div className={styles.tabContent}>
          {approvals.length > 0 ? (
            <div className={styles.cardList}>
              {approvals.map((approval) => (
                <ApprovalCard
                  key={approval.id}
                  approval={approval}
                  onApprove={approveRequest}
                  onReject={rejectRequest}
                />
              ))}
            </div>
          ) : (
            <Empty description="No pending approvals" />
          )}
        </div>
      ),
    },
    {
      key: "messages",
      label: (
        <span className={styles.tabLabel}>
          <Bell size={16} />
          Push Messages
          {summary.pushMessages.unread > 0 && (
            <Badge count={summary.pushMessages.unread} />
          )}
        </span>
      ),
      children: (
        <div className={styles.tabContent}>
          {pushMessages.length > 0 ? (
            <div className={styles.cardList}>
              {pushMessages.map((message) => (
                <PushMessageCard
                  key={message.id}
                  message={message}
                  onMarkAsRead={markMessageAsRead}
                  onView={handleViewMessage}
                />
              ))}
            </div>
          ) : (
            <Empty description="No push messages" />
          )}
        </div>
      ),
    },
    {
      key: "harvests",
      label: (
        <span className={styles.tabLabel}>
          <Sparkles size={16} />
          AI Harvest
          {summary.harvests.active > 0 && (
            <Badge count={summary.harvests.active} status="processing" />
          )}
        </span>
      ),
      children: (
        <div className={styles.tabContent}>
          {harvests.length > 0 ? (
            <div className={styles.harvestGrid}>
              {harvests.map((harvest) => (
                <HarvestCard
                  key={harvest.id}
                  harvest={harvest}
                  onTrigger={triggerHarvest}
                  onViewAll={handleViewHarvest}
                  onSettings={handleHarvestSettings}
                />
              ))}
            </div>
          ) : (
            <Empty
              description="No harvests yet"
              image={Empty.PRESENTED_IMAGE_SIMPLE}
            >
              <Button
                type="primary"
                icon={<Plus size={16} />}
                onClick={handleCreateHarvest}
              >
                Create Your First Harvest
              </Button>
            </Empty>
          )}
        </div>
      ),
    },
  ];

  return (
    <div className={styles.inboxPage}>
      <PageHeader
        items={[{ title: "AI Daily Harvest" }]}
        extra={
          activeTab === "harvests" && (
            <Button
              type="primary"
              icon={<Plus size={16} />}
              onClick={handleCreateHarvest}
            >
              Create Harvest
            </Button>
          )
        }
      />

      <div className={styles.pageContent}>
        <Tabs
          activeKey={activeTab}
          onChange={(key) => setActiveTab(key as TabKey)}
          items={tabItems}
          className={styles.inboxTabs}
        />
      </div>

      {/* Flipbook Reader Modal */}
      {currentContent && (
        <FlipbookReader
          open={readerOpen}
          content={currentContent}
          onClose={() => setReaderOpen(false)}
        />
      )}

      {/* Create Harvest Modal */}
      <CreateHarvestModal
        open={createModalOpen}
        onClose={() => setCreateModalOpen(false)}
        onSubmit={handleCreateSubmit}
      />
    </div>
  );
}

export default InboxPage;
