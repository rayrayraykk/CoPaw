import { useState } from "react";
import { Modal, Form, Input, Select, Radio, Button, Card } from "antd";
import { Sparkles } from "lucide-react";
import { motion } from "framer-motion";
import type { HarvestTemplate } from "../types";
import { HARVEST_THEMES } from "../themes";
import styles from "./CreateHarvestModal.module.less";

interface CreateHarvestModalProps {
  open: boolean;
  onClose: () => void;
  onSubmit: (values: any) => void;
}

const TEMPLATES: HarvestTemplate[] = [
  {
    id: "tech-daily",
    name: "Tech Frontier Harvest",
    emoji: "🚀",
    description:
      "Daily updates on AI, tech trends, and open source projects",
    estimatedReadTime: 8,
    defaultSchedule: {
      cron: "0 9 * * *",
      timezone: "Asia/Shanghai",
    },
    searchConfig: {
      keywords: ["AI", "GPT", "LLM", "OpenSource", "Tech Trends"],
      sources: ["TechCrunch", "Hacker News", "GitHub Trending"],
      language: "en",
      maxResults: 10,
    },
    defaultTheme: "apple",
  },
  {
    id: "industry-weekly",
    name: "Industry Intelligence",
    emoji: "📊",
    description: "Weekly deep-dive analysis of industry trends",
    estimatedReadTime: 15,
    defaultSchedule: {
      cron: "0 10 * * 1",
      timezone: "Asia/Shanghai",
    },
    searchConfig: {
      keywords: ["SaaS", "B2B", "Enterprise Software"],
      sources: ["Industry Reports", "Market Analysis"],
      language: "en",
      maxResults: 15,
    },
    defaultTheme: "deepblue",
  },
  {
    id: "competitor-daily",
    name: "Competitor Watch",
    emoji: "🏢",
    description: "Track your competitors' every move",
    estimatedReadTime: 5,
    defaultSchedule: {
      cron: "0 18 * * *",
      timezone: "Asia/Shanghai",
    },
    searchConfig: {
      keywords: ["competitor", "funding", "product launch"],
      sources: ["News", "Social Media", "Press Releases"],
      language: "en",
      maxResults: 8,
    },
    defaultTheme: "minimal",
  },
  {
    id: "academic-weekly",
    name: "Academic Frontier",
    emoji: "🎓",
    description: "Latest research papers and academic breakthroughs",
    estimatedReadTime: 20,
    defaultSchedule: {
      cron: "0 11 * * 3",
      timezone: "Asia/Shanghai",
    },
    searchConfig: {
      keywords: ["ArXiv", "Research", "AI Paper"],
      sources: ["ArXiv", "Google Scholar"],
      language: "en",
      maxResults: 5,
    },
    defaultTheme: "autumn",
  },
  {
    id: "investment-weekly",
    name: "Investment Opportunities",
    emoji: "💼",
    description: "Startup funding, VC deals, and market opportunities",
    estimatedReadTime: 12,
    defaultSchedule: {
      cron: "0 14 * * 5",
      timezone: "Asia/Shanghai",
    },
    searchConfig: {
      keywords: ["Startup", "Funding", "VC", "Investment"],
      sources: ["Crunchbase", "AngelList", "VC News"],
      language: "en",
      maxResults: 10,
    },
    defaultTheme: "magazine",
  },
  {
    id: "custom",
    name: "Custom Harvest",
    emoji: "✨",
    description: "Create your own custom harvest with specific keywords",
    estimatedReadTime: 10,
    defaultSchedule: {
      cron: "0 9 * * *",
      timezone: "Asia/Shanghai",
    },
    searchConfig: {
      keywords: [],
      sources: [],
      language: "en",
      maxResults: 10,
    },
    defaultTheme: "apple",
  },
];

