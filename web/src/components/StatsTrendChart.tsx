// web/src/components/StatsTrendChart.tsx
import React, { useMemo, useState } from 'react';

export interface StatsTrendSeries {
  key: string;
  label: string;
  /** Hex stroke/fill color, e.g. "#10b981". */
  color: string;
  /** Aligned 1:1 with the chart's `timestamps` array. */
  values: number[];
}

interface StatsTrendChartProps {
  /** Ascending, milliseconds since epoch. */
  timestamps: number[];
  series: StatsTrendSeries[];
  height?: number;
  title?: string;
  formatValue?: (v: number) => string;
}

function defaultFormatValue(v: number): string {
  if (v >= 1_000_000) return `${(v / 1_000_000).toFixed(1)}M`;
  if (v >= 1_000) return `${(v / 1_000).toFixed(1)}K`;
  return `${Math.round(v)}`;
}

function formatAgo(msAgo: number): string {
  const secs = msAgo / 1000;
  if (secs >= 86400) return `-${Math.round(secs / 86400)}d`;
  if (secs >= 3600) return `-${Math.round(secs / 3600)}h`;
  if (secs >= 60) return `-${Math.round(secs / 60)}m`;
  return `-${Math.round(secs)}s`;
}

/** A small generalized version of BandwidthChart's hand-rolled SVG line chart — fixed
 * viewBox, smooth cubic-Bezier paths, mouse-coordinate hover with an overlay tooltip — but
 * for N named series instead of a hardcoded download/upload pair, so the Stats page's several
 * trend charts (torrent count, size, seeders/leechers) don't need three near-duplicates. */
