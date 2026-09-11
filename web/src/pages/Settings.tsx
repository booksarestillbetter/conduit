import React, { useState, useEffect } from 'react';
import {
  Server,
  FolderSync,
  Tv,
  Film,
  Music,
  Bell,
  Sliders,
  Key,
  Shield,
  Save,
  Download,
  Upload,
  Plus,
  Trash2,
  CheckCircle2,
  XCircle,
  RotateCcw,
  Gauge,
  Radio,
  X,
  Send,
  Sparkles,
  Terminal,
  Cpu,
  ChevronDown,
  ChevronRight,
  Copy,
  Check,
  FileCode,
  ExternalLink,
  HelpCircle,
  Zap,
  Lock,
  Boxes,
  Globe,
} from 'lucide-react';
import {
  fetchSettings,
  saveSettings,
  exportBackup,
  restoreBackup,
  fetchTokens,
  createToken,
  deleteToken,
  testArrConnection,
  triggerArrSyncNow,
  fetchNodeSession,
  updateNodeSession,
  testNodePort,
  updateNodeBlocklist,
  testPlexConnection,
  refreshPlexSections,
  createPlexOAuthPin,
  pollPlexOAuthPin,
  addOAuthPlexServer,
  PlexDiscoveredServer,
  testTraktConnection,
  sendTestNotification,
  testPipelineRegex,
  classifyFile,
  fetchHookScript,
  fetchPlexScrobbles,
  fetchOmbiRequests,
  sendPlexWebhookTest,
  sendOmbiWebhookTest,
} from '../services/api';
import { AppConfig, ApiTokenRecord, ArrNodeConfig, ZoneConfig, ClassifyFileResponse, TransmissionNodeConfig, RemoteSyncFolderMapping, PlexScrobbleRecord, OmbiRequestRecord, APP_VERSION, NotificationChannel, NotificationTarget, NOTIFICATION_CATEGORIES, TrackerCircuitBreakerConfig } from '../types';

const DEFAULT_TRACKER_CB_CONFIG: TrackerCircuitBreakerConfig = {
  enabled: true,
  canary_probe_enabled: true,
  auto_resume_on_recovery: true,
  failure_ratio_threshold: 0.5,
  min_failures: 3,
  error_patterns: ['530', '502', '503', '504', '403', '429', 'The tracker is down', 'tracker is down', 'Connection refused', 'Could not connect', 'Timed out', 'Host not found', 'unreachable'],
  check_interval_secs: 20,
  max_tripped_secs: 21600,
  initial_backoff_secs: 30,
  recovery_ramp_secs: 30,
  recovery_success_threshold: 5,
};