export const CreateHarvestModal: React.FC<CreateHarvestModalProps> = ({
  open,
  onClose,
  onSubmit,
}) => {
  const [form] = Form.useForm();
  const [selectedTemplate, setSelectedTemplate] =
    useState<HarvestTemplate | null>(null);
  const [step, setStep] = useState<"template" | "config">("template");

  const handleTemplateSelect = (template: HarvestTemplate) => {
    setSelectedTemplate(template);
    form.setFieldsValue({
      name: template.name,
      keywords: template.searchConfig.keywords.join(", "),
      theme: template.defaultTheme,
      frequency: getCronFrequency(template.defaultSchedule.cron),
    });
    setStep("config");
  };

  const handleBack = () => {
    setStep("template");
    setSelectedTemplate(null);
  };

  const handleSubmit = (values: any) => {
    const harvestData = {
      ...values,
      templateId: selectedTemplate?.id,
      emoji: selectedTemplate?.emoji,
    };
    onSubmit(harvestData);
    onClose();
    form.resetFields();
    setStep("template");
    setSelectedTemplate(null);
  };

  const getCronFrequency = (cron: string): string => {
    if (cron.includes("* * *")) return "daily";
    if (cron.includes("* * 1")) return "weekly";
    return "custom";
  };

  return (
    <Modal
      open={open}
      onCancel={onClose}
      footer={null}
      width={900}
      title={
        <div className={styles.modalTitle}>
          <Sparkles size={20} />
          {step === "template"
            ? "Choose a Harvest Template"
            : "Configure Your Harvest"}
        </div>
      }
      className={styles.createModal}
    >
      {step === "template" && (
        <div className={styles.templateGrid}>
          {TEMPLATES.map((template, index) => (
            <motion.div
              key={template.id}
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: index * 0.05 }}
            >
              <Card
                hoverable
                className={styles.templateCard}
                onClick={() => handleTemplateSelect(template)}
              >
                <div className={styles.templateHeader}>
                  <span className={styles.templateEmoji}>{template.emoji}</span>
                  <h3>{template.name}</h3>
                </div>
                <p className={styles.templateDescription}>
                  {template.description}
                </p>
                <div className={styles.templateMeta}>
                  <span>📚 ~{template.estimatedReadTime} min read</span>
                  <span>
                    🎨{" "}
                    {HARVEST_THEMES[template.defaultTheme]?.name || "Default"}
                  </span>
                </div>
              </Card>
            </motion.div>
          ))}
        </div>
      )}

      {step === "config" && selectedTemplate && (
        <motion.div
          initial={{ opacity: 0, x: 20 }}
          animate={{ opacity: 1, x: 0 }}
        >
          <Form form={form} layout="vertical" onFinish={handleSubmit}>
            <Form.Item
              name="name"
              label="Harvest Name"
              rules={[
                { required: true, message: "Please enter a harvest name" },
              ]}
            >
              <Input
                size="large"
                placeholder="e.g., My Daily Tech News"
                prefix={<span style={{ fontSize: 20 }}>{selectedTemplate.emoji}</span>}
              />
            </Form.Item>

            <Form.Item
              name="keywords"
              label="Keywords"
              rules={[{ required: true, message: "Please enter keywords" }]}
              extra="Separate multiple keywords with commas"
            >
              <Input.TextArea
                rows={3}
                size="large"
                placeholder="e.g., AI, Machine Learning, GPT, OpenAI"
              />
            </Form.Item>

            <Form.Item name="frequency" label="Frequency" initialValue="daily">
              <Radio.Group size="large">
                <Radio.Button value="daily">Daily</Radio.Button>
                <Radio.Button value="weekly">Weekly</Radio.Button>
                <Radio.Button value="custom">Custom</Radio.Button>
              </Radio.Group>
            </Form.Item>

            <Form.Item
              name="theme"
              label="Theme"
              initialValue={selectedTemplate.defaultTheme}
            >
              <Select size="large" style={{ width: "100%" }}>
                {Object.values(HARVEST_THEMES).map((theme) => (
                  <Select.Option key={theme.id} value={theme.id}>
                    <div className={styles.themeOption}>
                      <div
                        className={styles.themeColor}
                        style={{ background: theme.colors.primary }}
                      />
                      <span>{theme.name}</span>
                      <span className={styles.themeDescription}>
                        {theme.description}
                      </span>
                    </div>
                  </Select.Option>
                ))}
              </Select>
            </Form.Item>

            <div className={styles.modalFooter}>
              <Button size="large" onClick={handleBack}>
                Back
              </Button>
              <Button type="primary" size="large" htmlType="submit">
                Create Harvest
              </Button>
            </div>
          </Form>
        </motion.div>
      )}
    </Modal>
  );
};
