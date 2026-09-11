import React, { useState, useEffect } from 'react';
import {
  X,
  Clock,
  CheckCircle2,
  AlertTriangle,
  Play,
  Pause,
  Copy,
  Check,
  Server,
  Folder,
  FileText,
  Users,
  Radio,
  Share2,
  HardDrive,
  ShieldCheck,
  Tv,
  Film,
  Sparkles,
  Download,
  Upload,
  Layers,
  Zap,
  Lock,
  Unlock,
  Edit3,
  Activity,
  Grid,
  ArrowRightLeft,
} from 'lucide-react';
import { DetailedTorrent, TorrentStatus } from '../types';
import { fetchTorrentDetails, enrichTorrent, setSequentialDownload, renameTorrentPath, fetchNodes, migrateTorrent } from '../services/api';
import { BandwidthChart, BandwidthDataPoint } from './BandwidthChart';
import { parseMediaRelease, getEffectivePoster, getPosterPlaceholder } from '../utils/mediaParser';

function decodePiecesBitfield(base64Str?: string | null, pieceCount?: number): boolean[] {
  if (!base64Str || !pieceCount || pieceCount <= 0) return [];
  try {
    const binary = atob(base64Str);
    const result: boolean[] = [];
    for (let i = 0; i < binary.length; i++) {
      const byte = binary.charCodeAt(i);
      for (let bit = 7; bit >= 0; bit--) {
        if (result.length < pieceCount) {
          result.push(((byte >> bit) & 1) === 1);
        }
      }
    }
    return result;
  } catch {
    return [];
  }
}

interface TorrentDetailsModalProps {
  compoundId: string | null;
  onClose: () => void;
}