export const StatsTrendChart: React.FC<StatsTrendChartProps> = ({
  timestamps,
  series,
  height = 180,
  title,
  formatValue = defaultFormatValue,
}) => {
  const [hoverIndex, setHoverIndex] = useState<number | null>(null);

  const svgWidth = 650;
  const marginLeft = 54;
  const marginRight = 15;
  const marginTop = 12;
  const marginBottom = 24;
  const plotWidth = svgWidth - marginLeft - marginRight;
  const plotHeight = height - marginTop - marginBottom;

  const { paths, yTicks, xTicks, pointXs } = useMemo(() => {
    let maxFound = 1;
    for (const s of series) {
      for (const v of s.values) {
        if (v > maxFound) maxFound = v;
      }
    }
    const peak = maxFound * 1.1;

    const yTickList = [1, 0.75, 0.5, 0.25, 0].map((ratio) => ({
      y: marginTop + plotHeight * (1 - ratio),
      val: formatValue(peak * ratio),
    }));

    const n = timestamps.length;
    if (n < 2) {
      return { paths: [], yTicks: yTickList, xTicks: [], pointXs: [] as number[] };
    }

    const stepX = plotWidth / (n - 1);
    const xs = timestamps.map((_, i) => marginLeft + i * stepX);

    const now = timestamps[n - 1];
    const xTickIdxs = [0, Math.floor((n - 1) * 0.25), Math.floor((n - 1) * 0.5), Math.floor((n - 1) * 0.75), n - 1];
    const xTickList = xTickIdxs.map((i, idx) => ({
      x: xs[i],
      label: idx === xTickIdxs.length - 1 ? 'Now' : formatAgo(now - timestamps[i]),
    }));

    const seriesPaths = series.map((s) => {
      const pts: [number, number][] = s.values.map((v, i) => {
        const y = marginTop + plotHeight - (v / peak) * plotHeight;
        return [xs[i], Math.max(marginTop, Math.min(marginTop + plotHeight, y))];
      });
      let d = `M ${pts[0][0]},${pts[0][1]}`;
      for (let i = 1; i < pts.length; i++) {
        const [px, py] = pts[i - 1];
        const [cx2, cy2] = pts[i];
        const cx = (px + cx2) / 2;
        d += ` C ${cx},${py} ${cx},${cy2} ${cx2},${cy2}`;
      }
      return { key: s.key, label: s.label, color: s.color, d, points: pts };
    });

    return { paths: seriesPaths, yTicks: yTickList, xTicks: xTickList, pointXs: xs };
  }, [timestamps, series, plotWidth, plotHeight, marginLeft, marginTop, formatValue]);

  const handleMouseMove = (e: React.MouseEvent<SVGSVGElement>) => {
    if (pointXs.length < 2) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = ((e.clientX - rect.left) / rect.width) * svgWidth;
    if (x < marginLeft || x > marginLeft + plotWidth) {
      setHoverIndex(null);
      return;
    }
    const ratio = (x - marginLeft) / plotWidth;
    setHoverIndex(Math.min(pointXs.length - 1, Math.max(0, Math.round(ratio * (pointXs.length - 1)))));
  };

  const activeX = hoverIndex !== null ? pointXs[hoverIndex] : null;

  return (
    <div className="flex flex-col space-y-2 relative">
      {title && (
        <div className="flex items-center justify-between text-xs">
          <span className="font-bold text-slate-200 uppercase tracking-wider">{title}</span>
          <div className="flex items-center space-x-3">
            {series.map((s) => (
              <span key={s.key} className="flex items-center space-x-1.5 font-mono text-[10px]" style={{ color: s.color }}>
                <span className="h-2 w-2 rounded-full" style={{ backgroundColor: s.color }} />
                <span>{s.label}</span>
              </span>
            ))}
          </div>
        </div>
      )}

      <div className="relative w-full rounded-xl border border-slate-800 bg-slate-950/70 p-2 overflow-visible">
        <svg
          viewBox={`0 0 ${svgWidth} ${height}`}
          className="w-full h-full overflow-visible cursor-pointer select-none"
          preserveAspectRatio="none"
          onMouseMove={handleMouseMove}
          onMouseLeave={() => setHoverIndex(null)}
        >
          {yTicks.map((t, idx) => (
            <g key={`y-${idx}`}>
              <line
                x1={marginLeft}
                y1={t.y}
                x2={marginLeft + plotWidth}
                y2={t.y}
                stroke="#334155"
                strokeWidth={idx === yTicks.length - 1 ? 1 : 0.5}
                strokeDasharray={idx === yTicks.length - 1 ? undefined : '3 3'}
                opacity={idx === yTicks.length - 1 ? 0.8 : 0.4}
              />
              <text x={marginLeft - 8} y={t.y + 3.5} textAnchor="end" className="fill-slate-500 font-mono text-[9px]">
                {t.val}
              </text>
            </g>
          ))}

          {xTicks.map((t, idx) => (
            <g key={`x-${idx}`}>
              <line
                x1={t.x}
                y1={marginTop}
                x2={t.x}
                y2={marginTop + plotHeight}
                stroke="#334155"
                strokeWidth={0.5}
                strokeDasharray="2 2"
                opacity={0.3}
              />
              <text
                x={t.x}
                y={marginTop + plotHeight + 14}
                textAnchor={idx === 0 ? 'start' : idx === xTicks.length - 1 ? 'end' : 'middle'}
                className="fill-slate-500 font-mono text-[9px]"
              >
                {t.label}
              </text>
            </g>
          ))}

          {paths.map((p) => (
            <path key={p.key} d={p.d} fill="none" stroke={p.color} strokeWidth="2" strokeLinecap="round" />
          ))}

          {activeX !== null && (
            <g>
              <line
                x1={activeX}
                y1={marginTop}
                x2={activeX}
                y2={marginTop + plotHeight}
                stroke="#94a3b8"
                strokeWidth={1}
                strokeDasharray="3 3"
                opacity={0.8}
              />
              {paths.map((p) => (
                <circle key={p.key} cx={activeX} cy={p.points[hoverIndex as number][1]} r={4} fill={p.color} stroke="#ffffff" strokeWidth={1.5} />
              ))}
            </g>
          )}
        </svg>

        {activeX !== null && hoverIndex !== null && (
          <div
            className="absolute z-30 w-56 rounded-xl border border-slate-700/90 bg-slate-900/98 p-2.5 shadow-2xl backdrop-blur-lg text-[11px] font-mono text-slate-200 pointer-events-none"
            style={{
              left: activeX / svgWidth > 0.52 ? undefined : `${Math.max(6, ((activeX + 15) / svgWidth) * 100)}%`,
              right: activeX / svgWidth > 0.52 ? `${Math.max(6, ((svgWidth - activeX + 15) / svgWidth) * 100)}%` : undefined,
              top: '6px',
            }}
          >
            <div className="text-[9.5px] text-slate-500 font-sans border-b border-slate-800 pb-1 mb-1.5">
              {new Date(timestamps[hoverIndex]).toLocaleString()}
            </div>
            <div className="space-y-1">
              {series.map((s, idx) => (
                <div key={s.key} className="flex items-center justify-between gap-2">
                  <span className="flex items-center space-x-1.5 text-slate-400">
                    <span className="h-2 w-2 rounded-full" style={{ backgroundColor: s.color }} />
                    <span className="font-sans">{s.label}</span>
                  </span>
                  <span className="font-semibold" style={{ color: s.color }}>
                    {formatValue(s.values[hoverIndex] ?? 0)}
                  </span>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