export const Settings: React.FC = () => {
  const [tab, setTab] = useState<'nodes' | 'arr' | 'zones' | 'sync' | 'plex_trakt' | 'notify' | 'rules' | 'system' | 'tokens' | 'backup'>('nodes');
  const [copiedZoneWebhook, setCopiedZoneWebhook] = useState<string | null>(null);
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [tokens, setTokens] = useState<ApiTokenRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [statusMsg, setStatusMsg] = useState('');

  // Arr Connection Testing & Visibility States
  const [showSonarrReplicas, setShowSonarrReplicas] = useState(false);
  const [showRadarrReplicas, setShowRadarrReplicas] = useState(false);
  const [showLidarrReplicas, setShowLidarrReplicas] = useState(false);
  const [testingSonarrMaster, setTestingSonarrMaster] = useState(false);
  const [sonarrMasterStatus, setSonarrMasterStatus] = useState<string | null>(null);
  const [testingRadarrMaster, setTestingRadarrMaster] = useState(false);
  const [radarrMasterStatus, setRadarrMasterStatus] = useState<string | null>(null);
  const [testingLidarrMaster, setTestingLidarrMaster] = useState(false);
  const [lidarrMasterStatus, setLidarrMasterStatus] = useState<string | null>(null);
  const [copiedLidarrWebhook, setCopiedLidarrWebhook] = useState(false);
  const [copiedSonarrWebhook, setCopiedSonarrWebhook] = useState(false);
  const [copiedRadarrWebhook, setCopiedRadarrWebhook] = useState(false);
  const [syncingArr, setSyncingArr] = useState<string | null>(null);

  // Plex & Trakt States
  const [testingPlexIdx, setTestingPlexIdx] = useState<number | null>(null);
  const [plexStatusMap, setPlexStatusMap] = useState<Record<number, string>>({});
  const [refreshingPlexIdx, setRefreshingPlexIdx] = useState<number | null>(null);
  const [plexOAuthState, setPlexOAuthState] = useState<'idle' | 'waiting' | 'error'>('idle');
  const [plexOAuthError, setPlexOAuthError] = useState<string | null>(null);
  const [plexDiscoveredServers, setPlexDiscoveredServers] = useState<PlexDiscoveredServer[]>([]);
  const [plexAccountToken, setPlexAccountToken] = useState<string | null>(null);
  const [addingPlexServerIdx, setAddingPlexServerIdx] = useState<number | null>(null);
  // Plex advertises every network interface/relay it knows about per server (LAN IP, other
  // local NICs, plex.direct remote relay) — which one is actually reachable from wherever Conduit
  // runs is environment-specific, so the "local" one it marks isn't a safe default to commit to
  // silently. Keyed by discovered-server index; falls back to the local-or-first heuristic below.
  const [selectedConnectionUri, setSelectedConnectionUri] = useState<Record<number, string>>({});
  const [testingTrakt, setTestingTrakt] = useState(false);
  const [traktStatus, setTraktStatus] = useState<string | null>(null);

  // Plex & Ombi Inbound Webhook States
  const [scrobbles, setScrobbles] = useState<PlexScrobbleRecord[]>([]);
  const [ombiRequests, setOmbiRequests] = useState<OmbiRequestRecord[]>([]);
  const [loadingScrobbles, setLoadingScrobbles] = useState(false);
  const [loadingOmbi, setLoadingOmbi] = useState(false);
  const [testingPlexWebhook, setTestingPlexWebhook] = useState(false);
  const [testingOmbiWebhook, setTestingOmbiWebhook] = useState(false);
  const [copiedPlexWebhook, setCopiedPlexWebhook] = useState(false);
  const [copiedOmbiWebhook, setCopiedOmbiWebhook] = useState(false);

  // Notification Testing States
  const [testingChannel, setTestingChannel] = useState<string | null>(null);
  const [notifyStatus, setNotifyStatus] = useState<string | null>(null);

  // Pipeline Regex Simulator States
  const [regexTestStr, setRegexTestStr] = useState('Torrent not registered with this tracker');
  const [regexResult, setRegexResult] = useState<{ matched: boolean; pattern_valid: boolean; error?: string } | null>(null);
  const [evaluatingRegex, setEvaluatingRegex] = useState(false);

  // Staging Classifier Simulator States
  const [testClassifyName, setTestClassifyName] = useState('Severance.S02E01.2160p.WEB-DL.DDP5.1.Atmos.H.265-FLUX.mkv');
  const [testClassifyTracker, setTestClassifyTracker] = useState('landof.tv');
  const [testClassifyNode, setTestClassifyNode] = useState('');
  const [testClassifyHash, setTestClassifyHash] = useState('');
  const [classifyResult, setClassifyResult] = useState<ClassifyFileResponse | null>(null);
  const [testingClassify, setTestingClassify] = useState(false);
  const [nodeForMediaOverride, setNodeForMediaOverride] = useState<string>('');

  // Hook Script Generator & Remote Ingestion States
  const [scriptFormat, setScriptFormat] = useState<'sh' | 'py' | 'pl'>('sh');
  const [copiedCurl, setCopiedCurl] = useState(false);
  const [copiedScript, setCopiedScript] = useState(false);
  const [scriptContent, setScriptContent] = useState<string>('');
  const [loadingScript, setLoadingScript] = useState(false);
  const [showAddMappingModal, setShowAddMappingModal] = useState(false);
  const [newMappingName, setNewMappingName] = useState('');
  const [newMappingWatch, setNewMappingWatch] = useState('');
  const [newMappingPost, setNewMappingPost] = useState('');
  const [newMappingMediaType, setNewMappingMediaType] = useState('tv');
  const [newMappingSettle, setNewMappingSettle] = useState(10);
  const [newMappingDeleteSource, setNewMappingDeleteSource] = useState(false);

  // Embedded Node Transmission RPC Modal
  const [editingDaemonNode, setEditingDaemonNode] = useState<string | null>(null);
  const [daemonSession, setDaemonSession] = useState<any>(null);
  const [loadingSession, setLoadingSession] = useState(false);
  const [savingSession, setSavingSession] = useState(false);
  const [testingPort, setTestingPort] = useState(false);
  const [portStatus, setPortStatus] = useState<boolean | null>(null);

  // Token creation
  const [newTokenName, setNewTokenName] = useState('');
  const [newTokenScope, setNewTokenScope] = useState<'fetcher' | '*' | 'read'>('fetcher');
  const [newRawToken, setNewRawToken] = useState<string | null>(null);

  // Backup
  const [backupPass, setBackupPass] = useState('');
  const [restoreBundle, setRestoreBundle] = useState('');
  const [restorePass, setRestorePass] = useState('');

  const loadHookScript = async (fmt: string) => {
    setLoadingScript(true);
    try {
      const text = await fetchHookScript(fmt);
      setScriptContent(text);
    } catch (e) {
      console.error('Failed to load hook script:', e);
    } finally {
      setLoadingScript(false);
    }
  };

  useEffect(() => {
    if (tab === 'sync') {
      loadHookScript(scriptFormat);
    }
  }, [tab, scriptFormat]);

  useEffect(() => {
    loadData();
  }, []);

  const loadData = async () => {
    setLoading(true);
    setLoadError(null);
    try {
      const [cfg, toks] = await Promise.all([fetchSettings(), fetchTokens().catch(() => [])]);
      setConfig(cfg);
      setTokens(toks);
      if ((cfg?.sonarr?.replicas?.length ?? 0) > 0 || (cfg?.sonarr?.slaves?.length ?? 0) > 0) {
        setShowSonarrReplicas(true);
      }
      if ((cfg?.radarr?.replicas?.length ?? 0) > 0 || (cfg?.radarr?.slaves?.length ?? 0) > 0) {
        setShowRadarrReplicas(true);
      }
    } catch (e: any) {
      // Previously uncaught: a failed fetchSettings() left config as null forever with the
      // "Loading settings..." message stuck on screen and no way to retry short of a full reload.
      setLoadError(e?.message || 'Failed to load settings');
    } finally {
      setLoading(false);
    }
  };

  const handleSave = async () => {
    if (!config) return;
    setSaving(true);
    setStatusMsg('');
    try {
      await saveSettings(config);
      window.dispatchEvent(new Event('settings-saved'));
      setStatusMsg('Configuration saved successfully!');
      setTimeout(() => setStatusMsg(''), 3000);
    } catch (e: any) {
      setStatusMsg(`Error: ${e.message}`);
    } finally {
      setSaving(false);
    }
  };

  // Arr Test Connections
  const handleTestSonarrPrimary = async () => {
    const primary = config?.sonarr.primary || config?.sonarr.master;
    if (!primary) return;
    setTestingSonarrMaster(true);
    setSonarrMasterStatus(null);
    try {
      const res = await testArrConnection('sonarr', primary.base_url, primary.api_key);
      setSonarrMasterStatus(res.success ? `Connected! v${res.app_version || '3.x/4.x'}` : `Failed: ${res.message}`);
    } catch (e: any) {
      setSonarrMasterStatus(`Error: ${e.message}`);
    } finally {
      setTestingSonarrMaster(false);
    }
  };

  const handleTestRadarrPrimary = async () => {
    const primary = config?.radarr.primary || config?.radarr.master;
    if (!primary) return;
    setTestingRadarrMaster(true);
    setRadarrMasterStatus(null);
    try {
      const res = await testArrConnection('radarr', primary.base_url, primary.api_key);
      setRadarrMasterStatus(res.success ? `Connected! v${res.app_version || '4.x/5.x'}` : `Failed: ${res.message}`);
    } catch (e: any) {
      setRadarrMasterStatus(`Error: ${e.message}`);
    } finally {
      setTestingRadarrMaster(false);
    }
  };

  const handleTestLidarrPrimary = async () => {
    const primary = config?.lidarr.primary || config?.lidarr.master;
    if (!primary) return;
    setTestingLidarrMaster(true);
    setLidarrMasterStatus(null);
    try {
      const res = await testArrConnection('lidarr', primary.base_url, primary.api_key);
      setLidarrMasterStatus(res.success ? `Connected! v${res.app_version || '1.x/2.x'}` : `Failed: ${res.message}`);
    } catch (e: any) {
      setLidarrMasterStatus(`Error: ${e.message}`);
    } finally {
      setTestingLidarrMaster(false);
    }
  };

  const handleTriggerSync = async (appType: string) => {
    setSyncingArr(appType);
    try {
      const res = await triggerArrSyncNow(appType);
      alert(`Sync triggered: ${res.message}`);
    } catch (e: any) {
      alert(`Sync failed: ${e.message}`);
    } finally {
      setSyncingArr(null);
    }
  };

  // Plex & Trakt Handlers
  const handleTestPlex = async (idx: number, url: string, tokenOverride?: string) => {
    const token = tokenOverride || config?.plex.token || '';
    if (!url || !token) {
      alert('Plex URL and Token are required.');
      return;
    }
    setTestingPlexIdx(idx);
    try {
      const res = await testPlexConnection(url, token);
      setPlexStatusMap((prev) => ({
        ...prev,
        [idx]: res.success ? `Connected: ${res.server_name || 'Plex'} (v${res.version || '1.x'})` : `Error: ${res.message}`,
      }));
    } catch (e: any) {
      setPlexStatusMap((prev) => ({ ...prev, [idx]: `Error: ${e.message}` }));
    } finally {
      setTestingPlexIdx(null);
    }
  };

  const handleRefreshPlex = async (idx: number, url: string, tokenOverride?: string) => {
    const token = tokenOverride || config?.plex.token || '';
    setRefreshingPlexIdx(idx);
    try {
      await refreshPlexSections(url, token);
      alert('Library sections refresh command sent to Plex!');
    } catch (e: any) {
      alert(`Refresh failed: ${e.message}`);
    } finally {
      setRefreshingPlexIdx(null);
    }
  };

  const handleConnectPlexAccount = async () => {
    setPlexOAuthState('waiting');
    setPlexOAuthError(null);
    setPlexDiscoveredServers([]);
    setPlexAccountToken(null);

    // Open the popup synchronously on the click (before any await) so browsers don't block it.
    const popup = window.open('', 'conduit-plex-oauth', 'width=500,height=700');

    try {
      const pin = await createPlexOAuthPin();
      if (popup) {
        popup.location.href = pin.signin_url;
      } else {
        window.open(pin.signin_url, '_blank');
      }

      const deadline = Date.now() + 5 * 60 * 1000; // give up after 5 minutes
      while (Date.now() < deadline) {
        await new Promise((r) => setTimeout(r, 2000));
        const result = await pollPlexOAuthPin(pin.pin_id);
        if (result.authenticated) {
          setPlexDiscoveredServers(result.servers || []);
          setPlexAccountToken(result.account_token || null);
          setPlexOAuthState('idle');
          popup?.close();
          return;
        }
        if (popup && popup.closed) {
          // User closed the popup without approving — stop polling.
          setPlexOAuthState('idle');
          return;
        }
      }
      setPlexOAuthError('Timed out waiting for Plex account approval. Please try again.');
      setPlexOAuthState('error');
      popup?.close();
    } catch (e: any) {
      setPlexOAuthError(e.message || 'Failed to connect Plex account');
      setPlexOAuthState('error');
      popup?.close();
    }
  };

  const handleAddDiscoveredServer = async (server: PlexDiscoveredServer, idx: number) => {
    if (!plexAccountToken) return;
    const defaultConnection = server.connections.find((c) => c.local) || server.connections[0];
    const uri = selectedConnectionUri[idx] || defaultConnection?.uri;
    if (!uri) {
      alert(`No reachable connection found for '${server.name}'.`);
      return;
    }
    setAddingPlexServerIdx(idx);
    try {
      await addOAuthPlexServer(server.name, uri, plexAccountToken);
      const updated = await fetchSettings();
      setConfig(updated);
      setPlexDiscoveredServers((prev) => prev.filter((_, i) => i !== idx));
    } catch (e: any) {
      alert(`Failed to add '${server.name}': ${e.message}`);
    } finally {
      setAddingPlexServerIdx(null);
    }
  };

  const handleTestTrakt = async () => {
    if (!config?.trakt.client_id) {
      alert('Trakt Client ID is required.');
      return;
    }
    setTestingTrakt(true);
    setTraktStatus(null);
    try {
      const res = await testTraktConnection(config.trakt.client_id, config.trakt.access_token);
      setTraktStatus(res.success ? res.message : `Failed: ${res.message}`);
    } catch (e: any) {
      setTraktStatus(`Error: ${e.message}`);
    } finally {
      setTestingTrakt(false);
    }
  };

  const loadScrobbles = async () => {
    setLoadingScrobbles(true);
    try {
      const res = await fetchPlexScrobbles({ limit: 10 });
      setScrobbles(res.items);
    } catch (e) {
      console.error('Failed to load scrobbles', e);
    } finally {
      setLoadingScrobbles(false);
    }
  };

  const loadOmbiRequests = async () => {
    setLoadingOmbi(true);
    try {
      const res = await fetchOmbiRequests({ limit: 10 });
      setOmbiRequests(res.items);
    } catch (e) {
      console.error('Failed to load Ombi requests', e);
    } finally {
      setLoadingOmbi(false);
    }
  };

  const handleTestPlexWebhook = async () => {
    setTestingPlexWebhook(true);
    try {
      await sendPlexWebhookTest();
      await loadScrobbles();
      alert('Plex test scrobble webhook dispatched and recorded successfully!');
    } catch (e: any) {
      alert(`Plex webhook test failed: ${e.message}`);
    } finally {
      setTestingPlexWebhook(false);
    }
  };

  const handleTestOmbiWebhook = async () => {
    setTestingOmbiWebhook(true);
    try {
      await sendOmbiWebhookTest();
      await loadOmbiRequests();
      alert('Ombi test request webhook dispatched and recorded successfully!');
    } catch (e: any) {
      alert(`Ombi webhook test failed: ${e.message}`);
    } finally {
      setTestingOmbiWebhook(false);
    }
  };

  // Notification Test Handler
  const handleTestNotification = async (targetId: string) => {
    setTestingChannel(targetId);
    setNotifyStatus(null);
    try {
      const res = await sendTestNotification(targetId);
      setNotifyStatus(res.message);
      setTimeout(() => setNotifyStatus(null), 4000);
    } catch (e: any) {
      setNotifyStatus(`Notification failed: ${e.message}`);
    } finally {
      setTestingChannel(null);
    }
  };

  const defaultChannelFor = (type: NotificationChannel['type']): NotificationChannel => {
    switch (type) {
      case 'mattermost':
        return { type: 'mattermost', enabled: true, webhook_url: '', bot_name: 'Conduit', use_living_cards: true, use_threading: true };
      case 'discord':
        return { type: 'discord', enabled: true, webhook_url: '', username: 'Conduit' };
      case 'pushover':
        return { type: 'pushover', user_key: '', api_token: '' };
      case 'webhook':
        return { type: 'webhook', enabled: true, url: '', payload_mode: 'summary' };
    }
  };

  const CHANNEL_LABELS: Record<NotificationChannel['type'], { label: string; icon: string }> = {
    mattermost: { label: 'Mattermost', icon: '💬' },
    discord: { label: 'Discord', icon: '🎮' },
    pushover: { label: 'Pushover', icon: '🔔' },
    webhook: { label: 'Webhook', icon: '🪝' },
  };

  const CATEGORY_LABELS: Record<string, string> = {
    grab: 'Arr Release Grabs',
    download: 'Downloads Staged',
    replacement: 'MediaReplacer / Self-Heal',
    health: 'Health Alerts',
    error: 'Other Errors',
    autopurge: 'Auto-Purge / Disk Cleanup',
    sync: 'Arr Sync Completed',
    scrobble: 'Plex Playback / Scrobbles',
  };

  // Pipeline Regex Evaluation Simulator
  const handleTestRegex = async () => {
    if (!config) return;
    setEvaluatingRegex(true);
    try {
      const res = await testPipelineRegex(config.rule_pipeline.unregistered_pattern, regexTestStr);
      setRegexResult(res);
    } catch (e: any) {
      setRegexResult({ matched: false, pattern_valid: false, error: e.message });
    } finally {
      setEvaluatingRegex(false);
    }
  };

  // Staging Classifier Simulator Handler
  const handleRunClassifySimulator = async () => {
    if (!testClassifyName.trim()) return;
    setTestingClassify(true);
    try {
      const res = await classifyFile({
        name: testClassifyName.trim(),
        hash: testClassifyHash.trim() || undefined,
        node: testClassifyNode.trim() || undefined,
        tracker: testClassifyTracker.trim() || undefined,
      });
      setClassifyResult(res);
    } catch (e: any) {
      alert(`Classification simulation failed: ${e.message}`);
    } finally {
      setTestingClassify(false);
    }
  };

  // Node Transmission RPC settings modal
  const openDaemonModal = async (nodeName: string) => {
    setEditingDaemonNode(nodeName);
    setLoadingSession(true);
    setPortStatus(null);
    try {
      const sess = await fetchNodeSession(nodeName);
      setDaemonSession(sess);
    } catch (e) {
      setDaemonSession(null);
    } finally {
      setLoadingSession(false);
    }
  };

  const handleSaveDaemonSession = async () => {
    if (!editingDaemonNode || !daemonSession) return;
    setSavingSession(true);
    try {
      await updateNodeSession(editingDaemonNode, {
        'download-dir': daemonSession['download-dir'],
        'speed-limit-down': Number(daemonSession['speed-limit-down']),
        'speed-limit-down-enabled': daemonSession['speed-limit-down-enabled'],
        'speed-limit-up': Number(daemonSession['speed-limit-up']),
        'speed-limit-up-enabled': daemonSession['speed-limit-up-enabled'],
        'alt-speed-down': Number(daemonSession['alt-speed-down']),
        'alt-speed-up': Number(daemonSession['alt-speed-up']),
        'alt-speed-enabled': daemonSession['alt-speed-enabled'],
        'peer-limit-global': Number(daemonSession['peer-limit-global']),
        'peer-limit-per-torrent': Number(daemonSession['peer-limit-per-torrent']),
        'peer-port': Number(daemonSession['peer-port']),
        'dht-enabled': daemonSession['dht-enabled'],
        'pex-enabled': daemonSession['pex-enabled'],
      });
      alert('Fetcher Daemon options applied successfully!');
      setEditingDaemonNode(null);
    } catch (e: any) {
      alert(`Failed to update daemon: ${e.message}`);
    } finally {
      setSavingSession(false);
    }
  };

  const handleTestPort = async () => {
    if (!editingDaemonNode) return;
    setTestingPort(true);
    setPortStatus(null);
    try {
      const open = await testNodePort(editingDaemonNode);
      setPortStatus(open);
    } catch (e) {
      setPortStatus(false);
    } finally {
      setTestingPort(false);
    }
  };

  const handleExportBackup = async () => {
    try {
      const bundle = await exportBackup(backupPass.trim() || undefined);
      const blob = new Blob([JSON.stringify(bundle, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `conduit-backup-${new Date().toISOString().slice(0, 10)}.json`;
      a.click();
    } catch (err: any) {
      alert(`Backup failed: ${err.message}`);
    }
  };

  const handleRestoreBackup = async () => {
    try {
      const parsed = JSON.parse(restoreBundle);
      await restoreBackup(parsed, restorePass.trim() || undefined);
      alert('Backup restored successfully!');
      loadData();
    } catch (err: any) {
      alert(`Restore failed: ${err.message}`);
    }
  };

  const handleCreateToken = async () => {
    if (!newTokenName.trim()) return;
    try {
      const scopes =
        newTokenScope === 'fetcher'
          ? ['fetcher', 'sync']
          : newTokenScope === 'read'
          ? ['torrents:read', 'system:read']
          : ['*'];
      const res = await createToken(newTokenName.trim(), scopes);
      setNewRawToken(res.raw_token);
      setNewTokenName('');
      const updated = await fetchTokens();
      setTokens(updated);
    } catch (err: any) {
      alert(`Token creation failed: ${err.message}`);
    }
  };

  const handleDeleteToken = async (id: string) => {
    if (!confirm('Revoke this API token?')) return;
    await deleteToken(id);
    const updated = await fetchTokens();
    setTokens(updated);
  };

  if (loading) {
    return <div className="p-8 text-slate-400">Loading settings...</div>;
  }

  if (loadError || !config) {
    return (
      <div className="p-8">
        <div className="max-w-md rounded-xl border border-rose-800/50 bg-rose-950/30 p-6">
          <p className="text-sm font-semibold text-rose-300">Couldn't load settings</p>
          <p className="mt-1 text-sm text-rose-400/90">{loadError || 'Unknown error'}</p>
          <button
            onClick={loadData}
            className="mt-4 rounded-lg bg-rose-600 px-4 py-2 text-sm font-semibold text-white hover:bg-rose-500"
          >
            Retry
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-auto p-8 space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <div className="flex items-center space-x-2.5">
            <h1 className="text-2xl font-bold text-slate-100">Conduit Application Settings</h1>
            <span className="rounded bg-slate-800 px-2 py-0.5 text-xs font-bold font-mono text-sky-400 border border-slate-700">
              v{APP_VERSION}
            </span>
          </div>
          <p className="text-sm text-slate-400 mt-1">Manage ecosystem nodes, media sync paths, automation pipelines, and vault.</p>
        </div>
        <button
          onClick={handleSave}
          disabled={saving}
          className="flex items-center space-x-2 rounded-xl bg-brand-600 px-5 py-2 text-sm font-semibold text-white shadow-lg shadow-brand-500/20 hover:bg-brand-500 disabled:opacity-50 transition-colors"
        >
          <Save className="h-4 w-4" />
          <span>{saving ? 'Saving...' : 'Save Changes'}</span>
        </button>
      </div>

      {statusMsg && (
        <div className="rounded-xl bg-emerald-500/10 border border-emerald-500/20 p-3 text-sm text-emerald-400">
          {statusMsg}
        </div>
      )}

      {/* Tabs */}
      <div className="flex space-x-2 border-b border-slate-800 pb-2 overflow-x-auto">
        {[
          { id: 'nodes', label: 'Retrievers', icon: Server },
          { id: 'arr', label: 'Arr Stack', icon: Tv },
          { id: 'zones', label: 'Zones', icon: Boxes },
          { id: 'sync', label: 'Node Media Routing', icon: FolderSync },
          { id: 'plex_trakt', label: 'Plex & Trakt', icon: Film },
          { id: 'notify', label: 'Notifications', icon: Bell },
          { id: 'rules', label: 'Rules & Pipeline', icon: Sliders },
          { id: 'system', label: 'System & Network', icon: Cpu },
          { id: 'tokens', label: 'API Tokens', icon: Key },
          { id: 'backup', label: 'Vault & Backup', icon: Shield },
        ].map((t) => {
          const Icon = t.icon;
          const active = tab === t.id;
          return (
            <button
              key={t.id}
              onClick={() => setTab(t.id as any)}
              className={`flex items-center space-x-2 rounded-lg px-3.5 py-2 text-sm font-semibold transition-colors shrink-0 ${
                active ? 'bg-brand-600 text-white' : 'text-slate-400 hover:bg-slate-800 hover:text-slate-200'
              }`}
            >
              <Icon className="h-4 w-4" />
              <span>{t.label}</span>
            </button>
          );
        })}
      </div>

      {/* Tab 1: Retrievers */}
      {tab === 'nodes' && (
        <div className="space-y-6">
          <div className="flex items-center justify-between">
            <h2 className="text-lg font-bold text-slate-200">Retriever Instances</h2>
            <button
              onClick={() => {
                const name = prompt('Enter new node name (e.g. seedbox):');
                if (name && !config.nodes[name]) {
                  setConfig({
                    ...config,
                    nodes: {
                      ...config.nodes,
                      [name]: {
                        name,
                        host: 'localhost',
                        port: 9091,
                        rpc_path: '/transmission/rpc',
                        use_ssl: false,
                        enabled: true,
                        fetcher_only: false,
                        auto_purge_enabled: true,
                      },
                    },
                  });
                }
              }}
              className="flex items-center space-x-1.5 rounded-lg bg-slate-800 px-3 py-1.5 text-xs font-semibold text-brand-400 hover:bg-slate-700"
            >
              <Plus className="h-4 w-4" />
              <span>Add Node</span>
            </button>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            {Object.entries(config.nodes).map(([name, node]) => (
              <div key={name} className="rounded-2xl border border-slate-800 bg-slate-900/60 p-5 space-y-4">
                <div className="flex items-center justify-between border-b border-slate-800 pb-3">
                  <div className="flex items-center space-x-2">
                    <span className="font-bold text-base text-slate-100">{name}</span>
                    <button
                      onClick={() => openDaemonModal(name)}
                      className="rounded-md bg-brand-500/10 px-2 py-0.5 text-xs font-semibold text-brand-400 border border-brand-500/20 hover:bg-brand-500/20 transition-colors flex items-center space-x-1"
                    >
                      <Sliders className="h-3 w-3" />
                      <span>Configure Daemon RPC</span>
                    </button>
                  </div>
                  <div className="flex items-center space-x-3">
                    <label className="flex items-center space-x-1.5 text-xs text-slate-300">
                      <input
                        type="checkbox"
                        checked={node.enabled}
                        onChange={(e) => {
                          setConfig({
                            ...config,
                            nodes: {
                              ...config.nodes,
                              [name]: { ...node, enabled: e.target.checked },
                            },
                          });
                        }}
                        className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                      />
                      <span>Enabled</span>
                    </label>
                    <button
                      onClick={() => {
                        const next = { ...config.nodes };
                        delete next[name];
                        setConfig({ ...config, nodes: next });
                      }}
                      className="text-rose-400 hover:text-rose-300 p-1"
                    >
                      <Trash2 className="h-4 w-4" />
                    </button>
                  </div>
                </div>

                <div className="grid grid-cols-3 gap-3">
                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Daemon Type</label>
                    <select
                      value={node.client_type || 'transmission'}
                      onChange={(e) => {
                        const nextType = e.target.value as 'transmission' | 'qbittorrent' | 'deluge' | 'synapse';
                        const knownDefaultPorts = [9091, 8080, 8112, 50051];
                        let defaultPort = node.port;
                        let defaultRpc: string;
                        if (nextType === 'qbittorrent') {
                          if (knownDefaultPorts.includes(defaultPort)) defaultPort = 8080;
                          defaultRpc = '/api/v2';
                        } else if (nextType === 'deluge') {
                          if (knownDefaultPorts.includes(defaultPort)) defaultPort = 8112;
                          defaultRpc = '/json';
                        } else if (nextType === 'synapse') {
                          // gRPC, not a path-addressed HTTP RPC endpoint — rpc_path doesn't apply.
                          if (knownDefaultPorts.includes(defaultPort)) defaultPort = 50051;
                          defaultRpc = '';
                        } else {
                          if (knownDefaultPorts.includes(defaultPort)) defaultPort = 9091;
                          defaultRpc = '/transmission/rpc';
                        }
                        setConfig({
                          ...config,
                          nodes: {
                            ...config.nodes,
                            [name]: { ...node, client_type: nextType, port: defaultPort, rpc_path: defaultRpc },
                          },
                        });
                      }}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                    >
                      <option value="transmission">Transmission</option>
                      <option value="qbittorrent">qBittorrent (Web v2)</option>
                      <option value="deluge">Deluge (Web RPC)</option>
                      <option value="synapse">Synapse (gRPC)</option>
                    </select>
                  </div>
                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Host</label>
                    <input
                      type="text"
                      value={node.host}
                      onChange={(e) => {
                        setConfig({
                          ...config,
                          nodes: { ...config.nodes, [name]: { ...node, host: e.target.value } },
                        });
                      }}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                    />
                  </div>
                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Port</label>
                    <input
                      type="number"
                      value={node.port}
                      onChange={(e) => {
                        setConfig({
                          ...config,
                          nodes: { ...config.nodes, [name]: { ...node, port: Number(e.target.value) } },
                        });
                      }}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                    />
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-3">
                  <div className={node.client_type === 'synapse' ? 'opacity-40' : ''}>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">RPC Username</label>
                    <input
                      type="text"
                      disabled={node.client_type === 'synapse'}
                      placeholder={node.client_type === 'synapse' ? 'Not used by Synapse' : ''}
                      value={node.username || ''}
                      onChange={(e) => {
                        setConfig({
                          ...config,
                          nodes: { ...config.nodes, [name]: { ...node, username: e.target.value || undefined } },
                        });
                      }}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none disabled:cursor-not-allowed"
                    />
                  </div>
                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                      {node.client_type === 'synapse' ? 'Auth Token (optional)' : 'RPC Password'}
                    </label>
                    <input
                      type="password"
                      placeholder={node.client_type === 'synapse' ? 'Matches [rpc].auth_token in synapse.toml, if set' : ''}
                      value={node.password || ''}
                      onChange={(e) => {
                        setConfig({
                          ...config,
                          nodes: { ...config.nodes, [name]: { ...node, password: e.target.value || undefined } },
                        });
                      }}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                    />
                  </div>
                </div>

                <div className="rounded-xl border border-slate-800/80 bg-slate-800/30 px-3.5 py-2.5 flex items-center justify-between text-xs text-slate-400">
                  <div className="flex items-center space-x-2">
                    <span className="text-brand-400 font-semibold">📁 Queue Routing:</span>
                    <span>Intake staging directories for this node are managed centrally under <strong>Media Routing Hub & Overrides</strong>.</span>
                  </div>
                  <button
                    type="button"
                    onClick={() => setTab('sync')}
                    className="text-brand-400 hover:text-brand-300 font-semibold underline shrink-0 ml-2"
                  >
                    Configure Overrides &rarr;
                  </button>
                </div>
              </div>
            ))}
          </div>

          {/* Tracker Circuit Breaker Configuration — moved here from System & Network since it's
              specific to Retrievers (Transmission nodes), not general system/networking config. */}
          <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-4">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <h3 className="text-sm font-bold text-slate-200 flex items-center space-x-2">
                  <Zap className="h-4 w-4 text-amber-400" />
                  <span>Tracker Circuit Breaker & Swarm Pressure Relief</span>
                </h3>
                <p className="text-xs text-slate-400">
                  Automatically pauses torrent swarms on unreachable trackers while keeping a single active canary probe running to monitor recovery.
                </p>
              </div>
              <label className="relative inline-flex cursor-pointer items-center">
                <input
                  type="checkbox"
                  checked={config.tracker_circuit_breaker?.enabled ?? true}
                  onChange={(e) => {
                    const currentCb = config.tracker_circuit_breaker ?? DEFAULT_TRACKER_CB_CONFIG;
                    setConfig({
                      ...config,
                      tracker_circuit_breaker: { ...currentCb, enabled: e.target.checked },
                    });
                  }}
                  className="peer sr-only"
                />
                <div className="peer h-6 w-11 rounded-full bg-slate-700 after:absolute after:top-[2px] after:left-[2px] after:h-5 after:w-5 after:rounded-full after:bg-white after:transition-all after:content-[''] peer-checked:bg-amber-600 peer-checked:after:translate-x-full"></div>
              </label>
            </div>

            {(config.tracker_circuit_breaker?.enabled ?? true) && (
              <div className="space-y-4 pt-2">
                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                      Failure Ratio Threshold (%)
                    </label>
                    <div className="flex items-center space-x-3">
                      <input
                        type="range"
                        min="10"
                        max="100"
                        step="5"
                        value={Math.round((config.tracker_circuit_breaker?.failure_ratio_threshold ?? 0.50) * 100)}
                        onChange={(e) => {
                          const currentCb = config.tracker_circuit_breaker ?? DEFAULT_TRACKER_CB_CONFIG;
                          setConfig({
                            ...config,
                            tracker_circuit_breaker: { ...currentCb, failure_ratio_threshold: Number(e.target.value) / 100 },
                          });
                        }}
                        className="w-full h-2 bg-slate-700 rounded-lg appearance-none cursor-pointer accent-amber-500"
                      />
                      <span className="text-sm font-bold font-mono text-amber-400 min-w-[45px]">
                        {Math.round((config.tracker_circuit_breaker?.failure_ratio_threshold ?? 0.50) * 100)}%
                      </span>
                    </div>
                    <span className="text-[11px] text-slate-500 mt-1 block">
                      Percentage of swarms that must fail announce before tripping breaker (e.g. 50%).
                    </span>
                  </div>

                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                      Minimum Failing Swarms
                    </label>
                    <input
                      type="number"
                      min="1"
                      max="100"
                      value={config.tracker_circuit_breaker?.min_failures ?? 3}
                      onChange={(e) => {
                        const currentCb = config.tracker_circuit_breaker ?? DEFAULT_TRACKER_CB_CONFIG;
                        setConfig({
                          ...config,
                          tracker_circuit_breaker: { ...currentCb, min_failures: Math.max(1, Number(e.target.value)) },
                        });
                      }}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-amber-500 focus:outline-none font-mono"
                    />
                    <span className="text-[11px] text-slate-500 mt-1 block">
                      Minimum number of torrents failing before breaker can trip (default: 3).
                    </span>
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                      Stuck-Open Timeout Ceiling (Hours)
                    </label>
                    <input
                      type="number"
                      min="1"
                      max="48"
                      value={Math.round((config.tracker_circuit_breaker?.max_tripped_secs ?? 21600) / 3600)}
                      onChange={(e) => {
                        const currentCb = config.tracker_circuit_breaker ?? DEFAULT_TRACKER_CB_CONFIG;
                        setConfig({
                          ...config,
                          tracker_circuit_breaker: { ...currentCb, max_tripped_secs: Math.max(1, Number(e.target.value)) * 3600 },
                        });
                      }}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-amber-500 focus:outline-none font-mono"
                    />
                    <span className="text-[11px] text-slate-500 mt-1 block">
                      Forces breaker recovery if canary announce state is lost (default: 6h).
                    </span>
                  </div>

                  <div className="flex items-center space-x-4 pt-4">
                    <label className="flex items-center space-x-2 text-xs text-slate-300 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={config.tracker_circuit_breaker?.auto_resume_on_recovery ?? true}
                        onChange={(e) => {
                          const currentCb = config.tracker_circuit_breaker ?? DEFAULT_TRACKER_CB_CONFIG;
                          setConfig({
                            ...config,
                            tracker_circuit_breaker: { ...currentCb, auto_resume_on_recovery: e.target.checked },
                          });
                        }}
                        className="rounded border-slate-700 bg-slate-800 text-amber-500 focus:ring-amber-500"
                      />
                      <span>Auto-resume all on recovery</span>
                    </label>
                    <label className="flex items-center space-x-2 text-xs text-slate-300 cursor-pointer" title="When off, a tripped tracker stays tripped until its cooldown fully elapses — no canary probe request is sent to test recovery early.">
                      <input
                        type="checkbox"
                        checked={config.tracker_circuit_breaker?.canary_probe_enabled ?? true}
                        onChange={(e) => {
                          const currentCb = config.tracker_circuit_breaker ?? DEFAULT_TRACKER_CB_CONFIG;
                          setConfig({
                            ...config,
                            tracker_circuit_breaker: { ...currentCb, canary_probe_enabled: e.target.checked },
                          });
                        }}
                        className="rounded border-slate-700 bg-slate-800 text-amber-500 focus:ring-amber-500"
                      />
                      <span>Canary probe recovery</span>
                    </label>
                  </div>
                </div>

                <div className="grid grid-cols-3 gap-4 pt-1">
                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                      Initial Backoff (Seconds)
                    </label>
                    <input
                      type="number"
                      min="1"
                      max="3600"
                      value={config.tracker_circuit_breaker?.initial_backoff_secs ?? DEFAULT_TRACKER_CB_CONFIG.initial_backoff_secs}
                      onChange={(e) => {
                        const currentCb = config.tracker_circuit_breaker ?? DEFAULT_TRACKER_CB_CONFIG;
                        setConfig({
                          ...config,
                          tracker_circuit_breaker: { ...currentCb, initial_backoff_secs: Math.max(1, Number(e.target.value)) },
                        });
                      }}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-amber-500 focus:outline-none font-mono"
                    />
                    <span className="text-[11px] text-slate-500 mt-1 block">
                      Cooldown before checking recovery, doubling on relapse (default: 30s).
                    </span>
                  </div>

                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                      Recovery Ramp Window (Seconds)
                    </label>
                    <input
                      type="number"
                      min="1"
                      max="3600"
                      value={config.tracker_circuit_breaker?.recovery_ramp_secs ?? DEFAULT_TRACKER_CB_CONFIG.recovery_ramp_secs}
                      onChange={(e) => {
                        const currentCb = config.tracker_circuit_breaker ?? DEFAULT_TRACKER_CB_CONFIG;
                        setConfig({
                          ...config,
                          tracker_circuit_breaker: { ...currentCb, recovery_ramp_secs: Math.max(1, Number(e.target.value)) },
                        });
                      }}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-amber-500 focus:outline-none font-mono"
                    />
                    <span className="text-[11px] text-slate-500 mt-1 block">
                      How long to stagger resuming paused torrents after a successful canary check (default: 30s).
                    </span>
                  </div>

                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                      Recovery Success Threshold
                    </label>
                    <input
                      type="number"
                      min="1"
                      max="50"
                      value={config.tracker_circuit_breaker?.recovery_success_threshold ?? DEFAULT_TRACKER_CB_CONFIG.recovery_success_threshold}
                      onChange={(e) => {
                        const currentCb = config.tracker_circuit_breaker ?? DEFAULT_TRACKER_CB_CONFIG;
                        setConfig({
                          ...config,
                          tracker_circuit_breaker: { ...currentCb, recovery_success_threshold: Math.max(1, Number(e.target.value)) },
                        });
                      }}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-amber-500 focus:outline-none font-mono"
                    />
                    <span className="text-[11px] text-slate-500 mt-1 block">
                      Consecutive successful canary checks required before declaring healthy (default: 5).
                    </span>
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {/* Tab 2: Arr Stack (Sonarr / Radarr / Lidarr) Primary & Replica Ecosystem */}
      {tab === 'arr' && (
        <div className="space-y-8 max-w-5xl">
          {/* Info Banner */}
          <div className="rounded-xl border border-sky-500/20 bg-sky-500/10 p-4 text-xs text-sky-300 leading-relaxed">
            <strong>MediaReplacer & Sync:</strong> Configure your Primary instance below to allow Conduit to automatically re-search dead/unregistered torrents when tracker errors occur. If you manage multiple instances (e.g. 4K, Anime, or Remote Seedbox replicas), expand the <em>Library Replication</em> section to synchronize libraries.
          </div>

          {/* Sonarr Primary & Replicas */}
          <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-5">
            <div className="flex items-center justify-between border-b border-slate-800 pb-3">
              <div className="flex items-center space-x-2.5">
                <div className="rounded-lg bg-sky-500/10 p-2 text-sky-400 border border-sky-500/20">
                  <Tv className="h-5 w-5" />
                </div>
                <div>
                  <h2 className="text-base font-bold text-slate-100">Sonarr TV Configuration</h2>
                  <p className="text-xs text-slate-400">Primary instance for automated episode re-searches and optional replica syncing.</p>
                </div>
              </div>
              <label className="flex items-center space-x-2 text-sm text-slate-300">
                <input
                  type="checkbox"
                  checked={config.sonarr.enabled}
                  onChange={(e) => setConfig({ ...config, sonarr: { ...config.sonarr, enabled: e.target.checked } })}
                  className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                />
                <span>Enable Sonarr Integration</span>
              </label>
            </div>

            {/* Inbound Webhook Card for Sonarr */}
            <div className="rounded-xl border border-sky-500/20 bg-sky-500/5 p-4 space-y-2">
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-2">
                  <Terminal className="h-4 w-4 text-sky-400" />
                  <span className="text-xs font-bold uppercase tracking-wider text-sky-300">Sonarr Inbound Webhook URL</span>
                </div>
                <button
                  type="button"
                  onClick={() => {
                    navigator.clipboard.writeText(`${window.location.origin}/api/sonarr/inbound`);
                    setCopiedSonarrWebhook(true);
                    setTimeout(() => setCopiedSonarrWebhook(false), 2000);
                  }}
                  className="rounded-lg bg-slate-800 border border-slate-700 px-3 py-1 text-xs font-semibold text-slate-200 hover:bg-slate-700 flex items-center space-x-1"
                >
                  {copiedSonarrWebhook ? <Check className="h-3.5 w-3.5 text-emerald-400" /> : <Copy className="h-3.5 w-3.5" />}
                  <span>{copiedSonarrWebhook ? 'Copied' : 'Copy Webhook'}</span>
                </button>
              </div>
              <input
                type="text"
                readOnly
                value={`${window.location.origin}/api/sonarr/inbound`}
                className="w-full rounded border border-sky-500/30 bg-slate-900 px-3 py-1.5 font-mono text-xs text-sky-300 focus:outline-none"
              />
              <p className="text-[11px] text-slate-400">
                In Sonarr, navigate to <strong>Settings &rarr; Connect &rarr; Add Notification &rarr; Webhook</strong>. Paste this URL and check <em>On Grab</em>, <em>On Import</em>, <em>On Upgrade</em>, <em>On Rename</em>, <em>On Series Add/Delete</em>, and <em>On Health Issue</em>.
              </p>
            </div>

            {/* Primary Instance Config */}
            <div className="space-y-3 rounded-xl border border-slate-800 bg-slate-800/40 p-4">
              <div className="flex items-center justify-between">
                <div>
                  <span className="text-xs font-bold uppercase tracking-wider text-sky-400">Primary Instance (MediaReplacer)</span>
                  <p className="text-[11px] text-slate-400 mt-0.5">Used by Conduit to trigger automatic episode re-searches on dead torrents.</p>
                </div>
                <div className="flex items-center space-x-3">
                  {sonarrMasterStatus && (
                    <span className="text-xs font-mono text-slate-300">{sonarrMasterStatus}</span>
                  )}
                  <button
                    type="button"
                    onClick={handleTestSonarrPrimary}
                    disabled={testingSonarrMaster || !(config.sonarr.primary || config.sonarr.master)}
                    className="rounded-lg bg-slate-800 px-3 py-1.5 text-xs font-semibold text-sky-400 hover:bg-slate-700 disabled:opacity-50"
                  >
                    {testingSonarrMaster ? 'Testing...' : 'Test Connection'}
                  </button>
                </div>
              </div>

              <div className="grid grid-cols-1 md:grid-cols-3 gap-3 pt-1">
                <div>
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Instance Name</label>
                  <input
                    type="text"
                    value={config.sonarr.primary?.name || config.sonarr.master?.name || ''}
                    placeholder="Sonarr-Primary"
                    onChange={(e) => {
                      const current = config.sonarr.primary || config.sonarr.master || { name: '', base_url: '', api_key: '', version: 3 };
                      setConfig({ ...config, sonarr: { ...config.sonarr, primary: { ...current, name: e.target.value } } });
                    }}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                </div>
                <div>
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Base URL</label>
                  <input
                    type="text"
                    value={config.sonarr.primary?.base_url || config.sonarr.master?.base_url || ''}
                    placeholder="http://192.168.1.50:8989"
                    onChange={(e) => {
                      const current = config.sonarr.primary || config.sonarr.master || { name: '', base_url: '', api_key: '', version: 3 };
                      setConfig({ ...config, sonarr: { ...config.sonarr, primary: { ...current, base_url: e.target.value } } });
                    }}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                </div>
                <div>
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">API Key</label>
                  <input
                    type="password"
                    value={config.sonarr.primary?.api_key || config.sonarr.master?.api_key || ''}
                    placeholder="Sonarr API Key"
                    onChange={(e) => {
                      const current = config.sonarr.primary || config.sonarr.master || { name: '', base_url: '', api_key: '', version: 3 };
                      setConfig({ ...config, sonarr: { ...config.sonarr, primary: { ...current, api_key: e.target.value } } });
                    }}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                </div>
              </div>
            </div>

            {/* Collapsible Multi-Instance Replicas */}
            <div className="rounded-xl border border-slate-800/80 bg-slate-900/40 overflow-hidden">
              <button
                type="button"
                onClick={() => setShowSonarrReplicas(!showSonarrReplicas)}
                className="w-full flex items-center justify-between p-4 text-left hover:bg-slate-800/30 transition-colors"
              >
                <div className="flex items-center space-x-2.5">
                  {showSonarrReplicas ? (
                    <ChevronDown className="h-4 w-4 text-slate-400" />
                  ) : (
                    <ChevronRight className="h-4 w-4 text-slate-400" />
                  )}
                  <span className="text-xs font-bold uppercase tracking-wider text-slate-300">
                    Multi-Instance Library Replication (Optional Replicas)
                  </span>
                  <span className="rounded-full bg-slate-800 px-2 py-0.5 text-[10px] font-mono text-slate-400 border border-slate-700">
                    {(config.sonarr.replicas || config.sonarr.slaves || []).length} Replicas
                  </span>
                </div>
                <span className="text-xs text-sky-400 font-medium">
                  {showSonarrReplicas ? 'Hide Replicas' : 'Configure Replicas'}
                </span>
              </button>

              {showSonarrReplicas && (
                <div className="p-4 pt-0 space-y-4 border-t border-slate-800/60">
                  <div className="flex items-center justify-between pt-3">
                    <p className="text-xs text-slate-400">
                      Synchronize series and monitored states from Primary to downstream replicas (e.g. secondary seedboxes).
                    </p>
                    <button
                      type="button"
                      onClick={() => {
                        const replicas = config.sonarr.replicas || config.sonarr.slaves || [];
                        const newReplica: ArrNodeConfig = {
                          name: `Sonarr-Replica-${replicas.length + 1}`,
                          base_url: 'http://localhost:8989',
                          api_key: '',
                          version: 3,
                        };
                        setConfig({
                          ...config,
                          sonarr: { ...config.sonarr, replicas: [...replicas, newReplica] },
                        });
                      }}
                      className="flex items-center space-x-1 rounded-lg bg-slate-800 px-3 py-1 text-xs font-semibold text-sky-400 hover:bg-slate-700"
                    >
                      <Plus className="h-3.5 w-3.5" />
                      <span>Add Replica</span>
                    </button>
                  </div>

                  {(config.sonarr.replicas || config.sonarr.slaves || []).map((replica, idx) => (
                    <div key={idx} className="rounded-xl border border-slate-800 bg-slate-800/20 p-3.5 space-y-3">
                      <div className="flex items-center justify-between">
                        <span className="font-semibold text-sm text-slate-200">{replica.name || `Replica #${idx + 1}`}</span>
                        <button
                          type="button"
                          onClick={() => {
                            const replicas = config.sonarr.replicas || config.sonarr.slaves || [];
                            const updated = replicas.filter((_, i) => i !== idx);
                            setConfig({ ...config, sonarr: { ...config.sonarr, replicas: updated } });
                          }}
                          className="text-rose-400 hover:text-rose-300"
                        >
                          <Trash2 className="h-4 w-4" />
                        </button>
                      </div>

                      <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                        <input
                          type="text"
                          value={replica.name}
                          placeholder="Replica Name"
                          onChange={(e) => {
                            const replicas = [...(config.sonarr.replicas || config.sonarr.slaves || [])];
                            replicas[idx] = { ...replicas[idx], name: e.target.value };
                            setConfig({ ...config, sonarr: { ...config.sonarr, replicas } });
                          }}
                          className="rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                        />
                        <input
                          type="text"
                          value={replica.base_url}
                          placeholder="http://remote-sonarr:8989"
                          onChange={(e) => {
                            const replicas = [...(config.sonarr.replicas || config.sonarr.slaves || [])];
                            replicas[idx] = { ...replicas[idx], base_url: e.target.value };
                            setConfig({ ...config, sonarr: { ...config.sonarr, replicas } });
                          }}
                          className="rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                        />
                        <input
                          type="password"
                          value={replica.api_key}
                          placeholder="Replica API Key"
                          onChange={(e) => {
                            const replicas = [...(config.sonarr.replicas || config.sonarr.slaves || [])];
                            replicas[idx] = { ...replicas[idx], api_key: e.target.value };
                            setConfig({ ...config, sonarr: { ...config.sonarr, replicas } });
                          }}
                          className="rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                        />
                      </div>
                    </div>
                  ))}

                  {/* Sync Controls */}
                  <div className="flex items-center justify-between pt-3 border-t border-slate-800">
                    <div className="flex items-center space-x-3 text-xs text-slate-400">
                      <span>Sync Interval:</span>
                      <input
                        type="number"
                        value={config.sonarr.sync_interval_mins}
                        onChange={(e) => setConfig({ ...config, sonarr: { ...config.sonarr, sync_interval_mins: Number(e.target.value) } })}
                        className="w-20 rounded border border-slate-700 bg-slate-800 px-2 py-1 text-slate-200"
                      />
                      <span>minutes</span>
                    </div>
                    <button
                      type="button"
                      onClick={() => handleTriggerSync('sonarr')}
                      disabled={syncingArr === 'sonarr' || !config.sonarr.enabled}
                      className="flex items-center space-x-1.5 rounded-lg bg-sky-600 px-4 py-1.5 text-xs font-semibold text-white hover:bg-sky-500 disabled:opacity-50"
                    >
                      <RotateCcw className={`h-3.5 w-3.5 ${syncingArr === 'sonarr' ? 'animate-spin' : ''}`} />
                      <span>Trigger Sonarr Sync Now</span>
                    </button>
                  </div>
                </div>
              )}
            </div>
          </div>

          {/* Radarr Primary & Replicas */}
          <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-5">
            <div className="flex items-center justify-between border-b border-slate-800 pb-3">
              <div className="flex items-center space-x-2.5">
                <div className="rounded-lg bg-amber-500/10 p-2 text-amber-400 border border-amber-500/20">
                  <Film className="h-5 w-5" />
                </div>
                <div>
                  <h2 className="text-base font-bold text-slate-100">Radarr Movies Configuration</h2>
                  <p className="text-xs text-slate-400">Primary instance for automated movie re-searches and optional replica syncing.</p>
                </div>
              </div>
              <label className="flex items-center space-x-2 text-sm text-slate-300">
                <input
                  type="checkbox"
                  checked={config.radarr.enabled}
                  onChange={(e) => setConfig({ ...config, radarr: { ...config.radarr, enabled: e.target.checked } })}
                  className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                />
                <span>Enable Radarr Integration</span>
              </label>
            </div>

            {/* Inbound Webhook Card for Radarr */}
            <div className="rounded-xl border border-amber-500/20 bg-amber-500/5 p-4 space-y-2">
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-2">
                  <Terminal className="h-4 w-4 text-amber-400" />
                  <span className="text-xs font-bold uppercase tracking-wider text-amber-300">Radarr Inbound Webhook URL</span>
                </div>
                <button
                  type="button"
                  onClick={() => {
                    navigator.clipboard.writeText(`${window.location.origin}/api/radarr/inbound`);
                    setCopiedRadarrWebhook(true);
                    setTimeout(() => setCopiedRadarrWebhook(false), 2000);
                  }}
                  className="rounded-lg bg-slate-800 border border-slate-700 px-3 py-1 text-xs font-semibold text-slate-200 hover:bg-slate-700 flex items-center space-x-1"
                >
                  {copiedRadarrWebhook ? <Check className="h-3.5 w-3.5 text-emerald-400" /> : <Copy className="h-3.5 w-3.5" />}
                  <span>{copiedRadarrWebhook ? 'Copied' : 'Copy Webhook'}</span>
                </button>
              </div>
              <input
                type="text"
                readOnly
                value={`${window.location.origin}/api/radarr/inbound`}
                className="w-full rounded border border-amber-500/30 bg-slate-900 px-3 py-1.5 font-mono text-xs text-amber-300 focus:outline-none"
              />
              <p className="text-[11px] text-slate-400">
                In Radarr, navigate to <strong>Settings &rarr; Connect &rarr; Add Notification &rarr; Webhook</strong>. Paste this URL and check <em>On Grab</em>, <em>On Import</em>, <em>On Upgrade</em>, <em>On Rename</em>, <em>On Movie Added/Delete</em>, and <em>On Health Issue</em>.
              </p>
            </div>

            {/* Primary Instance Config */}
            <div className="space-y-3 rounded-xl border border-slate-800 bg-slate-800/40 p-4">
              <div className="flex items-center justify-between">
                <div>
                  <span className="text-xs font-bold uppercase tracking-wider text-amber-400">Primary Instance (MediaReplacer)</span>
                  <p className="text-[11px] text-slate-400 mt-0.5">Used by Conduit to trigger automatic movie re-searches on dead torrents.</p>
                </div>
                <div className="flex items-center space-x-3">
                  {radarrMasterStatus && (
                    <span className="text-xs font-mono text-slate-300">{radarrMasterStatus}</span>
                  )}
                  <button
                    type="button"
                    onClick={handleTestRadarrPrimary}
                    disabled={testingRadarrMaster || !(config.radarr.primary || config.radarr.master)}
                    className="rounded-lg bg-slate-800 px-3 py-1.5 text-xs font-semibold text-amber-400 hover:bg-slate-700 disabled:opacity-50"
                  >
                    {testingRadarrMaster ? 'Testing...' : 'Test Connection'}
                  </button>
                </div>
              </div>

              <div className="grid grid-cols-1 md:grid-cols-3 gap-3 pt-1">
                <div>
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Instance Name</label>
                  <input
                    type="text"
                    value={config.radarr.primary?.name || config.radarr.master?.name || ''}
                    placeholder="Radarr-Primary"
                    onChange={(e) => {
                      const current = config.radarr.primary || config.radarr.master || { name: '', base_url: '', api_key: '', version: 4 };
                      setConfig({ ...config, radarr: { ...config.radarr, primary: { ...current, name: e.target.value } } });
                    }}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                </div>
                <div>
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Base URL</label>
                  <input
                    type="text"
                    value={config.radarr.primary?.base_url || config.radarr.master?.base_url || ''}
                    placeholder="http://192.168.1.50:7878"
                    onChange={(e) => {
                      const current = config.radarr.primary || config.radarr.master || { name: '', base_url: '', api_key: '', version: 4 };
                      setConfig({ ...config, radarr: { ...config.radarr, primary: { ...current, base_url: e.target.value } } });
                    }}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                </div>
                <div>
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">API Key</label>
                  <input
                    type="password"
                    value={config.radarr.primary?.api_key || config.radarr.master?.api_key || ''}
                    placeholder="Radarr API Key"
                    onChange={(e) => {
                      const current = config.radarr.primary || config.radarr.master || { name: '', base_url: '', api_key: '', version: 4 };
                      setConfig({ ...config, radarr: { ...config.radarr, primary: { ...current, api_key: e.target.value } } });
                    }}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                </div>
              </div>
            </div>

            {/* Collapsible Multi-Instance Replicas */}
            <div className="rounded-xl border border-slate-800/80 bg-slate-900/40 overflow-hidden">
              <button
                type="button"
                onClick={() => setShowRadarrReplicas(!showRadarrReplicas)}
                className="w-full flex items-center justify-between p-4 text-left hover:bg-slate-800/30 transition-colors"
              >
                <div className="flex items-center space-x-2.5">
                  {showRadarrReplicas ? (
                    <ChevronDown className="h-4 w-4 text-slate-400" />
                  ) : (
                    <ChevronRight className="h-4 w-4 text-slate-400" />
                  )}
                  <span className="text-xs font-bold uppercase tracking-wider text-slate-300">
                    Multi-Instance Library Replication (Optional Replicas)
                  </span>
                  <span className="rounded-full bg-slate-800 px-2 py-0.5 text-[10px] font-mono text-slate-400 border border-slate-700">
                    {(config.radarr.replicas || config.radarr.slaves || []).length} Replicas
                  </span>
                </div>
                <span className="text-xs text-amber-400 font-medium">
                  {showRadarrReplicas ? 'Hide Replicas' : 'Configure Replicas'}
                </span>
              </button>

              {showRadarrReplicas && (
                <div className="p-4 pt-0 space-y-4 border-t border-slate-800/60">
                  <div className="flex items-center justify-between pt-3">
                    <p className="text-xs text-slate-400">
                      Synchronize movies and monitored states from Primary to downstream replicas (e.g. secondary seedboxes).
                    </p>
                    <button
                      type="button"
                      onClick={() => {
                        const replicas = config.radarr.replicas || config.radarr.slaves || [];
                        const newReplica: ArrNodeConfig = {
                          name: `Radarr-Replica-${replicas.length + 1}`,
                          base_url: 'http://localhost:7878',
                          api_key: '',
                          version: 4,
                        };
                        setConfig({
                          ...config,
                          radarr: { ...config.radarr, replicas: [...replicas, newReplica] },
                        });
                      }}
                      className="flex items-center space-x-1 rounded-lg bg-slate-800 px-3 py-1 text-xs font-semibold text-amber-400 hover:bg-slate-700"
                    >
                      <Plus className="h-3.5 w-3.5" />
                      <span>Add Replica</span>
                    </button>
                  </div>

                  {(config.radarr.replicas || config.radarr.slaves || []).map((replica, idx) => (
                    <div key={idx} className="rounded-xl border border-slate-800 bg-slate-800/20 p-3.5 space-y-3">
                      <div className="flex items-center justify-between">
                        <span className="font-semibold text-sm text-slate-200">{replica.name || `Replica #${idx + 1}`}</span>
                        <button
                          type="button"
                          onClick={() => {
                            const replicas = config.radarr.replicas || config.radarr.slaves || [];
                            const updated = replicas.filter((_, i) => i !== idx);
                            setConfig({ ...config, radarr: { ...config.radarr, replicas: updated } });
                          }}
                          className="text-rose-400 hover:text-rose-300"
                        >
                          <Trash2 className="h-4 w-4" />
                        </button>
                      </div>

                      <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                        <input
                          type="text"
                          value={replica.name}
                          placeholder="Replica Name"
                          onChange={(e) => {
                            const replicas = [...(config.radarr.replicas || config.radarr.slaves || [])];
                            replicas[idx] = { ...replicas[idx], name: e.target.value };
                            setConfig({ ...config, radarr: { ...config.radarr, replicas } });
                          }}
                          className="rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                        />
                        <input
                          type="text"
                          value={replica.base_url}
                          placeholder="http://remote-radarr:7878"
                          onChange={(e) => {
                            const replicas = [...(config.radarr.replicas || config.radarr.slaves || [])];
                            replicas[idx] = { ...replicas[idx], base_url: e.target.value };
                            setConfig({ ...config, radarr: { ...config.radarr, replicas } });
                          }}
                          className="rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                        />
                        <input
                          type="password"
                          value={replica.api_key}
                          placeholder="Replica API Key"
                          onChange={(e) => {
                            const replicas = [...(config.radarr.replicas || config.radarr.slaves || [])];
                            replicas[idx] = { ...replicas[idx], api_key: e.target.value };
                            setConfig({ ...config, radarr: { ...config.radarr, replicas } });
                          }}
                          className="rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                        />
                      </div>
                    </div>
                  ))}

                  {/* Sync Controls */}
                  <div className="flex items-center justify-between pt-3 border-t border-slate-800">
                    <div className="flex items-center space-x-3 text-xs text-slate-400">
                      <span>Sync Interval:</span>
                      <input
                        type="number"
                        value={config.radarr.sync_interval_mins}
                        onChange={(e) => setConfig({ ...config, radarr: { ...config.radarr, sync_interval_mins: Number(e.target.value) } })}
                        className="w-20 rounded border border-slate-700 bg-slate-800 px-2 py-1 text-slate-200"
                      />
                      <span>minutes</span>
                    </div>
                    <button
                      type="button"
                      onClick={() => handleTriggerSync('radarr')}
                      disabled={syncingArr === 'radarr' || !config.radarr.enabled}
                      className="flex items-center space-x-1.5 rounded-lg bg-amber-600 px-4 py-1.5 text-xs font-semibold text-white hover:bg-amber-500 disabled:opacity-50"
                    >
                      <RotateCcw className={`h-3.5 w-3.5 ${syncingArr === 'radarr' ? 'animate-spin' : ''}`} />
                      <span>Trigger Radarr Sync Now</span>
                    </button>
                  </div>
                </div>
              )}
            </div>
          </div>

          {/* Lidarr (Music Automation) Card */}
          <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-6">
            <div className="flex items-center justify-between border-b border-slate-800 pb-3">
              <div className="flex items-center space-x-2.5">
                <div className="rounded-lg bg-emerald-500/10 p-2 text-emerald-400 border border-emerald-500/20">
                  <Music className="h-5 w-5" />
                </div>
                <div>
                  <h2 className="text-base font-bold text-slate-100">Lidarr (Music Automation)</h2>
                  <p className="text-xs text-slate-400">Track albums, artist discographies, FLAC/MP3 releases, and propagate across nodes.</p>
                </div>
              </div>
              <label className="relative inline-flex cursor-pointer items-center">
                <input
                  type="checkbox"
                  checked={config.lidarr.enabled}
                  onChange={(e) => setConfig({ ...config, lidarr: { ...config.lidarr, enabled: e.target.checked } })}
                  className="peer sr-only"
                />
                <div className="peer h-6 w-11 rounded-full bg-slate-800 after:absolute after:top-[2px] after:left-[2px] after:h-5 after:w-5 after:rounded-full after:border after:border-slate-300 after:bg-white after:transition-all after:content-[''] peer-checked:bg-emerald-600 peer-checked:after:translate-x-full peer-checked:after:border-white"></div>
              </label>
            </div>

            {/* Inbound Webhook Card for Lidarr */}
            <div className="rounded-xl border border-emerald-500/20 bg-emerald-500/5 p-4 space-y-2">
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-2">
                  <Terminal className="h-4 w-4 text-emerald-400" />
                  <span className="text-xs font-bold uppercase tracking-wider text-emerald-300">Lidarr Inbound Webhook URL</span>
                </div>
                <button
                  type="button"
                  onClick={() => {
                    navigator.clipboard.writeText(`${window.location.origin}/api/lidarr/inbound`);
                    setCopiedLidarrWebhook(true);
                    setTimeout(() => setCopiedLidarrWebhook(false), 2000);
                  }}
                  className="rounded-lg bg-slate-800 border border-slate-700 px-3 py-1 text-xs font-semibold text-slate-200 hover:bg-slate-700 flex items-center space-x-1"
                >
                  {copiedLidarrWebhook ? <Check className="h-3.5 w-3.5 text-emerald-400" /> : <Copy className="h-3.5 w-3.5" />}
                  <span>{copiedLidarrWebhook ? 'Copied' : 'Copy Webhook'}</span>
                </button>
              </div>
              <input
                type="text"
                readOnly
                value={`${window.location.origin}/api/lidarr/inbound`}
                className="w-full rounded border border-emerald-500/30 bg-slate-900 px-3 py-1.5 font-mono text-xs text-emerald-300 focus:outline-none"
              />
              <p className="text-[11px] text-slate-400">
                In Lidarr, navigate to <strong>Settings &rarr; Connect &rarr; Add Notification &rarr; Webhook</strong>. Paste this URL and check <em>On Grab</em>, <em>On Download</em>, <em>On Track File Delete</em>, and <em>On Rename</em>.
              </p>
            </div>

            {/* Master Node Configuration */}
            <div className="space-y-4">
              <div className="flex items-center justify-between border-b border-slate-800/60 pb-2">
                <div>
                  <h3 className="text-sm font-semibold text-slate-200">Primary / Master Lidarr Instance</h3>
                  <p className="text-[11px] text-slate-400 mt-0.5">Used by Conduit to track music library telemetry and propagate discographies.</p>
                </div>
                <div className="flex items-center space-x-3">
                  {lidarrMasterStatus && (
                    <span className="text-xs font-mono text-slate-300">{lidarrMasterStatus}</span>
                  )}
                  <button
                    type="button"
                    onClick={handleTestLidarrPrimary}
                    disabled={testingLidarrMaster || !(config.lidarr.primary || config.lidarr.master)}
                    className="rounded-lg bg-slate-800 px-3 py-1.5 text-xs font-semibold text-emerald-400 hover:bg-slate-700 disabled:opacity-50"
                  >
                    {testingLidarrMaster ? 'Testing...' : 'Test Connection'}
                  </button>
                </div>
              </div>

              <div className="grid grid-cols-1 md:grid-cols-3 gap-3 pt-1">
                <div>
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Instance Name</label>
                  <input
                    type="text"
                    value={config.lidarr.primary?.name || config.lidarr.master?.name || ''}
                    placeholder="Lidarr-Primary"
                    onChange={(e) => {
                      const current = config.lidarr.primary || config.lidarr.master || { name: '', base_url: '', api_key: '', version: 1 };
                      setConfig({ ...config, lidarr: { ...config.lidarr, primary: { ...current, name: e.target.value } } });
                    }}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                </div>
                <div>
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Base URL</label>
                  <input
                    type="text"
                    value={config.lidarr.primary?.base_url || config.lidarr.master?.base_url || ''}
                    placeholder="http://192.168.1.50:8686"
                    onChange={(e) => {
                      const current = config.lidarr.primary || config.lidarr.master || { name: '', base_url: '', api_key: '', version: 1 };
                      setConfig({ ...config, lidarr: { ...config.lidarr, primary: { ...current, base_url: e.target.value } } });
                    }}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                </div>
                <div>
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">API Key</label>
                  <input
                    type="password"
                    value={config.lidarr.primary?.api_key || config.lidarr.master?.api_key || ''}
                    placeholder="Lidarr API Key"
                    onChange={(e) => {
                      const current = config.lidarr.primary || config.lidarr.master || { name: '', base_url: '', api_key: '', version: 1 };
                      setConfig({ ...config, lidarr: { ...config.lidarr, primary: { ...current, api_key: e.target.value } } });
                    }}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                </div>
              </div>
            </div>

            {/* Collapsible Multi-Instance Replicas */}
            <div className="rounded-xl border border-slate-800/80 bg-slate-900/40 overflow-hidden">
              <button
                type="button"
                onClick={() => setShowLidarrReplicas(!showLidarrReplicas)}
                className="w-full flex items-center justify-between p-4 text-left hover:bg-slate-800/30 transition-colors"
              >
                <div className="flex items-center space-x-2.5">
                  {showLidarrReplicas ? (
                    <ChevronDown className="h-4 w-4 text-slate-400" />
                  ) : (
                    <ChevronRight className="h-4 w-4 text-slate-400" />
                  )}
                  <div>
                    <h3 className="text-sm font-semibold text-slate-200">
                      Multi-Node Replicas ({config.lidarr.replicas?.length || config.lidarr.slaves?.length || 0})
                    </h3>
                    <p className="text-[11px] text-slate-400">Synchronize artists, albums, and quality profiles from Master to secondary Lidarr instances.</p>
                  </div>
                </div>
                <span className="text-xs text-brand-400 font-semibold">{showLidarrReplicas ? 'Hide Replicas' : 'Manage Replicas'}</span>
              </button>

              {showLidarrReplicas && (
                <div className="p-4 pt-0 space-y-4 border-t border-slate-800/60">
                  <div className="flex items-center justify-between pt-2">
                    <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">Replica Nodes</span>
                    <button
                      type="button"
                      onClick={() => {
                        const replicas = [...(config.lidarr.replicas || config.lidarr.slaves || [])];
                        replicas.push({ name: `Lidarr-Slave-${replicas.length + 1}`, base_url: '', api_key: '', version: 1 });
                        setConfig({ ...config, lidarr: { ...config.lidarr, replicas } });
                      }}
                      className="flex items-center space-x-1 text-xs text-emerald-400 hover:text-emerald-300 font-semibold"
                    >
                      <Plus className="h-3.5 w-3.5" />
                      <span>Add Replica</span>
                    </button>
                  </div>

                  {(config.lidarr.replicas || config.lidarr.slaves || []).map((replica, idx) => (
                    <div key={idx} className="rounded-lg border border-slate-800 bg-slate-900/60 p-3 space-y-2">
                      <div className="flex items-center justify-between">
                        <span className="text-xs font-semibold text-slate-300">Replica #{idx + 1}</span>
                        <button
                          type="button"
                          onClick={() => {
                            const replicas = (config.lidarr.replicas || config.lidarr.slaves || []).filter((_, i) => i !== idx);
                            setConfig({ ...config, lidarr: { ...config.lidarr, replicas } });
                          }}
                          className="text-rose-400 hover:text-rose-300 text-xs"
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </button>
                      </div>
                      <div className="grid grid-cols-1 md:grid-cols-3 gap-2">
                        <input
                          type="text"
                          value={replica.name}
                          placeholder="Replica Name"
                          onChange={(e) => {
                            const replicas = [...(config.lidarr.replicas || config.lidarr.slaves || [])];
                            replicas[idx] = { ...replicas[idx], name: e.target.value };
                            setConfig({ ...config, lidarr: { ...config.lidarr, replicas } });
                          }}
                          className="rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                        />
                        <input
                          type="text"
                          value={replica.base_url}
                          placeholder="http://remote-lidarr:8686"
                          onChange={(e) => {
                            const replicas = [...(config.lidarr.replicas || config.lidarr.slaves || [])];
                            replicas[idx] = { ...replicas[idx], base_url: e.target.value };
                            setConfig({ ...config, lidarr: { ...config.lidarr, replicas } });
                          }}
                          className="rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                        />
                        <input
                          type="password"
                          value={replica.api_key}
                          placeholder="Replica API Key"
                          onChange={(e) => {
                            const replicas = [...(config.lidarr.replicas || config.lidarr.slaves || [])];
                            replicas[idx] = { ...replicas[idx], api_key: e.target.value };
                            setConfig({ ...config, lidarr: { ...config.lidarr, replicas } });
                          }}
                          className="rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                        />
                      </div>
                    </div>
                  ))}

                  {/* Sync Controls */}
                  <div className="flex items-center justify-between pt-3 border-t border-slate-800">
                    <div className="flex items-center space-x-3 text-xs text-slate-400">
                      <span>Sync Interval:</span>
                      <input
                        type="number"
                        value={config.lidarr.sync_interval_mins}
                        onChange={(e) => setConfig({ ...config, lidarr: { ...config.lidarr, sync_interval_mins: Number(e.target.value) } })}
                        className="w-20 rounded border border-slate-700 bg-slate-800 px-2 py-1 text-slate-200"
                      />
                      <span>minutes</span>
                    </div>
                    <button
                      type="button"
                      onClick={() => handleTriggerSync('lidarr')}
                      disabled={syncingArr === 'lidarr' || !config.lidarr.enabled}
                      className="flex items-center space-x-1.5 rounded-lg bg-emerald-600 px-4 py-1.5 text-xs font-semibold text-white hover:bg-emerald-500 disabled:opacity-50"
                    >
                      <RotateCcw className={`h-3.5 w-3.5 ${syncingArr === 'lidarr' ? 'animate-spin' : ''}`} />
                      <span>Trigger Lidarr Sync Now</span>
                    </button>
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {/* Tab: Multi-Tenant Zones (independent Sonarr/Radarr/Lidarr + Plex/fetcher groupings) */}
      {tab === 'zones' && (
        <div className="space-y-6 max-w-5xl">
          <div className="rounded-xl border border-purple-500/20 bg-purple-500/10 p-4 text-xs text-purple-300 leading-relaxed">
            <strong>Zones</strong> let you run fully independent stacks (e.g. a "General Library" and a "4K Library"), each with its own Sonarr/Radarr/Lidarr instance and a subset of your Plex servers and fetcher nodes. Point each zone's Sonarr/Radarr/Lidarr webhook at the zone-specific URL below — Conduit tags every grab with its zone, and the sidebar zone switcher filters the Dashboard, Pipeline, and Fetchers views. Leave this empty if you only run one stack; nothing here is required.
          </div>

          <div className="flex items-center justify-between">
            <h2 className="text-lg font-bold text-slate-200">Configured Zones</h2>
            <button
              onClick={() => {
                const id = prompt('Enter a stable zone slug (used in webhook URLs, e.g. "4k"):');
                if (!id) return;
                const zones = config.zones || [];
                if (zones.some((z) => z.id === id)) {
                  alert(`A zone with id '${id}' already exists.`);
                  return;
                }
                const name = prompt('Enter a display name (e.g. "4K Library"):', id) || id;
                const newZone: ZoneConfig = {
                  id,
                  name,
                  plex_node_names: [],
                  fetcher_node_names: [],
                };
                setConfig({ ...config, zones: [...zones, newZone] });
              }}
              className="flex items-center space-x-1.5 rounded-lg bg-slate-800 px-3 py-1.5 text-xs font-semibold text-purple-400 hover:bg-slate-700"
            >
              <Plus className="h-4 w-4" />
              <span>Add Zone</span>
            </button>
          </div>

          {(config.zones || []).length === 0 && (
            <div className="rounded-xl border border-slate-800 bg-slate-900/40 p-8 text-center text-sm text-slate-500">
              No zones configured — Conduit behaves as a single combined stack.
            </div>
          )}

          {(config.zones || []).map((zone, zIdx) => {
            const updateZone = (patch: Partial<ZoneConfig>) => {
              const zones = [...(config.zones || [])];
              zones[zIdx] = { ...zones[zIdx], ...patch };
              setConfig({ ...config, zones });
            };
            const updateZoneApp = (app: 'sonarr' | 'radarr' | 'lidarr', patch: Partial<ArrNodeConfig>) => {
              const defaultVersion = app === 'sonarr' ? 3 : app === 'radarr' ? 4 : 1;
              const current = zone[app] || { name: '', base_url: '', api_key: '', version: defaultVersion };
              updateZone({ [app]: { ...current, ...patch } });
            };
            const toggleZoneList = (field: 'plex_node_names' | 'fetcher_node_names', name: string) => {
              const list = zone[field] || [];
              const next = list.includes(name) ? list.filter((n) => n !== name) : [...list, name];
              updateZone({ [field]: next });
            };

            return (
              <div key={zone.id} className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-5">
                <div className="flex items-center justify-between border-b border-slate-800 pb-3">
                  <div className="flex items-center space-x-2.5">
                    <div className="rounded-lg bg-purple-500/10 p-2 text-purple-400 border border-purple-500/20">
                      <Boxes className="h-5 w-5" />
                    </div>
                    <div>
                      <input
                        type="text"
                        value={zone.name}
                        onChange={(e) => updateZone({ name: e.target.value })}
                        className="bg-transparent text-base font-bold text-slate-100 focus:outline-none focus:border-b focus:border-purple-500"
                      />
                      <p className="text-xs text-slate-400 font-mono">id: {zone.id}</p>
                    </div>
                  </div>
                  <button
                    onClick={() => {
                      if (!window.confirm(`Delete zone '${zone.name}'? Existing tagged grabs keep their zone_id but the zone filter will no longer match it.`)) return;
                      setConfig({ ...config, zones: (config.zones || []).filter((_, i) => i !== zIdx) });
                    }}
                    className="text-rose-400 hover:text-rose-300"
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>

                {/* Per-zone Sonarr/Radarr/Lidarr */}
                <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                  {(['sonarr', 'radarr', 'lidarr'] as const).map((app) => (
                    <div key={app} className="space-y-2 rounded-xl border border-slate-800 bg-slate-800/40 p-3.5">
                      <span className="text-xs font-bold uppercase tracking-wider text-slate-400 capitalize">{app}</span>
                      <input
                        type="text"
                        value={zone[app]?.name || ''}
                        placeholder={`${app}-${zone.id}`}
                        onChange={(e) => updateZoneApp(app, { name: e.target.value })}
                        className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-xs text-slate-200 focus:border-brand-500 focus:outline-none"
                      />
                      <input
                        type="text"
                        value={zone[app]?.base_url || ''}
                        placeholder="Base URL"
                        onChange={(e) => updateZoneApp(app, { base_url: e.target.value })}
                        className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-xs text-slate-200 focus:border-brand-500 focus:outline-none"
                      />
                      <input
                        type="password"
                        value={zone[app]?.api_key || ''}
                        placeholder="API Key"
                        onChange={(e) => updateZoneApp(app, { api_key: e.target.value })}
                        className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-xs text-slate-200 focus:border-brand-500 focus:outline-none"
                      />
                    </div>
                  ))}
                </div>

                {/* Webhook secret + zone-scoped webhook URLs */}
                <div className="space-y-2">
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider">Zone Webhook Secret (optional)</label>
                  <input
                    type="password"
                    value={zone.webhook_secret || ''}
                    placeholder="Shared secret for this zone's Sonarr/Radarr/Lidarr inbound webhooks"
                    onChange={(e) => updateZone({ webhook_secret: e.target.value })}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                </div>

                <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                  {(['sonarr', 'radarr', 'lidarr'] as const).map((app) => {
                    const url = `${window.location.origin}/api/${app}/inbound?zone=${encodeURIComponent(zone.id)}`;
                    const key = `${zone.id}:${app}`;
                    return (
                      <div key={app} className="rounded-xl border border-purple-500/20 bg-purple-500/5 p-3 space-y-1.5">
                        <div className="flex items-center justify-between">
                          <span className="text-[10px] font-bold uppercase tracking-wider text-purple-300 capitalize">{app} Webhook</span>
                          <button
                            type="button"
                            onClick={() => {
                              navigator.clipboard.writeText(url);
                              setCopiedZoneWebhook(key);
                              setTimeout(() => setCopiedZoneWebhook((cur) => (cur === key ? null : cur)), 2000);
                            }}
                            className="rounded-lg bg-slate-800 border border-slate-700 px-2 py-0.5 text-[10px] font-semibold text-slate-200 hover:bg-slate-700 flex items-center space-x-1"
                          >
                            {copiedZoneWebhook === key ? <Check className="h-3 w-3 text-emerald-400" /> : <Copy className="h-3 w-3" />}
                            <span>{copiedZoneWebhook === key ? 'Copied' : 'Copy'}</span>
                          </button>
                        </div>
                        <input
                          type="text"
                          readOnly
                          value={url}
                          className="w-full rounded border border-purple-500/30 bg-slate-900 px-2 py-1 font-mono text-[10px] text-purple-300 focus:outline-none"
                        />
                      </div>
                    );
                  })}
                </div>

                {/* Plex node multi-select */}
                <div className="space-y-2">
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider">Plex Servers in this Zone</label>
                  {(config.plex.nodes || []).length === 0 ? (
                    <p className="text-xs text-slate-500">No Plex servers configured yet — add one in the Plex & Trakt tab.</p>
                  ) : (
                    <div className="flex flex-wrap gap-2">
                      {config.plex.nodes.map((node) => (
                        <label
                          key={node.name}
                          className={`flex items-center space-x-1.5 rounded-lg border px-2.5 py-1 text-xs cursor-pointer transition-colors ${
                            zone.plex_node_names.includes(node.name)
                              ? 'border-purple-500/40 bg-purple-500/10 text-purple-300'
                              : 'border-slate-700 bg-slate-800 text-slate-400 hover:text-slate-200'
                          }`}
                        >
                          <input
                            type="checkbox"
                            checked={zone.plex_node_names.includes(node.name)}
                            onChange={() => toggleZoneList('plex_node_names', node.name)}
                            className="rounded border-slate-700 bg-slate-800 text-purple-600 focus:ring-purple-500"
                          />
                          <span>{node.name}</span>
                        </label>
                      ))}
                    </div>
                  )}
                </div>

                {/* Fetcher node multi-select */}
                <div>
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider">Fetcher Nodes in this Zone</label>
                  {Object.keys(config.nodes).length === 0 ? (
                    <p className="text-xs text-slate-500">No fetcher nodes configured yet.</p>
                  ) : (
                    <div className="mt-2 grid grid-cols-2 gap-2 sm:grid-cols-3 md:grid-cols-4">
                      {Object.keys(config.nodes).map((nodeName) => {
                        const isChecked =
                          zone.fetcher_node_names.includes(nodeName) ||
                          (zone.transmission_node_names?.includes(nodeName) ?? false);
                        return (
                          <label
                            key={nodeName}
                            className={`flex items-center gap-2 rounded-lg border px-3 py-2 text-xs transition-colors cursor-pointer ${
                              isChecked
                                ? 'border-brand-500/60 bg-brand-500/10 text-brand-300 font-medium'
                                : 'border-slate-800 bg-slate-900/60 text-slate-400 hover:border-slate-700'
                            }`}
                          >
                            <input
                              type="checkbox"
                              className="rounded border-slate-700 bg-slate-800 text-brand-500 focus:ring-0"
                              checked={!!isChecked}
                              onChange={() => toggleZoneList('fetcher_node_names', nodeName)}
                            />
                            <span className="truncate font-mono">{nodeName}</span>
                          </label>
                        );
                      })}
                    </div>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* Tab 3: File Sync & Media Routing Hub */}
      {tab === 'sync' && (
        <div className="space-y-8 max-w-5xl">
          {/* Main Staging & Media Routing Card */}
          <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-6">
            <div className="flex items-center justify-between border-b border-slate-800 pb-3">
              <div className="flex items-center space-x-2.5">
                <div className="rounded-lg bg-emerald-500/10 p-2 text-emerald-400 border border-emerald-500/20">
                  <FolderSync className="h-5 w-5" />
                </div>
                <div>
                  <h2 className="text-base font-bold text-slate-100">Media Type Classification & Routing Engine</h2>
                  <p className="text-xs text-slate-400">
                    Hierarchical 4-tier routing: 1) Arr Grabs &rarr; 2) Tracker Registry &rarr; 3) Filename Regex &rarr; 4) Default Fallback with Node Folder Overrides.
                  </p>
                </div>
              </div>
              <label className="flex items-center space-x-2 text-sm text-slate-300">
                <input
                  type="checkbox"
                  checked={config.queue_routing?.enabled ?? true}
                  onChange={(e) => setConfig({
                    ...config,
                    queue_routing: { ...config.queue_routing, enabled: e.target.checked }
                  })}
                  className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                />
                <span>Enable Media Routing</span>
              </label>
            </div>

            {/* Global Routing Parameters */}
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <div>
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                  Post-Staging Command
                </label>
                <input
                  type="text"
                  value={config.queue_routing?.post_cmd || 'cp -av'}
                  onChange={(e) => setConfig({
                    ...config,
                    queue_routing: { ...config.queue_routing, post_cmd: e.target.value }
                  })}
                  className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none font-mono"
                />
                <p className="text-[11px] text-slate-500 mt-1">
                  Default: <code className="text-slate-400">cp -av</code> (safe copy). Use <code className="text-slate-400">cp -alv</code> for zero-copy hardlinking if all nodes share one filesystem — avoid on SMB/CIFS mounts (lease collisions). Read live on every hook execution, so changes here apply immediately without re-installing the hook script on any node.
                </p>
              </div>

              <div>
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                  Default Fallback Media Type
                </label>
                <input
                  type="text"
                  value={config.queue_routing?.default_media_type || 'misc'}
                  onChange={(e) => setConfig({
                    ...config,
                    queue_routing: { ...config.queue_routing, default_media_type: e.target.value }
                  })}
                  className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                />
                <p className="text-[11px] text-slate-500 mt-1">Used if no Arr grab, tracker rule, or regex matches.</p>
              </div>

              <div>
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                  UHD / 4K Resolution Markers
                </label>
                <input
                  type="text"
                  value={(config.queue_routing?.uhd_markers || []).join(', ')}
                  onChange={(e) => {
                    const markers = e.target.value.split(',').map((s) => s.trim()).filter(Boolean);
                    setConfig({
                      ...config,
                      queue_routing: { ...config.queue_routing, uhd_markers: markers }
                    });
                  }}
                  className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                />
                <p className="text-[11px] text-slate-500 mt-1">Promotes matching files (e.g. tv &rarr; tvUHD, movie &rarr; movieUHD).</p>
              </div>
            </div>

            {/* 1. Registered Media Types Table */}
            <div className="space-y-3 pt-2">
              <div className="flex items-center justify-between">
                <div>
                  <span className="text-xs font-bold uppercase tracking-wider text-slate-300">
                    1. Registered Media Types & Intake Directories
                  </span>
                  <p className="text-[11px] text-slate-400">Media categories and default hardlink staging queues across your cluster.</p>
                </div>
                <button
                  type="button"
                  onClick={() => {
                    const typeId = prompt('Enter new Media Type ID (e.g. anime or books):');
                    if (!typeId) return;
                    const cleanId = typeId.trim().toLowerCase();
                    const currentTypes = config.queue_routing?.media_types || [];
                    if (currentTypes.some((t) => t.id === cleanId)) {
                      alert('Media type ID already exists.');
                      return;
                    }
                    const name = prompt('Enter display label (e.g. Japanese Anime):', cleanId) || cleanId;
                    setConfig({
                      ...config,
                      queue_routing: {
                        ...config.queue_routing,
                        media_types: [
                          ...currentTypes,
                          {
                            id: cleanId,
                            name,
                            default_queue_dir: `/media/queue/${cleanId}Queue/`,
                            uhd_queue_dir: `/media/queue/${cleanId}UHDqueue/`,
                            description: 'Custom media queue',
                          },
                        ],
                      },
                    });
                  }}
                  className="flex items-center space-x-1 rounded-lg bg-slate-800 px-3 py-1 text-xs font-semibold text-emerald-400 hover:bg-slate-700"
                >
                  <Plus className="h-3.5 w-3.5" />
                  <span>Add Media Type</span>
                </button>
              </div>

              <div className="overflow-hidden rounded-xl border border-slate-800 bg-slate-800/30">
                <table className="w-full text-left text-xs">
                  <thead className="border-b border-slate-800 bg-slate-950/60 text-slate-400">
                    <tr>
                      <th className="p-3 font-semibold w-24">Type ID</th>
                      <th className="p-3 font-semibold w-40">Label</th>
                      <th className="p-3 font-semibold">Standard Queue Folder</th>
                      <th className="p-3 font-semibold">UHD / 4K Queue Folder</th>
                      <th className="p-3 font-semibold w-16 text-right">Action</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-800/60">
                    {(config.queue_routing?.media_types || []).map((mtype, idx) => (
                      <tr key={mtype.id} className="hover:bg-slate-800/20">
                        <td className="p-3 font-mono font-bold text-slate-200">{mtype.id}</td>
                        <td className="p-3">
                          <input
                            type="text"
                            value={mtype.name}
                            onChange={(e) => {
                              const types = [...(config.queue_routing.media_types || [])];
                              types[idx] = { ...types[idx], name: e.target.value };
                              setConfig({
                                ...config,
                                queue_routing: { ...config.queue_routing, media_types: types },
                              });
                            }}
                            className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1 px-2 text-xs text-slate-200 focus:border-brand-500 focus:outline-none"
                          />
                        </td>
                        <td className="p-3">
                          <input
                            type="text"
                            value={mtype.default_queue_dir}
                            onChange={(e) => {
                              const types = [...(config.queue_routing.media_types || [])];
                              types[idx] = { ...types[idx], default_queue_dir: e.target.value };
                              setConfig({
                                ...config,
                                queue_routing: { ...config.queue_routing, media_types: types },
                              });
                            }}
                            className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1 px-2 text-xs text-slate-200 font-mono focus:border-brand-500 focus:outline-none"
                          />
                        </td>
                        <td className="p-3">
                          <input
                            type="text"
                            value={mtype.uhd_queue_dir || ''}
                            placeholder="Optional 4K queue"
                            onChange={(e) => {
                              const types = [...(config.queue_routing.media_types || [])];
                              types[idx] = { ...types[idx], uhd_queue_dir: e.target.value || undefined };
                              setConfig({
                                ...config,
                                queue_routing: { ...config.queue_routing, media_types: types },
                              });
                            }}
                            className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1 px-2 text-xs text-slate-200 font-mono focus:border-brand-500 focus:outline-none"
                          />
                        </td>
                        <td className="p-3 text-right">
                          <button
                            type="button"
                            onClick={() => {
                              const types = (config.queue_routing.media_types || []).filter((_, i) => i !== idx);
                              setConfig({
                                ...config,
                                queue_routing: { ...config.queue_routing, media_types: types },
                              });
                            }}
                            className="text-rose-400 hover:text-rose-300"
                            title="Delete Media Type"
                          >
                            <Trash2 className="h-4 w-4" />
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>

            {/* 2. Centralized Tracker Mapping Rules Table */}
            <div className="space-y-3 pt-4 border-t border-slate-800/80">
              <div className="flex items-center justify-between">
                <div>
                  <span className="text-xs font-bold uppercase tracking-wider text-slate-300">
                    2. Centralized Tracker-to-Media-Type Registry
                  </span>
                  <p className="text-[11px] text-slate-400">
                    Maps tracker announce host patterns to media types when torrents are downloaded outside Sonarr/Radarr.
                  </p>
                </div>
                <button
                  type="button"
                  onClick={() => {
                    const pattern = prompt('Enter Tracker Domain / Wildcard Pattern (e.g. *broadcasthe.net* or *ptp*):');
                    if (!pattern) return;
                    const mediaType = (config.queue_routing?.media_types?.[0]?.id || 'tv');
                    const currentRules = config.queue_routing?.tracker_mappings || [];
                    setConfig({
                      ...config,
                      queue_routing: {
                        ...config.queue_routing,
                        tracker_mappings: [
                          ...currentRules,
                          {
                            id: `rule_${Date.now()}`,
                            pattern: pattern.trim(),
                            media_type: mediaType,
                            priority: 10,
                            comment: 'Custom tracker rule',
                          },
                        ],
                      },
                    });
                  }}
                  className="flex items-center space-x-1 rounded-lg bg-slate-800 px-3 py-1 text-xs font-semibold text-sky-400 hover:bg-slate-700"
                >
                  <Plus className="h-3.5 w-3.5" />
                  <span>Add Tracker Rule</span>
                </button>
              </div>

              <div className="overflow-hidden rounded-xl border border-slate-800 bg-slate-800/30">
                <table className="w-full text-left text-xs">
                  <thead className="border-b border-slate-800 bg-slate-950/60 text-slate-400">
                    <tr>
                      <th className="p-3 font-semibold">Tracker Host Pattern</th>
                      <th className="p-3 font-semibold w-44">Target Media Type</th>
                      <th className="p-3 font-semibold w-24 text-center">Priority</th>
                      <th className="p-3 font-semibold">Comment / Note</th>
                      <th className="p-3 font-semibold w-16 text-right">Action</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-800/60">
                    {(config.queue_routing?.tracker_mappings || []).map((rule, idx) => (
                      <tr key={rule.id || idx} className="hover:bg-slate-800/20">
                        <td className="p-3">
                          <input
                            type="text"
                            value={rule.pattern}
                            onChange={(e) => {
                              const rules = [...(config.queue_routing.tracker_mappings || [])];
                              rules[idx] = { ...rules[idx], pattern: e.target.value };
                              setConfig({
                                ...config,
                                queue_routing: { ...config.queue_routing, tracker_mappings: rules },
                              });
                            }}
                            className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1 px-2 text-xs text-slate-200 font-mono focus:border-brand-500 focus:outline-none"
                          />
                        </td>
                        <td className="p-3">
                          <select
                            value={rule.media_type}
                            onChange={(e) => {
                              const rules = [...(config.queue_routing.tracker_mappings || [])];
                              rules[idx] = { ...rules[idx], media_type: e.target.value };
                              setConfig({
                                ...config,
                                queue_routing: { ...config.queue_routing, tracker_mappings: rules },
                              });
                            }}
                            className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1 px-2 text-xs text-slate-200 focus:border-brand-500 focus:outline-none"
                          >
                            {(config.queue_routing?.media_types || []).map((mt) => (
                              <option key={mt.id} value={mt.id}>{mt.name} ({mt.id})</option>
                            ))}
                          </select>
                        </td>
                        <td className="p-3 text-center">
                          <input
                            type="number"
                            value={rule.priority}
                            onChange={(e) => {
                              const rules = [...(config.queue_routing.tracker_mappings || [])];
                              rules[idx] = { ...rules[idx], priority: parseInt(e.target.value, 10) || 0 };
                              setConfig({
                                ...config,
                                queue_routing: { ...config.queue_routing, tracker_mappings: rules },
                              });
                            }}
                            className="w-16 text-center rounded-lg border border-slate-700 bg-slate-800 py-1 px-1 text-xs text-slate-200 font-mono focus:border-brand-500 focus:outline-none"
                          />
                        </td>
                        <td className="p-3">
                          <input
                            type="text"
                            value={rule.comment || ''}
                            placeholder="Optional comment"
                            onChange={(e) => {
                              const rules = [...(config.queue_routing.tracker_mappings || [])];
                              rules[idx] = { ...rules[idx], comment: e.target.value || undefined };
                              setConfig({
                                ...config,
                                queue_routing: { ...config.queue_routing, tracker_mappings: rules },
                              });
                            }}
                            className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1 px-2 text-xs text-slate-400 focus:border-brand-500 focus:outline-none"
                          />
                        </td>
                        <td className="p-3 text-right">
                          <button
                            type="button"
                            onClick={() => {
                              const rules = (config.queue_routing.tracker_mappings || []).filter((_, i) => i !== idx);
                              setConfig({
                                ...config,
                                queue_routing: { ...config.queue_routing, tracker_mappings: rules },
                              });
                            }}
                            className="text-rose-400 hover:text-rose-300"
                            title="Delete Rule"
                          >
                            <Trash2 className="h-4 w-4" />
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>

            {/* 3. Per-Node Intake Directory Overrides */}
            <div className="space-y-3 pt-4 border-t border-slate-800/80">
              <div className="flex items-center justify-between">
                <div>
                  <span className="text-xs font-bold uppercase tracking-wider text-slate-300">
                    3. Per-Node Media Folder Overrides
                  </span>
                  <p className="text-[11px] text-slate-400">
                    Customize directory mount points on specific fetcher nodes if they differ from cluster defaults.
                  </p>
                </div>
                <div className="flex items-center space-x-2">
                  <span className="text-xs text-slate-400">Select Node:</span>
                  <select
                    value={nodeForMediaOverride || Object.keys(config.nodes || {})[0] || ''}
                    onChange={(e) => setNodeForMediaOverride(e.target.value)}
                    className="rounded-lg border border-slate-700 bg-slate-800 py-1 px-3 text-xs font-semibold text-slate-200 focus:border-brand-500 focus:outline-none"
                  >
                    {Object.entries(config.nodes || {}).map(([key, n]) => (
                      <option key={key} value={key}>{n.name || key}</option>
                    ))}
                  </select>
                </div>
              </div>

              {(() => {
                const targetNodeKey = nodeForMediaOverride || Object.keys(config.nodes || {})[0];
                const activeNodeConfig = targetNodeKey ? config.nodes?.[targetNodeKey] : null;
                if (!activeNodeConfig) {
                  return <p className="text-xs text-slate-500 italic p-3">No nodes configured.</p>;
                }
                const overrides = activeNodeConfig.media_dir_overrides || {};

                return (
                  <div className="overflow-hidden rounded-xl border border-slate-800 bg-slate-800/30 p-4 space-y-3">
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                      {(config.queue_routing?.media_types || []).map((mt) => {
                        const currentOverride = overrides[mt.id] || '';
                        return (
                          <div key={mt.id} className="space-y-1">
                            <label className="flex items-center justify-between text-xs text-slate-300 font-semibold">
                              <span>{mt.name} (<code className="text-brand-400">{mt.id}</code>)</span>
                              <span className="text-[10px] text-slate-500 font-mono truncate max-w-[200px]">Default: {mt.default_queue_dir}</span>
                            </label>
                            <input
                              type="text"
                              value={currentOverride}
                              placeholder={mt.default_queue_dir}
                              onChange={(e) => {
                                const newOverrides = { ...overrides, [mt.id]: e.target.value };
                                if (!e.target.value) delete newOverrides[mt.id];
                                const updatedNodes = { ...config.nodes };
                                updatedNodes[targetNodeKey] = {
                                  ...activeNodeConfig,
                                  media_dir_overrides: newOverrides,
                                };
                                setConfig({ ...config, nodes: updatedNodes });
                              }}
                              className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1 px-2.5 text-xs text-slate-200 font-mono focus:border-brand-500 focus:outline-none"
                            />
                          </div>
                        );
                      })}
                    </div>
                  </div>
                );
              })()}
            </div>
          </div>

          {/* Interactive 4-Tier Routing Classifier Simulator */}
          <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-4">
            <div className="flex items-center justify-between border-b border-slate-800 pb-3">
              <div className="flex items-center space-x-2">
                <Sparkles className="h-4 w-4 text-brand-400" />
                <h3 className="text-sm font-bold text-slate-100 uppercase tracking-wider">
                  Live 4-Tier Pipeline Classifier Simulator
                </h3>
              </div>
              <span className="text-xs text-slate-400">Inspect the exact decision trace and folder resolution</span>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-3">
              <div className="lg:col-span-2">
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                  Torrent / Release Name
                </label>
                <input
                  type="text"
                  value={testClassifyName}
                  onChange={(e) => setTestClassifyName(e.target.value)}
                  placeholder="e.g. Severance.S02E01.2160p.WEB-DL.mkv"
                  className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-xs text-slate-200 focus:border-brand-500 focus:outline-none"
                />
              </div>
              <div>
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                  Tracker Host (Optional)
                </label>
                <input
                  type="text"
                  value={testClassifyTracker}
                  onChange={(e) => setTestClassifyTracker(e.target.value)}
                  placeholder="e.g. landof.tv or passthepopcorn.me"
                  className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-xs text-slate-200 focus:border-brand-500 focus:outline-none"
                />
              </div>
              <div>
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                  Node (Optional)
                </label>
                <input
                  type="text"
                  value={testClassifyNode}
                  onChange={(e) => setTestClassifyNode(e.target.value)}
                  placeholder="e.g. idyllwild - 4k"
                  className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-xs text-slate-200 focus:border-brand-500 focus:outline-none"
                />
              </div>
            </div>

            <div className="flex items-center justify-between pt-1">
              <button
                type="button"
                onClick={handleRunClassifySimulator}
                disabled={testingClassify}
                className="flex items-center space-x-1.5 rounded-lg bg-brand-600 px-4 py-1.5 text-xs font-semibold text-white hover:bg-brand-500 disabled:opacity-50"
              >
                <Sparkles className={`h-3.5 w-3.5 ${testingClassify ? 'animate-spin' : ''}`} />
                <span>Simulate 4-Tier Pipeline</span>
              </button>

              {classifyResult && (
                <div className="flex items-center space-x-2 text-xs">
                  <span className="text-slate-400">Match Source:</span>
                  <span className="rounded bg-sky-500/10 px-2 py-0.5 text-sky-300 font-mono border border-sky-500/20 font-semibold uppercase">
                    {classifyResult.match_source}
                  </span>
                </div>
              )}
            </div>

            {classifyResult && (
              <div className="rounded-xl border border-emerald-500/30 bg-emerald-500/10 p-4 space-y-3 text-xs">
                <div className="flex items-center justify-between">
                  <div className="flex items-center space-x-2">
                    <CheckCircle2 className="h-4 w-4 text-emerald-400" />
                    <span className="font-bold text-emerald-300">Resolved Media Type & Queue:</span>
                    <span className="rounded bg-emerald-500/20 px-2 py-0.5 font-bold font-mono text-emerald-200">
                      {classifyResult.queue} ({classifyResult.media_type})
                    </span>
                  </div>
                  <span className="text-slate-300">Notify: <strong>{classifyResult.notify ? 'Yes' : 'No'}</strong></span>
                </div>
                <div className="grid grid-cols-1 md:grid-cols-2 gap-2 pt-1 font-mono text-[11px] text-slate-300">
                  <div><strong>Target Intake Folder:</strong> {classifyResult.target_dir}</div>
                  <div><strong>Command:</strong> {classifyResult.post_cmd}</div>
                </div>

                {/* Decision Trace Steps */}
                {classifyResult.decision_trace && classifyResult.decision_trace.length > 0 && (
                  <div className="pt-2 border-t border-emerald-500/20 space-y-1">
                    <span className="text-[11px] font-bold text-emerald-400 uppercase tracking-wider">Pipeline Decision Trace:</span>
                    <div className="space-y-1 bg-slate-950/80 rounded-lg p-2.5 font-mono text-[11px] text-slate-300 border border-slate-800">
                      {classifyResult.decision_trace.map((step, sIdx) => (
                        <div key={sIdx} className="flex items-start space-x-1.5">
                          <span className="text-emerald-400 font-bold">&rarr;</span>
                          <span>{step}</span>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            )}
          </div>

          {/* Remote Sync & Resilio Destination Ingestion Watcher (Disabled by default) */}
          <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-5">
            <div className="flex items-center justify-between border-b border-slate-800 pb-3">
              <div className="flex items-center space-x-2.5">
                <div className="rounded-lg bg-indigo-500/10 p-2 text-indigo-400 border border-indigo-500/20">
                  <FolderSync className="h-5 w-5" />
                </div>
                <div>
                  <h3 className="text-base font-bold text-slate-100">Remote Resilio Sync & Destination Folder Ingestion Watcher</h3>
                  <p className="text-xs text-slate-400">
                    Monitors local destination directories synced from remote seedboxes/fetchers (e.g. via Resilio Sync or Syncthing) and moves completed files into Arr intake queues.
                  </p>
                </div>
              </div>
              <label className="flex items-center space-x-2 text-sm text-slate-300">
                <input
                  type="checkbox"
                  checked={config.file_sync?.enabled ?? false}
                  onChange={(e) => setConfig({
                    ...config,
                    file_sync: { ...config.file_sync, enabled: e.target.checked }
                  })}
                  className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                />
                <span className="font-semibold">Enable Ingestion Watcher</span>
              </label>
            </div>

            {/* Explanatory Notice */}
            <div className="rounded-xl border border-indigo-500/20 bg-indigo-500/5 p-3.5 text-xs text-indigo-300 space-y-1">
              <div className="font-bold flex items-center space-x-1.5">
                <HelpCircle className="h-4 w-4" />
                <span>How Remote Ingestion Sync Works:</span>
              </div>
              <p className="text-slate-300 leading-relaxed">
                When a remote fetcher node finishes downloading, it copies to its local Resilio Sync folder. Resilio transfers the files across machines to a destination folder on this server. Conduit monitors that destination directory, waits for file write completion (settle time), and automatically hardlinks/moves it into the final <code className="text-indigo-300">post_dir</code> for Sonarr/Radarr library intake.
              </p>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <div>
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                  Ingestion Command
                </label>
                <input
                  type="text"
                  value={config.file_sync?.sync_cmd || 'cp -al'}
                  onChange={(e) => setConfig({
                    ...config,
                    file_sync: { ...config.file_sync, sync_cmd: e.target.value }
                  })}
                  className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none font-mono"
                />
                <p className="text-[11px] text-slate-500 mt-1">e.g. <code className="text-slate-400">cp -al</code> (hardlink) or <code className="text-slate-400">mv</code> (move)</p>
              </div>

              <div>
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                  Directory Scan Interval
                </label>
                <div className="flex items-center space-x-2">
                  <input
                    type="number"
                    value={config.file_sync?.interval_secs || 30}
                    onChange={(e) => setConfig({
                      ...config,
                      file_sync: { ...config.file_sync, interval_secs: Math.max(10, Number(e.target.value)) }
                    })}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                  <span className="text-xs text-slate-400">sec</span>
                </div>
              </div>

              <div>
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                  Clean Old Staged Files
                </label>
                <div className="flex items-center space-x-2">
                  <input
                    type="number"
                    value={config.file_sync?.clean_queue_days || 30}
                    onChange={(e) => setConfig({
                      ...config,
                      file_sync: { ...config.file_sync, clean_queue_days: Number(e.target.value) }
                    })}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                  <span className="text-xs text-slate-400">days</span>
                </div>
              </div>
            </div>

            {/* Folder Mappings Table */}
            <div className="space-y-3 pt-2">
              <div className="flex items-center justify-between">
                <span className="text-xs font-bold uppercase tracking-wider text-slate-400">
                  Watched Remote Sync Folders ({config.file_sync?.mappings?.length || 0})
                </span>
                <button
                  type="button"
                  onClick={() => setShowAddMappingModal(true)}
                  className="flex items-center space-x-1 rounded-lg bg-slate-800 px-3 py-1 text-xs font-semibold text-indigo-400 hover:bg-slate-700"
                >
                  <Plus className="h-3.5 w-3.5" />
                  <span>Add Ingestion Path</span>
                </button>
              </div>

              {(!config.file_sync?.mappings || config.file_sync.mappings.length === 0) ? (
                <div className="rounded-xl border border-slate-800 bg-slate-950/40 p-4 text-center text-xs text-slate-500">
                  No remote sync watch directories configured. Click "Add Ingestion Path" above if you use Resilio Sync or Syncthing.
                </div>
              ) : (
                <div className="rounded-xl border border-slate-800 overflow-hidden">
                  <table className="w-full text-left text-xs border-collapse">
                    <thead className="bg-slate-950/80 border-b border-slate-800 text-slate-400 font-bold uppercase tracking-wider text-[10px]">
                      <tr>
                        <th className="py-2.5 px-3">Mapping Name</th>
                        <th className="py-2.5 px-3">Incoming Resilio Sync Folder</th>
                        <th className="py-2.5 px-3">Destination Arr Intake Folder</th>
                        <th className="py-2.5 px-3 w-20">Media</th>
                        <th className="py-2.5 px-3 w-20">Settle</th>
                        <th className="py-2.5 px-3 w-12 text-right">Actions</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-slate-800/60 font-mono text-[11px]">
                      {config.file_sync.mappings.map((m, mIdx) => (
                        <tr key={m.id || mIdx} className="hover:bg-slate-800/30">
                          <td className="py-2.5 px-3 font-sans font-semibold text-slate-200">{m.name}</td>
                          <td className="py-2.5 px-3 text-amber-300 truncate max-w-xs">{m.watch_dir}</td>
                          <td className="py-2.5 px-3 text-emerald-300 truncate max-w-xs">{m.post_dir}</td>
                          <td className="py-2.5 px-3 font-sans text-slate-300">{m.media_type}</td>
                          <td className="py-2.5 px-3 text-slate-400">{m.settle_time_secs}s</td>
                          <td className="py-2.5 px-3 text-right">
                            <button
                              type="button"
                              onClick={() => {
                                const next = (config.file_sync.mappings || []).filter((_, i) => i !== mIdx);
                                setConfig({
                                  ...config,
                                  file_sync: { ...config.file_sync, mappings: next }
                                });
                              }}
                              className="text-rose-400 hover:text-rose-300"
                            >
                              <Trash2 className="h-3.5 w-3.5" />
                            </button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>

            {/* Modal for adding mapping */}
            {showAddMappingModal && (
              <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4">
                <div className="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-900 p-6 space-y-4 shadow-2xl">
                  <div className="flex items-center justify-between border-b border-slate-800 pb-3">
                    <h3 className="font-bold text-slate-100 text-base">Add Remote Sync Ingestion Path</h3>
                    <button type="button" onClick={() => setShowAddMappingModal(false)} className="text-slate-400 hover:text-slate-200">
                      <X className="h-5 w-5" />
                    </button>
                  </div>

                  <div className="space-y-3 text-xs">
                    <div>
                      <label className="block font-semibold text-slate-400 uppercase tracking-wider mb-1">Mapping Label</label>
                      <input
                        type="text"
                        value={newMappingName}
                        onChange={(e) => setNewMappingName(e.target.value)}
                        placeholder="e.g. Remote Seedbox TV Ingestion"
                        className="w-full rounded-xl border border-slate-700 bg-slate-800 p-2.5 text-slate-200 font-sans focus:outline-none focus:border-brand-500"
                      />
                    </div>

                    <div>
                      <label className="block font-semibold text-slate-400 uppercase tracking-wider mb-1">
                        Incoming Watch Directory (Resilio Destination)
                      </label>
                      <input
                        type="text"
                        value={newMappingWatch}
                        onChange={(e) => setNewMappingWatch(e.target.value)}
                        placeholder="/sync/resilio/tv_incoming/"
                        className="w-full rounded-xl border border-slate-700 bg-slate-800 p-2.5 text-slate-200 font-mono focus:outline-none focus:border-brand-500"
                      />
                    </div>

                    <div>
                      <label className="block font-semibold text-slate-400 uppercase tracking-wider mb-1">
                        Post-Media Arr Intake Directory
                      </label>
                      <input
                        type="text"
                        value={newMappingPost}
                        onChange={(e) => setNewMappingPost(e.target.value)}
                        placeholder="/media/tv/intake_queue/"
                        className="w-full rounded-xl border border-slate-700 bg-slate-800 p-2.5 text-slate-200 font-mono focus:outline-none focus:border-brand-500"
                      />
                    </div>

                    <div className="grid grid-cols-2 gap-3">
                      <div>
                        <label className="block font-semibold text-slate-400 uppercase tracking-wider mb-1">Media Category</label>
                        <select
                          value={newMappingMediaType}
                          onChange={(e) => setNewMappingMediaType(e.target.value)}
                          className="w-full rounded-xl border border-slate-700 bg-slate-800 p-2.5 text-slate-200 focus:outline-none focus:border-brand-500"
                        >
                          <option value="tv">TV Series</option>
                          <option value="movie">Movies</option>
                          <option value="music">Music</option>
                          <option value="anime">Anime</option>
                          <option value="books">Books</option>
                          <option value="misc">Miscellaneous</option>
                        </select>
                      </div>

                      <div>
                        <label className="block font-semibold text-slate-400 uppercase tracking-wider mb-1">Settle Time (Seconds)</label>
                        <input
                          type="number"
                          value={newMappingSettle}
                          onChange={(e) => setNewMappingSettle(Number(e.target.value))}
                          className="w-full rounded-xl border border-slate-700 bg-slate-800 p-2.5 text-slate-200 focus:outline-none focus:border-brand-500"
                        />
                      </div>
                    </div>

                    <label className="flex items-center space-x-2 pt-2 text-slate-300 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={newMappingDeleteSource}
                        onChange={(e) => setNewMappingDeleteSource(e.target.checked)}
                        className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                      />
                      <span>Delete source file after move (move instead of hardlink)</span>
                    </label>
                  </div>

                  <div className="flex items-center justify-end space-x-2 pt-3 border-t border-slate-800">
                    <button
                      type="button"
                      onClick={() => setShowAddMappingModal(false)}
                      className="rounded-xl border border-slate-700 bg-slate-800 px-4 py-2 text-xs font-semibold text-slate-300 hover:bg-slate-700"
                    >
                      Cancel
                    </button>
                    <button
                      type="button"
                      onClick={() => {
                        if (!newMappingWatch.trim() || !newMappingPost.trim()) {
                          alert('Watch Directory and Post Directory are required');
                          return;
                        }
                        const newM: RemoteSyncFolderMapping = {
                          id: `map-${Date.now()}`,
                          name: newMappingName.trim() || 'Custom Remote Sync Ingestion',
                          watch_dir: newMappingWatch.trim(),
                          post_dir: newMappingPost.trim(),
                          media_type: newMappingMediaType,
                          settle_time_secs: newMappingSettle,
                          delete_source_after_move: newMappingDeleteSource,
                        };
                        const current = config.file_sync?.mappings || [];
                        setConfig({
                          ...config,
                          file_sync: { ...config.file_sync, mappings: [...current, newM] }
                        });
                        setShowAddMappingModal(false);
                        setNewMappingName('');
                        setNewMappingWatch('');
                        setNewMappingPost('');
                      }}
                      className="rounded-xl bg-brand-600 px-4 py-2 text-xs font-semibold text-white hover:bg-brand-500"
                    >
                      Add Mapping
                    </button>
                  </div>
                </div>
              </div>
            )}
          </div>

          {/* Transmission Client Hook Script Generator & cURL Downloader */}
          <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-5">
            <div className="flex items-center justify-between border-b border-slate-800 pb-3">
              <div className="flex items-center space-x-2.5">
                <div className="rounded-lg bg-sky-500/10 p-2 text-sky-400 border border-sky-500/20">
                  <Terminal className="h-5 w-5" />
                </div>
                <div>
                  <h3 className="text-base font-bold text-slate-100">Fetcher Conduit Intake Hook Generator (`conduit-fetch-hook`)</h3>
                  <p className="text-xs text-slate-400">
                    Auto-generated intake hook script for fetcher nodes. Communicates exclusively with Conduit's API (no hardcoded directories or third-party webhooks).
                  </p>
                </div>
              </div>

              {/* Language Switcher */}
              <div className="flex rounded-xl bg-slate-800/80 p-1 border border-slate-700 text-xs">
                <button
                  type="button"
                  onClick={() => setScriptFormat('sh')}
                  className={`px-3 py-1 rounded-lg font-semibold transition-colors ${
                    scriptFormat === 'sh' ? 'bg-brand-600 text-white' : 'text-slate-400 hover:text-slate-200'
                  }`}
                >
                  POSIX / Bash (.sh)
                </button>
                <button
                  type="button"
                  onClick={() => setScriptFormat('py')}
                  className={`px-3 py-1 rounded-lg font-semibold transition-colors ${
                    scriptFormat === 'py' ? 'bg-brand-600 text-white' : 'text-slate-400 hover:text-slate-200'
                  }`}
                >
                  Python (.py)
                </button>
                <button
                  type="button"
                  onClick={() => setScriptFormat('pl')}
                  className={`px-3 py-1 rounded-lg font-semibold transition-colors ${
                    scriptFormat === 'pl' ? 'bg-brand-600 text-white' : 'text-slate-400 hover:text-slate-200'
                  }`}
                >
                  Perl (.pl)
                </button>
              </div>
            </div>

            {/* Quick Install cURL Box */}
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <label className="text-xs font-bold uppercase tracking-wider text-slate-400 flex items-center space-x-1.5">
                  <Download className="h-3.5 w-3.5 text-sky-400" />
                  <span>One-Liner cURL Download Command for Fetcher Nodes</span>
                </label>
                <span className="text-[11px] text-slate-500 font-mono">
                  Environment variables take precedence on runtime
                </span>
              </div>

              {(() => {
                const host = window.location.origin;
                const ext = scriptFormat === 'py' ? 'py' : scriptFormat === 'pl' ? 'pl' : 'sh';
                const curlCmd = `curl -sSL "${host}/api/sync/hook-script?format=${scriptFormat}" -o /opt/transmission/conduit-fetch-hook.${ext} && chmod +x /opt/transmission/conduit-fetch-hook.${ext}`;

                return (
                  <div className="flex items-center space-x-2 rounded-xl border border-slate-800 bg-slate-950 p-2.5 font-mono text-xs text-sky-300">
                    <pre className="flex-1 overflow-x-auto select-all pr-2">{curlCmd}</pre>
                    <button
                      type="button"
                      onClick={() => {
                        navigator.clipboard.writeText(curlCmd);
                        setCopiedCurl(true);
                        setTimeout(() => setCopiedCurl(false), 2500);
                      }}
                      className="flex items-center space-x-1 rounded-lg bg-slate-800 px-3 py-1.5 text-xs font-semibold text-slate-200 hover:bg-slate-700 shrink-0 transition-colors"
                    >
                      {copiedCurl ? <Check className="h-3.5 w-3.5 text-emerald-400" /> : <Copy className="h-3.5 w-3.5" />}
                      <span>{copiedCurl ? 'Copied!' : 'Copy cURL'}</span>
                    </button>
                  </div>
                );
              })()}
            </div>

            {/* Transmission settings.json instructions */}
            <div className="rounded-xl border border-slate-800 bg-slate-950 p-4 space-y-2 font-mono text-xs text-slate-300">
              <div className="text-slate-400 font-sans font-semibold text-[11px] uppercase tracking-wider mb-1 flex items-center justify-between">
                <span>Transmission <code className="text-sky-300">settings.json</code> Configuration:</span>
                <span className="text-slate-500 font-normal">Reload transmission-daemon after saving</span>
              </div>
              <pre className="text-amber-300 overflow-x-auto">{`"script-torrent-done-enabled": true,
"script-torrent-done-filename": "/opt/transmission/conduit-fetch-hook.${scriptFormat === 'py' ? 'py' : scriptFormat === 'pl' ? 'pl' : 'sh'}"`}</pre>
            </div>

            {/* Live Script Source Code Preview */}
            <div className="space-y-2 pt-2">
              <div className="flex items-center justify-between">
                <span className="text-xs font-bold uppercase tracking-wider text-slate-400 flex items-center space-x-1.5">
                  <FileCode className="h-3.5 w-3.5 text-indigo-400" />
                  <span>Generated Script Source Code ({scriptFormat.toUpperCase()})</span>
                </span>
                <button
                  type="button"
                  onClick={() => {
                    if (scriptContent) {
                      navigator.clipboard.writeText(scriptContent);
                      setCopiedScript(true);
                      setTimeout(() => setCopiedScript(false), 2500);
                    }
                  }}
                  className="flex items-center space-x-1 rounded-lg bg-slate-800 px-3 py-1 text-xs font-semibold text-slate-300 hover:bg-slate-700 transition-colors"
                >
                  {copiedScript ? <Check className="h-3 w-3 text-emerald-400" /> : <Copy className="h-3 w-3" />}
                  <span>{copiedScript ? 'Copied Source!' : 'Copy Code'}</span>
                </button>
              </div>

              <div className="rounded-xl border border-slate-800 bg-slate-950 p-4 max-h-72 overflow-y-auto font-mono text-[11px] text-slate-300">
                {loadingScript ? (
                  <div className="text-slate-500 py-6 text-center">Generating script...</div>
                ) : (
                  <pre className="whitespace-pre overflow-x-auto text-sky-200">{scriptContent}</pre>
                )}
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Tab 4: Plex & Trakt Integrations */}
      {tab === 'plex_trakt' && (
        <div className="space-y-8 max-w-5xl">
          {/* Plex Media Server Configuration */}
          <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-5">
            <div className="flex items-center justify-between border-b border-slate-800 pb-3">
              <div className="flex items-center space-x-2.5">
                <div className="rounded-lg bg-amber-500/10 p-2 text-amber-400 border border-amber-500/20">
                  <Film className="h-5 w-5" />
                </div>
                <div>
                  <h2 className="text-base font-bold text-slate-100">Plex Media Server Integration</h2>
                  <p className="text-xs text-slate-400">Library refresh automation and watch history synchronization.</p>
                </div>
              </div>
              <label className="flex items-center space-x-2 text-sm text-slate-300">
                <input
                  type="checkbox"
                  checked={config.plex.enabled}
                  onChange={(e) => setConfig({ ...config, plex: { ...config.plex, enabled: e.target.checked } })}
                  className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                />
                <span>Enable Plex</span>
              </label>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div>
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                  Global Plex Token (X-Plex-Token)
                </label>
                <input
                  type="password"
                  value={config.plex.token}
                  placeholder="Plex API Token"
                  onChange={(e) => setConfig({ ...config, plex: { ...config.plex, token: e.target.value } })}
                  className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                />
              </div>
              <div className="flex flex-col sm:flex-row items-start sm:items-center gap-4 pt-5">
                <label className="flex items-center space-x-2 text-sm text-slate-300">
                  <input
                    type="checkbox"
                    checked={config.plex.refresh_on_sync}
                    onChange={(e) => setConfig({ ...config, plex: { ...config.plex, refresh_on_sync: e.target.checked } })}
                    className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                  />
                  <span>Trigger Library Refresh on Sync</span>
                </label>
                <label className="flex items-center space-x-2 text-sm text-slate-300">
                  <input
                    type="checkbox"
                    checked={config.plex.digest_scrobbles ?? true}
                    onChange={(e) => setConfig({ ...config, plex: { ...config.plex, digest_scrobbles: e.target.checked } })}
                    className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                  />
                  <span>Digest Scrobble Events</span>
                </label>
                <label className="flex items-center space-x-2 text-sm text-slate-300">
                  <input
                    type="checkbox"
                    checked={config.plex.notify_on_import ?? true}
                    onChange={(e) => setConfig({ ...config, plex: { ...config.plex, notify_on_import: e.target.checked } })}
                    className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                  />
                  <span>Conduit Notifies Plex on Import</span>
                </label>
              </div>
            </div>

            <div className="rounded-xl border border-amber-500/20 bg-amber-500/5 p-4 text-xs text-amber-200 leading-relaxed">
              <strong>Taking over from Sonarr/Radarr:</strong> with "Conduit Notifies Plex on Import" enabled, Conduit itself tells Plex to rescan just the affected show/movie folder the moment Sonarr or Radarr reports an import — the same targeted refresh Sonarr/Radarr's own built-in Plex connection performs. If you had that connection configured under Sonarr/Radarr's own <em>Settings &rarr; Connect</em>, disable or remove it there now to avoid Plex being told to scan the same folder twice.
            </div>

            {/* Connect Plex Account (OAuth) */}
            <div className="rounded-xl border border-slate-800 bg-slate-800/20 p-4 space-y-3">
              <div className="flex items-center justify-between">
                <div>
                  <span className="text-xs font-bold uppercase tracking-wider text-slate-300">Connect Plex Account</span>
                  <p className="text-[11px] text-slate-500 mt-0.5">Link your Plex account to register Conduit as its own device and discover your owned servers — no manual token copying.</p>
                </div>
                <button
                  type="button"
                  onClick={handleConnectPlexAccount}
                  disabled={plexOAuthState === 'waiting'}
                  className="flex items-center space-x-1.5 rounded-lg bg-amber-600/20 hover:bg-amber-600/30 border border-amber-500/30 px-3 py-1.5 text-xs font-semibold text-amber-300 disabled:opacity-50"
                >
                  <ExternalLink className="h-3.5 w-3.5" />
                  <span>{plexOAuthState === 'waiting' ? 'Waiting for approval...' : 'Connect Plex Account'}</span>
                </button>
              </div>

              {plexOAuthState === 'error' && plexOAuthError && (
                <div className="rounded-lg bg-rose-950/40 border border-rose-500/30 p-2 text-xs text-rose-300">{plexOAuthError}</div>
              )}

              {plexDiscoveredServers.length > 0 && (
                <div className="space-y-2 pt-1">
                  <span className="text-[11px] font-semibold text-slate-400 uppercase tracking-wider">Discovered Servers</span>
                  {plexDiscoveredServers.map((server, idx) => {
                    const defaultUri = (server.connections.find((c) => c.local) || server.connections[0])?.uri || '';
                    const currentUri = selectedConnectionUri[idx] ?? defaultUri;
                    return (
                      <div key={server.name} className="rounded-lg border border-slate-700 bg-slate-900/60 px-3 py-2 space-y-1.5">
                        <div className="flex items-center justify-between">
                          <span className="text-sm font-semibold text-slate-200">{server.name}</span>
                          <button
                            type="button"
                            onClick={() => handleAddDiscoveredServer(server, idx)}
                            disabled={addingPlexServerIdx === idx || !currentUri}
                            className="rounded-lg bg-emerald-600/20 hover:bg-emerald-600/30 border border-emerald-500/30 px-3 py-1 text-xs font-semibold text-emerald-300 disabled:opacity-50"
                          >
                            {addingPlexServerIdx === idx ? 'Adding...' : 'Add'}
                          </button>
                        </div>
                        <div>
                          <label className="block text-[10px] font-semibold text-slate-500 uppercase tracking-wider mb-0.5">
                            Interface / Connection {server.connections.length > 0 && `(${server.connections.length} discovered)`}
                          </label>
                          {server.connections.length > 0 && (
                            <select
                              value={server.connections.some((c) => c.uri === currentUri) ? currentUri : ''}
                              onChange={(e) => setSelectedConnectionUri((prev) => ({ ...prev, [idx]: e.target.value }))}
                              className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1 px-2 text-[11px] font-mono text-slate-300 focus:border-brand-500 focus:outline-none mb-1.5"
                            >
                              <option value="" disabled>
                                Pick a discovered connection, or type a custom URL below
                              </option>
                              {server.connections.map((c) => (
                                <option key={c.uri} value={c.uri}>
                                  {c.local ? 'LAN' : 'Remote'} — {c.uri}
                                </option>
                              ))}
                            </select>
                          )}
                          <input
                            type="text"
                            value={currentUri}
                            placeholder="http://192.168.1.50:32400"
                            onChange={(e) => setSelectedConnectionUri((prev) => ({ ...prev, [idx]: e.target.value }))}
                            className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1 px-2 text-[11px] font-mono text-slate-300 focus:border-brand-500 focus:outline-none"
                          />
                          <p className="text-[10px] text-slate-500 mt-1">
                            Plex sometimes advertises a "local" address that isn't actually reachable from wherever Conduit runs — e.g. a container's internal IP rather than its host's real LAN IP. If none of the discovered connections work, type this server's real LAN address directly.
                          </p>
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>

            {/* Plex Servers List */}
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <span className="text-xs font-bold uppercase tracking-wider text-slate-400">
                  Plex Server Nodes ({config.plex.nodes.length})
                </span>
                <button
                  type="button"
                  onClick={() => {
                    const newServer = {
                      name: `Plex-Server-${config.plex.nodes.length + 1}`,
                      url: 'http://localhost:32400',
                    };
                    setConfig({
                      ...config,
                      plex: { ...config.plex, nodes: [...config.plex.nodes, newServer] },
                    });
                  }}
                  className="flex items-center space-x-1 rounded-lg bg-slate-800 px-3 py-1 text-xs font-semibold text-amber-400 hover:bg-slate-700"
                >
                  <Plus className="h-3.5 w-3.5" />
                  <span>Add Plex Server</span>
                </button>
              </div>

              {config.plex.nodes.map((server, idx) => (
                <div key={idx} className="rounded-xl border border-slate-800 bg-slate-800/20 p-4 space-y-3">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center space-x-3">
                      <span className="font-semibold text-sm text-slate-200">{server.name}</span>
                      {plexStatusMap[idx] && (
                        <span className="text-xs font-mono text-slate-300">{plexStatusMap[idx]}</span>
                      )}
                    </div>
                    <div className="flex items-center space-x-2">
                      <button
                        type="button"
                        onClick={() => handleTestPlex(idx, server.url, server.token_override)}
                        disabled={testingPlexIdx === idx}
                        className="rounded-lg bg-slate-800 px-3 py-1 text-xs font-semibold text-amber-400 hover:bg-slate-700 disabled:opacity-50"
                      >
                        {testingPlexIdx === idx ? 'Testing...' : 'Test Connection'}
                      </button>
                      <button
                        type="button"
                        onClick={() => handleRefreshPlex(idx, server.url, server.token_override)}
                        disabled={refreshingPlexIdx === idx}
                        className="rounded-lg bg-slate-800 px-3 py-1 text-xs font-semibold text-sky-400 hover:bg-slate-700 disabled:opacity-50 flex items-center space-x-1"
                      >
                        <RotateCcw className={`h-3 w-3 ${refreshingPlexIdx === idx ? 'animate-spin' : ''}`} />
                        <span>Refresh Sections</span>
                      </button>
                      <button
                        type="button"
                        onClick={() => {
                          const nodes = config.plex.nodes.filter((_, i) => i !== idx);
                          setConfig({ ...config, plex: { ...config.plex, nodes } });
                        }}
                        className="text-rose-400 hover:text-rose-300 p-1"
                      >
                        <Trash2 className="h-4 w-4" />
                      </button>
                    </div>
                  </div>

                  <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                    <input
                      type="text"
                      value={server.name}
                      placeholder="Server Name"
                      onChange={(e) => {
                        const nodes = [...config.plex.nodes];
                        nodes[idx] = { ...nodes[idx], name: e.target.value };
                        setConfig({ ...config, plex: { ...config.plex, nodes } });
                      }}
                      className="rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                    />
                    <input
                      type="text"
                      value={server.url}
                      placeholder="http://192.168.1.50:32400"
                      onChange={(e) => {
                        const nodes = [...config.plex.nodes];
                        nodes[idx] = { ...nodes[idx], url: e.target.value };
                        setConfig({ ...config, plex: { ...config.plex, nodes } });
                      }}
                      className="rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                    />
                    <input
                      type="password"
                      value={server.token_override || ''}
                      placeholder="Token Override (Optional)"
                      onChange={(e) => {
                        const nodes = [...config.plex.nodes];
                        nodes[idx] = { ...nodes[idx], token_override: e.target.value || undefined };
                        setConfig({ ...config, plex: { ...config.plex, nodes } });
                      }}
                      className="rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                    />
                  </div>

                  <label className="flex items-center space-x-2 text-xs text-slate-300 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={server.sync_watch_status ?? false}
                      onChange={(e) => {
                        const nodes = [...config.plex.nodes];
                        nodes[idx] = { ...nodes[idx], sync_watch_status: e.target.checked };
                        setConfig({ ...config, plex: { ...config.plex, nodes } });
                      }}
                      className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                    />
                    <span>Include in Trakt-backed watch-status sync</span>
                    {server.sync_watch_status && !server.server_uuid && (
                      <span className="text-[10px] text-slate-500">(detecting server identity...)</span>
                    )}
                  </label>
                </div>
              ))}
            </div>
          </div>

          {/* Trakt Integration */}
          <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-5">
            <div className="flex items-center justify-between border-b border-slate-800 pb-3">
              <div className="flex items-center space-x-2.5">
                <div className="rounded-lg bg-rose-500/10 p-2 text-rose-400 border border-rose-500/20">
                  <Sparkles className="h-5 w-5" />
                </div>
                <div>
                  <h2 className="text-base font-bold text-slate-100">Trakt.tv Scrobble & Watch Sync</h2>
                  <p className="text-xs text-slate-400">Sync watched movie & episode history across nodes and Trakt.</p>
                </div>
              </div>
              <label className="flex items-center space-x-2 text-sm text-slate-300">
                <input
                  type="checkbox"
                  checked={config.trakt.enabled}
                  onChange={(e) => setConfig({ ...config, trakt: { ...config.trakt, enabled: e.target.checked } })}
                  className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                />
                <span>Enable Trakt</span>
              </label>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <div>
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Client ID</label>
                <input
                  type="text"
                  value={config.trakt.client_id}
                  placeholder="Trakt App Client ID"
                  onChange={(e) => setConfig({ ...config, trakt: { ...config.trakt, client_id: e.target.value } })}
                  className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                />
              </div>
              <div>
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Client Secret</label>
                <input
                  type="password"
                  value={config.trakt.client_secret}
                  placeholder="Trakt App Client Secret"
                  onChange={(e) => setConfig({ ...config, trakt: { ...config.trakt, client_secret: e.target.value } })}
                  className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                />
              </div>
              <div>
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Access Token</label>
                <input
                  type="password"
                  value={config.trakt.access_token || ''}
                  placeholder="OAuth Access Token"
                  onChange={(e) => setConfig({ ...config, trakt: { ...config.trakt, access_token: e.target.value || undefined } })}
                  className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                />
              </div>
            </div>

            <div className="rounded-xl border border-amber-500/20 bg-amber-500/5 p-4 space-y-2">
              <label className="flex items-start space-x-3 cursor-pointer">
                <input
                  type="checkbox"
                  checked={config.trakt.sync_watched_back_to_plex ?? false}
                  onChange={(e) => setConfig({ ...config, trakt: { ...config.trakt, sync_watched_back_to_plex: e.target.checked } })}
                  className="mt-0.5 rounded border-slate-700 bg-slate-800 text-amber-500 focus:ring-amber-500"
                />
                <div>
                  <span className="block text-sm font-semibold text-slate-200">Also mark watched on my other Plex servers</span>
                  <span className="block text-xs text-slate-400 mt-0.5">
                    Writes to Plex, not just Trakt. When something is watched on any Plex server with "Include in Trakt-backed watch-status sync" checked above, Conduit looks it up by IMDb/TMDb/TVDb ID on every other participating server's own library and marks it watched there too — a best-effort match.
                  </span>
                </div>
              </label>
            </div>

            <div className="flex items-center justify-between pt-3 border-t border-slate-800">
              <div className="flex items-center space-x-3 text-xs text-slate-400">
                <span>Sync Interval:</span>
                <input
                  type="number"
                  value={config.trakt.sync_interval_mins}
                  onChange={(e) => setConfig({ ...config, trakt: { ...config.trakt, sync_interval_mins: Number(e.target.value) } })}
                  className="w-20 rounded border border-slate-700 bg-slate-800 px-2 py-1 text-slate-200"
                />
                <span>minutes</span>
              </div>
              <div className="flex items-center space-x-3">
                {traktStatus && <span className="text-xs font-mono text-slate-300">{traktStatus}</span>}
                <button
                  type="button"
                  onClick={handleTestTrakt}
                  disabled={testingTrakt}
                  className="rounded-lg bg-slate-800 px-4 py-1.5 text-xs font-semibold text-rose-400 hover:bg-slate-700 disabled:opacity-50"
                >
                  {testingTrakt ? 'Testing...' : 'Test Trakt Credentials'}
                </button>
              </div>
            </div>
          </div>

          {/* Plex Inbound Webhook & Scrobble Ingestion */}
          <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-5">
            <div className="flex items-center justify-between border-b border-slate-800 pb-3">
              <div className="flex items-center space-x-2.5">
                <div className="rounded-lg bg-amber-500/10 p-2 text-amber-400 border border-amber-500/20">
                  <Film className="h-5 w-5" />
                </div>
                <div>
                  <h2 className="text-base font-bold text-slate-100">Plex Inbound Webhook & Scrobble Ingestion</h2>
                  <p className="text-xs text-slate-400">Receive real-time playback scrobbles, media progress, and library indexing from Plex Media Server.</p>
                </div>
              </div>
              <button
                type="button"
                onClick={loadScrobbles}
                disabled={loadingScrobbles}
                className="rounded-lg bg-slate-800 px-3 py-1.5 text-xs font-semibold text-slate-300 hover:bg-slate-700 disabled:opacity-50"
              >
                {loadingScrobbles ? 'Refreshing...' : 'Refresh History'}
              </button>
            </div>

            <div className="space-y-3">
              <div>
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Inbound Webhook URL (for Plex)</label>
                <div className="flex items-center space-x-2">
                  <input
                    type="text"
                    readOnly
                    value={`${window.location.origin}/api/plex/inbound`}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm font-mono text-amber-300 focus:outline-none"
                  />
                  <button
                    type="button"
                    onClick={() => {
                      navigator.clipboard.writeText(`${window.location.origin}/api/plex/inbound`);
                      setCopiedPlexWebhook(true);
                      setTimeout(() => setCopiedPlexWebhook(false), 2000);
                    }}
                    className="rounded-lg bg-slate-800 border border-slate-700 px-4 py-2 text-xs font-semibold text-slate-200 hover:bg-slate-700 flex items-center space-x-1.5 shrink-0"
                  >
                    {copiedPlexWebhook ? <Check className="h-4 w-4 text-emerald-400" /> : <Copy className="h-4 w-4" />}
                    <span>{copiedPlexWebhook ? 'Copied' : 'Copy URL'}</span>
                  </button>
                  <button
                    type="button"
                    onClick={handleTestPlexWebhook}
                    disabled={testingPlexWebhook}
                    className="rounded-lg bg-amber-500/20 border border-amber-500/30 px-4 py-2 text-xs font-semibold text-amber-300 hover:bg-amber-500/30 disabled:opacity-50 shrink-0"
                  >
                    {testingPlexWebhook ? 'Simulating...' : 'Simulate Scrobble'}
                  </button>
                </div>
                <p className="text-xs text-slate-400 mt-1.5">
                  In Plex Web, navigate to <strong>Settings &rarr; Webhooks &rarr; Add Webhook</strong> and paste the URL above.
                </p>
              </div>

              {/* Scrobbles List */}
              {scrobbles.length > 0 && (
                <div className="mt-4 border-t border-slate-800/80 pt-3">
                  <h3 className="text-xs font-bold uppercase tracking-wider text-slate-400 mb-2">Recent Plex Scrobbles</h3>
                  <div className="space-y-2">
                    {scrobbles.slice(0, 5).map((s) => (
                      <div key={s.id} className="flex items-center justify-between rounded-lg bg-slate-800/50 p-2.5 text-xs text-slate-300">
                        <div className="flex items-center space-x-2">
                          <span className="font-semibold text-slate-200">{s.series_title ? `${s.series_title} - ${s.title}` : s.title}</span>
                          <span className="rounded bg-amber-500/20 px-1.5 py-0.5 text-[10px] font-mono text-amber-300">{s.event}</span>
                          <span className="text-slate-400">by {s.user_name}</span>
                        </div>
                        <div className="flex items-center space-x-2">
                          {s.trakt_synced && (
                            <span className="rounded bg-rose-500/20 px-1.5 py-0.5 text-[10px] font-mono text-rose-300">Trakt Synced</span>
                          )}
                          <span className="text-slate-500 font-mono text-[10px]">{new Date(s.created_at).toLocaleTimeString()}</span>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </div>

          {/* Ombi Inbound Webhook & Request Pipeline */}
          <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-5">
            <div className="flex items-center justify-between border-b border-slate-800 pb-3">
              <div className="flex items-center space-x-2.5">
                <div className="rounded-lg bg-pink-500/10 p-2 text-pink-400 border border-pink-500/20 text-lg">
                  🍖
                </div>
                <div>
                  <h2 className="text-base font-bold text-slate-100">Kibble Requests (Ombi) Inbound Webhook</h2>
                  <p className="text-xs text-slate-400">Ingest user media kibble requests and approval events directly into the Conduit single living card pipeline.</p>
                </div>
              </div>
              <button
                type="button"
                onClick={loadOmbiRequests}
                disabled={loadingOmbi}
                className="rounded-lg bg-slate-800 px-3 py-1.5 text-xs font-semibold text-slate-300 hover:bg-slate-700 disabled:opacity-50"
              >
                {loadingOmbi ? 'Refreshing...' : 'Refresh Kibble Orders'}
              </button>
            </div>

            <div className="space-y-3">
              <div>
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Inbound Webhook URL (for Ombi)</label>
                <div className="flex items-center space-x-2">
                  <input
                    type="text"
                    readOnly
                    value={`${window.location.origin}/api/ombi/inbound`}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm font-mono text-pink-300 focus:outline-none"
                  />
                  <button
                    type="button"
                    onClick={() => {
                      navigator.clipboard.writeText(`${window.location.origin}/api/ombi/inbound`);
                      setCopiedOmbiWebhook(true);
                      setTimeout(() => setCopiedOmbiWebhook(false), 2000);
                    }}
                    className="rounded-lg bg-slate-800 border border-slate-700 px-4 py-2 text-xs font-semibold text-slate-200 hover:bg-slate-700 flex items-center space-x-1.5 shrink-0"
                  >
                    {copiedOmbiWebhook ? <Check className="h-4 w-4 text-emerald-400" /> : <Copy className="h-4 w-4" />}
                    <span>{copiedOmbiWebhook ? 'Copied' : 'Copy URL'}</span>
                  </button>
                  <button
                    type="button"
                    onClick={handleTestOmbiWebhook}
                    disabled={testingOmbiWebhook}
                    className="rounded-lg bg-pink-500/20 border border-pink-500/30 px-4 py-2 text-xs font-semibold text-pink-300 hover:bg-pink-500/30 disabled:opacity-50 shrink-0"
                  >
                    {testingOmbiWebhook ? 'Simulating...' : 'Simulate Kibble Request'}
                  </button>
                </div>
                <p className="text-xs text-slate-400 mt-1.5">
                  In Ombi, navigate to <strong>Settings &rarr; Notifications &rarr; Webhook</strong>, paste the URL above, and enable request notifications.
                </p>
              </div>

              {/* Ombi Requests List */}
              {ombiRequests.length > 0 && (
                <div className="mt-4 border-t border-slate-800/80 pt-3">
                  <h3 className="text-xs font-bold uppercase tracking-wider text-slate-400 mb-2">Recent Kibble Orders (Ombi)</h3>
                  <div className="space-y-2">
                    {ombiRequests.slice(0, 5).map((r) => (
                      <div key={r.id} className="flex items-center justify-between rounded-lg bg-slate-800/50 p-2.5 text-xs text-slate-300">
                        <div className="flex items-center space-x-2">
                          <span className="font-semibold text-slate-200">{r.title} {r.year ? `(${r.year})` : ''}</span>
                          <span className="rounded bg-pink-500/20 px-1.5 py-0.5 text-[10px] font-mono text-pink-300">{r.status}</span>
                          <span className="text-slate-400">ordered by {r.requested_by}</span>
                        </div>
                        <span className="text-slate-500 font-mono text-[10px]">{new Date(r.created_at).toLocaleTimeString()}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {/* Tab 5: Notifications */}
      {tab === 'notify' && (
        <div className="space-y-6 max-w-5xl">
          {notifyStatus && (
            <div className="rounded-xl bg-sky-500/10 border border-sky-500/20 p-3 text-xs font-semibold text-sky-400">
              {notifyStatus}
            </div>
          )}

          <div className="rounded-xl border border-sky-500/20 bg-sky-500/10 p-4 text-xs text-sky-300 leading-relaxed">
            Each <strong>target</strong> is an independent destination — pick a channel, choose which event categories it receives (leave all unchecked for "every category"), and optionally scope it to one zone. Run as many targets as you want, including multiple of the same channel type (e.g. one Mattermost feed for everything, another scoped to just one zone).
          </div>

          <div className="flex items-center justify-between">
            <h2 className="text-lg font-bold text-slate-200">Notification Targets</h2>
            <div className="flex items-center space-x-2">
              {(['mattermost', 'discord', 'pushover', 'webhook'] as const).map((chType) => (
                <button
                  key={chType}
                  onClick={() => {
                    const name = prompt(`Name for this ${CHANNEL_LABELS[chType].label} target:`, CHANNEL_LABELS[chType].label);
                    if (!name) return;
                    const id = `${chType}-${Date.now()}`;
                    const newTarget: NotificationTarget = {
                      id,
                      name,
                      enabled: true,
                      channel: defaultChannelFor(chType),
                      categories: [],
                    };
                    setConfig({ ...config, notifications: { ...config.notifications, targets: [...config.notifications.targets, newTarget] } });
                  }}
                  className="flex items-center space-x-1.5 rounded-lg bg-slate-800 px-3 py-1.5 text-xs font-semibold text-brand-400 hover:bg-slate-700"
                >
                  <Plus className="h-3.5 w-3.5" />
                  <span>{CHANNEL_LABELS[chType].icon} {CHANNEL_LABELS[chType].label}</span>
                </button>
              ))}
            </div>
          </div>

          {config.notifications.targets.length === 0 && (
            <div className="rounded-xl border border-slate-800 bg-slate-900/40 p-8 text-center text-sm text-slate-500">
              No notification targets configured yet — add one above.
            </div>
          )}

          {config.notifications.targets.map((target, tIdx) => {
            const updateTarget = (patch: Partial<NotificationTarget>) => {
              const targets = [...config.notifications.targets];
              targets[tIdx] = { ...targets[tIdx], ...patch };
              setConfig({ ...config, notifications: { ...config.notifications, targets } });
            };
            const updateChannel = (patch: Partial<NotificationChannel>) => {
              updateTarget({ channel: { ...target.channel, ...patch } as NotificationChannel });
            };
            const toggleCategory = (cat: string) => {
              const cats = target.categories.includes(cat)
                ? target.categories.filter((c) => c !== cat)
                : [...target.categories, cat];
              updateTarget({ categories: cats });
            };

            return (
              <div key={target.id} className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-4">
                <div className="flex items-center justify-between border-b border-slate-800 pb-3">
                  <div className="flex items-center space-x-2.5">
                    <span className="text-xl">{CHANNEL_LABELS[target.channel.type].icon}</span>
                    <div>
                      <input
                        type="text"
                        value={target.name}
                        onChange={(e) => updateTarget({ name: e.target.value })}
                        className="bg-transparent text-base font-bold text-slate-100 focus:outline-none focus:border-b focus:border-brand-500"
                      />
                      <p className="text-xs text-slate-400">{CHANNEL_LABELS[target.channel.type].label}</p>
                    </div>
                  </div>
                  <div className="flex items-center space-x-3">
                    <button
                      type="button"
                      onClick={() => handleTestNotification(target.id)}
                      disabled={testingChannel === target.id}
                      className="flex items-center space-x-1.5 rounded-lg bg-slate-800 px-3 py-1.5 text-xs font-semibold text-sky-400 hover:bg-slate-700 disabled:opacity-50"
                    >
                      <Send className="h-3 w-3" />
                      <span>{testingChannel === target.id ? 'Sending...' : 'Send Test'}</span>
                    </button>
                    <label className="flex items-center space-x-1.5 text-sm text-slate-300">
                      <input
                        type="checkbox"
                        checked={target.enabled}
                        onChange={(e) => updateTarget({ enabled: e.target.checked })}
                        className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                      />
                      <span>Enabled</span>
                    </label>
                    <button
                      onClick={() => {
                        if (!window.confirm(`Delete notification target '${target.name}'?`)) return;
                        setConfig({ ...config, notifications: { ...config.notifications, targets: config.notifications.targets.filter((_, i) => i !== tIdx) } });
                      }}
                      className="text-rose-400 hover:text-rose-300"
                    >
                      <Trash2 className="h-4 w-4" />
                    </button>
                  </div>
                </div>

                {/* Channel-specific fields */}
                {target.channel.type === 'mattermost' && (
                  <div className="space-y-4">
                    <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                      <div>
                        <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Server URL</label>
                        <input
                          type="text"
                          value={target.channel.server_url || ''}
                          placeholder="https://mattermost.domain.com"
                          onChange={(e) => updateChannel({ server_url: e.target.value })}
                          className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                        />
                      </div>
                      <div>
                        <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Bot Access Token</label>
                        <input
                          type="password"
                          value={target.channel.bot_token || ''}
                          placeholder="e.g. 1f85xcsdibri9qks7sczk76nxc"
                          onChange={(e) => updateChannel({ bot_token: e.target.value })}
                          className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                        />
                      </div>
                      <div>
                        <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Target Channel ID</label>
                        <input
                          type="text"
                          value={target.channel.channel_id || ''}
                          placeholder="26-character Channel ID"
                          onChange={(e) => updateChannel({ channel_id: e.target.value })}
                          className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                        />
                      </div>
                    </div>
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4 rounded-xl border border-slate-800/80 bg-slate-950/40 p-4">
                      <label className="flex items-start space-x-3 cursor-pointer">
                        <input
                          type="checkbox"
                          checked={target.channel.use_living_cards}
                          onChange={(e) => updateChannel({ use_living_cards: e.target.checked })}
                          className="mt-0.5 rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                        />
                        <div>
                          <span className="block text-sm font-semibold text-slate-200">Evolving Living Cards</span>
                          <span className="block text-xs text-slate-400">Edits the card in-place as media transitions stages. Only the first enabled Mattermost target gets this — additional ones post independently.</span>
                        </div>
                      </label>
                      <label className="flex items-start space-x-3 cursor-pointer">
                        <input
                          type="checkbox"
                          checked={target.channel.use_threading}
                          onChange={(e) => updateChannel({ use_threading: e.target.checked })}
                          className="mt-0.5 rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                        />
                        <div>
                          <span className="block text-sm font-semibold text-slate-200">Thread Activity Updates</span>
                          <span className="block text-xs text-slate-400">Replies in the parent card's thread for stage logs and re-searches.</span>
                        </div>
                      </label>
                    </div>
                    <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                      <div className="md:col-span-2">
                        <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                          Incoming Webhook URL <span className="text-slate-500 font-normal">(fallback if bot token unset)</span>
                        </label>
                        <input
                          type="text"
                          value={target.channel.webhook_url}
                          placeholder="https://mattermost.example.com/hooks/xxx..."
                          onChange={(e) => updateChannel({ webhook_url: e.target.value })}
                          className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                        />
                      </div>
                      <div>
                        <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Bot Name</label>
                        <input
                          type="text"
                          value={target.channel.bot_name}
                          placeholder="Conduit"
                          onChange={(e) => updateChannel({ bot_name: e.target.value })}
                          className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                        />
                      </div>
                    </div>
                  </div>
                )}

                {target.channel.type === 'discord' && (
                  <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                    <div className="md:col-span-2">
                      <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Webhook URL</label>
                      <input
                        type="text"
                        value={target.channel.webhook_url}
                        placeholder="https://discord.com/api/webhooks/xxx/yyy"
                        onChange={(e) => updateChannel({ webhook_url: e.target.value })}
                        className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                      />
                    </div>
                    <div>
                      <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Username</label>
                      <input
                        type="text"
                        value={target.channel.username}
                        placeholder="Conduit"
                        onChange={(e) => updateChannel({ username: e.target.value })}
                        className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                      />
                    </div>
                  </div>
                )}

                {target.channel.type === 'pushover' && (
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                    <div>
                      <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">User Key</label>
                      <input
                        type="password"
                        value={target.channel.user_key}
                        onChange={(e) => updateChannel({ user_key: e.target.value })}
                        className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                      />
                    </div>
                    <div>
                      <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">API/App Token</label>
                      <input
                        type="password"
                        value={target.channel.api_token}
                        onChange={(e) => updateChannel({ api_token: e.target.value })}
                        className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                      />
                    </div>
                    <div>
                      <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Device (optional)</label>
                      <input
                        type="text"
                        value={target.channel.device || ''}
                        onChange={(e) => updateChannel({ device: e.target.value })}
                        className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                      />
                    </div>
                    <div>
                      <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Sound (optional)</label>
                      <input
                        type="text"
                        value={target.channel.sound || ''}
                        onChange={(e) => updateChannel({ sound: e.target.value })}
                        className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                      />
                    </div>
                  </div>
                )}

                {target.channel.type === 'webhook' && (
                  <div className="space-y-3">
                    <div>
                      <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Webhook URL</label>
                      <input
                        type="text"
                        value={target.channel.url}
                        placeholder="https://example.com/ingest/conduit"
                        onChange={(e) => updateChannel({ url: e.target.value })}
                        className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                      />
                    </div>
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                      <div>
                        <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Shared Secret (optional)</label>
                        <input
                          type="password"
                          value={target.channel.secret || ''}
                          onChange={(e) => updateChannel({ secret: e.target.value })}
                          className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                        />
                        <span className="text-[11px] text-slate-500 mt-1 block">Sent as the X-Conduit-Secret header.</span>
                      </div>
                      <div>
                        <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Payload</label>
                        <select
                          value={target.channel.payload_mode}
                          onChange={(e) => updateChannel({ payload_mode: e.target.value as 'summary' | 'full' })}
                          className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                        >
                          <option value="summary">Summary (event, title, message)</option>
                          <option value="full">Full (adds structured fields/overview)</option>
                        </select>
                      </div>
                    </div>
                  </div>
                )}

                {/* Event categories */}
                <div>
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-2">
                    Event Categories <span className="text-slate-500 font-normal normal-case">(none checked = every category)</span>
                  </label>
                  <div className="grid grid-cols-2 md:grid-cols-4 gap-2">
                    {NOTIFICATION_CATEGORIES.map((cat) => (
                      <label
                        key={cat}
                        className={`flex items-center space-x-1.5 rounded-lg border px-2.5 py-1.5 text-xs cursor-pointer transition-colors ${
                          target.categories.includes(cat)
                            ? 'border-brand-500/40 bg-brand-500/10 text-brand-300'
                            : 'border-slate-700 bg-slate-800 text-slate-400 hover:text-slate-200'
                        }`}
                      >
                        <input
                          type="checkbox"
                          checked={target.categories.includes(cat)}
                          onChange={() => toggleCategory(cat)}
                          className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                        />
                        <span>{CATEGORY_LABELS[cat] || cat}</span>
                      </label>
                    ))}
                  </div>
                </div>

                {/* Zone scope */}
                {(config.zones?.length || 0) > 0 && (
                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Zone Scope</label>
                    <select
                      value={target.zone_id || ''}
                      onChange={(e) => updateTarget({ zone_id: e.target.value || undefined })}
                      className="w-full md:w-64 rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                    >
                      <option value="">All Zones (Global)</option>
                      {config.zones!.map((z) => (
                        <option key={z.id} value={z.id}>{z.name}</option>
                      ))}
                    </select>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* Tab 6: Rules & Pipeline */}
      {tab === 'rules' && (
        <div className="space-y-8 max-w-5xl">
          {/* Error Pipeline & MediaReplacer Engine */}
          <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-5">
            <div className="flex items-center justify-between border-b border-slate-800 pb-3">
              <div className="flex items-center space-x-2.5">
                <div className="rounded-lg bg-rose-500/10 p-2 text-rose-400 border border-rose-500/20">
                  <Sliders className="h-5 w-5" />
                </div>
                <div>
                  <h2 className="text-base font-bold text-slate-100">Error Pipeline & MediaReplacer Engine</h2>
                  <p className="text-xs text-slate-400">Automate detection and re-search of trumped / unregistered torrents.</p>
                </div>
              </div>
              <label className="flex items-center space-x-2 text-sm text-slate-300">
                <input
                  type="checkbox"
                  checked={config.rule_pipeline.enabled}
                  onChange={(e) => setConfig({
                    ...config,
                    rule_pipeline: { ...config.rule_pipeline, enabled: e.target.checked },
                  })}
                  className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                />
                <span>Enable Rule Pipeline</span>
              </label>
            </div>

            <div className="space-y-4">
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <label className="flex items-center space-x-2.5 text-sm text-slate-300">
                  <input
                    type="checkbox"
                    checked={config.rule_pipeline.auto_delete_unregistered}
                    onChange={(e) => setConfig({
                      ...config,
                      rule_pipeline: { ...config.rule_pipeline, auto_delete_unregistered: e.target.checked },
                    })}
                    className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                  />
                  <span>Auto-delete dead/unregistered torrent from fetcher node</span>
                </label>

                <label className="flex items-center space-x-2.5 text-sm text-slate-300">
                  <input
                    type="checkbox"
                    checked={config.rule_pipeline.auto_trigger_media_replacer}
                    onChange={(e) => setConfig({
                      ...config,
                      rule_pipeline: { ...config.rule_pipeline, auto_trigger_media_replacer: e.target.checked },
                    })}
                    className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                  />
                  <span>Trigger Sonarr/Radarr automatic episode/movie re-search</span>
                </label>
              </div>

              <div>
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                  Unregistered / Dead Tracker Error Regex Pattern
                </label>
                <input
                  type="text"
                  value={config.rule_pipeline.unregistered_pattern}
                  onChange={(e) => setConfig({
                    ...config,
                    rule_pipeline: { ...config.rule_pipeline, unregistered_pattern: e.target.value },
                  })}
                  className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm font-mono text-slate-200 focus:border-brand-500 focus:outline-none"
                />
              </div>

              {/* Interactive Regex Tester Simulator */}
              <div className="rounded-xl border border-slate-800 bg-slate-800/30 p-4 space-y-3">
                <div className="flex items-center justify-between">
                  <span className="text-xs font-bold uppercase tracking-wider text-slate-400 flex items-center space-x-1.5">
                    <Terminal className="h-3.5 w-3.5 text-brand-400" />
                    <span>Interactive Regex Tester Simulator</span>
                  </span>
                  <button
                    type="button"
                    onClick={handleTestRegex}
                    disabled={evaluatingRegex}
                    className="rounded-lg bg-slate-800 px-3 py-1 text-xs font-semibold text-brand-400 hover:bg-slate-700 disabled:opacity-50"
                  >
                    {evaluatingRegex ? 'Evaluating...' : 'Evaluate Regex'}
                  </button>
                </div>

                <input
                  type="text"
                  value={regexTestStr}
                  onChange={(e) => setRegexTestStr(e.target.value)}
                  placeholder="Enter sample tracker error message here..."
                  className="w-full rounded-lg border border-slate-700 bg-slate-900 py-1.5 px-3 text-xs font-mono text-slate-200 focus:border-brand-500 focus:outline-none"
                />

                {regexResult && (
                  <div className={`rounded-lg p-2.5 text-xs font-mono flex items-center space-x-2 ${
                    regexResult.matched
                      ? 'bg-emerald-500/10 border border-emerald-500/30 text-emerald-400'
                      : 'bg-rose-500/10 border border-rose-500/30 text-rose-400'
                  }`}>
                    {regexResult.matched ? (
                      <>
                        <CheckCircle2 className="h-4 w-4 shrink-0" />
                        <span>MATCHED! Error message triggers auto-purge and MediaReplacer re-search.</span>
                      </>
                    ) : (
                      <>
                        <XCircle className="h-4 w-4 shrink-0" />
                        <span>NO MATCH. {regexResult.error ? `Regex Error: ${regexResult.error}` : 'String did not match pattern.'}</span>
                      </>
                    )}
                  </div>
                )}
              </div>
            </div>
          </div>

          {/* Space Management & Auto-Purge Engine */}
          <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-5">
            <div className="flex items-center justify-between border-b border-slate-800 pb-3">
              <div className="flex items-center space-x-2.5">
                <div className="rounded-lg bg-purple-500/10 p-2 text-purple-400 border border-purple-500/20">
                  <Gauge className="h-5 w-5" />
                </div>
                <div>
                  <h2 className="text-base font-bold text-slate-100">Space Management & Auto-Purge Engine</h2>
                  <p className="text-xs text-slate-400">Automate disk headroom maintenance with multi-factor ratio scoring.</p>
                </div>
              </div>
              <label className="flex items-center space-x-2 text-sm text-slate-300">
                <input
                  type="checkbox"
                  checked={config.space_manager.enabled}
                  onChange={(e) => setConfig({
                    ...config,
                    space_manager: { ...config.space_manager, enabled: e.target.checked },
                  })}
                  className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                />
                <span>Enable Space Manager</span>
              </label>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <div>
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                  Global Min Free Space (GB)
                </label>
                <input
                  type="number"
                  value={config.space_manager.global_min_space_gb}
                  onChange={(e) => setConfig({
                    ...config,
                    space_manager: { ...config.space_manager, global_min_space_gb: Number(e.target.value) },
                  })}
                  className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                />
              </div>
              <div>
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                  Check Interval (Seconds)
                </label>
                <input
                  type="number"
                  value={config.space_manager.check_interval_secs}
                  onChange={(e) => setConfig({
                    ...config,
                    space_manager: { ...config.space_manager, check_interval_secs: Number(e.target.value) },
                  })}
                  className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                />
              </div>
              <div>
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                  Max Purges Per Cycle
                </label>
                <input
                  type="number"
                  value={config.space_manager.max_purges_per_cycle}
                  onChange={(e) => setConfig({
                    ...config,
                    space_manager: { ...config.space_manager, max_purges_per_cycle: Number(e.target.value) },
                  })}
                  className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                />
              </div>
            </div>

            {/* Multi-Factor Scoring Parameters */}
            <div className="pt-4 border-t border-slate-800/80 space-y-3">
              <h3 className="text-xs font-bold text-slate-300 uppercase tracking-wider">Multi-Factor Scoring & Eligibility Rules</h3>
              <p className="text-xs text-slate-400 leading-relaxed">
                When a node's free disk space drops below the threshold, torrents qualifying under the criteria below are selected for removal in order of highest ratio.
              </p>

              <div className="grid grid-cols-1 md:grid-cols-4 gap-3 pt-2">
                <div>
                  <label className="block text-xs font-semibold text-slate-400 mb-1">
                    Target Seed Ratio
                  </label>
                  <input
                    type="number"
                    step="0.1"
                    value={config.space_manager.default_target_ratio ?? 2.0}
                    onChange={(e) => setConfig({
                      ...config,
                      space_manager: { ...config.space_manager, default_target_ratio: Number(e.target.value) },
                    })}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                  <span className="text-[10px] text-slate-500 mt-0.5 block">e.g. 2.0 (200% seeded)</span>
                </div>

                <div>
                  <label className="block text-xs font-semibold text-slate-400 mb-1">
                    Min Seeding Age (Days)
                  </label>
                  <input
                    type="number"
                    value={config.space_manager.default_target_age_days ?? 14}
                    onChange={(e) => setConfig({
                      ...config,
                      space_manager: { ...config.space_manager, default_target_age_days: Number(e.target.value) },
                    })}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                  <span className="text-[10px] text-slate-500 mt-0.5 block">e.g. 14 days finished</span>
                </div>

                <div>
                  <label className="block text-xs font-semibold text-slate-400 mb-1">
                    Swarm Seeder Pool
                  </label>
                  <input
                    type="number"
                    value={config.space_manager.default_target_seeds ?? 20}
                    onChange={(e) => setConfig({
                      ...config,
                      space_manager: { ...config.space_manager, default_target_seeds: Number(e.target.value) },
                    })}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                  <span className="text-[10px] text-slate-500 mt-0.5 block">e.g. &ge; 20 seeders alive</span>
                </div>

                <div>
                  <label className="block text-xs font-semibold text-slate-400 mb-1">
                    Required Condition Matches
                  </label>
                  <select
                    value={config.space_manager.default_required_match_count ?? 2}
                    onChange={(e) => setConfig({
                      ...config,
                      space_manager: { ...config.space_manager, default_required_match_count: Number(e.target.value) },
                    })}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  >
                    <option value={1}>Match 1 of 3 (Aggressive)</option>
                    <option value={2}>Match 2 of 3 (Balanced)</option>
                    <option value={3}>Match 3 of 3 (Conservative)</option>
                  </select>
                  <span className="text-[10px] text-slate-500 mt-0.5 block">Ratio + Age + Swarm</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Tab 7: System & Networking */}
      {tab === 'system' && (
        <div className="max-w-2xl space-y-5 rounded-2xl border border-slate-800 bg-slate-900/60 p-6">
          <h2 className="text-base font-bold text-slate-100 flex items-center space-x-2">
            <Cpu className="h-5 w-5 text-brand-400" />
            <span>System & Networking Configuration</span>
          </h2>

          <div>
            <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Dashboard Title</label>
            <input
              type="text"
              value={config.system.dashboard_title ?? ''}
              onChange={(e) => setConfig({
                ...config,
                system: { ...config.system, dashboard_title: e.target.value },
              })}
              placeholder="Conduit"
              className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
            />
            <span className="text-[10px] text-slate-500 mt-0.5 block">
              Shown in the top navbar and browser tab. Purely cosmetic — doesn't rename the app, just this instance's display name.
            </span>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Media Request Portal (optional)</label>
              <input
                type="text"
                value={config.system.request_portal_url ?? ''}
                onChange={(e) => setConfig({
                  ...config,
                  system: { ...config.system, request_portal_url: e.target.value || undefined },
                })}
                placeholder="https://requests.example.com"
                className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
              />
              <span className="text-[10px] text-slate-500 mt-0.5 block">
                Sidebar quick-link (e.g. Ombi/Overseerr/Jellyseerr). Hidden if left blank.
              </span>
            </div>
            <div>
              <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Request Portal Label</label>
              <input
                type="text"
                value={config.system.request_portal_label ?? ''}
                onChange={(e) => setConfig({
                  ...config,
                  system: { ...config.system, request_portal_label: e.target.value },
                })}
                placeholder="Media Requests"
                className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
              />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Media Playback Portal (optional)</label>
              <input
                type="text"
                value={config.system.media_portal_url ?? ''}
                onChange={(e) => setConfig({
                  ...config,
                  system: { ...config.system, media_portal_url: e.target.value || undefined },
                })}
                placeholder="https://watch.example.com"
                className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
              />
              <span className="text-[10px] text-slate-500 mt-0.5 block">
                Sidebar quick-link (e.g. Plex/Jellyfin/Emby). Hidden if left blank.
              </span>
            </div>
            <div>
              <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Media Portal Label</label>
              <input
                type="text"
                value={config.system.media_portal_label ?? ''}
                onChange={(e) => setConfig({
                  ...config,
                  system: { ...config.system, media_portal_label: e.target.value },
                })}
                placeholder="Media Portal"
                className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
              />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Bind Address</label>
              <input
                type="text"
                value={config.system.bind_addr}
                onChange={(e) => setConfig({
                  ...config,
                  system: { ...config.system, bind_addr: e.target.value },
                })}
                className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
              />
            </div>
            <div>
              <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Server Port</label>
              <input
                type="number"
                value={config.system.port}
                onChange={(e) => setConfig({
                  ...config,
                  system: { ...config.system, port: Number(e.target.value) },
                })}
                className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
              />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                Poller Interval (Seconds)
              </label>
              <input
                type="number"
                value={config.system.poll_interval_secs}
                onChange={(e) => setConfig({
                  ...config,
                  system: { ...config.system, poll_interval_secs: Number(e.target.value) },
                })}
                className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
              />
            </div>
            <div>
              <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Log Level</label>
              <select
                value={config.system.log_level}
                onChange={(e) => setConfig({
                  ...config,
                  system: { ...config.system, log_level: e.target.value },
                })}
                className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
              >
                <option value="trace">Trace (Most Verbose)</option>
                <option value="debug">Debug</option>
                <option value="info">Info (Standard)</option>
                <option value="warn">Warn</option>
                <option value="error">Error</option>
              </select>
            </div>
          </div>

          {/* HTTPS / TLS Server Security Configuration */}
          <div className="border-t border-slate-800 pt-6 space-y-4">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <h3 className="text-sm font-bold text-slate-200 flex items-center space-x-2">
                  <Lock className="h-4 w-4 text-emerald-400" />
                  <span>HTTPS / TLS Server Security</span>
                </h3>
                <p className="text-xs text-slate-400">
                  Enable native TLS termination for secure HTTPS and WSS WebSocket telemetry.
                </p>
              </div>
              <label className="relative inline-flex cursor-pointer items-center">
                <input
                  type="checkbox"
                  checked={config.system.ssl_enabled ?? false}
                  onChange={(e) => {
                    setConfig({
                      ...config,
                      system: { ...config.system, ssl_enabled: e.target.checked },
                    });
                  }}
                  className="peer sr-only"
                />
                <div className="peer h-6 w-11 rounded-full bg-slate-700 after:absolute after:top-[2px] after:left-[2px] after:h-5 after:w-5 after:rounded-full after:bg-white after:transition-all after:content-[''] peer-checked:bg-emerald-600 peer-checked:after:translate-x-full"></div>
              </label>
            </div>

            {(config.system.ssl_enabled ?? false) && (
              <div className="space-y-4 pt-2">
                <div>
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                    Certificate Chain File Path (.pem / .crt)
                  </label>
                  <input
                    type="text"
                    placeholder="/data/ssl/fullchain.pem"
                    value={config.system.ssl_cert ?? ''}
                    onChange={(e) => setConfig({
                      ...config,
                      system: { ...config.system, ssl_cert: e.target.value },
                    })}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 font-mono focus:border-brand-500 focus:outline-none"
                  />
                  <span className="text-[11px] text-slate-500 mt-1 block">
                    Absolute path to the full-chain certificate PEM file containing server cert & intermediate CA.
                  </span>
                </div>

                <div>
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                    Private Key File Path (.key / .pem)
                  </label>
                  <input
                    type="text"
                    placeholder="/data/ssl/privkey.pem"
                    value={config.system.ssl_key ?? ''}
                    onChange={(e) => setConfig({
                      ...config,
                      system: { ...config.system, ssl_key: e.target.value },
                    })}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 font-mono focus:border-brand-500 focus:outline-none"
                  />
                  <span className="text-[11px] text-slate-500 mt-1 block">
                    Absolute path to the unencrypted PEM private key file (RSA or PKCS#8).
                  </span>
                </div>

                <div className="rounded-xl border border-emerald-500/20 bg-emerald-500/5 p-3 flex items-start space-x-2.5">
                  <Shield className="h-4 w-4 text-emerald-400 shrink-0 mt-0.5" />
                  <div className="text-xs text-emerald-300 leading-relaxed">
                    <strong>Note:</strong> Changes to TLS certificates require saving settings and restarting the Conduit daemon service to bind the HTTPS socket.
                  </div>
                </div>
              </div>
            )}
          </div>

          {/* Network Listener Summary — Conduit has exactly one listening socket; there is no
              separate WS/WSS port to configure. WebSocket upgrades happen over whatever
              transport (plain or TLS) accepted the underlying connection, on this same port. */}
          <div className="border-t border-slate-800 pt-6 space-y-3">
            <h3 className="text-sm font-bold text-slate-200 flex items-center space-x-2">
              <Server className="h-4 w-4 text-sky-400" />
              <span>Network Listener</span>
            </h3>
            <div className="rounded-xl border border-slate-800 bg-slate-950/60 p-4 space-y-2 font-mono text-xs">
              <div className="flex items-center justify-between">
                <span className="text-slate-400">HTTP API + Web UI</span>
                <span className="text-slate-200">
                  {config.system.ssl_enabled ? 'https' : 'http'}://{config.system.bind_addr}:{config.system.port}
                </span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-slate-400">WebSocket Telemetry</span>
                <span className="text-slate-200">
                  {config.system.ssl_enabled ? 'wss' : 'ws'}://{config.system.bind_addr}:{config.system.port}/api/ws
                </span>
              </div>
            </div>
            <p className="text-[11px] text-slate-500 leading-relaxed font-sans">
              Conduit terminates TLS natively (via rustls) on this single port when HTTPS is enabled above — there is no separate WS/WSS port or config; the WebSocket upgrade automatically follows whichever scheme this port is already serving. Running a reverse proxy (nginx, Caddy, Traefik) in front is still fully supported: just leave HTTPS disabled here and let the proxy terminate TLS instead.
            </p>
          </div>

          {/* InfluxDB v2 Metrics Export */}
          <div className="border-t border-slate-800 pt-6 space-y-4">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <h3 className="text-sm font-bold text-slate-200 flex items-center space-x-2">
                  <Gauge className="h-4 w-4 text-purple-400" />
                  <span>InfluxDB v2 Metrics Export</span>
                </h3>
                <p className="text-xs text-slate-400">
                  Pushes per-node download/upload speed, torrent counts, and free space to an InfluxDB v2 bucket every 30s for use in Grafana/Chronograf dashboards.
                </p>
              </div>
              <label className="relative inline-flex cursor-pointer items-center">
                <input
                  type="checkbox"
                  checked={config.influx?.enabled ?? false}
                  onChange={(e) => setConfig({
                    ...config,
                    influx: { ...(config.influx ?? { enabled: false, host: 'localhost', port: 8086, org: '', bucket: '', token: '', use_ssl: false }), enabled: e.target.checked },
                  })}
                  className="peer sr-only"
                />
                <div className="peer h-6 w-11 rounded-full bg-slate-700 after:absolute after:top-[2px] after:left-[2px] after:h-5 after:w-5 after:rounded-full after:bg-white after:transition-all after:content-[''] peer-checked:bg-purple-600 peer-checked:after:translate-x-full"></div>
              </label>
            </div>

            {config.influx?.enabled && (
              <div className="space-y-4 pt-2">
                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Host</label>
                    <input
                      type="text"
                      value={config.influx.host}
                      placeholder="localhost"
                      onChange={(e) => setConfig({ ...config, influx: { ...config.influx, host: e.target.value } })}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-purple-500 focus:outline-none"
                    />
                  </div>
                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Port</label>
                    <input
                      type="number"
                      value={config.influx.port}
                      onChange={(e) => setConfig({ ...config, influx: { ...config.influx, port: Number(e.target.value) } })}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-purple-500 focus:outline-none"
                    />
                  </div>
                </div>
                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Organization</label>
                    <input
                      type="text"
                      value={config.influx.org}
                      onChange={(e) => setConfig({ ...config, influx: { ...config.influx, org: e.target.value } })}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-purple-500 focus:outline-none"
                    />
                  </div>
                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Bucket</label>
                    <input
                      type="text"
                      value={config.influx.bucket}
                      onChange={(e) => setConfig({ ...config, influx: { ...config.influx, bucket: e.target.value } })}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 focus:border-purple-500 focus:outline-none"
                    />
                  </div>
                </div>
                <div>
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">API Token</label>
                  <input
                    type="password"
                    value={config.influx.token}
                    onChange={(e) => setConfig({ ...config, influx: { ...config.influx, token: e.target.value } })}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 px-3 text-sm text-slate-200 font-mono focus:border-purple-500 focus:outline-none"
                  />
                </div>
                <label className="flex items-center space-x-2 text-xs text-slate-300 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={config.influx.use_ssl}
                    onChange={(e) => setConfig({ ...config, influx: { ...config.influx, use_ssl: e.target.checked } })}
                    className="rounded border-slate-700 bg-slate-800 text-purple-500 focus:ring-purple-500"
                  />
                  <span>Use HTTPS to reach InfluxDB</span>
                </label>
              </div>
            )}
          </div>

          {/* IP-to-ASN Peer Enrichment */}
          <div className="border-t border-slate-800 pt-6 space-y-4">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <h3 className="text-sm font-bold text-slate-200 flex items-center space-x-2">
                  <Globe className="h-4 w-4 text-emerald-400" />
                  <span>Peer IP Enrichment (Country & ASN)</span>
                </h3>
                <p className="text-xs text-slate-400 max-w-xl">
                  Shows each peer's country and network operator in the torrent detail view, using iptoasn.com's free database (no account/license required, unlike MaxMind). Downloaded once and refreshed weekly.
                </p>
              </div>
              <label className="relative inline-flex cursor-pointer items-center shrink-0 ml-4">
                <input
                  type="checkbox"
                  checked={config.ip_asn?.enabled ?? false}
                  onChange={(e) => setConfig({ ...config, ip_asn: { enabled: e.target.checked } })}
                  className="peer sr-only"
                />
                <div className="peer h-6 w-11 rounded-full bg-slate-700 after:absolute after:top-[2px] after:left-[2px] after:h-5 after:w-5 after:rounded-full after:bg-white after:transition-all after:content-[''] peer-checked:bg-emerald-600 peer-checked:after:translate-x-full"></div>
              </label>
            </div>
            {config.ip_asn?.enabled && (
              <div className="rounded-xl border border-amber-500/20 bg-amber-500/5 p-3 text-xs text-amber-300">
                Loads roughly 50-60MB of IP range data into memory once enabled and Conduit is restarted or the weekly refresh runs. Disable this if memory footprint matters more than peer geolocation on your setup.
              </div>
            )}
          </div>

        </div>
      )}

      {/* Tab 8: API Tokens */}
      {tab === 'tokens' && (
        <div className="max-w-3xl space-y-6">
          <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-4">
            <h2 className="text-base font-bold text-slate-200">Create Scoped API Token</h2>
            <p className="text-xs text-slate-400">
              Generate isolated API tokens for fetcher scripts, HomeAssistant, or remote sync nodes.
            </p>

            <div className="space-y-3">
              <input
                type="text"
                value={newTokenName}
                onChange={(e) => setNewTokenName(e.target.value)}
                placeholder="Token name (e.g. Pacoli-Fetcher, Copy2Queue Hook)"
                className="w-full rounded-xl border border-slate-700 bg-slate-800 py-2 px-4 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
              />

              <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
                <label
                  onClick={() => setNewTokenScope('fetcher')}
                  className={`p-3 rounded-xl border cursor-pointer transition-colors ${
                    newTokenScope === 'fetcher'
                      ? 'border-indigo-500/80 bg-indigo-500/10 text-white'
                      : 'border-slate-800 bg-slate-950/40 text-slate-400 hover:border-slate-700'
                  }`}
                >
                  <div className="font-semibold text-xs text-indigo-300">📡 Fetcher / Inbound Only</div>
                  <div className="text-[11px] text-slate-400 mt-1 leading-snug">
                    Can only classify files & notify downloads (fetcher hook)
                  </div>
                </label>

                <label
                  onClick={() => setNewTokenScope('read')}
                  className={`p-3 rounded-xl border cursor-pointer transition-colors ${
                    newTokenScope === 'read'
                      ? 'border-sky-500/80 bg-sky-500/10 text-white'
                      : 'border-slate-800 bg-slate-950/40 text-slate-400 hover:border-slate-700'
                  }`}
                >
                  <div className="font-semibold text-xs text-sky-300">👁️ Read Only</div>
                  <div className="text-[11px] text-slate-400 mt-1 leading-snug">
                    Telemetry, system health & status monitoring
                  </div>
                </label>

                <label
                  onClick={() => setNewTokenScope('*')}
                  className={`p-3 rounded-xl border cursor-pointer transition-colors ${
                    newTokenScope === '*'
                      ? 'border-amber-500/80 bg-amber-500/10 text-white'
                      : 'border-slate-800 bg-slate-950/40 text-slate-400 hover:border-slate-700'
                  }`}
                >
                  <div className="font-semibold text-xs text-amber-300">⚡ Full Admin (*)</div>
                  <div className="text-[11px] text-slate-400 mt-1 leading-snug">
                    Full read/write access to settings, users & controls
                  </div>
                </label>
              </div>

              <div className="flex justify-end pt-2">
                <button
                  onClick={handleCreateToken}
                  className="rounded-xl bg-brand-600 px-5 py-2 text-sm font-semibold text-white hover:bg-brand-500 cursor-pointer"
                >
                  Generate Scoped Token
                </button>
              </div>
            </div>

            {newRawToken && (
              <div className="rounded-xl bg-emerald-500/10 border border-emerald-500/30 p-4 space-y-2">
                <p className="text-xs font-semibold uppercase tracking-wider text-emerald-400">
                  Save this token now — it will not be shown again:
                </p>
                <code className="block rounded-lg bg-slate-950 p-2.5 text-xs font-mono text-emerald-300 select-all break-all">
                  {newRawToken}
                </code>
              </div>
            )}
          </div>

          <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-4">
            <h2 className="text-base font-bold text-slate-200">Active API Tokens</h2>
            <div className="divide-y divide-slate-800">
              {tokens.length === 0 ? (
                <p className="text-sm text-slate-500 py-3">No active API tokens.</p>
              ) : (
                tokens.map((tok) => (
                  <div key={tok.id} className="py-3 flex items-center justify-between">
                    <div>
                      <div className="font-semibold text-sm text-slate-200">{tok.name}</div>
                      <div className="text-xs text-slate-400 font-mono">
                        Created: {new Date(tok.created_at).toLocaleDateString()} &bull; Scopes: {tok.scopes.join(', ')}
                      </div>
                    </div>
                    <button
                      onClick={() => handleDeleteToken(tok.id)}
                      className="rounded-lg p-1.5 text-rose-400 hover:bg-rose-950/40 hover:text-rose-300"
                      title="Revoke Token"
                    >
                      <Trash2 className="h-4 w-4" />
                    </button>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      )}

      {/* Tab 9: Backup & Vault */}
      {tab === 'backup' && (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6 max-w-4xl">
          <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-4">
            <h2 className="text-base font-bold text-slate-200 flex items-center space-x-2">
              <Download className="h-5 w-5 text-brand-400" />
              <span>Export Encrypted Backup</span>
            </h2>
            <p className="text-sm text-slate-400">
              Download your complete Conduit configuration, encrypted with AES-256-GCM and Argon2id.
            </p>
            <div>
              <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                Passphrase (Required, 8+ characters)
              </label>
              <input
                type="password"
                value={backupPass}
                onChange={(e) => setBackupPass(e.target.value)}
                placeholder="A passphrase is required — the backup contains every API key and secret in plaintext once decrypted"
                className="w-full rounded-xl border border-slate-700 bg-slate-800 py-2 px-4 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
              />
            </div>
            <button
              onClick={handleExportBackup}
              className="w-full flex items-center justify-center space-x-2 rounded-xl bg-brand-600 py-2.5 text-sm font-semibold text-white hover:bg-brand-500"
            >
              <Download className="h-4 w-4" />
              <span>Download Backup File</span>
            </button>
          </div>

          <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-4">
            <h2 className="text-base font-bold text-slate-200 flex items-center space-x-2">
              <Upload className="h-5 w-5 text-emerald-400" />
              <span>Restore Backup</span>
            </h2>
            <div>
              <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                Backup JSON Bundle
              </label>
              <textarea
                value={restoreBundle}
                onChange={(e) => setRestoreBundle(e.target.value)}
                rows={3}
                placeholder="Paste backup JSON content here..."
                className="w-full rounded-xl border border-slate-700 bg-slate-800 p-3 text-xs font-mono text-slate-200 focus:border-brand-500 focus:outline-none"
              />
            </div>
            <div>
              <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                Passphrase (If encrypted)
              </label>
              <input
                type="password"
                value={restorePass}
                onChange={(e) => setRestorePass(e.target.value)}
                className="w-full rounded-xl border border-slate-700 bg-slate-800 py-2 px-4 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
              />
            </div>
            <button
              onClick={handleRestoreBackup}
              className="w-full flex items-center justify-center space-x-2 rounded-xl bg-emerald-600 py-2.5 text-sm font-semibold text-white hover:bg-emerald-500"
            >
              <Upload className="h-4 w-4" />
              <span>Restore Configuration</span>
            </button>
          </div>
        </div>
      )}

      {/* Embedded Transmission Daemon Options Modal */}
      {editingDaemonNode && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4">
          <div className="w-full max-w-2xl rounded-2xl border border-slate-800 bg-slate-900 p-6 shadow-2xl space-y-5 animate-in zoom-in-95">
            <div className="flex items-center justify-between border-b border-slate-800 pb-3">
              <h3 className="text-base font-bold text-slate-100 flex items-center space-x-2">
                <Sliders className="h-5 w-5 text-brand-400" />
                <span>Fetcher Daemon RPC Options: {editingDaemonNode}</span>
              </h3>
              <button
                onClick={() => setEditingDaemonNode(null)}
                className="rounded p-1 text-slate-400 hover:text-white"
              >
                <X className="h-5 w-5" />
              </button>
            </div>

            {loadingSession ? (
              <div className="py-8 text-center text-slate-400">Loading daemon settings...</div>
            ) : !daemonSession ? (
              <div className="rounded-xl bg-rose-500/10 border border-rose-500/20 p-4 text-sm text-rose-400">
                Could not connect to fetcher daemon on "{editingDaemonNode}". Verify host and port.
              </div>
            ) : (
              <div className="space-y-4 max-h-[70vh] overflow-y-auto pr-1">
                {/* Download Directory */}
                <div>
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                    Default Download Directory
                  </label>
                  <input
                    type="text"
                    value={daemonSession['download-dir']}
                    onChange={(e) => setDaemonSession({ ...daemonSession, 'download-dir': e.target.value })}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                </div>

                {/* Speed limits */}
                <div className="grid grid-cols-2 gap-4 rounded-xl border border-slate-800 bg-slate-800/30 p-4">
                  <div className="space-y-2">
                    <label className="flex items-center space-x-2 text-xs font-semibold text-slate-300">
                      <input
                        type="checkbox"
                        checked={daemonSession['speed-limit-down-enabled']}
                        onChange={(e) => setDaemonSession({ ...daemonSession, 'speed-limit-down-enabled': e.target.checked })}
                        className="rounded border-slate-700 bg-slate-800 text-brand-600"
                      />
                      <span>Limit Download (KB/s)</span>
                    </label>
                    <input
                      type="number"
                      value={daemonSession['speed-limit-down']}
                      onChange={(e) => setDaemonSession({ ...daemonSession, 'speed-limit-down': e.target.value })}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1 px-3 text-sm text-slate-200"
                    />
                  </div>

                  <div className="space-y-2">
                    <label className="flex items-center space-x-2 text-xs font-semibold text-slate-300">
                      <input
                        type="checkbox"
                        checked={daemonSession['speed-limit-up-enabled']}
                        onChange={(e) => setDaemonSession({ ...daemonSession, 'speed-limit-up-enabled': e.target.checked })}
                        className="rounded border-slate-700 bg-slate-800 text-brand-600"
                      />
                      <span>Limit Upload (KB/s)</span>
                    </label>
                    <input
                      type="number"
                      value={daemonSession['speed-limit-up']}
                      onChange={(e) => setDaemonSession({ ...daemonSession, 'speed-limit-up': e.target.value })}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1 px-3 text-sm text-slate-200"
                    />
                  </div>
                </div>

                {/* Alternate Speed Limits */}
                <div className="rounded-xl border border-slate-800 bg-slate-800/30 p-4 space-y-3">
                  <label className="flex items-center space-x-2 text-xs font-semibold text-amber-400">
                    <input
                      type="checkbox"
                      checked={daemonSession['alt-speed-enabled']}
                      onChange={(e) => setDaemonSession({ ...daemonSession, 'alt-speed-enabled': e.target.checked })}
                      className="rounded border-slate-700 bg-slate-800 text-brand-600"
                    />
                    <span>Alternate Speed Limits (Turtle Mode)</span>
                  </label>

                  <div className="grid grid-cols-2 gap-4">
                    <div>
                      <label className="block text-[11px] text-slate-400 mb-1">Alt Down (KB/s)</label>
                      <input
                        type="number"
                        value={daemonSession['alt-speed-down']}
                        onChange={(e) => setDaemonSession({ ...daemonSession, 'alt-speed-down': e.target.value })}
                        className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1 px-3 text-sm text-slate-200"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-slate-400 mb-1">Alt Up (KB/s)</label>
                      <input
                        type="number"
                        value={daemonSession['alt-speed-up']}
                        onChange={(e) => setDaemonSession({ ...daemonSession, 'alt-speed-up': e.target.value })}
                        className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1 px-3 text-sm text-slate-200"
                      />
                    </div>
                  </div>
                </div>

                {/* Peer Port & Test */}
                <div className="rounded-xl border border-slate-800 bg-slate-800/30 p-4 space-y-3">
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider">
                    Peer Listening Port
                  </label>
                  <div className="flex items-center space-x-3">
                    <input
                      type="number"
                      value={daemonSession['peer-port']}
                      onChange={(e) => setDaemonSession({ ...daemonSession, 'peer-port': e.target.value })}
                      className="w-32 rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200"
                    />
                    <button
                      type="button"
                      onClick={handleTestPort}
                      disabled={testingPort}
                      className="rounded-lg bg-slate-800 px-4 py-1.5 text-xs font-semibold text-sky-400 hover:bg-slate-700"
                    >
                      {testingPort ? 'Testing...' : 'Test Peer Port'}
                    </button>
                    {portStatus !== null && (
                      <div className="flex items-center space-x-1.5 text-xs font-semibold">
                        {portStatus ? (
                          <span className="text-emerald-400 flex items-center space-x-1">
                            <CheckCircle2 className="h-4 w-4" />
                            <span>Port Open</span>
                          </span>
                        ) : (
                          <span className="text-rose-400 flex items-center space-x-1">
                            <XCircle className="h-4 w-4" />
                            <span>Port Closed</span>
                          </span>
                        )}
                      </div>
                    )}
                  </div>
                </div>

                {/* IP Blocklist Settings */}
                <div className="rounded-xl border border-slate-800 bg-slate-800/30 p-4 space-y-3">
                  <div className="flex items-center justify-between">
                    <label className="flex items-center space-x-2 text-xs font-semibold text-sky-400">
                      <input
                        type="checkbox"
                        checked={daemonSession['blocklist-enabled']}
                        onChange={(e) => setDaemonSession({ ...daemonSession, 'blocklist-enabled': e.target.checked })}
                        className="rounded border-slate-700 bg-slate-800 text-brand-600"
                      />
                      <span>Enable IP Blocklist</span>
                    </label>
                    <button
                      type="button"
                      onClick={async () => {
                        if (!editingDaemonNode) return;
                        try {
                          const res = await updateNodeBlocklist(editingDaemonNode);
                          alert(`🛡️ ${res.message}`);
                        } catch (err: any) {
                          alert(`Blocklist update failed: ${err.message}`);
                        }
                      }}
                      className="px-3 py-1 rounded bg-slate-800 text-xs font-semibold text-slate-200 hover:bg-slate-700 hover:text-brand-400 transition-colors"
                    >
                      Update Blocklist Now
                    </button>
                  </div>

                  <div>
                    <label className="block text-[11px] text-slate-400 mb-1">Blocklist URL (.bin or .gz list)</label>
                    <input
                      type="text"
                      placeholder="e.g. http://list.iblocklist.com/?list=bt_level1&fileformat=p2p&archiveformat=gz"
                      value={daemonSession['blocklist-url'] || ''}
                      onChange={(e) => setDaemonSession({ ...daemonSession, 'blocklist-url': e.target.value })}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-xs text-slate-200 font-mono focus:border-brand-500 focus:outline-none"
                    />
                  </div>
                </div>

                {/* Modal Footer */}
                <div className="flex items-center justify-end space-x-3 pt-3 border-t border-slate-800">
                  <button
                    type="button"
                    onClick={() => setEditingDaemonNode(null)}
                    className="rounded-lg px-4 py-2 text-sm text-slate-400 hover:bg-slate-800"
                  >
                    Cancel
                  </button>
                  <button
                    type="button"
                    onClick={handleSaveDaemonSession}
                    disabled={savingSession}
                    className="rounded-lg bg-brand-600 px-5 py-2 text-sm font-semibold text-white hover:bg-brand-500 disabled:opacity-50"
                  >
                    {savingSession ? 'Applying...' : 'Apply Changes'}
                  </button>
                </div>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
};