function formatBytes(bytes: number): string {
  if (!bytes || bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(2)} ${sizes[i]}`;
}

function formatSpeed(bytesPerSec: number): string {
  if (!bytesPerSec || bytesPerSec === 0) return '0 B/s';
  return `${formatBytes(bytesPerSec)}/s`;
}

function formatDuration(seconds: number): string {
  if (!seconds || seconds <= 0) return 'Unknown';
  if (seconds < 60) return `${seconds}s`;
  const mins = Math.floor(seconds / 60);
  const secs = seconds % 60;
  if (mins < 60) return `${mins}m ${secs}s`;
  const hours = Math.floor(mins / 60);
  const remMins = mins % 60;
  if (hours < 24) return `${hours}h ${remMins}m`;
  const days = Math.floor(hours / 24);
  const remHours = hours % 24;
  return `${days}d ${remHours}h`;
}

function getStatusBadge(status: TorrentStatus, error: number, isCircuitBroken?: boolean, isCanaryProbe?: boolean) {
  if (isCanaryProbe) {
    return (
      <span className="rounded-full bg-amber-500/20 px-2.5 py-0.5 text-xs font-bold text-amber-300 border border-amber-500/40 flex items-center space-x-1">
        <Zap className="h-3 w-3 animate-pulse text-amber-400" />
        <span>Canary Probe</span>
      </span>
    );
  }
  if (isCircuitBroken) {
    return (
      <span className="rounded-full bg-amber-500/10 px-2.5 py-0.5 text-xs font-bold text-amber-400 border border-amber-500/30 flex items-center space-x-1">
        <Zap className="h-3 w-3 text-amber-400" />
        <span>Pressure Relieved</span>
      </span>
    );
  }
  if (error !== 0) {
    return <span className="rounded-full bg-rose-500/10 px-2.5 py-0.5 text-xs font-semibold text-rose-400 border border-rose-500/20">Error</span>;
  }
  switch (status) {
    case 'downloading':
      return <span className="rounded-full bg-emerald-500/10 px-2.5 py-0.5 text-xs font-semibold text-emerald-400 border border-emerald-500/20">Downloading</span>;
    case 'seeding':
      return <span className="rounded-full bg-sky-500/10 px-2.5 py-0.5 text-xs font-semibold text-sky-400 border border-sky-500/20">Seeding</span>;
    case 'queued':
      return <span className="rounded-full bg-amber-500/10 px-2.5 py-0.5 text-xs font-semibold text-amber-300 border border-amber-500/20">Queued</span>;
    case 'queuedseed':
      return <span className="rounded-full bg-sky-500/10 px-2.5 py-0.5 text-xs font-semibold text-sky-300 border border-sky-500/20">Queued (Seed)</span>;
    case 'stopped':
      return <span className="rounded-full bg-slate-700/50 px-2.5 py-0.5 text-xs font-semibold text-slate-300 border border-slate-600">Paused</span>;
    case 'checking':
    case 'checkwait':
      return <span className="rounded-full bg-amber-500/10 px-2.5 py-0.5 text-xs font-semibold text-amber-400 border border-amber-500/20">Checking</span>;
    case 'idle':
      return <span className="rounded-full bg-purple-500/10 px-2.5 py-0.5 text-xs font-semibold text-purple-400 border border-purple-500/20">Idle</span>;
    default:
      return <span className="rounded-full bg-slate-800 px-2.5 py-0.5 text-xs font-semibold text-slate-400">{status}</span>;
  }
}

export const TorrentDetailsModal: React.FC<TorrentDetailsModalProps> = ({ compoundId, onClose }) => {
  const [torrent, setTorrent] = useState<DetailedTorrent | null>(null);
  const [speedHistory, setSpeedHistory] = useState<BandwidthDataPoint[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<'overview' | 'files' | 'pieces' | 'peers' | 'trackers' | 'arr'>('overview');
  const [copiedHash, setCopiedHash] = useState(false);
  const [copiedMagnet, setCopiedMagnet] = useState(false);
  const [enriching, setEnriching] = useState(false);
  const [fileFilter, setFileFilter] = useState('');
  const [renamingPath, setRenamingPath] = useState<string | null>(null);
  const [newPathName, setNewPathName] = useState<string>('');
  const [showMigrate, setShowMigrate] = useState(false);
  const [nodeOptions, setNodeOptions] = useState<string[]>([]);
  const [migrateTarget, setMigrateTarget] = useState('');
  const [migrateDeleteSource, setMigrateDeleteSource] = useState(false);
  const [migrateDeleteData, setMigrateDeleteData] = useState(false);
  const [migrating, setMigrating] = useState(false);

  useEffect(() => {
    if (!compoundId) return;
    setSpeedHistory([]);
    loadDetails();
    const interval = setInterval(loadDetails, 1000);
    return () => clearInterval(interval);
  }, [compoundId]);

  const loadDetails = async () => {
    if (!compoundId) return;
    try {
      const data = await fetchTorrentDetails(compoundId);
      setTorrent(data);
      setSpeedHistory((prev) => {
        // Find peers actively transferring or top connected peers in the swarm
        const activePeers = (data.peers || [])
          .filter((p) => (p.rateToClient > 0 || p.rateToPeer > 0))
          .sort((a, b) => ((b.rateToClient || 0) + (b.rateToPeer || 0)) - ((a.rateToClient || 0) + (a.rateToPeer || 0)))
          .slice(0, 15)
          .map((p) => ({
            address: p.address,
            clientName: p.clientName || 'Unknown Client',
            flagStr: p.flagstr,
            downloadSpeed: p.rateToClient || 0,
            uploadSpeed: p.rateToPeer || 0,
            progress: p.progress,
            isEncrypted: p.isEncrypted,
            countryCode: p.countryCode,
            asName: p.asName,
          }));

        // If none are currently transferring data at this second, list top connected seeds/peers
        const displayPeers = activePeers.length > 0
          ? activePeers
          : (data.peers || [])
              .slice(0, 8)
              .map((p) => ({
                address: p.address,
                clientName: p.clientName || 'Unknown Client',
                flagStr: p.flagstr,
                downloadSpeed: p.rateToClient || 0,
                uploadSpeed: p.rateToPeer || 0,
                progress: p.progress,
                isEncrypted: p.isEncrypted,
                countryCode: p.countryCode,
                asName: p.asName,
              }));

        const next = [
          ...prev,
          {
            time: Date.now(),
            downloadSpeed: data.rate_download || 0,
            uploadSpeed: data.rate_upload || 0,
            topPeers: displayPeers,
          },
        ];
        return next.slice(-300); // 300 data points @ 1s = 5 minutes
      });
    } catch (e: any) {
      setError(e.message || 'Failed to load details');
    } finally {
      setLoading(false);
    }
  };

  const handleEnrich = async () => {
    if (!compoundId) return;
    setEnriching(true);
    try {
      const updated = await enrichTorrent(compoundId);
      setTorrent(updated);
    } catch (e: any) {
      alert(`Enrichment failed: ${e.message}`);
    } finally {
      setEnriching(false);
    }
  };

  const openMigrate = async () => {
    setShowMigrate(true);
    setMigrateDeleteSource(false);
    setMigrateDeleteData(false);
    try {
      const nodes = await fetchNodes();
      const currentNode = compoundId?.split(':')[0];
      const others = (nodes || []).map((n: any) => n.node).filter((n: string) => n && n !== currentNode);
      setNodeOptions(others);
      setMigrateTarget(others[0] || '');
    } catch {
      setNodeOptions([]);
    }
  };

  const handleMigrate = async () => {
    if (!compoundId || !torrent?.hash_string || !migrateTarget) return;
    setMigrating(true);
    try {
      await migrateTorrent({
        source_node: compoundId.split(':')[0],
        target_node: migrateTarget,
        hash: torrent.hash_string,
        delete_source_torrent: migrateDeleteSource,
        delete_source_data: migrateDeleteSource && migrateDeleteData,
      });
      setShowMigrate(false);
      onClose();
    } catch (e: any) {
      alert(`Migration failed: ${e.message}`);
    } finally {
      setMigrating(false);
    }
  };

  const handleCopyHash = () => {
    if (!torrent?.hash_string) return;
    navigator.clipboard.writeText(torrent.hash_string);
    setCopiedHash(true);
    setTimeout(() => setCopiedHash(false), 2000);
  };

  const handleCopyMagnet = () => {
    if (!torrent?.magnet_link) return;
    navigator.clipboard.writeText(torrent.magnet_link);
    setCopiedMagnet(true);
    setTimeout(() => setCopiedMagnet(false), 2000);
  };

  if (!compoundId) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/75 p-4 backdrop-blur-md animate-fadeIn">
      <div className="flex h-[90vh] w-full max-w-5xl flex-col rounded-2xl border border-slate-800 bg-slate-900 shadow-2xl overflow-hidden">
        {/* Modal Header */}
        <div className="flex items-center justify-between border-b border-slate-800 bg-slate-950/60 px-6 py-4">
          <div className="flex items-center space-x-3 overflow-hidden">
            <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-brand-500/10 text-brand-400 border border-brand-500/20">
              <HardDrive className="h-5 w-5" />
            </div>
            <div className="min-w-0 flex-1">
              <div className="flex items-center space-x-2.5">
                <h2 className="truncate text-base font-bold text-slate-100" title={torrent?.name || compoundId}>
                  {torrent ? parseMediaRelease(torrent.name, torrent.arr_grab).displayTitle : 'Loading torrent details...'}
                </h2>
                {torrent && getStatusBadge(torrent.status, torrent.error, torrent.is_circuit_broken, torrent.is_canary_probe)}
              </div>
              <div className="flex items-center space-x-3 text-xs text-slate-400 mt-0.5">
                <span className="flex items-center space-x-1">
                  <Server className="h-3 w-3 text-slate-500" />
                  <strong className="text-slate-300">{compoundId.split(':')[0]}</strong>
                </span>
                {torrent?.hash_string && (
                  <>
                    <span>•</span>
                    <button
                      onClick={handleCopyHash}
                      className="flex items-center space-x-1 text-slate-400 hover:text-brand-400 transition-colors font-mono"
                      title="Copy Info Hash"
                    >
                      <span>{torrent.hash_string.substring(0, 10)}...</span>
                      {copiedHash ? <Check className="h-3 w-3 text-emerald-400" /> : <Copy className="h-3 w-3" />}
                    </button>
                  </>
                )}
              </div>
            </div>
          </div>
          <div className="flex items-center space-x-2">
            {!torrent?.arr_grab && (
              <button
                onClick={handleEnrich}
                disabled={enriching}
                className="flex items-center space-x-1.5 px-3 py-1.5 rounded-lg bg-indigo-600/20 hover:bg-indigo-600/30 text-indigo-300 border border-indigo-500/30 text-xs font-semibold transition-colors disabled:opacity-50"
                title="Match with Sonarr/Radarr and download rich metadata"
              >
                <Sparkles className={`h-3.5 w-3.5 ${enriching ? 'animate-spin' : ''}`} />
                <span>{enriching ? 'Enriching...' : 'Enrich from Arr'}</span>
              </button>
            )}
            {torrent?.hash_string && (
              <button
                onClick={openMigrate}
                className="flex items-center space-x-1.5 px-3 py-1.5 rounded-lg bg-sky-600/20 hover:bg-sky-600/30 text-sky-300 border border-sky-500/30 text-xs font-semibold transition-colors"
                title="Move this torrent to another fetcher node"
              >
                <ArrowRightLeft className="h-3.5 w-3.5" />
                <span>Migrate</span>
              </button>
            )}
            <button
              onClick={onClose}
              className="rounded-lg p-2 text-slate-400 hover:bg-slate-800 hover:text-slate-200 transition-colors"
            >
              <X className="h-5 w-5" />
            </button>
          </div>
        </div>

        {/* Modal Body */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {loading && !torrent ? (
            <div className="flex h-64 items-center justify-center space-x-3 text-slate-400">
              <Clock className="h-6 w-6 animate-spin text-brand-400" />
              <span>Fetching live RPC telemetry...</span>
            </div>
          ) : error && !torrent ? (
            <div className="rounded-xl border border-rose-500/20 bg-rose-500/10 p-6 text-center text-rose-300">
              <AlertTriangle className="h-8 w-8 mx-auto mb-2 text-rose-400" />
              <p className="font-semibold">{error}</p>
            </div>
          ) : torrent && (
            <>
              {/* LARGE ACTIVE SEEDING / UPLOAD OR DOWNLOAD HERO BANNER */}
              {(torrent.status === 'seeding' || (torrent.percent_done >= 1 && torrent.status !== 'downloading')) ? (
                <div className="rounded-2xl border border-sky-500/30 bg-gradient-to-r from-sky-950/40 via-indigo-950/40 to-slate-950 p-4 shadow-xl relative overflow-hidden backdrop-blur-sm">
                  <div className="flex items-center justify-between text-xs mb-2.5 flex-wrap gap-2">
                    <div className="flex items-center space-x-2.5">
                      <div className="flex h-7 w-7 items-center justify-center rounded-xl bg-sky-500/20 text-sky-400 border border-sky-500/30 shadow-inner">
                        <Upload className="h-4 w-4 animate-bounce" />
                      </div>
                      <div>
                        <div className="flex items-center space-x-2">
                          <span className="font-bold text-slate-200 uppercase tracking-wider text-[11px]">
                            {torrent.status === 'seeding' ? 'Actively Seeding Swarm' : torrent.status === 'stopped' ? 'Completed (Paused)' : 'Seeding Swarm'}
                          </span>
                          <span className={`font-mono font-extrabold text-sm px-2 py-0.5 rounded-md border ${
                            torrent.upload_ratio >= 2.0
                              ? 'bg-emerald-500/20 text-emerald-300 border-emerald-500/40'
                              : torrent.upload_ratio >= 1.0
                              ? 'bg-sky-500/20 text-sky-300 border-sky-500/40'
                              : 'bg-indigo-500/20 text-indigo-300 border-indigo-500/40'
                          }`}>
                            {torrent.upload_ratio.toFixed(2)}x Ratio
                          </span>
                        </div>
                        <div className="text-[11px] text-slate-400 font-mono">
                          Uploaded <strong className="text-sky-300">{formatBytes(torrent.uploaded_ever)}</strong> ({((torrent.uploaded_ever / Math.max(1, torrent.total_size)) * 100).toFixed(0)}% payload) • Size: {formatBytes(torrent.total_size)}
                        </div>
                      </div>
                    </div>
                    <div className="flex items-center space-x-4 text-xs font-mono">
                      <div className="flex items-center space-x-1.5 bg-slate-950/60 px-2.5 py-1 rounded-lg border border-slate-800">
                        <Upload className="h-3.5 w-3.5 text-sky-400" />
                        <span className="text-sky-400 font-bold">{formatSpeed(torrent.rate_upload)}</span>
                      </div>
                      {torrent.rate_download > 0 && (
                        <div className="flex items-center space-x-1.5 bg-slate-950/60 px-2.5 py-1 rounded-lg border border-slate-800">
                          <Download className="h-3.5 w-3.5 text-emerald-400" />
                          <span className="text-emerald-400 font-bold">{formatSpeed(torrent.rate_download)}</span>
                        </div>
                      )}
                      {torrent.peers_connected !== undefined && (
                        <div className="flex items-center space-x-1 text-slate-400 hidden sm:flex">
                          <Users className="h-3.5 w-3.5 text-sky-400" />
                          <span>{torrent.peers_getting_from_us || 0}/{torrent.peers_connected} leechers</span>
                        </div>
                      )}
                      {torrent.seconds_seeding > 0 && (
                        <div className="text-sky-300 font-sans text-xs bg-sky-500/10 px-2.5 py-1 rounded-lg border border-sky-500/25 flex items-center space-x-1">
                          <span>⏱️ Seed Time:</span>
                          <strong className="font-mono">{formatDuration(torrent.seconds_seeding)}</strong>
                        </div>
                      )}
                    </div>
                  </div>
                  {/* High-Contrast Shimmer Ratio Bar (Target: 2.0x) */}
                  <div className="h-4 w-full overflow-hidden rounded-full bg-slate-950 p-0.5 border border-slate-800 shadow-inner relative">
                    <div
                      className="h-full rounded-full bg-gradient-to-r from-indigo-500 via-sky-500 to-emerald-400 transition-all duration-500 relative overflow-hidden shadow-sm"
                      style={{ width: `${Math.max(2, Math.min(100, (torrent.upload_ratio / 2.0) * 100))}%` }}
                    >
                      <div className="absolute inset-0 bg-[linear-gradient(45deg,rgba(255,255,255,0.25)_25%,transparent_25%,transparent_50%,rgba(255,255,255,0.25)_50%,rgba(255,255,255,0.25)_75%,transparent_75%,transparent)] bg-[length:1rem_1rem] animate-pulse" />
                    </div>
                  </div>
                </div>
              ) : (torrent.status === 'downloading' || torrent.rate_download > 0 || torrent.percent_done < 1) ? (
                <div className="rounded-2xl border border-emerald-500/30 bg-gradient-to-r from-emerald-950/40 via-slate-900/80 to-slate-950 p-4 shadow-xl relative overflow-hidden backdrop-blur-sm">
                  <div className="flex items-center justify-between text-xs mb-2.5 flex-wrap gap-2">
                    <div className="flex items-center space-x-2.5">
                      <div className="flex h-7 w-7 items-center justify-center rounded-xl bg-emerald-500/20 text-emerald-400 border border-emerald-500/30 shadow-inner">
                        <Download className="h-4 w-4 animate-bounce" />
                      </div>
                      <div>
                        <div className="flex items-center space-x-2">
                          <span className="font-bold text-slate-200 uppercase tracking-wider text-[11px]">
                            {torrent.status === 'downloading' ? 'Actively Downloading' : 'Swarm Intake'}
                          </span>
                          <span className="font-mono text-emerald-400 font-extrabold text-sm">
                            {(torrent.percent_done * 100).toFixed(1)}%
                          </span>
                          <span className={`font-mono text-xs px-1.5 py-0.5 rounded border ${
                            torrent.upload_ratio >= 1.0 ? 'text-sky-300 bg-sky-500/10 border-sky-500/25' : 'text-slate-400 bg-slate-800 border-slate-700'
                          }`}>
                            {torrent.upload_ratio.toFixed(2)}x Ratio
                          </span>
                        </div>
                        <div className="text-[11px] text-slate-400 font-mono">
                          {formatBytes(torrent.downloaded_ever || (torrent.total_size * torrent.percent_done))} of {formatBytes(torrent.total_size)} • Uploaded: {formatBytes(torrent.uploaded_ever)}
                        </div>
                      </div>
                    </div>
                    <div className="flex items-center space-x-4 text-xs font-mono">
                      <div className="flex items-center space-x-1.5 bg-slate-950/60 px-2.5 py-1 rounded-lg border border-slate-800">
                        <Download className="h-3.5 w-3.5 text-emerald-400" />
                        <span className="text-emerald-400 font-bold">{formatSpeed(torrent.rate_download)}</span>
                      </div>
                      {torrent.rate_upload > 0 && (
                        <div className="flex items-center space-x-1.5 bg-slate-950/60 px-2.5 py-1 rounded-lg border border-slate-800">
                          <Upload className="h-3.5 w-3.5 text-sky-400" />
                          <span className="text-sky-400 font-bold">{formatSpeed(torrent.rate_upload)}</span>
                        </div>
                      )}
                      {torrent.peers_connected !== undefined && (
                        <div className="flex items-center space-x-1 text-slate-400 hidden sm:flex">
                          <Users className="h-3.5 w-3.5 text-slate-500" />
                          <span>{torrent.peers_sending_to_us || 0}/{torrent.peers_connected} peers</span>
                        </div>
                      )}
                      {torrent.eta > 0 && (
                        <div className="text-amber-300 font-sans text-xs bg-amber-500/10 px-2.5 py-1 rounded-lg border border-amber-500/25 flex items-center space-x-1">
                          <span>⏱️ ETA:</span>
                          <strong className="font-mono">{formatDuration(torrent.eta)}</strong>
                        </div>
                      )}
                    </div>
                  </div>
                  {/* Large High-Contrast Animated Progress Bar */}
                  <div className="h-4 w-full overflow-hidden rounded-full bg-slate-950 p-0.5 border border-slate-800 shadow-inner relative">
                    <div
                      className="h-full rounded-full bg-gradient-to-r from-amber-500 via-emerald-500 to-teal-400 transition-all duration-500 relative overflow-hidden shadow-sm"
                      style={{ width: `${Math.max(1, Math.min(100, torrent.percent_done * 100))}%` }}
                    >
                      <div className="absolute inset-0 bg-[linear-gradient(45deg,rgba(255,255,255,0.25)_25%,transparent_25%,transparent_50%,rgba(255,255,255,0.25)_50%,rgba(255,255,255,0.25)_75%,transparent_75%,transparent)] bg-[length:1rem_1rem] animate-pulse" />
                    </div>
                  </div>
                </div>
              ) : null}

              {/* RICH MEDIA HERO CARD */}
              {(() => {
                const mediaInfo = parseMediaRelease(torrent.name, torrent.arr_grab);
                const posterSrc = getEffectivePoster(torrent.arr_grab?.poster_url, torrent.name);
                return (
                  <div className="rounded-2xl border border-slate-800 bg-gradient-to-r from-slate-950 via-slate-900 to-slate-950 p-5 shadow-xl flex flex-col md:flex-row items-start space-y-4 md:space-y-0 md:space-x-5 relative overflow-hidden">
                    {/* Poster Art */}
                    <div className="w-24 h-36 rounded-xl overflow-hidden bg-slate-950 border border-slate-800 flex-shrink-0 shadow-lg relative">
                      <img
                        src={posterSrc}
                        alt={torrent.arr_grab?.title || torrent.name}
                        className="w-full h-full object-cover"
                        referrerPolicy="no-referrer"
                        onError={(e) => {
                          (e.currentTarget as HTMLImageElement).src = getPosterPlaceholder(torrent.name);
                        }}
                      />
                    </div>

                    {/* Media Info */}
                    <div className="flex-1 min-w-0 space-y-2">
                      <div className="flex items-center space-x-2.5 flex-wrap gap-y-1">
                        <span className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-amber-500/10 text-amber-300 border border-amber-500/20 flex items-center space-x-1">
                          <span>🐕</span>
                          <span>Conduit Tracked</span>
                        </span>
                        {torrent.arr_grab ? (
                          <span className={`px-2 py-0.5 rounded-full text-[10px] font-bold ${
                            mediaInfo.itemType === 'music'
                              ? 'bg-emerald-500/10 text-emerald-300 border border-emerald-500/20'
                              : mediaInfo.itemType === 'movie'
                              ? 'bg-indigo-500/10 text-indigo-300 border border-indigo-500/20'
                              : 'bg-blue-500/10 text-blue-300 border border-blue-500/20'
                          }`}>
                            {mediaInfo.itemType === 'music' ? '🎵 Lidarr Music' : mediaInfo.itemType === 'movie' ? '🎬 Radarr Movie' : '📺 Sonarr TV'}
                          </span>
                        ) : (
                          <span className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-slate-800 text-slate-400 border border-slate-700">
                            Direct Fetcher Download
                          </span>
                        )}
                        {mediaInfo.isSeasonPack && mediaInfo.seasonNumber !== undefined && (
                          <span className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-sky-500/15 text-sky-300 border border-sky-500/30">
                            📦 Season {mediaInfo.seasonNumber} Pack
                          </span>
                        )}
                        {!mediaInfo.isSeasonPack && mediaInfo.episodeLabel && (
                          <span className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-sky-500/15 text-sky-300 border border-sky-500/30">
                            📺 {mediaInfo.episodeLabel}
                          </span>
                        )}
                        {mediaInfo.isCompleteSeries && (
                          <span className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-indigo-500/15 text-indigo-300 border border-indigo-500/30">
                            📚 Complete Series
                          </span>
                        )}
                        {mediaInfo.quality && (
                          <span className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-emerald-500/10 text-emerald-300 border border-emerald-500/20">
                            🏷️ {mediaInfo.quality}
                          </span>
                        )}
                        {torrent.arr_grab?.indexer && (
                          <span className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-purple-500/10 text-purple-300 border border-purple-500/20">
                            📡 {torrent.arr_grab.indexer}
                          </span>
                        )}
                      </div>

                      <div className="flex items-baseline space-x-2">
                        <h3 className="text-xl font-bold text-white tracking-tight">
                          {mediaInfo.displayTitle}
                        </h3>
                      </div>

                      {torrent.arr_grab?.overview ? (
                        <p className="text-xs text-slate-300 line-clamp-3 leading-relaxed font-sans">
                          {torrent.arr_grab.overview}
                        </p>
                      ) : (
                        <p className="text-xs text-slate-400 font-mono truncate">
                          {torrent.name}
                        </p>
                      )}

                      <div className="flex items-center space-x-4 text-xs text-slate-400 pt-1 flex-wrap gap-y-1">
                        {torrent.arr_grab?.genres && (
                          <span>🎬 <strong>Genres:</strong> {torrent.arr_grab.genres}</span>
                        )}
                        {torrent.arr_grab?.runtime_mins && (
                          <span>⏱️ <strong>Runtime:</strong> {torrent.arr_grab.runtime_mins}m</span>
                        )}
                        {torrent.arr_grab?.imdb_id && (
                          <a
                            href={`https://www.imdb.com/title/${torrent.arr_grab.imdb_id}`}
                            target="_blank"
                            rel="noreferrer"
                            className="text-amber-400 hover:underline flex items-center space-x-1"
                          >
                            <span>IMDb ↗</span>
                          </a>
                        )}
                        {torrent.arr_grab?.tmdb_id && (
                          <a
                            href={`https://www.themoviedb.org/movie/${torrent.arr_grab.tmdb_id}`}
                            target="_blank"
                            rel="noreferrer"
                            className="text-sky-400 hover:underline flex items-center space-x-1"
                          >
                            <span>TMDb ↗</span>
                          </a>
                        )}
                      </div>
                    </div>
                  </div>
                );
              })()}

              {/* FRONT-AND-CENTER VISUAL LIFECYCLE TIMELINE (Conduit's Trail) */}
              <div className="rounded-2xl border border-slate-800 bg-slate-950/40 p-5 space-y-4 shadow-inner">
                <div className="flex items-center justify-between border-b border-slate-800/80 pb-2.5">
                  <div className="flex items-center space-x-2">
                    <Clock className="h-4 w-4 text-amber-400" />
                    <h3 className="text-xs font-bold uppercase tracking-wider text-slate-200">
                      🐕 Conduit's Scent Trail & Lifecycle History
                    </h3>
                  </div>
                  <div className="flex items-center space-x-2 text-xs">
                    <span className="text-slate-400">Progress:</span>
                    <span className="font-mono font-bold text-slate-100">{(torrent.percent_done * 100).toFixed(1)}%</span>
                    <span className="text-slate-400">• Ratio:</span>
                    <span className={`font-mono font-bold ${torrent.upload_ratio >= 2.0 ? 'text-emerald-400' : torrent.upload_ratio >= 1.0 ? 'text-sky-400' : 'text-slate-300'}`}>
                      {torrent.upload_ratio.toFixed(2)}x
                    </span>
                  </div>
                </div>

                {/* Horizontal Scrolling Scent Trail Stepper */}
                <div className="flex items-stretch space-x-3 overflow-x-auto pb-2 pt-1 scrollbar-thin scrollbar-thumb-slate-700/60 scrollbar-track-slate-900/40">
                  {torrent.timeline && torrent.timeline.length > 0 ? (
                    torrent.timeline.map((event, idx) => {
                      let statusBorder = 'border-slate-800 bg-slate-900/50 text-slate-300';
                      let iconColor = 'text-slate-400 bg-slate-800';
                      let StatusIcon = Clock;

                      if (event.status === 'success') {
                        statusBorder = 'border-emerald-500/30 bg-emerald-500/5 text-emerald-300';
                        iconColor = 'text-emerald-400 bg-emerald-500/20';
                        StatusIcon = CheckCircle2;
                      } else if (event.status === 'info') {
                        statusBorder = 'border-sky-500/30 bg-sky-500/5 text-sky-300';
                        iconColor = 'text-sky-400 bg-sky-500/20';
                        StatusIcon = Play;
                      } else if (event.status === 'warning') {
                        statusBorder = 'border-amber-500/30 bg-amber-500/5 text-amber-300';
                        iconColor = 'text-amber-400 bg-amber-500/20';
                        StatusIcon = AlertTriangle;
                      } else if (event.status === 'error') {
                        statusBorder = 'border-rose-500/30 bg-rose-500/5 text-rose-300';
                        iconColor = 'text-rose-400 bg-rose-500/20';
                        StatusIcon = AlertTriangle;
                      }

                      return (
                        <div
                          key={idx}
                          className={`min-w-[230px] max-w-[280px] shrink-0 rounded-xl border p-3 flex flex-col justify-between space-y-2.5 relative shadow-sm ${statusBorder}`}
                        >
                          <div className="flex items-start space-x-2.5">
                            <div className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-full font-bold text-xs ${iconColor}`}>
                              <StatusIcon className="h-4 w-4" />
                            </div>
                            <div className="min-w-0 flex-1">
                              <div className="flex items-center justify-between">
                                <span className="text-[10px] font-mono uppercase tracking-wider text-slate-400 font-bold">
                                  {event.stage}
                                </span>
                              </div>
                              <h4 className="text-xs font-bold text-slate-100 truncate mt-0.5" title={event.title}>
                                {event.title}
                              </h4>
                              <p className="text-[11px] text-slate-400 line-clamp-2 mt-1 leading-snug" title={event.description}>
                                {event.description}
                              </p>
                            </div>
                          </div>
                          <div className="text-[10px] font-mono text-slate-500 border-t border-slate-800/60 pt-1.5 flex justify-between items-center">
                            <span>{event.formatted_time.split(' ')[0]}</span>
                            <span>{event.formatted_time.split(' ').slice(1).join(' ')}</span>
                          </div>
                        </div>
                      );
                    })
                  ) : (
                    <div className="w-full text-center text-xs text-slate-500 py-3">
                      No lifecycle events recorded for this torrent yet.
                    </div>
                  )}
                </div>
              </div>

              {/* TAB NAVIGATION */}
              <div className="flex border-b border-slate-800 space-x-1 overflow-x-auto">
                {[
                  { id: 'overview', label: 'Overview & Details', icon: FileText },
                  { id: 'files', label: `Files (${torrent.files?.length || 0})`, icon: Folder },
                  { id: 'pieces', label: `Pieces (${torrent.piece_count || 0})`, icon: Grid },
                  { id: 'peers', label: `Peers & Swarm (${torrent.peers?.length || 0})`, icon: Users },
                  { id: 'trackers', label: `Trackers (${torrent.tracker_stats?.length || 0})`, icon: Radio },
                  ...(torrent.arr_grab ? [{ id: 'arr', label: 'Arr Grab Metadata', icon: Sparkles }] : []),
                ].map((t) => {
                  const Icon = t.icon;
                  const active = activeTab === t.id;
                  return (
                    <button
                      key={t.id}
                      onClick={() => setActiveTab(t.id as any)}
                      className={`flex items-center space-x-2 px-4 py-2.5 text-xs font-bold border-b-2 transition-colors shrink-0 ${
                        active
                          ? 'border-brand-500 text-brand-400 bg-brand-500/5'
                          : 'border-transparent text-slate-400 hover:text-slate-200 hover:bg-slate-800/50'
                      }`}
                    >
                      <Icon className="h-3.5 w-3.5" />
                      <span>{t.label}</span>
                    </button>
                  );
                })}
              </div>

              {/* TAB 1: OVERVIEW */}
              {activeTab === 'overview' && (
                <div className="space-y-4">
                  {/* Circuit Breaker Pressure Relief Banner */}
                  {torrent.is_circuit_broken && (
                    <div className="rounded-xl border border-amber-500/40 bg-amber-950/20 p-4 space-y-1.5 shadow-sm">
                      <div className="flex items-center space-x-2 text-amber-300 font-bold text-xs">
                        <Zap className="h-4 w-4 text-amber-400" />
                        <span>⚡ Swarm Pressure Relief Active (Tracker Circuit Breaker)</span>
                      </div>
                      <p className="text-xs text-slate-300 leading-relaxed">
                        {torrent.circuit_breaker_reason || 'Tracker is currently unreachable. Paused by Conduit to prevent excessive announce retries and remove load from the daemon and tracker. A designated canary probe is monitoring the tracker and will automatically resume this fetcher when connectivity recovers.'}
                      </p>
                    </div>
                  )}

                  {torrent.is_canary_probe && (
                    <div className="rounded-xl border border-amber-500/40 bg-amber-950/20 p-4 space-y-1.5 shadow-sm">
                      <div className="flex items-center space-x-2 text-amber-300 font-bold text-xs">
                        <Radio className="h-4 w-4 text-amber-400 animate-pulse" />
                        <span>⚡ Designated Active Canary Probe (Swarm Pressure Relief)</span>
                      </div>
                      <p className="text-xs text-slate-300 leading-relaxed">
                        {torrent.circuit_breaker_reason || 'Tracker is currently unreachable. This fetcher is actively running as the single designated probe monitoring the tracker for recovery while the rest of the swarm is paused.'}
                      </p>
                    </div>
                  )}

                  {/* 5-Minute Live Throughput Bandwidth Chart */}
                  <div className="rounded-2xl border border-slate-800 bg-slate-950/60 p-4 shadow-inner">
                    <BandwidthChart
                      history={speedHistory}
                      height={120}
                      timeWindowSecs={300}
                      title={`Fetcher Live Throughput (${torrent.status === 'seeding' ? 'Active Seeding' : torrent.status === 'downloading' ? 'Active Downloading' : 'Active Swarm'})`}
                    />
                  </div>

                  {/* 4-Card Comprehensive Telemetry Grid */}
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                    {/* Card 1: Seeding & Upload Telemetry */}
                    <div className="rounded-xl border border-sky-500/20 bg-gradient-to-b from-sky-950/20 to-slate-900/60 p-4 space-y-3 shadow-sm">
                      <div className="text-xs font-bold uppercase tracking-wider text-sky-400 flex items-center justify-between border-b border-slate-800/80 pb-2">
                        <div className="flex items-center space-x-1.5">
                          <Upload className="h-3.5 w-3.5" />
                          <span>Seeding & Upload Telemetry</span>
                        </div>
                        <span className={`text-[10px] font-mono font-bold px-2 py-0.5 rounded border ${
                          torrent.upload_ratio >= 2.0
                            ? 'bg-emerald-500/20 text-emerald-300 border-emerald-500/40'
                            : torrent.upload_ratio >= 1.0
                            ? 'bg-sky-500/20 text-sky-300 border-sky-500/40'
                            : 'bg-slate-800 text-slate-400 border-slate-700'
                        }`}>
                          {torrent.upload_ratio >= 2.0 ? '🎯 Target Met' : torrent.upload_ratio >= 1.0 ? '🟢 Positive (>1.0x)' : '🟡 Seeding (<1.0x)'}
                        </span>
                      </div>
                      <div className="grid grid-cols-2 gap-2.5 text-xs">
                        <div>
                          <span className="text-slate-400">Share Ratio:</span>
                          <div className={`font-bold font-mono text-sm ${
                            torrent.upload_ratio >= 2.0 ? 'text-emerald-400' : torrent.upload_ratio >= 1.0 ? 'text-sky-400' : 'text-slate-200'
                          }`}>
                            {torrent.upload_ratio.toFixed(2)}x
                          </div>
                        </div>
                        <div>
                          <span className="text-slate-400">Total Uploaded:</span>
                          <div className="font-bold font-mono text-slate-100">{formatBytes(torrent.uploaded_ever)}</div>
                        </div>
                        <div>
                          <span className="text-slate-400">Upload Rate:</span>
                          <div className="font-bold font-mono text-sky-400">{formatSpeed(torrent.rate_upload)}</div>
                        </div>
                        <div>
                          <span className="text-slate-400">Uploading To:</span>
                          <div className="font-bold font-mono text-slate-100">
                            {torrent.peers_getting_from_us || 0} leechers
                          </div>
                        </div>
                        <div>
                          <span className="text-slate-400">Time Seeding:</span>
                          <div className="font-bold font-mono text-slate-200">{formatDuration(torrent.seconds_seeding)}</div>
                        </div>
                        <div>
                          <span className="text-slate-400">Payload Ratio:</span>
                          <div className="font-bold font-mono text-slate-200">
                            {((torrent.uploaded_ever / Math.max(1, torrent.total_size)) * 100).toFixed(1)}% of size
                          </div>
                        </div>
                      </div>
                    </div>

                    {/* Card 2: Intake & Download Telemetry */}
                    <div className="rounded-xl border border-emerald-500/20 bg-gradient-to-b from-emerald-950/10 to-slate-900/60 p-4 space-y-3 shadow-sm">
                      <div className="text-xs font-bold uppercase tracking-wider text-emerald-400 flex items-center justify-between border-b border-slate-800/80 pb-2">
                        <div className="flex items-center space-x-1.5">
                          <Download className="h-3.5 w-3.5" />
                          <span>Intake & Download Telemetry</span>
                        </div>
                        {torrent.queue_position !== undefined && (
                          <span className="text-[11px] font-mono font-bold text-slate-400 bg-slate-800 px-2 py-0.5 rounded border border-slate-700">
                            Queue #{torrent.queue_position}
                          </span>
                        )}
                      </div>
                      <div className="grid grid-cols-2 gap-2.5 text-xs">
                        <div>
                          <span className="text-slate-400">Total Size:</span>
                          <div className="font-bold font-mono text-slate-100">{formatBytes(torrent.total_size)}</div>
                        </div>
                        <div>
                          <span className="text-slate-400">Downloaded Ever:</span>
                          <div className="font-bold font-mono text-slate-100">
                            {formatBytes(torrent.downloaded_ever || (torrent.total_size * torrent.percent_done))}
                          </div>
                        </div>
                        <div>
                          <span className="text-slate-400">Left Until Done:</span>
                          <div className="font-bold font-mono text-slate-100">{formatBytes(torrent.left_until_done)}</div>
                        </div>
                        <div>
                          <span className="text-slate-400">Download Rate:</span>
                          <div className="font-bold font-mono text-emerald-400">{formatSpeed(torrent.rate_download)}</div>
                        </div>
                        <div>
                          <span className="text-slate-400">Time Downloading:</span>
                          <div className="font-bold font-mono text-slate-200">{formatDuration(torrent.seconds_downloading)}</div>
                        </div>
                        <div>
                          <span className="text-slate-400">Pieces:</span>
                          <div className="font-bold font-mono text-slate-200">
                            {torrent.piece_count || 'N/A'} ({formatBytes(torrent.piece_size)} ea)
                          </div>
                        </div>
                      </div>
                    </div>

                    {/* Card 3: Swarm & Delivery Mode Card */}
                    <div className="rounded-xl border border-purple-500/20 bg-gradient-to-b from-purple-950/10 to-slate-900/60 p-4 space-y-3 shadow-sm">
                      <div className="text-xs font-bold uppercase tracking-wider text-purple-400 flex items-center justify-between border-b border-slate-800/80 pb-2">
                        <div className="flex items-center space-x-1.5">
                          <Users className="h-3.5 w-3.5" />
                          <span>Swarm & Peer Dynamics</span>
                        </div>
                        <button
                          onClick={async () => {
                            if (!compoundId) return;
                            try {
                              const next = !torrent.sequential_download;
                              await setSequentialDownload(compoundId, next);
                              setTorrent({ ...torrent, sequential_download: next });
                            } catch (err: any) {
                              alert(`Failed to toggle sequential download: ${err.message}`);
                            }
                          }}
                          className={`px-2 py-0.5 rounded text-[11px] font-bold border transition-colors cursor-pointer flex items-center space-x-1 ${
                            torrent.sequential_download
                              ? 'bg-indigo-500/20 text-indigo-300 border-indigo-500/40'
                              : 'bg-slate-800 text-slate-400 border-slate-700 hover:text-slate-200'
                          }`}
                          title="Click to toggle Sequential Piece Download Mode"
                        >
                          <span>🎬 Sequential:</span>
                          <span>{torrent.sequential_download ? 'ON' : 'OFF'}</span>
                        </button>
                      </div>
                      <div className="grid grid-cols-2 gap-2.5 text-xs">
                        <div>
                          <span className="text-slate-400">Connected Peers:</span>
                          <div className="font-bold font-mono text-slate-100">{torrent.peers_connected}</div>
                        </div>
                        <div>
                          <span className="text-slate-400">Downloading From:</span>
                          <div className="font-bold font-mono text-slate-100">{torrent.peers_sending_to_us} seeders</div>
                        </div>
                        <div>
                          <span className="text-slate-400">Uploading To:</span>
                          <div className="font-bold font-mono text-slate-100">{torrent.peers_getting_from_us || 0} leechers</div>
                        </div>
                        <div>
                          <span className="text-slate-400">Swarm Privacy:</span>
                          <div className="font-bold font-mono text-slate-200">{torrent.is_private ? '🔒 Private Tracker' : '🌐 Public Swarm'}</div>
                        </div>
                        <div>
                          <span className="text-slate-400">Corrupted Data:</span>
                          <div className="font-bold font-mono text-slate-200">{formatBytes(torrent.corrupt_ever)}</div>
                        </div>
                        <div>
                          <span className="text-slate-400">Delivery Strategy:</span>
                          <div className="font-bold font-mono text-slate-200">
                            {torrent.sequential_download ? 'Sequential Stream' : 'Rarest-First'}
                          </div>
                        </div>
                      </div>
                    </div>

                    {/* Card 4: Lifecycle & Timestamps Card */}
                    <div className="rounded-xl border border-slate-800 bg-slate-800/30 p-4 space-y-3 shadow-sm">
                      <div className="text-xs font-bold uppercase tracking-wider text-amber-400 flex items-center justify-between border-b border-slate-800/80 pb-2">
                        <div className="flex items-center space-x-1.5">
                          <Clock className="h-3.5 w-3.5" />
                          <span>Lifecycle & Activity Dates</span>
                        </div>
                        {torrent.done_date > 0 && (
                          <span className="text-[10px] font-mono font-bold text-emerald-400 bg-emerald-500/10 px-2 py-0.5 rounded border border-emerald-500/20 flex items-center space-x-1">
                            <CheckCircle2 className="h-3 w-3" />
                            <span>Completed</span>
                          </span>
                        )}
                      </div>
                      <div className="grid grid-cols-2 gap-2.5 text-xs">
                        <div>
                          <span className="text-slate-400">Date Added:</span>
                          <div className="font-mono text-slate-200 truncate" title={torrent.added_date > 0 ? new Date(torrent.added_date * 1000).toLocaleString() : 'N/A'}>
                            {torrent.added_date > 0 ? new Date(torrent.added_date * 1000).toLocaleDateString() + ' ' + new Date(torrent.added_date * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : 'N/A'}
                          </div>
                        </div>
                        <div>
                          <span className="text-slate-400">Date Completed:</span>
                          <div className="font-mono text-slate-200 truncate" title={torrent.done_date > 0 ? new Date(torrent.done_date * 1000).toLocaleString() : 'In Progress'}>
                            {torrent.done_date > 0 ? new Date(torrent.done_date * 1000).toLocaleDateString() + ' ' + new Date(torrent.done_date * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : (torrent.percent_done >= 1 ? 'Complete' : 'In Progress')}
                          </div>
                        </div>
                        <div>
                          <span className="text-slate-400">Date Created (.torrent):</span>
                          <div className="font-mono text-slate-300 truncate" title={torrent.date_created > 0 ? new Date(torrent.date_created * 1000).toLocaleString() : 'N/A'}>
                            {torrent.date_created > 0 ? new Date(torrent.date_created * 1000).toLocaleDateString() : 'N/A'}
                          </div>
                        </div>
                        <div>
                          <span className="text-slate-400">Last Swarm Activity:</span>
                          <div className="font-mono text-slate-200 truncate">
                            {torrent.activity_date > 0 ? `${formatDuration(Math.max(0, Math.floor(Date.now() / 1000) - torrent.activity_date))} ago` : 'Active now'}
                          </div>
                        </div>
                      </div>
                    </div>
                  </div>

                  {/* Metadata Fields */}
                  <div className="rounded-xl border border-slate-800 bg-slate-800/20 p-4 space-y-3 text-xs">
                    <div className="flex items-center justify-between border-b border-slate-800 pb-2">
                      <span className="text-slate-400 font-semibold">Download Directory:</span>
                      <span className="font-mono text-slate-200 truncate max-w-lg">{torrent.download_dir}</span>
                    </div>

                    {torrent.comment && (
                      <div className="flex items-start justify-between border-b border-slate-800 pb-2">
                        <span className="text-slate-400 font-semibold shrink-0">Comment:</span>
                        <span className="text-slate-300 ml-4">{torrent.comment}</span>
                      </div>
                    )}

                    {torrent.creator && (
                      <div className="flex items-center justify-between border-b border-slate-800 pb-2">
                        <span className="text-slate-400 font-semibold">Created By:</span>
                        <span className="text-slate-300">{torrent.creator}</span>
                      </div>
                    )}

                    {torrent.magnet_link && (
                      <div className="flex items-center justify-between pt-1">
                        <span className="text-slate-400 font-semibold shrink-0">Magnet Link:</span>
                        <button
                          onClick={handleCopyMagnet}
                          className="flex items-center space-x-1.5 rounded-lg bg-slate-800 px-3 py-1 text-slate-300 hover:bg-slate-700 hover:text-brand-400 transition-colors"
                        >
                          {copiedMagnet ? <Check className="h-3.5 w-3.5 text-emerald-400" /> : <Share2 className="h-3.5 w-3.5" />}
                          <span>{copiedMagnet ? 'Magnet Copied!' : 'Copy Magnet Link'}</span>
                        </button>
                      </div>
                    )}
                  </div>
                </div>
              )}

              {/* TAB 2: FILES (With In-App Path Renaming) */}
              {activeTab === 'files' && (
                <div className="space-y-3">
                  <div className="flex items-center justify-between">
                    <span className="text-xs font-semibold text-slate-400">
                      Total Files: <strong className="text-slate-200">{torrent.files?.length || 0}</strong>
                    </span>
                    <input
                      type="text"
                      placeholder="Filter files by name or extension..."
                      value={fileFilter}
                      onChange={(e) => setFileFilter(e.target.value)}
                      className="rounded-lg border border-slate-700 bg-slate-800 px-3 py-1 text-xs text-slate-200 placeholder-slate-500 focus:border-brand-500 focus:outline-none w-72"
                    />
                  </div>

                  <div className="max-h-96 overflow-y-auto rounded-xl border border-slate-800 bg-slate-900/60 divide-y divide-slate-800/60">
                    {(torrent.files || [])
                      .filter((f) => !fileFilter || f.name.toLowerCase().includes(fileFilter.toLowerCase()))
                      .map((f, idx) => {
                        const pct = f.length > 0 ? (f.bytesCompleted / f.length) * 100 : 100;
                        const isRenaming = renamingPath === f.name;
                        return (
                          <div key={idx} className="p-3 hover:bg-slate-800/30 transition-colors space-y-2">
                            <div className="flex items-center justify-between text-xs">
                              <div className="flex items-center space-x-2 truncate pr-4 flex-1">
                                <FileText className="h-3.5 w-3.5 shrink-0 text-slate-500" />
                                {isRenaming ? (
                                  <div className="flex items-center space-x-2 flex-1">
                                    <input
                                      type="text"
                                      value={newPathName}
                                      onChange={(e) => setNewPathName(e.target.value)}
                                      className="rounded border border-brand-500 bg-slate-800 px-2 py-0.5 text-xs text-white font-mono w-full max-w-md focus:outline-none"
                                      autoFocus
                                    />
                                    <button
                                      onClick={async () => {
                                        if (!compoundId || !newPathName.trim()) return;
                                        try {
                                          await renameTorrentPath(compoundId, f.name, newPathName.trim());
                                          setRenamingPath(null);
                                          loadDetails();
                                        } catch (err: any) {
                                          alert(`Rename failed: ${err.message}`);
                                        }
                                      }}
                                      className="px-2 py-0.5 rounded bg-brand-600 text-[11px] font-bold text-white hover:bg-brand-500"
                                    >
                                      Save
                                    </button>
                                    <button
                                      onClick={() => setRenamingPath(null)}
                                      className="px-2 py-0.5 rounded bg-slate-800 text-[11px] text-slate-400 hover:text-white"
                                    >
                                      Cancel
                                    </button>
                                  </div>
                                ) : (
                                  <div className="flex items-center space-x-2 truncate">
                                    <span className="font-mono text-slate-200 truncate" title={f.name}>{f.name}</span>
                                    <button
                                      onClick={() => {
                                        setRenamingPath(f.name);
                                        setNewPathName(f.name.split('/').pop() || f.name);
                                      }}
                                      className="text-slate-500 hover:text-brand-400 transition-colors p-1"
                                      title="Rename file / path"
                                    >
                                      <Edit3 className="h-3 w-3" />
                                    </button>
                                  </div>
                                )}
                              </div>
                              <div className="shrink-0 space-x-2 font-mono text-[11px] text-slate-400">
                                <span>{formatBytes(f.bytesCompleted)} / {formatBytes(f.length)}</span>
                                <span className={`font-bold ${pct >= 100 ? 'text-emerald-400' : 'text-sky-400'}`}>
                                  ({pct.toFixed(1)}%)
                                </span>
                              </div>
                            </div>
                            <div className="h-1.5 w-full overflow-hidden rounded-full bg-slate-800">
                              <div
                                className={`h-full transition-all duration-300 ${pct >= 100 ? 'bg-emerald-500' : 'bg-sky-500'}`}
                                style={{ width: `${Math.min(100, Math.max(0, pct))}%` }}
                              />
                            </div>
                          </div>
                        );
                      })}
                  </div>
                </div>
              )}

              {/* TAB 3: PIECES HEATMAP VISUALIZER */}
              {activeTab === 'pieces' && (
                <div className="space-y-4">
                  <div className="flex items-center justify-between text-xs text-slate-400 bg-slate-900/60 p-3 rounded-xl border border-slate-800">
                    <div className="flex items-center space-x-4">
                      <span>Total Pieces: <strong className="text-slate-200">{torrent.piece_count || 0}</strong></span>
                      <span>Piece Size: <strong className="text-slate-200">{formatBytes(torrent.piece_size || 0)}</strong></span>
                      <span>Mode: <strong className={torrent.sequential_download ? 'text-indigo-400' : 'text-slate-300'}>
                        {torrent.sequential_download ? '🎬 Sequential Download' : '⚡ Rarest-First'}
                      </strong></span>
                    </div>
                    <div className="flex items-center space-x-3 text-[11px]">
                      <div className="flex items-center space-x-1.5">
                        <span className="h-2.5 w-2.5 rounded-xs bg-emerald-500" />
                        <span>Completed</span>
                      </div>
                      <div className="flex items-center space-x-1.5">
                        <span className="h-2.5 w-2.5 rounded-xs bg-sky-600" />
                        <span>Available in Swarm</span>
                      </div>
                      <div className="flex items-center space-x-1.5">
                        <span className="h-2.5 w-2.5 rounded-xs bg-slate-800" />
                        <span>Missing</span>
                      </div>
                    </div>
                  </div>

                  {(() => {
                    const pieceCount = torrent.piece_count || 0;
                    const downloadedBits = decodePiecesBitfield(torrent.pieces, pieceCount);
                    const availability = torrent.availability || [];

                    if (pieceCount === 0) {
                      return (
                        <div className="rounded-xl border border-slate-800 bg-slate-900/40 p-8 text-center text-xs text-slate-500">
                          Piece map telemetry unavailable.
                        </div>
                      );
                    }

                    // Render max 400 piece blocks or aggregated representation
                    const maxBlocks = 300;
                    const step = Math.max(1, Math.ceil(pieceCount / maxBlocks));
                    const blocksCount = Math.ceil(pieceCount / step);
                    const blocks = Array.from({ length: blocksCount }, (_, bIdx) => {
                      const start = bIdx * step;
                      const end = Math.min(pieceCount, start + step);
                      let dlCount = 0;
                      let avgAvail = 0;

                      for (let i = start; i < end; i++) {
                        if (downloadedBits[i]) dlCount++;
                        if (availability[i]) avgAvail += availability[i];
                      }
                      const dlPct = dlCount / (end - start);
                      const avail = (end - start > 0) ? (avgAvail / (end - start)) : 0;

                      return { bIdx, start, end, dlPct, avail };
                    });

                    return (
                      <div className="rounded-xl border border-slate-800 bg-slate-950/60 p-4 space-y-3">
                        <div className="text-xs font-bold text-slate-300 flex items-center justify-between">
                          <span>Piece Availability Heatmap ({pieceCount} pieces)</span>
                          <span className="font-mono text-emerald-400">
                            {(torrent.percent_done * 100).toFixed(1)}% Complete
                          </span>
                        </div>

                        {/* Visual Heatmap Grid */}
                        <div className="grid grid-cols-20 sm:grid-cols-30 md:grid-cols-50 gap-1 max-h-72 overflow-y-auto p-1 bg-slate-900/80 rounded-lg border border-slate-800/80">
                          {blocks.map((b) => {
                            let bgColor = 'bg-slate-800/60';
                            if (b.dlPct >= 1) {
                              bgColor = 'bg-emerald-500 shadow-xs shadow-emerald-500/20';
                            } else if (b.dlPct > 0) {
                              bgColor = 'bg-emerald-400/60';
                            } else if (b.avail > 3) {
                              bgColor = 'bg-sky-500';
                            } else if (b.avail > 0) {
                              bgColor = 'bg-sky-700';
                            }

                            return (
                              <div
                                key={b.bIdx}
                                className={`h-3.5 w-full rounded-xs transition-colors cursor-pointer hover:ring-2 hover:ring-white ${bgColor}`}
                                title={`Piece ${b.start}${b.end > b.start + 1 ? `-${b.end - 1}` : ''}: ${(b.dlPct * 100).toFixed(0)}% downloaded, Swarm Availability: ${b.avail.toFixed(1)}`}
                              />
                            );
                          })}
                        </div>
                      </div>
                    );
                  })()}
                </div>
              )}

              {/* TAB 4: PEERS & SWARM (With Country Flags, SSL Encryption & uTP) */}
              {activeTab === 'peers' && (
                <div className="space-y-3">
                  <div className="flex items-center justify-between text-xs text-slate-400">
                    <span>Connected Peer Swarm: <strong className="text-slate-200">{torrent.peers?.length || 0} active peers</strong></span>
                  </div>

                  {torrent.peers && torrent.peers.length > 0 ? (
                    <div className="max-h-96 overflow-y-auto rounded-xl border border-slate-800 bg-slate-900/60 overflow-hidden">
                      <table className="w-full text-left text-xs">
                        <thead className="border-b border-slate-800 bg-slate-950/60 text-slate-400">
                          <tr>
                            <th className="p-2.5 font-semibold">Peer IP & Port</th>
                            <th className="p-2.5 font-semibold">Client Agent</th>
                            <th className="p-2.5 font-semibold">Protocol</th>
                            <th className="p-2.5 font-semibold">Flags</th>
                            <th className="p-2.5 font-semibold">Encryption</th>
                            <th className="p-2.5 font-semibold text-right">Down Speed</th>
                            <th className="p-2.5 font-semibold text-right">Up Speed</th>
                            <th className="p-2.5 font-semibold text-right">Progress</th>
                          </tr>
                        </thead>
                        <tbody className="divide-y divide-slate-800/60 font-mono text-[11px]">
                          {torrent.peers.map((p, idx) => {
                            const isUtp = p.isUtp || (p.flagstr && p.flagstr.includes('T'));
                            return (
                              <tr key={idx} className="hover:bg-slate-800/30 transition-colors">
                                <td className="p-2.5 text-slate-200">
                                  <div className="flex items-center space-x-1.5">
                                    {p.countryCode && (
                                      <span className="text-xs" title={`Country: ${p.countryCode}`}>
                                        🌐 {p.countryCode}
                                      </span>
                                    )}
                                    <span>{p.address}:{p.port}</span>
                                  </div>
                                  {p.asName && (
                                    <div className="text-[10px] text-slate-500 font-sans truncate max-w-[220px]" title={p.asName}>
                                      {p.asName}
                                    </div>
                                  )}
                                </td>
                                <td className="p-2.5 text-slate-300 font-sans">{p.clientName || 'Unknown'}</td>
                                <td className="p-2.5">
                                  {isUtp ? (
                                    <span className="rounded bg-sky-500/10 px-1.5 py-0.5 text-[10px] font-bold text-sky-400 border border-sky-500/20">
                                      uTP
                                    </span>
                                  ) : (
                                    <span className="rounded bg-slate-800 px-1.5 py-0.5 text-[10px] text-slate-400">
                                      TCP
                                    </span>
                                  )}
                                </td>
                                <td className="p-2.5 text-slate-400">{p.flagstr || '-'}</td>
                                <td className="p-2.5">
                                  {p.isEncrypted ? (
                                    <span className="inline-flex items-center space-x-1 rounded bg-emerald-500/10 px-1.5 py-0.5 text-[10px] text-emerald-400 border border-emerald-500/20">
                                      <Lock className="h-2.5 w-2.5" />
                                      <span>SSL</span>
                                    </span>
                                  ) : (
                                    <span className="inline-flex items-center space-x-1 rounded bg-slate-800 px-1.5 py-0.5 text-[10px] text-slate-400">
                                      <Unlock className="h-2.5 w-2.5 text-slate-500" />
                                      <span>Plain</span>
                                    </span>
                                  )}
                                </td>
                                <td className="p-2.5 text-right text-sky-400 font-bold">{formatSpeed(p.rateToClient)}</td>
                                <td className="p-2.5 text-right text-purple-400 font-bold">{formatSpeed(p.rateToPeer)}</td>
                                <td className="p-2.5 text-right text-slate-200">{(p.progress * 100).toFixed(1)}%</td>
                              </tr>
                            );
                          })}
                        </tbody>
                      </table>
                    </div>
                  ) : (
                    <div className="rounded-xl border border-slate-800 bg-slate-900/40 p-8 text-center text-xs text-slate-500">
                      No active peer connections currently reporting telemetry.
                    </div>
                  )}
                </div>
              )}

              {/* TAB 4: TRACKERS */}
              {activeTab === 'trackers' && (
                <div className="space-y-3">
                  <div className="max-h-96 overflow-y-auto rounded-xl border border-slate-800 bg-slate-900/60 divide-y divide-slate-800/60">
                    {torrent.tracker_stats && torrent.tracker_stats.length > 0 ? (
                      torrent.tracker_stats.map((t, idx) => {
                        const announceStr = t.announce || '';
                        const hostStr = t.host || announceStr;
                        const resultStr = t.last_announce_result || (t as any).lastAnnounceResult || '';
                        const hasSucceeded = t.lastAnnounceSucceeded || (t as any).last_announce_succeeded || (resultStr && resultStr.toLowerCase().includes('success'));
                        const seeders = t.seederCount ?? (t as any).seeder_count ?? 0;
                        const leechers = t.leecherCount ?? (t as any).leecher_count ?? 0;
                        const downloads = t.downloadCount ?? (t as any).download_count ?? 0;

                        const isWarning = !hasSucceeded && resultStr && (
                          resultStr.toLowerCase().includes('backpressure') ||
                          resultStr.toLowerCase().includes('slow down') ||
                          resultStr.toLowerCase().includes('rate limit') ||
                          resultStr.toLowerCase().includes('retry in') ||
                          resultStr.toLowerCase().includes('warning')
                        );

                        return (
                          <div key={idx} className="p-4 hover:bg-slate-800/30 transition-colors space-y-2 text-xs">
                            <div className="flex items-center justify-between">
                              <div className="flex items-center space-x-2">
                                <Radio className="h-4 w-4 text-sky-400" />
                                <span className="font-bold text-slate-200">{hostStr || 'Tracker Host'}</span>
                              </div>
                              <span className={`rounded-full px-2 py-0.5 text-[10px] font-semibold ${
                                hasSucceeded
                                  ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20'
                                  : isWarning
                                  ? 'bg-amber-500/10 text-amber-400 border border-amber-500/20'
                                  : resultStr
                                  ? 'bg-rose-500/10 text-rose-400 border border-rose-500/20'
                                  : 'bg-slate-800 text-slate-400'
                              }`}>
                                {resultStr || (hasSucceeded ? 'Announce OK' : 'Pending Announce')}
                              </span>
                            </div>

                            <div className="font-mono text-[11px] text-slate-400 truncate" title={announceStr}>
                              {announceStr || 'No announce URL specified'}
                            </div>

                            <div className="grid grid-cols-3 gap-2 pt-1 border-t border-slate-800/60 text-center font-mono">
                              <div className="rounded-lg bg-slate-800/50 p-2">
                                <div className="text-[10px] text-slate-400 font-sans">Seeders</div>
                                <div className="font-bold text-emerald-400 text-sm mt-0.5">{seeders}</div>
                              </div>
                              <div className="rounded-lg bg-slate-800/50 p-2">
                                <div className="text-[10px] text-slate-400 font-sans">Leechers</div>
                                <div className="font-bold text-sky-400 text-sm mt-0.5">{leechers}</div>
                              </div>
                              <div className="rounded-lg bg-slate-800/50 p-2">
                                <div className="text-[10px] text-slate-400 font-sans">Completed Grabs</div>
                                <div className="font-bold text-slate-200 text-sm mt-0.5">{downloads}</div>
                              </div>
                            </div>
                          </div>
                        );
                      })
                    ) : (
                      <div className="p-8 text-center text-xs text-slate-500">No tracker records found.</div>
                    )}
                  </div>
                </div>
              )}

              {/* TAB 5: ARR GRAB METADATA */}
              {activeTab === 'arr' && torrent.arr_grab && (
                <div className="rounded-xl border border-slate-800 bg-slate-800/20 p-5 space-y-4 text-xs">
                  <div className="flex items-center space-x-2 text-brand-400 font-bold border-b border-slate-800 pb-3">
                    {torrent.arr_grab.item_type === 'series' ? <Tv className="h-4 w-4" /> : <Film className="h-4 w-4" />}
                    <span>Correlated Arr Grab: {torrent.arr_grab.item_type === 'series' ? 'Sonarr TV' : 'Radarr Movie'}</span>
                  </div>

                  <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                    <div>
                      <span className="text-slate-400 font-semibold block mb-1">Release Title:</span>
                      <span className="font-mono text-slate-200">{torrent.arr_grab.release_title}</span>
                    </div>
                    <div>
                      <span className="text-slate-400 font-semibold block mb-1">Scene / Series Name:</span>
                      <span className="text-slate-200">{torrent.arr_grab.scene_name}</span>
                    </div>
                    <div>
                      <span className="text-slate-400 font-semibold block mb-1">Indexer:</span>
                      <span className="text-slate-200">{torrent.arr_grab.indexer || 'Default Indexer'}</span>
                    </div>
                    <div>
                      <span className="text-slate-400 font-semibold block mb-1">Lifecycle Status:</span>
                      <span className="rounded bg-sky-500/10 px-2 py-0.5 text-sky-400 font-semibold uppercase">{torrent.arr_grab.status}</span>
                    </div>
                    <div>
                      <span className="text-slate-400 font-semibold block mb-1">Re-Searched by MediaReplacer:</span>
                      <span className="text-slate-200">{torrent.arr_grab.re_searched ? `Yes (${torrent.arr_grab.re_search_count} times)` : 'No'}</span>
                    </div>
                    <div>
                      <span className="text-slate-400 font-semibold block mb-1">Grabbed At:</span>
                      <span className="font-mono text-slate-200">{torrent.arr_grab.created_at}</span>
                    </div>
                  </div>
                </div>
              )}
            </>
          )}
        </div>

        {/* Modal Footer */}
        <div className="flex items-center justify-between border-t border-slate-800 bg-slate-950/60 px-6 py-3 text-xs text-slate-400">
          <span>Live telemetry updates every 5 seconds.</span>
          <button
            onClick={onClose}
            className="rounded-lg bg-slate-800 px-4 py-2 font-semibold text-slate-200 hover:bg-slate-700 transition-colors"
          >
            Close
          </button>
        </div>
      </div>

      {showMigrate && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/75 p-4 backdrop-blur-sm">
          <div className="w-full max-w-md rounded-2xl border border-slate-800 bg-slate-900 shadow-2xl p-6 space-y-4">
            <div className="flex items-center space-x-2">
              <ArrowRightLeft className="h-5 w-5 text-sky-400" />
              <h3 className="text-sm font-bold text-slate-100">Migrate Torrent</h3>
            </div>
            <p className="text-xs text-slate-400">
              Adds this torrent to the target node (from its magnet link) and optionally removes it from{' '}
              <strong className="text-slate-300">{compoundId?.split(':')[0]}</strong>. Any existing downloaded data
              is not copied automatically — the target node will re-fetch or re-verify it.
            </p>

            <div className="space-y-1.5">
              <label className="text-xs font-semibold text-slate-300">Target Node</label>
              {nodeOptions.length === 0 ? (
                <p className="text-xs text-rose-400">No other nodes available to migrate to.</p>
              ) : (
                <select
                  value={migrateTarget}
                  onChange={(e) => setMigrateTarget(e.target.value)}
                  className="w-full rounded-lg border border-slate-700 bg-slate-800 px-3 py-2 text-xs text-slate-200 focus:border-brand-500 focus:outline-none"
                >
                  {nodeOptions.map((n) => (
                    <option key={n} value={n}>{n}</option>
                  ))}
                </select>
              )}
            </div>

            <label className="flex items-center space-x-2 text-xs text-slate-300">
              <input
                type="checkbox"
                checked={migrateDeleteSource}
                onChange={(e) => setMigrateDeleteSource(e.target.checked)}
                className="rounded border-slate-600 bg-slate-800 text-brand-500 focus:ring-brand-500"
              />
              <span>Remove torrent from source node after migration</span>
            </label>
            {migrateDeleteSource && (
              <label className="flex items-center space-x-2 text-xs text-rose-300 pl-6">
                <input
                  type="checkbox"
                  checked={migrateDeleteData}
                  onChange={(e) => setMigrateDeleteData(e.target.checked)}
                  className="rounded border-slate-600 bg-slate-800 text-rose-500 focus:ring-rose-500"
                />
                <span>Also delete the source node's downloaded data from disk</span>
              </label>
            )}

            <div className="flex items-center justify-end space-x-2 pt-2">
              <button
                onClick={() => setShowMigrate(false)}
                disabled={migrating}
                className="px-3 py-1.5 rounded-lg bg-slate-800 text-xs text-slate-300 hover:text-white disabled:opacity-50"
              >
                Cancel
              </button>
              <button
                onClick={handleMigrate}
                disabled={migrating || !migrateTarget}
                className="px-3 py-1.5 rounded-lg bg-sky-600 text-xs font-semibold text-white hover:bg-sky-500 disabled:opacity-50"
              >
                {migrating ? 'Migrating...' : 'Migrate'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
