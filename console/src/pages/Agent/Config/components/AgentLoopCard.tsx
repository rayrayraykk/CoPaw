import { useState, useEffect, useCallback, useMemo } from "react";
import {
  InputNumber,
  Input,
  Button,
  message,
  Modal,
} from "@agentscope-ai/design";
import {
  Plus,
  Repeat,
  Shield,
  CheckCircle,
  Target,
  Rocket,
  Wallet,
  Save,
  Loader2,
  GripVertical,
  Lock,
  ChevronDown,
  Search,
  Gauge,
  RotateCcw,
  MousePointerClick,
  X,
  Trash2,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import api from "@/api";
import type {
  ProfileInfo,
  ProfileGateInfo,
  GateCatalogEntry,
} from "@/api/types/agent";
import s from "./AgentLoopCard.module.less";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const GATE_ICONS: Record<string, any> = {
  iteration: Repeat,
  doom_loop: Shield,
  rubric: CheckCircle,
  budget: Wallet,
};

const GATE_CATEGORY_STYLES: Record<
  string,
  { bg: string; color: string; cls: string }
> = {
  safety: { bg: "#fef3e8", color: "#8b6914", cls: "safety" },
  budget: { bg: "#eef2ff", color: "#4a5fc1", cls: "budget" },
  completion: { bg: "rgba(44,95,74,0.08)", color: "#2c5f4a", cls: "completion" },
  plugin: { bg: "#f3eef8", color: "#7c5cbf", cls: "plugin" },
};

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const PROFILE_ICONS: Record<string, any> = {
  default: Repeat,
  goal: Target,
  mission: Rocket,
};

const ACTION_OPTIONS = [
  { value: "modify_prompt", label: "Send Reminder" },
  { value: "stop", label: "Pause & Ask" },
];

function getGateMeta(gate: ProfileGateInfo): string {
  if (gate.type === "iteration") {
    return `max ${gate.params.max_iterations} iterations`;
  }
  if (gate.type === "budget") {
    const t = gate.params.max_tokens as number;
    return `budget ${(t / 1000).toFixed(0)}K tokens`;
  }
  if (gate.type === "doom_loop") {
    const stages = (gate.params.stages || []) as unknown[];
    return `${stages.length} escalation stages`;
  }
  if (gate.type === "rubric") {
    return `max ${gate.params.max_interventions || 1} interventions`;
  }
  return "";
}

/* ── Gate Catalog Panel (left) ── */
function CatalogPanel({
  catalog,
  isBuiltin,
  onAddGate,
}: {
  catalog: GateCatalogEntry[];
  isBuiltin: boolean;
  onAddGate: (entry: GateCatalogEntry) => void;
}) {
  const [search, setSearch] = useState("");

  const grouped = useMemo(() => {
    const map: Record<string, GateCatalogEntry[]> = {};
    for (const entry of catalog) {
      const lower = `${entry.name} ${entry.description}`.toLowerCase();
      if (search && !lower.includes(search.toLowerCase())) continue;
      const cat = entry.category || "other";
      if (!map[cat]) map[cat] = [];
      map[cat].push(entry);
    }
    return map;
  }, [catalog, search]);

  const categoryLabels: Record<string, string> = {
    safety: "Safety",
    budget: "Budget",
    completion: "Completion",
    plugin: "Plugin",
    other: "Other",
  };

  return (
    <aside className={s.panel}>
      <div className={s.panelHeader}>
        Gate Catalog
        <span className={s.panelHeaderCount}>{catalog.length} available</span>
      </div>
      <div className={s.panelBody}>
        <div style={{ position: "relative", marginBottom: 10 }}>
          <Search
            size={13}
            style={{
              position: "absolute",
              left: 9,
              top: "50%",
              transform: "translateY(-50%)",
              opacity: 0.35,
              pointerEvents: "none",
            }}
          />
          <input
            className={s.catalogSearch}
            type="search"
            placeholder="Search gates..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
        {isBuiltin && (
          <div
            style={{
              fontSize: 11,
              color: "var(--text-quaternary, #9c9c96)",
              padding: "6px 8px",
              marginBottom: 10,
              background: "var(--bg-elevated, #f7f6f3)",
              borderRadius: 6,
              display: "flex",
              alignItems: "center",
              gap: 4,
            }}
          >
            <Lock size={10} />
            Built-in template — create custom to add gates
          </div>
        )}
        {Object.entries(grouped).map(([cat, entries]) => (
          <div key={cat} className={s.catalogGroup}>
            <div className={s.catalogGroupTitle}>
              {categoryLabels[cat] || cat}
            </div>
            {entries.map((entry) => {
              const cStyle =
                GATE_CATEGORY_STYLES[entry.category] ||
                GATE_CATEGORY_STYLES.plugin;
              const Icon = GATE_ICONS[entry.type] || Gauge;
              return (
                <div
                  key={entry.type}
                  className={s.catalogItem}
                  onClick={() => !isBuiltin && onAddGate(entry)}
                  style={{
                    cursor: isBuiltin ? "default" : "pointer",
                    opacity: isBuiltin ? 0.7 : 1,
                  }}
                  title={
                    isBuiltin
                      ? "Structure locked for built-in templates"
                      : `Click to add ${entry.name} to pipeline`
                  }
                >
                  <div className={`${s.catalogIcon} ${s[cStyle.cls]}`}>
                    <Icon size={14} />
                  </div>
                  <div className={s.catalogInfo}>
                    <div className={s.catalogName}>{entry.name}</div>
                    <div className={s.catalogDesc}>{entry.description}</div>
                  </div>
                  {!isBuiltin ? (
                    <Plus size={14} style={{ opacity: 0.4, flexShrink: 0 }} />
                  ) : (
                    <span className={s.catalogPriority}>
                      P{entry.default_priority}
                    </span>
                  )}
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </aside>
  );
}

/* ── Pipeline Panel (center) ── */
function PipelinePanel({
  profile,
  selectedGateId,
  onSelectGate,
  onToggleGate,
  onRemoveGate,
}: {
  profile: ProfileInfo;
  selectedGateId: string | null;
  onSelectGate: (id: string) => void;
  onToggleGate: (id: string, enabled: boolean) => void;
  onRemoveGate?: (id: string) => void;
}) {
  const label =
    profile.name.charAt(0).toUpperCase() + profile.name.slice(1);

  return (
    <section className={s.panel}>
      <div className={s.panelHeader}>
        {label} Profile — Gate Pipeline
        <span className={s.panelHeaderCount}>
          {profile.gates.length} gates
        </span>
      </div>
      <div className={s.panelBody}>
        <p className={s.pipelineHint}>
          {profile.is_builtin
            ? "Evaluation order top → bottom · structure locked"
            : "Click gates in catalog to add · click ✕ to remove"}
        </p>
        <div className={s.pipelineFlow}>
          {profile.gates.length === 0 && (
            <div
              style={{
                textAlign: "center",
                padding: "40px 20px",
                color: "var(--text-quaternary, #9c9c96)",
                fontSize: 13,
              }}
            >
              No gates yet. Click gates in the catalog to add.
            </div>
          )}
          {profile.gates.map((gate, idx) => {
            const cStyle =
              GATE_CATEGORY_STYLES[gate.category] ||
              GATE_CATEGORY_STYLES.plugin;
            const Icon = GATE_ICONS[gate.type] || Gauge;
            const selected = selectedGateId === gate.id;
            return (
              <div key={gate.id} style={{ width: "100%", display: "flex", flexDirection: "column", alignItems: "center" }}>
                {idx > 0 && <div className={s.flowArrow} />}
                <div
                  className={[
                    s.pipelineNode,
                    selected ? s.selected : "",
                    !gate.enabled ? s.disabled : "",
                  ].join(" ")}
                  onClick={() => onSelectGate(gate.id)}
                >
                  <span className={s.nodeDrag}>
                    <GripVertical size={14} />
                  </span>
                  <div
                    className={s.nodeIcon}
                    style={{ background: cStyle.bg, color: cStyle.color }}
                  >
                    <Icon size={16} />
                  </div>
                  <div className={s.nodeBody}>
                    <div className={s.nodeTitle}>
                      {gate.name}
                      {profile.is_builtin && (
                        <span className={s.lockBadge}>
                          <Lock size={9} />
                          built-in
                        </span>
                      )}
                    </div>
                    <div className={s.nodeMeta}>
                      {gate.enabled ? getGateMeta(gate) : "Disabled"}
                    </div>
                  </div>
                  <span className={s.nodePriority}>#{idx + 1}</span>
                  <div
                    className={`${s.nodeToggle} ${!gate.enabled ? s.off : ""}`}
                    onClick={(e) => {
                      e.stopPropagation();
                      onToggleGate(gate.id, !gate.enabled);
                    }}
                  />
                  {!profile.is_builtin && onRemoveGate && (
                    <button
                      className={s.nodeRemove}
                      onClick={(e) => {
                        e.stopPropagation();
                        onRemoveGate(gate.id);
                      }}
                      title="Remove gate"
                    >
                      <X size={12} />
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </section>
  );
}

/* ── Inspector Panel (right) ── */
function InspectorPanel({
  gate,
  onParamsChange,
}: {
  gate: ProfileGateInfo | null;
  onParamsChange: (gateId: string, params: Record<string, unknown>) => void;
}) {
  const { t } = useTranslation();

  if (!gate) {
    return (
      <aside className={s.panel}>
        <div className={s.panelHeader}>Inspector</div>
        <div className={s.panelBody}>
          <div className={s.inspectorEmpty}>
            <MousePointerClick
              size={28}
              style={{ opacity: 0.2, marginBottom: 12, display: "block", margin: "0 auto 12px" }}
            />
            Select a gate from the pipeline to inspect and edit its parameters
          </div>
        </div>
      </aside>
    );
  }

  const params = gate.params;
  const update = (p: Record<string, unknown>) => onParamsChange(gate.id, p);

  return (
    <aside className={s.panel}>
      <div className={s.panelHeader}>Inspector</div>
      <div className={s.panelBody}>
        <div className={s.inspectorHeader}>
          <div className={s.inspectorTitle}>{gate.name}</div>
          <div className={s.inspectorMeta}>
            type: {gate.type}
          </div>
        </div>


        {gate.type === "iteration" && (
          <div className={s.formGroup}>
            <label className={s.formLabel}>
              {t("agentConfig.iterationMaxIterations", "Max Iterations")}
            </label>
            <InputNumber
              min={1}
              max={500}
              value={params.max_iterations as number}
              onChange={(val) =>
                update({ ...params, max_iterations: val ?? 100 })
              }
              style={{ width: "100%" }}
              size="small"
            />
            <div className={s.formHint}>
              Maximum loop turns before the agent stops
            </div>
          </div>
        )}

        {gate.type === "budget" && (
          <div className={s.formGroup}>
            <label className={s.formLabel}>Token Budget</label>
            <InputNumber
              min={1000}
              max={10_000_000}
              step={10000}
              value={params.max_tokens as number}
              onChange={(val) =>
                update({ ...params, max_tokens: val ?? 300_000 })
              }
              style={{ width: "100%" }}
              size="small"
            />
            <div className={s.formHint}>
              Maximum token spend for this loop session
            </div>
          </div>
        )}

        {gate.type === "doom_loop" && (
          <>
            <div className={s.formGroup}>
              <label className={s.formLabel}>
                {t("agentConfig.doomLoopWindowSize", "Detection Range")}
              </label>
              <InputNumber
                min={2}
                max={20}
                value={params.window_size as number}
                onChange={(val) =>
                  update({ ...params, window_size: val ?? 3 })
                }
                style={{ width: "100%" }}
                size="small"
              />
              <div className={s.formHint}>
                How many recent actions to check for repetition
              </div>
            </div>
            <div className={s.formGroup}>
              <label className={s.formLabel}>
                {t("agentConfig.doomLoopSimilarity", "Match Sensitivity")}
              </label>
              <InputNumber
                min={0}
                max={1}
                step={0.05}
                value={params.similarity_threshold as number}
                onChange={(val) =>
                  update({ ...params, similarity_threshold: val ?? 1.0 })
                }
                style={{ width: "100%" }}
                size="small"
              />
            </div>
            <div className={s.formGroup}>
              <label className={s.formLabel}>
                {t("agentConfig.doomLoopStages", "Escalation Stages")}
              </label>
              {(
                (params.stages || []) as Array<{
                  after: number;
                  action: string;
                  prompt: string;
                }>
              ).map((stage, idx) => (
                <div key={idx} className={s.stageRow}>
                  <input
                    className={s.formInput}
                    type="number"
                    min={1}
                    value={stage.after}
                    onChange={(e) => {
                      const stages = [
                        ...((params.stages || []) as Array<{
                          after: number;
                          action: string;
                          prompt: string;
                        }>),
                      ];
                      stages[idx] = {
                        ...stages[idx],
                        after: parseInt(e.target.value) || 1,
                      };
                      update({ ...params, stages });
                    }}
                  />
                  <select
                    className={s.formInput}
                    value={stage.action}
                    onChange={(e) => {
                      const stages = [
                        ...((params.stages || []) as Array<{
                          after: number;
                          action: string;
                          prompt: string;
                        }>),
                      ];
                      stages[idx] = {
                        ...stages[idx],
                        action: e.target.value,
                      };
                      update({ ...params, stages });
                    }}
                  >
                    {ACTION_OPTIONS.map((opt) => (
                      <option key={opt.value} value={opt.value}>
                        {opt.label}
                      </option>
                    ))}
                  </select>
                  <input
                    className={s.formInput}
                    type="text"
                    value={stage.prompt}
                    placeholder="Message..."
                    onChange={(e) => {
                      const stages = [
                        ...((params.stages || []) as Array<{
                          after: number;
                          action: string;
                          prompt: string;
                        }>),
                      ];
                      stages[idx] = {
                        ...stages[idx],
                        prompt: e.target.value,
                      };
                      update({ ...params, stages });
                    }}
                  />
                </div>
              ))}
            </div>
          </>
        )}

        {gate.type === "rubric" && (
          <>
            <div className={s.formGroup}>
              <label className={s.formLabel}>
                {t("agentConfig.rubricPrompt", "Re-prompt Message")}
              </label>
              <Input.TextArea
                autoSize={{ minRows: 3, maxRows: 6 }}
                value={params.prompt as string}
                onChange={(e) =>
                  update({ ...params, prompt: e.target.value })
                }
                size="small"
              />
            </div>
            <div className={s.formGroup}>
              <label className={s.formLabel}>
                {t(
                  "agentConfig.rubricMaxInterventions",
                  "Max Interventions per Turn",
                )}
              </label>
              <InputNumber
                min={1}
                max={10}
                value={params.max_interventions as number}
                onChange={(val) =>
                  update({ ...params, max_interventions: val ?? 1 })
                }
                style={{ width: "100%" }}
                size="small"
              />
              <div className={s.formHint}>
                Prevents infinite re-prompting
              </div>
            </div>
          </>
        )}

        <div className={s.inspectorActions}>
          <Button
            size="small"
            icon={<RotateCcw size={11} />}
            style={{ flex: 1, fontSize: 12 }}
          >
            Reset Default
          </Button>
        </div>
      </div>
    </aside>
  );
}

/* ── Flow Preview ── */
function FlowPreview({ profile }: { profile: ProfileInfo }) {
  const [open, setOpen] = useState(false);
  const sorted = [...profile.gates].sort((a, b) => a.priority - b.priority);

  return (
    <section className={s.flowPreview}>
      <div className={s.flowPreviewHeader} onClick={() => setOpen(!open)}>
        <span>Evaluation Flow Preview</span>
        <ChevronDown
          size={13}
          style={{
            opacity: 0.35,
            transform: open ? "rotate(0deg)" : "rotate(-90deg)",
            transition: "transform 0.2s ease",
          }}
        />
      </div>
      {open && (
        <div className={s.flowPreviewBody}>
          <div className={s.flowDiagram}>
            <div className={s.flowStep}>
              <div className={s.flowChip}>Agent Turn</div>
              <div className={s.flowLabel}>input</div>
            </div>
            {sorted.map((gate) => (
              <div
                key={gate.id}
                style={{ display: "flex", alignItems: "center" }}
              >
                <div className={s.flowConnector} />
                <div className={s.flowStep}>
                  <div
                    className={`${s.flowChip} ${gate.enabled ? s.stop : ""}`}
                    style={!gate.enabled ? { opacity: 0.35 } : undefined}
                  >
                    P{gate.priority} {gate.type.replace("_", "-")}
                  </div>
                  <div className={s.flowLabel}>
                    {!gate.enabled ? "disabled" : "can STOP"}
                  </div>
                </div>
              </div>
            ))}
            <div className={s.flowConnector} />
            <div className={s.flowStep}>
              <div className={s.flowChip}>No opinion</div>
              <div className={s.flowLabel}>→ STOP</div>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}

/* ── Simple Mode ── */
function SimpleMode({
  profile,
  onToggleGate,
  onParamsChange,
}: {
  profile: ProfileInfo;
  onToggleGate: (id: string, enabled: boolean) => void;
  onParamsChange: (gateId: string, params: Record<string, unknown>) => void;
}) {
  const { t } = useTranslation();

  return (
    <div className={s.simpleMode}>
      <p
        style={{
          color: "var(--text-secondary, #6b6b66)",
          fontSize: 13,
          marginBottom: 14,
          lineHeight: 1.6,
        }}
      >
        {t(
          "agentConfig.simpleModeDesc",
          "Toggle and configure each gate. Switch to Pipeline for the full orchestrator view.",
        )}
      </p>
      {profile.gates.map((gate) => {
        const Icon = GATE_ICONS[gate.type] || Gauge;
        return (
          <div
            key={gate.id}
            className={`${s.simpleGate} ${!gate.enabled ? s.simpleDisabled : ""}`}
          >
            <div className={s.simpleGateHeader}>
              <div className={s.simpleGateTitle}>
                <Icon size={15} style={{ opacity: 0.6 }} />
                <span style={{ fontWeight: 500, fontSize: 13 }}>
                  {gate.name}
                </span>
                <span
                  style={{
                    fontSize: 10,
                    color: "var(--text-quaternary, #9c9c96)",
                    fontFamily: "monospace",
                  }}
                >
                  P{gate.priority}
                </span>
              </div>
              <div
                className={`${s.nodeToggle} ${!gate.enabled ? s.off : ""}`}
                onClick={() => onToggleGate(gate.id, !gate.enabled)}
              />
            </div>
            {gate.enabled && (
              <div className={s.simpleGateBody}>
                {gate.type === "iteration" && (
                  <div>
                    <label className={s.formLabel}>Max Iterations</label>
                    <InputNumber
                      min={1}
                      max={500}
                      value={gate.params.max_iterations as number}
                      onChange={(v) =>
                        onParamsChange(gate.id, {
                          ...gate.params,
                          max_iterations: v ?? 100,
                        })
                      }
                      style={{ width: 180 }}
                      size="small"
                    />
                  </div>
                )}
                {gate.type === "budget" && (
                  <div>
                    <label className={s.formLabel}>Token Budget</label>
                    <InputNumber
                      min={1000}
                      max={10_000_000}
                      step={10000}
                      value={gate.params.max_tokens as number}
                      onChange={(v) =>
                        onParamsChange(gate.id, {
                          ...gate.params,
                          max_tokens: v ?? 300_000,
                        })
                      }
                      style={{ width: 180 }}
                      size="small"
                    />
                  </div>
                )}
                {gate.type === "doom_loop" && (
                  <div
                    style={{
                      fontSize: 12,
                      color: "var(--text-secondary, #6b6b66)",
                      lineHeight: 1.5,
                    }}
                  >
                    {getGateMeta(gate)}
                    <span
                      style={{
                        marginLeft: 6,
                        fontSize: 11,
                        color: "var(--text-quaternary, #9c9c96)",
                      }}
                    >
                      — switch to Pipeline to configure stages
                    </span>
                  </div>
                )}
                {gate.type === "rubric" && (
                  <div>
                    <label className={s.formLabel}>Re-prompt Message</label>
                    <Input.TextArea
                      autoSize={{ minRows: 2, maxRows: 4 }}
                      value={gate.params.prompt as string}
                      onChange={(e) =>
                        onParamsChange(gate.id, {
                          ...gate.params,
                          prompt: e.target.value,
                        })
                      }
                      size="small"
                    />
                  </div>
                )}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

/* ── Main Component ── */
export function AgentLoopCard() {
  const { t } = useTranslation();
  const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
  const [catalog, setCatalog] = useState<GateCatalogEntry[]>([]);
  const [activeProfile, setActiveProfile] = useState("default");
  const [selectedGateId, setSelectedGateId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState<Set<string>>(new Set());
  const [mode, setMode] = useState<"simple" | "advanced">("advanced");
  const [createModalOpen, setCreateModalOpen] = useState(false);
  const [newProfileName, setNewProfileName] = useState("");
  const [newProfileDesc, setNewProfileDesc] = useState("");

  const fetchData = useCallback(async () => {
    try {
      setLoading(true);
      const [profilesData, catalogData] = await Promise.all([
        api.getLoopProfiles(),
        api.getGateCatalog(),
      ]);
      setProfiles(profilesData);
      setCatalog(catalogData.gates);
      setDirty(new Set());
    } catch {
      message.error("Failed to load loop profiles");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  const currentProfile = profiles.find((p) => p.name === activeProfile);
  const selectedGate =
    currentProfile?.gates.find((g) => g.id === selectedGateId) ?? null;

  const handleToggleGate = useCallback(
    (gateId: string, enabled: boolean) => {
      setProfiles((prev) =>
        prev.map((p) =>
          p.name === activeProfile
            ? {
                ...p,
                gates: p.gates.map((g) =>
                  g.id === gateId ? { ...g, enabled } : g,
                ),
              }
            : p,
        ),
      );
      setDirty((prev) => new Set(prev).add(activeProfile));
    },
    [activeProfile],
  );

  const handleParamsChange = useCallback(
    (gateId: string, params: Record<string, unknown>) => {
      setProfiles((prev) =>
        prev.map((p) =>
          p.name === activeProfile
            ? {
                ...p,
                gates: p.gates.map((g) =>
                  g.id === gateId ? { ...g, params } : g,
                ),
              }
            : p,
        ),
      );
      setDirty((prev) => new Set(prev).add(activeProfile));
    },
    [activeProfile],
  );

  const handleAddGate = useCallback(
    (entry: GateCatalogEntry) => {
      if (!currentProfile || currentProfile.is_builtin) return;
      const newGate: ProfileGateInfo = {
        id: `${activeProfile}-${entry.type}-${Date.now()}`,
        type: entry.type,
        name: entry.name,
        description: entry.description,
        category: entry.category,
        enabled: true,
        priority: entry.default_priority,
        params: {},
        params_schema: entry.params_schema,
      };
      setProfiles((prev) =>
        prev.map((p) =>
          p.name === activeProfile
            ? { ...p, gates: [...p.gates, newGate] }
            : p,
        ),
      );
      setDirty((prev) => new Set(prev).add(activeProfile));
      setSelectedGateId(newGate.id);
    },
    [activeProfile, currentProfile],
  );

  const handleRemoveGate = useCallback(
    (gateId: string) => {
      if (!currentProfile || currentProfile.is_builtin) return;
      setProfiles((prev) =>
        prev.map((p) =>
          p.name === activeProfile
            ? { ...p, gates: p.gates.filter((g) => g.id !== gateId) }
            : p,
        ),
      );
      if (selectedGateId === gateId) setSelectedGateId(null);
      setDirty((prev) => new Set(prev).add(activeProfile));
    },
    [activeProfile, currentProfile, selectedGateId],
  );

  const handleCreateProfile = useCallback(async () => {
    const name = newProfileName.trim().toLowerCase().replace(/\s+/g, "_");
    if (!name) {
      message.warning("Profile name is required");
      return;
    }
    if (profiles.some((p) => p.name === name)) {
      message.warning(`Profile "${name}" already exists`);
      return;
    }
    try {
      await api.createLoopProfile(name, newProfileDesc.trim(), []);
      setCreateModalOpen(false);
      setNewProfileName("");
      setNewProfileDesc("");
      await fetchData();
      setActiveProfile(name);
    } catch {
      message.error("Failed to create profile");
    }
  }, [newProfileName, newProfileDesc, profiles, fetchData]);

  const handleDeleteProfile = useCallback(async () => {
    if (!currentProfile || currentProfile.is_builtin) return;
    Modal.confirm({
      title: `Delete "${activeProfile}"?`,
      content: "This action cannot be undone.",
      okText: "Delete",
      okButtonProps: { danger: true },
      onOk: async () => {
        try {
          await api.deleteLoopProfile(activeProfile);
          setActiveProfile("default");
          await fetchData();
          message.success("Profile deleted");
        } catch {
          message.error("Failed to delete profile");
        }
      },
    });
  }, [currentProfile, activeProfile, fetchData]);

  const handleSave = useCallback(async () => {
    if (!currentProfile) return;
    try {
      setSaving(true);
      const gatesPayload = currentProfile.gates.map((g, idx) => ({
        id: g.id,
        type: g.type,
        enabled: g.enabled,
        priority: (idx + 1) * 10,
        params: g.params,
      }));
      if (currentProfile.is_builtin) {
        await api.updateLoopProfile(
          currentProfile.name,
          gatesPayload,
        );
      } else {
        await api.createLoopProfile(
          currentProfile.name,
          currentProfile.description || "",
          gatesPayload,
        );
      }
      setDirty((prev) => {
        const next = new Set(prev);
        next.delete(activeProfile);
        return next;
      });
      message.success(`Profile "${activeProfile}" saved`);
    } catch {
      message.error(`Failed to save profile "${activeProfile}"`);
    } finally {
      setSaving(false);
    }
  }, [currentProfile, activeProfile]);

  if (loading) {
    return (
      <div className={s.loopCard}>
        <div className={s.loadingCenter}>
          <Loader2 size={22} className={s.spinner} />
        </div>
      </div>
    );
  }

  return (
    <div className={s.loopCard}>
      {/* Profile Tabs */}
      <nav className={s.profileTabs}>
        {profiles.map((profile) => {
          const Icon = PROFILE_ICONS[profile.name] || Gauge;
          return (
            <button
              key={profile.name}
              className={`${s.profileTab} ${activeProfile === profile.name ? s.active : ""}`}
              onClick={() => {
                setActiveProfile(profile.name);
                setSelectedGateId(null);
              }}
            >
              <Icon size={13} />
              {profile.name.charAt(0).toUpperCase() + profile.name.slice(1)}
              {dirty.has(profile.name) && <span className={s.dirtyDot} />}
            </button>
          );
        })}
        <button
          className={`${s.profileTab} ${s.addTab}`}
          title="Create custom profile"
          onClick={() => setCreateModalOpen(true)}
        >
          <Plus size={13} />
        </button>
      </nav>

      {/* Toolbar */}
      {currentProfile && (
        <div className={s.toolbar}>
          <div className={s.modeToggle}>
            <button
              className={`${s.modeBtn} ${mode === "simple" ? s.active : ""}`}
              onClick={() => setMode("simple")}
            >
              Simple
            </button>
            <button
              className={`${s.modeBtn} ${mode === "advanced" ? s.active : ""}`}
              onClick={() => setMode("advanced")}
            >
              Pipeline
            </button>
          </div>
          <div className={s.toolbarRight}>
            <span className={s.scopeBadge}>
              scope: {currentProfile.scope}
            </span>
            {currentProfile.is_builtin ? (
              <span
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 3,
                  fontSize: 11,
                  color: "var(--text-quaternary, #9c9c96)",
                }}
              >
                <Lock size={10} />
                Structure locked
              </span>
            ) : (
              <button
                onClick={handleDeleteProfile}
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 3,
                  fontSize: 11,
                  color: "#e53e3e",
                  background: "none",
                  border: "none",
                  cursor: "pointer",
                  padding: "2px 6px",
                  borderRadius: 4,
                }}
              >
                <Trash2 size={11} />
                Delete Profile
              </button>
            )}
          </div>
        </div>
      )}

      {/* Orchestrator (advanced mode) */}
      {currentProfile && mode === "advanced" && (
        <>
          <div className={s.orchestrator}>
            <CatalogPanel
              catalog={catalog}
              isBuiltin={currentProfile.is_builtin}
              onAddGate={handleAddGate}
            />
            <PipelinePanel
              profile={currentProfile}
              selectedGateId={selectedGateId}
              onSelectGate={setSelectedGateId}
              onToggleGate={handleToggleGate}
              onRemoveGate={
                currentProfile.is_builtin ? undefined : handleRemoveGate
              }
            />
            <InspectorPanel
              gate={selectedGate}
              onParamsChange={handleParamsChange}
            />
          </div>
          <FlowPreview profile={currentProfile} />
        </>
      )}

      {/* Simple mode */}
      {currentProfile && mode === "simple" && (
        <SimpleMode
          profile={currentProfile}
          onToggleGate={handleToggleGate}
          onParamsChange={handleParamsChange}
        />
      )}

      {/* Save bar */}
      <div className={s.saveBar}>
        <Button
          size="small"
          onClick={fetchData}
          disabled={saving}
          style={{ fontSize: 12, display: "inline-flex", alignItems: "center", gap: 4 }}
        >
          <RotateCcw size={12} />
          {t("common.reset", "Reset")}
        </Button>
        <Button
          type="primary"
          size="small"
          loading={saving}
          disabled={!dirty.has(activeProfile)}
          onClick={handleSave}
          style={{ fontSize: 12, display: "inline-flex", alignItems: "center", gap: 4 }}
        >
          <Save size={12} />
          {t("common.save", "Save")}
        </Button>
      </div>

      {/* Create Profile Modal */}
      <Modal
        title="Create Custom Profile"
        open={createModalOpen}
        onCancel={() => {
          setCreateModalOpen(false);
          setNewProfileName("");
          setNewProfileDesc("");
        }}
        onOk={handleCreateProfile}
        okText="Create"
      >
        <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          <div>
            <label
              style={{ fontSize: 12, fontWeight: 500, marginBottom: 4, display: "block" }}
            >
              Profile Name
            </label>
            <Input
              value={newProfileName}
              onChange={(e) => setNewProfileName(e.target.value)}
              placeholder="e.g. strict, relaxed, custom_flow"
              size="small"
            />
          </div>
          <div>
            <label
              style={{ fontSize: 12, fontWeight: 500, marginBottom: 4, display: "block" }}
            >
              Description (optional)
            </label>
            <Input.TextArea
              value={newProfileDesc}
              onChange={(e) => setNewProfileDesc(e.target.value)}
              placeholder="Describe the purpose of this profile"
              autoSize={{ minRows: 2, maxRows: 4 }}
              size="small"
            />
          </div>
        </div>
      </Modal>
    </div>
  );
}
