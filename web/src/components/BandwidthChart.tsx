// web/src/components/BandwidthChart.tsx
import React, { useMemo, useState, useRef, useEffect } from 'react';
import { ArrowDown, ArrowUp, Activity, Pin, X, ExternalLink } from 'lucide-react';
import { BandwidthDataPoint } from '../types';

export type { BandwidthDataPoint };

interface BandwidthChartProps {
  history: BandwidthDataPoint[];
  height?: number;
  showCurrentStats?: boolean;
  title?: string;
  timeWindowSecs?: number; // default 300 (5 minutes)
  onViewTorrent?: (compoundId: string) => void;
}

function formatSpeed(bytesPerSec: number): string {
  if (!bytesPerSec || bytesPerSec <= 0) return '0 B/s';
  const k = 1024;
  const sizes = ['B/s', 'KB/s', 'MB/s', 'GB/s', 'TB/s'];
  const i = Math.floor(Math.log(bytesPerSec) / Math.log(k));
  return `${(bytesPerSec / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

function formatRelativeTime(secondsAgo: number): string {
  if (secondsAgo <= 2) return 'Just now';
  if (secondsAgo < 60) return `${Math.round(secondsAgo)}s ago`;
  const mins = Math.floor(secondsAgo / 60);
  const secs = Math.round(secondsAgo % 60);
  return secs > 0 ? `${mins}m ${secs}s ago` : `${mins}m ago`;
}

export const BandwidthChart: React.FC<BandwidthChartProps> = ({
  history,
  height = 160,
  showCurrentStats = true,
  title,
  timeWindowSecs = 300,
  onViewTorrent,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const [hoverIndex, setHoverIndex] = useState<number | null>(null);
  const [pinnedIndex, setPinnedIndex] = useState<number | null>(null);

  const currentDown = history.length > 0 ? history[history.length - 1].downloadSpeed : 0;
  const currentUp = history.length > 0 ? history[history.length - 1].uploadSpeed : 0;

  // Chart dimensions & margins
  const svgWidth = 650;
  const marginLeft = 60;
  const marginRight = 15;
  const marginTop = 12;
  const marginBottom = 24;
  const plotWidth = svgWidth - marginLeft - marginRight;
  const plotHeight = height - marginTop - marginBottom;

  // Unpin on Escape key press
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && pinnedIndex !== null) {
        setPinnedIndex(null);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [pinnedIndex]);

  const { peak, downPath, upPath, downAreaPath, upAreaPath, yTicks, xTicks, pointsCoords } = useMemo(() => {
    let maxFound = 1024 * 1024; // min 1 MB/s scale
    for (const pt of history) {
      if (pt.downloadSpeed > maxFound) maxFound = pt.downloadSpeed;
      if (pt.uploadSpeed > maxFound) maxFound = pt.uploadSpeed;
    }
    // Add 10% headroom
    const calculatedPeak = maxFound * 1.1;

    // Y Ticks (0%, 25%, 50%, 75%, 100%)
    const yTickList = [
      { y: marginTop, val: formatSpeed(calculatedPeak), ratio: 1.0 },
      { y: marginTop + plotHeight * 0.25, val: formatSpeed(calculatedPeak * 0.75), ratio: 0.75 },
      { y: marginTop + plotHeight * 0.50, val: formatSpeed(calculatedPeak * 0.50), ratio: 0.50 },
      { y: marginTop + plotHeight * 0.75, val: formatSpeed(calculatedPeak * 0.25), ratio: 0.25 },
      { y: marginTop + plotHeight, val: '0 B/s', ratio: 0.0 },
    ];

    // X Ticks according to timeWindowSecs — hour-scale windows (long-range history) get "-Xh"
    // labels instead of an unreadable "-4320s".
    const formatAgo = (secs: number) => {
      if (secs >= 3600) return `-${Math.round(secs / 3600)}h`;
      if (secs >= 60) return `-${Math.round(secs / 60)}m`;
      return `-${Math.round(secs)}s`;
    };
    const xTickList =
      timeWindowSecs >= 300
        ? [
            { x: marginLeft, label: formatAgo(timeWindowSecs) },
            { x: marginLeft + plotWidth * 0.2, label: formatAgo(timeWindowSecs * 0.8) },
            { x: marginLeft + plotWidth * 0.4, label: formatAgo(timeWindowSecs * 0.6) },
            { x: marginLeft + plotWidth * 0.6, label: formatAgo(timeWindowSecs * 0.4) },
            { x: marginLeft + plotWidth * 0.8, label: formatAgo(timeWindowSecs * 0.2) },
            { x: marginLeft + plotWidth, label: 'Now' },
          ]
        : [
            { x: marginLeft, label: `-${timeWindowSecs}s` },
            { x: marginLeft + plotWidth * 0.25, label: `-${Math.round(timeWindowSecs * 0.75)}s` },
            { x: marginLeft + plotWidth * 0.50, label: `-${Math.round(timeWindowSecs * 0.50)}s` },
            { x: marginLeft + plotWidth * 0.75, label: `-${Math.round(timeWindowSecs * 0.25)}s` },
            { x: marginLeft + plotWidth, label: 'Now' },
          ];

    if (history.length < 2) {
      return {
        peak: calculatedPeak,
        downPath: '',
        upPath: '',
        downAreaPath: '',
        upAreaPath: '',
        yTicks: yTickList,
        xTicks: xTickList,
        pointsCoords: [],
      };
    }

    const pointsCount = history.length;
    const stepX = plotWidth / (pointsCount - 1);

    const downPoints: [number, number][] = [];
    const upPoints: [number, number][] = [];
    const coords: { x: number; downY: number; upY: number; pt: BandwidthDataPoint }[] = [];

    history.forEach((pt, i) => {
      const x = marginLeft + i * stepX;
      const downY = marginTop + plotHeight - (pt.downloadSpeed / calculatedPeak) * plotHeight;
      const upY = marginTop + plotHeight - (pt.uploadSpeed / calculatedPeak) * plotHeight;
      const clampedDownY = Math.max(marginTop, Math.min(marginTop + plotHeight, downY));
      const clampedUpY = Math.max(marginTop, Math.min(marginTop + plotHeight, upY));
      downPoints.push([x, clampedDownY]);
      upPoints.push([x, clampedUpY]);
      coords.push({ x, downY: clampedDownY, upY: clampedUpY, pt });
    });

    const createSmoothPath = (pts: [number, number][]) => {
      if (pts.length === 0) return '';
      let d = `M ${pts[0][0]},${pts[0][1]}`;
      for (let i = 1; i < pts.length; i++) {
        const prev = pts[i - 1];
        const curr = pts[i];
        const cx = (prev[0] + curr[0]) / 2;
        d += ` C ${cx},${prev[1]} ${cx},${curr[1]} ${curr[0]},${curr[1]}`;
      }
      return d;
    };

    const dPath = createSmoothPath(downPoints);
    const uPath = createSmoothPath(upPoints);

    const bottomY = marginTop + plotHeight;
    const dArea = dPath
      ? `${dPath} L ${marginLeft + plotWidth},${bottomY} L ${marginLeft},${bottomY} Z`
      : '';
    const uArea = uPath
      ? `${uPath} L ${marginLeft + plotWidth},${bottomY} L ${marginLeft},${bottomY} Z`
      : '';

    return {
      peak: calculatedPeak,
      downPath: dPath,
      upPath: uPath,
      downAreaPath: dArea,
      upAreaPath: uArea,
      yTicks: yTickList,
      xTicks: xTickList,
      pointsCoords: coords,
    };
  }, [history, height, plotWidth, plotHeight, marginLeft, marginRight, marginTop, marginBottom, timeWindowSecs]);

  const peakDown = useMemo(() => {
    return history.reduce((max, p) => Math.max(max, p.downloadSpeed), 0);
  }, [history]);

  const peakUp = useMemo(() => {
    return history.reduce((max, p) => Math.max(max, p.uploadSpeed), 0);
  }, [history]);

  const getPointIndexFromEvent = (e: React.MouseEvent<SVGSVGElement>): number | null => {
    if (pointsCoords.length < 2) return null;
    const svgRect = e.currentTarget.getBoundingClientRect();
    const clickX = ((e.clientX - svgRect.left) / svgRect.width) * svgWidth;
    if (clickX < marginLeft || clickX > marginLeft + plotWidth) {
      return null;
    }
    const ratio = (clickX - marginLeft) / plotWidth;
    return Math.min(
      pointsCoords.length - 1,
      Math.max(0, Math.round(ratio * (pointsCoords.length - 1)))
    );
  };

  const handleMouseMove = (e: React.MouseEvent<SVGSVGElement>) => {
    if (pinnedIndex !== null) return;
    const idx = getPointIndexFromEvent(e);
    setHoverIndex(idx);
  };

  const handleMouseLeave = () => {
    if (pinnedIndex !== null) return;
    setHoverIndex(null);
  };

  const handleChartClick = (e: React.MouseEvent<SVGSVGElement>) => {
    const idx = getPointIndexFromEvent(e);
    if (idx !== null) {
      setPinnedIndex((prev) => (prev === idx ? null : idx));
      setHoverIndex(idx);
    }
  };

  // The active point is pinned point if set, otherwise hovered point
  const activeIndex = pinnedIndex !== null ? pinnedIndex : hoverIndex;
  const activePoint = activeIndex !== null && pointsCoords[activeIndex] ? pointsCoords[activeIndex] : null;
  const isPinned = pinnedIndex !== null;

  return (
    <div className="flex flex-col space-y-2 relative" ref={containerRef}>
      {showCurrentStats && (
        <div className="flex flex-wrap items-center justify-between gap-2 border-b border-slate-800/80 pb-2 text-xs">
          <div className="flex items-center space-x-2">
            <Activity className="h-4 w-4 text-brand-400" />
            <span className="font-bold text-slate-200 uppercase tracking-wider">
              {title || 'Cluster Bandwidth Live Throughput & History'}
            </span>
            <span className="rounded bg-brand-500/10 px-1.5 py-0.5 text-[10px] font-semibold text-brand-400 border border-brand-500/20">
              {timeWindowSecs >= 3600 ? `${Math.round(timeWindowSecs / 3600)}h Window` : `${Math.round(timeWindowSecs / 60)}m Window`}
            </span>
          </div>

          <div className="flex items-center space-x-4">
            <div className="flex items-center space-x-1.5 font-mono">
              <span className="flex h-2 w-2 rounded-full bg-emerald-400"></span>
              <ArrowDown className="h-3.5 w-3.5 text-emerald-400" />
              <span className="font-bold text-emerald-400">{formatSpeed(currentDown)}</span>
              <span className="text-[10px] text-slate-500 font-sans">(Peak: {formatSpeed(peakDown)})</span>
            </div>

            <div className="flex items-center space-x-1.5 font-mono">
              <span className="flex h-2 w-2 rounded-full bg-sky-400"></span>
              <ArrowUp className="h-3.5 w-3.5 text-sky-400" />
              <span className="font-bold text-sky-400">{formatSpeed(currentUp)}</span>
              <span className="text-[10px] text-slate-500 font-sans">(Peak: {formatSpeed(peakUp)})</span>
            </div>
          </div>
        </div>
      )}

      {/* SVG Time-Series Chart with X and Y Axes Markers (overflow-visible to allow tooltip to float freely) */}
      <div className="relative w-full rounded-xl border border-slate-800 bg-slate-950/70 p-2 overflow-visible">
        <svg
          viewBox={`0 0 ${svgWidth} ${height}`}
          className="w-full h-full overflow-visible cursor-pointer select-none"
          preserveAspectRatio="none"
          onMouseMove={handleMouseMove}
          onMouseLeave={handleMouseLeave}
          onClick={handleChartClick}
        >
          <defs>
            <linearGradient id="downGradient" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#10b981" stopOpacity="0.4" />
              <stop offset="100%" stopColor="#10b981" stopOpacity="0.0" />
            </linearGradient>
            <linearGradient id="upGradient" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#0ea5e9" stopOpacity="0.35" />
              <stop offset="100%" stopColor="#0ea5e9" stopOpacity="0.0" />
            </linearGradient>
          </defs>

          {/* Y-Axis Gridlines & Speed Labels */}
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
              <text
                x={marginLeft - 8}
                y={t.y + 3.5}
                textAnchor="end"
                className="fill-slate-500 font-mono text-[9px]"
              >
                {t.val}
              </text>
            </g>
          ))}

          {/* X-Axis Gridlines & Time Labels */}
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

          {/* Area Fills */}
          {downAreaPath && <path d={downAreaPath} fill="url(#downGradient)" />}
          {upAreaPath && <path d={upAreaPath} fill="url(#upGradient)" />}

          {/* Stroke Lines */}
          {downPath && <path d={downPath} fill="none" stroke="#10b981" strokeWidth="2" strokeLinecap="round" />}
          {upPath && <path d={upPath} fill="none" stroke="#0ea5e9" strokeWidth="2" strokeLinecap="round" />}

          {/* Interactive Hover / Pinned Vertical Guide & Indicator Dots */}
          {activePoint && (
            <g>
              <line
                x1={activePoint.x}
                y1={marginTop}
                x2={activePoint.x}
                y2={marginTop + plotHeight}
                stroke={isPinned ? '#38bdf8' : '#94a3b8'}
                strokeWidth={isPinned ? 1.5 : 1}
                strokeDasharray={isPinned ? '4 2' : '3 3'}
                opacity={isPinned ? 1 : 0.8}
              />
              {/* Download dot */}
              <circle
                cx={activePoint.x}
                cy={activePoint.downY}
                r={isPinned ? 5 : 4}
                fill="#10b981"
                stroke="#ffffff"
                strokeWidth={isPinned ? 2 : 1.5}
              />
              {/* Upload dot */}
              <circle
                cx={activePoint.x}
                cy={activePoint.upY}
                r={isPinned ? 5 : 4}
                fill="#0ea5e9"
                stroke="#ffffff"
                strokeWidth={isPinned ? 2 : 1.5}
              />
            </g>
          )}
        </svg>

        {/* Hover / Pinned Tooltip Overlay */}
        {activePoint && (
          <div
            className={`absolute z-30 w-80 max-w-[92vw] rounded-xl border border-slate-700/90 bg-slate-900/98 p-3 shadow-2xl backdrop-blur-lg text-[11px] font-mono text-slate-200 transition-all duration-100 ${
              isPinned ? 'pointer-events-auto ring-2 ring-brand-500/40' : 'pointer-events-none'
            }`}
            style={{
              left: (activePoint.x / svgWidth) > 0.52
                ? undefined
                : `${Math.max(6, ((activePoint.x + 15) / svgWidth) * 100)}%`,
              right: (activePoint.x / svgWidth) > 0.52
                ? `${Math.max(6, ((svgWidth - activePoint.x + 15) / svgWidth) * 100)}%`
                : undefined,
              top: '6px',
            }}
          >
            {/* Header: Status, Timestamp & Pin / Close controls */}
            <div className="text-[10px] text-slate-400 font-sans font-medium border-b border-slate-800 pb-1.5 mb-2 flex items-center justify-between gap-2">
              <div className="flex items-center space-x-1.5">
                {isPinned ? (
                  <span className="flex items-center space-x-1 bg-brand-500/20 text-brand-300 px-1.5 py-0.5 rounded text-[9px] font-bold border border-brand-500/30">
                    <Pin className="h-2.5 w-2.5 inline" />
                    <span>PINNED</span>
                  </span>
                ) : (
                  <span className="text-slate-400 text-[9px] bg-slate-800 px-1.5 py-0.5 rounded font-medium">
                    Click graph to Pin
                  </span>
                )}
                <span className="font-semibold text-slate-200">
                  {formatRelativeTime((Date.now() - activePoint.pt.time) / 1000)}
                </span>
              </div>

              <div className="flex items-center space-x-2">
                <span className="text-slate-500 text-[9.5px]">
                  {new Date(activePoint.pt.time).toLocaleTimeString()}
                </span>
                {isPinned && (
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      setPinnedIndex(null);
                    }}
                    className="rounded p-0.5 text-slate-400 hover:bg-slate-800 hover:text-slate-200 transition-colors"
                    title="Unpin snapshot (Esc)"
                  >
                    <X className="h-3.5 w-3.5" />
                  </button>
                )}
              </div>
            </div>

            {/* Total Speed Summary */}
            <div className="grid grid-cols-2 gap-2 pb-2 mb-2 border-b border-slate-800/80">
              <div className="flex items-center gap-1.5 text-emerald-400 font-bold bg-emerald-950/40 px-2 py-1 rounded border border-emerald-900/40">
                <ArrowDown className="h-3 w-3 shrink-0" />
                <span className="truncate">DL: {formatSpeed(activePoint.pt.downloadSpeed)}</span>
              </div>
              <div className="flex items-center gap-1.5 text-sky-400 font-bold bg-sky-950/40 px-2 py-1 rounded border border-sky-900/40">
                <ArrowUp className="h-3 w-3 shrink-0" />
                <span className="truncate">UL: {formatSpeed(activePoint.pt.uploadSpeed)}</span>
              </div>
            </div>

            {/* Active Contributors / Connected Peers Breakdown */}
            <div>
              <div className="flex items-center justify-between text-[10px] font-sans font-bold uppercase tracking-wider text-slate-400 mb-1.5">
                <span>
                  {activePoint.pt.topPeers ? 'Connected Seeds & Clients' : 'Active Swarms / Contributors'}
                </span>
                {activePoint.pt.topPeers && activePoint.pt.topPeers.length > 0 && (
                  <span className="text-emerald-400 font-mono text-[9px]">
                    {activePoint.pt.topPeers.filter((p) => p.downloadSpeed > 0 || p.uploadSpeed > 0).length > 0
                      ? `${activePoint.pt.topPeers.filter((p) => p.downloadSpeed > 0 || p.uploadSpeed > 0).length} transferring`
                      : `${activePoint.pt.topPeers.length} in swarm`}
                  </span>
                )}
                {activePoint.pt.topTorrents && activePoint.pt.topTorrents.length > 0 && (
                  <span className="text-brand-400 font-mono text-[9px]">
                    {activePoint.pt.topTorrents.length} active
                  </span>
                )}
              </div>

              {activePoint.pt.topPeers && activePoint.pt.topPeers.length > 0 ? (
                <div className="space-y-1.5 max-h-60 overflow-y-auto pr-1">
                  {activePoint.pt.topPeers.map((p, idx) => {
                    const dlShare = activePoint.pt.downloadSpeed > 0 && p.downloadSpeed > 0
                      ? Math.round((p.downloadSpeed / activePoint.pt.downloadSpeed) * 100)
                      : 0;
                    const ulShare = activePoint.pt.uploadSpeed > 0 && p.uploadSpeed > 0
                      ? Math.round((p.uploadSpeed / activePoint.pt.uploadSpeed) * 100)
                      : 0;
                    const isSeed = (p.progress ?? 0) >= 0.999;

                    return (
                      <div
                        key={`${p.address}-${idx}`}
                        className="bg-slate-950/90 rounded-lg p-2 border border-slate-800/90 flex flex-col gap-1 text-[10px]"
                      >
                        <div className="flex items-center justify-between gap-1.5">
                          <div className="flex items-center space-x-1.5 min-w-0">
                            {isSeed ? (
                              <span className="px-1 py-0.2 rounded bg-emerald-500/20 text-emerald-300 border border-emerald-500/30 text-[8.5px] font-bold shrink-0">
                                🌱 Seed
                              </span>
                            ) : (
                              <span className="px-1 py-0.2 rounded bg-sky-500/20 text-sky-300 border border-sky-500/30 text-[8.5px] font-bold shrink-0">
                                💧 {Math.round((p.progress ?? 0) * 100)}%
                              </span>
                            )}
                            <span
                              className="font-sans font-medium text-slate-200 truncate max-w-[150px]"
                              title={p.clientName}
                            >
                              {p.clientName}
                            </span>
                            {p.isEncrypted && (
                              <span title="Encrypted peer connection" className="text-amber-400 text-[9px]">🔒</span>
                            )}
                          </div>
                          <span
                            className="bg-slate-800 px-1 py-0.2 rounded text-[8.5px] font-mono text-slate-400 shrink-0 truncate max-w-[110px]"
                            title={p.address}
                          >
                            {p.countryCode ? `${p.countryCode} · ` : ''}{p.address}
                          </span>
                        </div>

                        <div className="flex items-center justify-between text-[9.5px]">
                          {p.downloadSpeed > 0 ? (
                            <span className="text-emerald-400 font-semibold flex items-center gap-0.5">
                              <ArrowDown className="h-2.5 w-2.5 inline" />
                              {formatSpeed(p.downloadSpeed)}
                              {dlShare > 0 && (
                                <span className="text-[8px] text-emerald-500/80 font-normal">({dlShare}%)</span>
                              )}
                            </span>
                          ) : (
                            <span className="text-slate-600">0 B/s DL</span>
                          )}

                          {p.uploadSpeed > 0 ? (
                            <span className="text-sky-400 font-semibold flex items-center gap-0.5">
                              <ArrowUp className="h-2.5 w-2.5 inline" />
                              {formatSpeed(p.uploadSpeed)}
                              {ulShare > 0 && (
                                <span className="text-[8px] text-sky-500/80 font-normal">({ulShare}%)</span>
                              )}
                            </span>
                          ) : (
                            <span className="text-slate-600">0 B/s UL</span>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              ) : activePoint.pt.topTorrents && activePoint.pt.topTorrents.length > 0 ? (
                <div className="space-y-1.5 max-h-56 overflow-y-auto pr-1">
                  {activePoint.pt.topTorrents.map((t, idx) => {
                    const dlShare = activePoint.pt.downloadSpeed > 0 && t.downloadSpeed > 0
                      ? Math.round((t.downloadSpeed / activePoint.pt.downloadSpeed) * 100)
                      : 0;
                    const ulShare = activePoint.pt.uploadSpeed > 0 && t.uploadSpeed > 0
                      ? Math.round((t.uploadSpeed / activePoint.pt.uploadSpeed) * 100)
                      : 0;

                    return (
                      <div
                        key={`${t.id}-${idx}`}
                        onClick={() => {
                          if (isPinned && onViewTorrent) {
                            onViewTorrent(t.id);
                          }
                        }}
                        className={`bg-slate-950/90 rounded-lg p-2 border border-slate-800/90 flex flex-col gap-1 text-[10px] transition-colors ${
                          isPinned && onViewTorrent
                            ? 'cursor-pointer hover:border-brand-500/50 hover:bg-slate-800/60'
                            : ''
                        }`}
                      >
                        <div className="flex items-center justify-between gap-1.5">
                          <span
                            className="font-sans font-medium text-slate-200 truncate max-w-[190px]"
                            title={t.name}
                          >
                            {t.name}
                          </span>
                          <div className="flex items-center space-x-1 shrink-0">
                            <span className="bg-slate-800 px-1 py-0.2 rounded text-[8.5px] font-semibold text-slate-400">
                              {t.node}
                            </span>
                            {isPinned && onViewTorrent && (
                              <ExternalLink className="h-2.5 w-2.5 text-slate-500 inline" />
                            )}
                          </div>
                        </div>

                        <div className="flex items-center justify-between text-[9.5px]">
                          {t.downloadSpeed > 0 ? (
                            <span className="text-emerald-400 font-semibold flex items-center gap-0.5">
                              <ArrowDown className="h-2.5 w-2.5 inline" />
                              {formatSpeed(t.downloadSpeed)}
                              {dlShare > 0 && (
                                <span className="text-[8px] text-emerald-500/80 font-normal">({dlShare}%)</span>
                              )}
                            </span>
                          ) : (
                            <span className="text-slate-600">0 B/s DL</span>
                          )}

                          {t.uploadSpeed > 0 ? (
                            <span className="text-sky-400 font-semibold flex items-center gap-0.5">
                              <ArrowUp className="h-2.5 w-2.5 inline" />
                              {formatSpeed(t.uploadSpeed)}
                              {ulShare > 0 && (
                                <span className="text-[8px] text-sky-500/80 font-normal">({ulShare}%)</span>
                              )}
                            </span>
                          ) : (
                            <span className="text-slate-600">0 B/s UL</span>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              ) : (
                <div className="text-[10px] text-slate-500 font-sans italic py-1 text-center">
                  {activePoint.pt.downloadSpeed === 0 && activePoint.pt.uploadSpeed === 0
                    ? 'Swarm / cluster was idle (0 B/s)'
                    : 'No specific contributors attributed to this sample'}
                </div>
              )}

              {isPinned && activePoint.pt.topTorrents && activePoint.pt.topTorrents.length > 0 && onViewTorrent && (
                <div className="mt-1.5 pt-1 border-t border-slate-800/60 text-center text-[9px] font-sans text-brand-400/80">
                  Click any swarm above to view details
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
