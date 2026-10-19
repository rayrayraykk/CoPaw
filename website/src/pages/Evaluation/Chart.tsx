import { useTranslation } from "react-i18next";
import { useEffect, useRef, useState } from "react";
import { Download, RotateCcw, ZoomIn, ZoomOut } from "lucide-react";
import { bestRows, frontier } from "./metrics";
import { logo, money, type Row } from "./types";

export default function Chart({
  rows,
  title,
  compareHarnesses,
  history,
  onSelect,
}: {
  rows: Row[];
  title: string;
  compareHarnesses: boolean;
  history: boolean;
  onSelect: (row: Row) => void;
}) {
  const { t, i18n } = useTranslation("evaluation");
  const container = useRef<HTMLDivElement>(null);
  const svg = useRef<SVGSVGElement>(null);
  const [width, setWidth] = useState(1000);
  const [hovered, setHovered] = useState<string | null>(null);
  const [zoom, setZoom] = useState(1);
  useEffect(() => {
    const observer = new ResizeObserver(([entry]) =>
      setWidth(entry.contentRect.width),
    );
    observer.observe(container.current!);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    setZoom(1);
    setHovered(null);
  }, [rows, history, compareHarnesses]);
  const mobile = width < 700;
  const height = mobile ? 380 : 520;
  const left = mobile ? 48 : 78,
    right = width - (mobile ? 18 : 46);
  const top = 66,
    bottom = height - 64;
  const best = bestRows(rows, compareHarnesses);
  const shown = rows.filter(
    (r) => r.score !== null && (history || best.has(r.id)),
  );
  const known = shown.filter((r) => r.cost !== null && r.cost > 0);
  const missing = shown.filter((r) => r.cost === null || r.cost === 0);
  const end = missing.length ? left + (right - left) * 0.87 : right;
  const min = known.length
    ? Math.log10(Math.min(...known.map((r) => r.cost!))) - 0.22
    : -2;
  const max = known.length
    ? Math.log10(Math.max(...known.map((r) => r.cost!))) + 0.22
    : 1;
  const center = (min + max) / 2,
    low = center - (max - min) / (2 * zoom),
    high = center + (max - min) / (2 * zoom);
  const floor = shown.length
    ? Math.max(
        0,
        Math.floor((Math.min(...shown.map((r) => r.score!)) - 8) / 10) * 10,
      )
    : 0;
  const x = (cost: number) =>
    left + ((Math.log10(cost) - low) / (high - low)) * (end - left);
  const y = (score: number) =>
    bottom - ((score - floor) / (103 - floor)) * (bottom - top);
  const position = (r: Row) =>
    r.cost !== null && r.cost > 0
      ? x(r.cost)
      : end + (right - end) * (0.4 + (missing.indexOf(r) % 4) * 0.18);
  const pareto = frontier(shown.filter((r) => best.has(r.id))).filter(
    (r) => r.cost! > 0,
  );
  const ticks: number[] = [];
  for (
    let exponent = Math.floor(low);
    exponent <= Math.ceil(high);
    exponent++
  ) {
    for (const n of [1, 2, 3, 5]) {
      const cost = n * 10 ** exponent;
      if (Math.log10(cost) >= low && Math.log10(cost) <= high) ticks.push(cost);
    }
  }
  const number = (value: number) =>
    new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 3 }).format(
      value,
    );
  const active = shown.find((r) => r.id === hovered);
  const exportImage = async () => {
    const copy = svg.current!.cloneNode(true) as SVGSVGElement;
    const style = getComputedStyle(container.current!);
    copy.setAttribute("xmlns", "http://www.w3.org/2000/svg");
    copy.style.color = style.getPropertyValue("--eval-text");
    copy.style.background = style.getPropertyValue("--eval-surface");
    for (const name of ["--eval-border", "--eval-accent"])
      copy.style.setProperty(name, style.getPropertyValue(name));
    await Promise.all(
      Array.from(copy.querySelectorAll("image")).map(async (image) => {
        const response = await fetch(image.getAttribute("href")!);
        const blob = await response.blob();
        const data = await new Promise<string>((resolve) => {
          const reader = new FileReader();
          reader.onload = () => resolve(reader.result as string);
          reader.readAsDataURL(blob);
        });
        image.setAttribute("href", data);
      }),
    );
    const url = URL.createObjectURL(
      new Blob([new XMLSerializer().serializeToString(copy)], {
        type: "image/svg+xml",
      }),
    );
    const link = document.createElement("a");
    link.href = url;
    link.download = "qwenpaw-evaluation.svg";
    link.click();
    URL.revokeObjectURL(url);
  };
  return (
    <div className="evaluation-plot evaluation-svg-chart" ref={container}>
      <div className="evaluation-chart-tools">
        <button
          aria-label={t("zoomIn")}
          title={t("zoomIn")}
          onClick={() => setZoom((z) => Math.min(8, z * 1.5))}
        >
          <ZoomIn size={16} />
        </button>
        <button
          aria-label={t("zoomOut")}
          title={t("zoomOut")}
          onClick={() => setZoom((z) => Math.max(1, z / 1.5))}
        >
          <ZoomOut size={16} />
        </button>
        <button
          aria-label={t("resetChart")}
          title={t("resetChart")}
          onClick={() => setZoom(1)}
        >
          <RotateCcw size={16} />
        </button>
        <button
          aria-label={t("exportChart")}
          title={t("exportChart")}
          onClick={() => void exportImage()}
        >
          <Download size={16} />
        </button>
      </div>
      <svg
        ref={svg}
        viewBox={`0 0 ${width} ${height}`}
        role="img"
        aria-label={t("chartAlt", { title })}
        onDoubleClick={() => setZoom(1)}
      >
        <text x={mobile ? 16 : 28} y={28} fontSize={mobile ? 13 : 16}>
          {t("chartTitle", { title })}
        </text>
        {ticks.map((cost) => (
          <g key={cost}>
            <line
              x1={x(cost)}
              x2={x(cost)}
              y1={top}
              y2={bottom}
              stroke="var(--eval-border)"
            />
            <text x={x(cost)} y={bottom + 22} textAnchor="middle" fontSize={11}>
              {money(cost, i18n.language, t("unknown"))}
            </text>
          </g>
        ))}
        {Array.from(
          { length: (100 - floor) / 10 + 1 },
          (_, i) => floor + i * 10,
        ).map((score) => (
          <g key={score}>
            <line
              x1={left}
              x2={right}
              y1={y(score)}
              y2={y(score)}
              stroke="var(--eval-border)"
            />
            <text x={left - 12} y={y(score) + 4} textAnchor="end" fontSize={12}>
              {score}
            </text>
          </g>
        ))}
        <text
          x={(left + end) / 2}
          y={height - 12}
          textAnchor="middle"
          fontSize={mobile ? 11 : 14}
        >
          {t("xAxis")}
        </text>
        <text
          transform={`translate(18 ${(top + bottom) / 2}) rotate(-90)`}
          textAnchor="middle"
          fontSize={mobile ? 11 : 14}
        >
          {t("averageScore")}
        </text>
        {missing.length > 0 && (
          <>
            <line
              x1={end + 12}
              x2={end + 12}
              y1={top}
              y2={bottom}
              stroke="var(--eval-border)"
              strokeDasharray="3 4"
            />
            <text x={right} y={top - 14} textAnchor="end" fontSize={10}>
              {t("noCost")}
            </text>
          </>
        )}
        <svg
          x={left - 16}
          y={top - 30}
          width={right - left + 32}
          height={bottom - top + 46}
          viewBox={`${left - 16} ${top - 30} ${right - left + 32} ${
            bottom - top + 46
          }`}
        >
          <polyline
            points={pareto.map((r) => `${x(r.cost!)},${y(r.score!)}`).join(" ")}
            fill="none"
            stroke="var(--eval-accent)"
            strokeWidth={1.5}
            strokeDasharray="7 6"
          />
          {pareto.map((r) => (
            <text
              key={r.id}
              x={position(r)}
              y={y(r.score!) - 23}
              textAnchor="middle"
              fontSize={mobile ? 9 : 11}
            >
              {r.model}
            </text>
          ))}
          {shown.map((r) => (
            <g
              key={r.id}
              className="evaluation-chart-point"
              tabIndex={0}
              role="button"
              aria-label={`${r.model}: ${number(r.score!)}`}
              opacity={best.has(r.id) ? 1 : 0.23}
              onPointerEnter={() => setHovered(r.id)}
              onPointerLeave={() => setHovered(null)}
              onFocus={() => setHovered(r.id)}
              onBlur={() => setHovered(null)}
              onClick={() => onSelect(r)}
              onKeyDown={(e) => {
                if (e.key === "Enter") onSelect(r);
              }}
            >
              <circle
                cx={position(r)}
                cy={y(r.score!)}
                r={20}
                fill="transparent"
              />
              {logo(r.model) ? (
                <image
                  href={logo(r.model)}
                  x={position(r) - 16}
                  y={y(r.score!) - 16}
                  width={32}
                  height={32}
                  pointerEvents="none"
                />
              ) : (
                <circle
                  cx={position(r)}
                  cy={y(r.score!)}
                  r={7}
                  fill="var(--eval-accent)"
                />
              )}
            </g>
          ))}
        </svg>
        {!shown.length && (
          <text x={width / 2} y={height / 2} textAnchor="middle">
            {t("noComplete")}
          </text>
        )}
      </svg>
      {active && (
        <div
          role="tooltip"
          className="evaluation-point-tooltip"
          style={{
            left: Math.max(
              8,
              Math.min(
                width - Math.min(290, width - 16),
                position(active) + 20,
              ),
            ),
            top: Math.max(44, y(active.score!) - 140),
          }}
        >
          <strong>{active.model}</strong>
          <br />
          {active.harness === "qwenpaw"
            ? t("qwenpawDefault")
            : active.harness}{" "}
          · SDK {active.sdk_version}
          <br />
          {t("score")}: {number(active.score!)}
          <br />
          {t("averageCost")}: {money(active.cost, i18n.language, t("unknown"))}
          {active.upperBound ? ` (${t("upperBound")})` : ""}
          <br />
          {active.partialCost && (
            <>
              {t("costCoverage", {
                known: active.costKnown,
                total: active.costTotal,
              })}
              <br />
            </>
          )}
          {t("averageRuntime")}:{" "}
          {active.runtime === null
            ? t("unknown")
            : t("seconds", { value: number(Math.round(active.runtime)) })}
          <br />
          {new Intl.DateTimeFormat(i18n.language, {
            dateStyle: "medium",
            timeZone: "UTC",
          }).format(new Date(active.run.date))}{" "}
          · {best.has(active.id) ? t("bestShort") : t("history")}
        </div>
      )}
    </div>
  );
}
