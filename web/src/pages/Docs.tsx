// web/src/pages/Docs.tsx
import React, { useState } from 'react';
import { 
  BookOpen, Tv, Film, RotateCcw, HardDrive, FolderSync, 
  Tv2, KeyRound, Shield, Check, Copy, AlertTriangle, ArrowRight, Zap
} from 'lucide-react';

export const Docs: React.FC = () => {
  const [activeSection, setActiveSection] = useState<
    'overview' | 'arr' | 'replacer' | 'space' | 'sync' | 'plex_trakt' | 'security'
  >('overview');
  const [copiedUrl, setCopiedUrl] = useState<string | null>(null);

  const copyToClipboard = (text: string, id: string) => {
    navigator.clipboard.writeText(text);
    setCopiedUrl(id);
    setTimeout(() => setCopiedUrl(null), 2000);
  };

  const origin = window.location.origin;
  const sonarrWebhookUrl = `${origin}/api/sonarr/inbound`;
  const radarrWebhookUrl = `${origin}/api/radarr/inbound`;

  const sections = [
    { id: 'overview', label: '1. Architecture & Overview', icon: BookOpen },
    { id: 'arr', label: '2. Sonarr & Radarr Webhooks', icon: Tv },
    { id: 'replacer', label: '3. MediaReplacer & Auto Re-Search', icon: RotateCcw },
    { id: 'space', label: '4. Space Management & Scoring', icon: HardDrive },
    { id: 'sync', label: '5. File Sync & Staging Queues', icon: FolderSync },
    { id: 'plex_trakt', label: '6. Plex & Trakt Sync', icon: Tv2 },
    { id: 'security', label: '7. Encrypted Vault & Security', icon: Shield },
  ];

  return (
    <div className="flex-1 flex overflow-hidden bg-slate-950 text-slate-100">
      {/* Table of Contents Sidebar */}
      <aside className="w-64 border-r border-slate-800/80 bg-slate-900/40 p-6 flex flex-col space-y-2 overflow-y-auto">
        <div className="flex items-center space-x-2 text-brand-400 mb-4 px-2">
          <BookOpen className="h-5 w-5" />
          <span className="font-bold text-sm tracking-wide uppercase">Conduit Manual</span>
        </div>

        {sections.map((sec) => {
          const Icon = sec.icon;
          const isActive = activeSection === sec.id;
          return (
            <button
              key={sec.id}
              onClick={() => setActiveSection(sec.id as any)}
              className={`flex items-center space-x-3 rounded-xl px-3 py-2.5 text-xs font-semibold transition-all text-left ${
                isActive
                  ? 'bg-brand-600/20 text-brand-300 border border-brand-500/30'
                  : 'text-slate-400 hover:bg-slate-800/60 hover:text-slate-200'
              }`}
            >
              <Icon className={`h-4 w-4 shrink-0 ${isActive ? 'text-brand-400' : 'text-slate-500'}`} />
              <span className="truncate">{sec.label}</span>
            </button>
          );
        })}
      </aside>

      {/* Main Documentation Content */}
      <main className="flex-1 overflow-y-auto p-8 lg:p-12 space-y-8 max-w-5xl">
        {/* 1. Architecture & Overview */}
        {activeSection === 'overview' && (
          <section className="space-y-6">
            <div>
              <span className="text-xs font-bold text-brand-400 uppercase tracking-wider">Getting Started</span>
              <h1 className="text-3xl font-extrabold text-slate-100 mt-1">Conduit System Architecture</h1>
              <p className="text-sm text-slate-400 mt-2 leading-relaxed">
                Conduit is a unified high-performance media orchestrator and BitTorrent swarm commander written in Rust. It coordinates multi-daemon seedboxes (Synapse, Transmission, qBittorrent, Deluge), automated media lifecycle pipelines, file staging queues, and metadata synchronization across your ecosystem.
              </p>
            </div>

            <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-4">
              <h3 className="text-base font-bold text-slate-200 flex items-center space-x-2">
                <span>Core Subsystems</span>
              </h3>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div className="rounded-xl border border-slate-800/80 bg-slate-950/60 p-4 space-y-2">
                  <span className="text-xs font-bold text-sky-400">Multi-Daemon Fetcher Pool</span>
                  <p className="text-xs text-slate-400 leading-relaxed">
                    Maintains persistent connections to unlimited Synapse, Transmission, qBittorrent, and Deluge seedboxes/instances, pooling torrents into unified compound IDs (<code className="text-sky-300">node:id</code>).
                  </p>
                </div>
                <div className="rounded-xl border border-slate-800/80 bg-slate-950/60 p-4 space-y-2">
                  <span className="text-xs font-bold text-emerald-400">MediaReplacer Engine</span>
                  <p className="text-xs text-slate-400 leading-relaxed">
                    Tracks grabs from Sonarr & Radarr. When a torrent is unregistered or deleted on a tracker, Conduit triggers an automated re-search via Sonarr/Radarr API and cleans up the dead download.
                  </p>
                </div>
                <div className="rounded-xl border border-slate-800/80 bg-slate-950/60 p-4 space-y-2">
                  <span className="text-xs font-bold text-purple-400">Space Management & Auto-Purge</span>
                  <p className="text-xs text-slate-400 leading-relaxed">
                    Monitors disk headroom across nodes. When free storage falls below threshold, it purges qualifying torrents based on multi-factor ratio, age, and seeder metrics.
                  </p>
                </div>
                <div className="rounded-xl border border-slate-800/80 bg-slate-950/60 p-4 space-y-2">
                  <span className="text-xs font-bold text-amber-400">File Staging & Queue Hardlinker</span>
                  <p className="text-xs text-slate-400 leading-relaxed">
                    Processes incoming staging directories (<code className="text-amber-300">tv_pre</code>, <code className="text-amber-300">movie_pre</code>), creates instant hardlinks (<code className="text-amber-300">cp -al</code>) into post queues, and prunes stale folders.
                  </p>
                </div>
              </div>
            </div>
          </section>
        )}

        {/* 2. Sonarr & Radarr Webhooks & Centralized Notification Hub */}
        {activeSection === 'arr' && (
          <section className="space-y-6">
            <div>
              <span className="text-xs font-bold text-sky-400 uppercase tracking-wider">Integration & Notification Hub</span>
              <h1 className="text-3xl font-extrabold text-slate-100 mt-1">Sonarr & Radarr Centralized Lifecycle</h1>
              <p className="text-sm text-slate-400 mt-2 leading-relaxed">
                Connect your Sonarr and Radarr instances directly to Conduit. Conduit bridges release grabs, fetcher downloads, staging queues, and library imports into a unified timeline—and acts as the <strong>Single Source of Truth</strong> for all your Mattermost and Discord notifications.
              </p>
            </div>

            <div className="rounded-2xl border border-brand-500/30 bg-brand-950/20 p-5 text-xs text-slate-200 space-y-2">
              <div className="flex items-center space-x-2 text-brand-400 font-bold">
                <Zap className="h-4 w-4" />
                <span>Consolidated Notification Architecture</span>
              </div>
              <p className="text-slate-300">
                You can <strong>turn off Mattermost/Discord/Email notifications inside Sonarr, Radarr, and fetcher daemons</strong>. Once connected to Conduit, all events across your entire cluster are deduplicated, enriched with node & ratio telemetry, and dispatched through Conduit's notification engine.
              </p>
            </div>

            <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-4">
              <h3 className="text-sm font-bold text-slate-200 uppercase tracking-wider">Step 1: Get Inbound Webhook Endpoints</h3>

              <div className="space-y-3">
                <div>
                  <label className="block text-xs font-semibold text-sky-400 mb-1">Sonarr Inbound Webhook URL</label>
                  <div className="flex items-center space-x-2">
                    <input
                      type="text"
                      readOnly
                      value={sonarrWebhookUrl}
                      className="flex-1 rounded-lg border border-slate-700 bg-slate-950 py-2 px-3 text-xs font-mono text-slate-300 select-all"
                    />
                    <button
                      onClick={() => copyToClipboard(sonarrWebhookUrl, 'sonarr')}
                      className="flex items-center space-x-1 rounded-lg bg-sky-600 px-3 py-2 text-xs font-semibold text-white hover:bg-sky-500"
                    >
                      {copiedUrl === 'sonarr' ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
                      <span>{copiedUrl === 'sonarr' ? 'Copied' : 'Copy'}</span>
                    </button>
                  </div>
                </div>

                <div>
                  <label className="block text-xs font-semibold text-amber-400 mb-1">Radarr Inbound Webhook URL</label>
                  <div className="flex items-center space-x-2">
                    <input
                      type="text"
                      readOnly
                      value={radarrWebhookUrl}
                      className="flex-1 rounded-lg border border-slate-700 bg-slate-950 py-2 px-3 text-xs font-mono text-slate-300 select-all"
                    />
                    <button
                      onClick={() => copyToClipboard(radarrWebhookUrl, 'radarr')}
                      className="flex items-center space-x-1 rounded-lg bg-amber-600 px-3 py-2 text-xs font-semibold text-white hover:bg-amber-500"
                    >
                      {copiedUrl === 'radarr' ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
                      <span>{copiedUrl === 'radarr' ? 'Copied' : 'Copy'}</span>
                    </button>
                  </div>
                </div>
              </div>
            </div>

            <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-4">
              <h3 className="text-sm font-bold text-slate-200 uppercase tracking-wider">Step 2: Add Webhook in Sonarr / Radarr</h3>
              <ol className="list-decimal list-inside space-y-2 text-xs text-slate-300 leading-relaxed">
                <li>In Sonarr or Radarr, go to <strong className="text-slate-100">Settings &rarr; Connect</strong>.</li>
                <li>Click the <strong className="text-slate-100">+ (Add)</strong> button and select <strong className="text-slate-100">Webhook</strong>.</li>
                <li>Set Name to <strong className="text-brand-400">Conduit Media Hub</strong>.</li>
                <li>Paste the respective URL from above into the <strong className="text-slate-100">URL</strong> field.</li>
                <li>Set Method to <strong className="text-slate-100">POST</strong>.</li>
                <li>Check the following event triggers:
                  <ul className="list-disc list-inside ml-4 mt-1 space-y-1 text-slate-400">
                    <li><strong className="text-emerald-400">On Grab</strong> (Records release & links torrent hash to TV/Movie ID)</li>
                    <li><strong className="text-emerald-400">On Download / On Import</strong> (Marks file as imported & refreshes Plex)</li>
                    <li><strong className="text-emerald-400">On Upgrade</strong></li>
                    <li><strong className="text-emerald-400">On File Delete</strong></li>
                  </ul>
                </li>
                <li>Click <strong className="text-slate-100">Save</strong>.</li>
              </ol>
            </div>

            <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-3">
              <h3 className="text-sm font-bold text-slate-200 uppercase tracking-wider">Step 3: Primary Configuration & API Keys</h3>
              <p className="text-xs text-slate-300 leading-relaxed">
                Navigate to <strong className="text-brand-400">Conduit Settings &rarr; Sonarr & Radarr</strong> and enter the Primary instance URL (e.g. <code className="text-sky-300">http://192.168.1.100:8989</code>) and API Key.
              </p>
              <div className="rounded-xl bg-amber-500/10 border border-amber-500/20 p-3 text-xs text-amber-300 flex items-start space-x-2">
                <AlertTriangle className="h-4 w-4 shrink-0 mt-0.5" />
                <span>
                  <strong>What if no Primary is configured?</strong> Conduit will still clean up dead torrents according to your rules and record the event in the System Audit Log, but automated re-search triggers will be safely skipped.
                </span>
              </div>
            </div>
          </section>
        )}

        {/* 3. MediaReplacer & Auto Re-Search */}
        {activeSection === 'replacer' && (
          <section className="space-y-6">
            <div>
              <span className="text-xs font-bold text-emerald-400 uppercase tracking-wider">Automation Pipeline</span>
              <h1 className="text-3xl font-extrabold text-slate-100 mt-1">MediaReplacer & Error Pipeline</h1>
              <p className="text-sm text-slate-400 mt-2 leading-relaxed">
                How Conduit detects unregistered, deleted, or trumped torrents and triggers instant automated re-search in Sonarr/Radarr.
              </p>
            </div>

            <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-4">
              <h3 className="text-sm font-bold text-slate-200 uppercase tracking-wider">Lifecycle Sequence</h3>
              <div className="space-y-4 text-xs">
                <div className="flex items-start space-x-3 p-3 rounded-xl bg-slate-950/60 border border-slate-800">
                  <div className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-sky-500/20 text-sky-400 font-bold text-xs">1</div>
                  <div>
                    <h4 className="font-bold text-slate-200">Grab Recorded</h4>
                    <p className="text-slate-400 mt-0.5">Sonarr/Radarr sends <code className="text-sky-300">On Grab</code> webhook with torrent infohash, release title, and series/episode/movie ID. Conduit indexes this in the SQLCipher database.</p>
                  </div>
                </div>

                <div className="flex items-start space-x-3 p-3 rounded-xl bg-slate-950/60 border border-slate-800">
                  <div className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-amber-500/20 text-amber-400 font-bold text-xs">2</div>
                  <div>
                    <h4 className="font-bold text-slate-200">Tracker Error Detection</h4>
                    <p className="text-slate-400 mt-0.5">Conduit's background poller inspects tracker error strings against your customizable regular expression (<code className="text-amber-300">(?i)unregistered|torrent deleted|not found</code>).</p>
                  </div>
                </div>

                <div className="flex items-start space-x-3 p-3 rounded-xl bg-slate-950/60 border border-slate-800">
                  <div className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-emerald-500/20 text-emerald-400 font-bold text-xs">3</div>
                  <div>
                    <h4 className="font-bold text-slate-200">Automated Re-Search Command</h4>
                    <p className="text-slate-400 mt-0.5">Conduit sends an API command to the configured Primary instance:
                      <br /><code className="text-emerald-300">POST /api/v3/command &#123; name: "EpisodeSearch", episodeIds: [...] &#125;</code> (Sonarr)
                      <br /><code className="text-emerald-300">POST /api/v3/command &#123; name: "MoviesSearch", movieIds: [...] &#125;</code> (Radarr)
                    </p>
                  </div>
                </div>

                <div className="flex items-start space-x-3 p-3 rounded-xl bg-slate-950/60 border border-slate-800">
                  <div className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-purple-500/20 text-purple-400 font-bold text-xs">4</div>
                  <div>
                    <h4 className="font-bold text-slate-200">Dead Torrent Cleaned & Dispatched</h4>
                    <p className="text-slate-400 mt-0.5">The dead download is purged from the fetcher node with files deleted, and an alert is broadcast to your configured Mattermost/Discord channels.</p>
                  </div>
                </div>
              </div>
            </div>
          </section>
        )}

        {/* 4. Space Management & Scoring */}
        {activeSection === 'space' && (
          <section className="space-y-6">
            <div>
              <span className="text-xs font-bold text-purple-400 uppercase tracking-wider">Disk Headroom Maintenance</span>
              <h1 className="text-3xl font-extrabold text-slate-100 mt-1">Space Management & Multi-Factor Scoring</h1>
              <p className="text-sm text-slate-400 mt-2 leading-relaxed">
                Conduit continuously safeguards disk headroom across all connected fetcher daemons.
              </p>
            </div>

            <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-4">
              <h3 className="text-sm font-bold text-slate-200 uppercase tracking-wider">Multi-Factor Eligibility Formula</h3>
              <p className="text-xs text-slate-300 leading-relaxed">
                When a node's available disk space drops below <strong className="text-slate-100">Min Free Space (GB)</strong>, Conduit evaluates completed seeding torrents against 3 criteria:
              </p>

              <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                <div className="p-3 rounded-xl bg-slate-950/60 border border-slate-800 space-y-1">
                  <span className="text-xs font-bold text-sky-400">1. Target Seed Ratio</span>
                  <p className="text-xs text-slate-400">Torrent upload ratio &ge; <strong className="text-slate-200">Target Ratio</strong> (e.g. 2.0).</p>
                </div>
                <div className="p-3 rounded-xl bg-slate-950/60 border border-slate-800 space-y-1">
                  <span className="text-xs font-bold text-emerald-400">2. Seeding Age</span>
                  <p className="text-xs text-slate-400">Days since download completed &ge; <strong className="text-slate-200">Target Age Days</strong> (e.g. 14).</p>
                </div>
                <div className="p-3 rounded-xl bg-slate-950/60 border border-slate-800 space-y-1">
                  <span className="text-xs font-bold text-purple-400">3. Swarm Seeder Pool</span>
                  <p className="text-xs text-slate-400">Tracker reports &ge; <strong className="text-slate-200">Target Seeders</strong> (e.g. 20) active.</p>
                </div>
              </div>

              <div className="rounded-xl bg-slate-950/80 border border-slate-800 p-4 text-xs text-slate-300 space-y-2">
                <p><strong>Required Match Count:</strong> Specifies whether 1, 2, or all 3 conditions must be satisfied before a torrent becomes eligible for purge.</p>
                <p>Eligible torrents are purged in order of highest ratio first, up to the <strong className="text-slate-100">Max Purges Per Cycle</strong> safety cap.</p>
              </div>
            </div>
          </section>
        )}

        {/* 5. File Sync & Staging Queues */}
        {activeSection === 'sync' && (
          <section className="space-y-6">
            <div>
              <span className="text-xs font-bold text-amber-400 uppercase tracking-wider">Storage Automation & Hooks</span>
              <h1 className="text-3xl font-extrabold text-slate-100 mt-1">Fetcher Staging & Conduit Intake Hook</h1>
              <p className="text-sm text-slate-400 mt-2 leading-relaxed">
                Automate high-speed hardlink synchronization from fetcher nodes into your Arr queue folders using Conduit's centralized classifier and notification hooks.
              </p>
            </div>

            <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-4">
              <h3 className="text-sm font-bold text-slate-200 uppercase tracking-wider">Fetcher Post-Download Hook (`conduit-fetch-hook`)</h3>
              <p className="text-xs text-slate-300 leading-relaxed">
                Fetcher nodes execute <code className="text-amber-300">conduit-fetch-hook.sh</code> (or <code className="text-amber-300">conduit-fetch-hook.py</code> / <code className="text-amber-300">conduit-fetch-hook.pl</code>) automatically upon completing a torrent. The script queries Conduit's <code className="text-sky-300">/api/sync/classify</code> endpoint, determines whether the release is Music, HD, 4K/UHD, Movie, or TV based on rules and grab history, hardlinks it to the queue directory, and reports back to Conduit.
              </p>

              <div className="rounded-xl bg-slate-950/80 border border-slate-800 p-4 space-y-2 text-xs">
                <span className="font-semibold text-slate-200">Transmission settings.json Configuration Example:</span>
                <pre className="p-3 rounded-lg bg-slate-900 text-slate-200 font-mono text-[11px] overflow-x-auto border border-slate-800">
{`"script-torrent-done-enabled": true,
"script-torrent-done-filename": "/opt/transmission/conduit-fetch-hook.sh"`}
                </pre>
              </div>

              <div className="space-y-2 text-xs text-slate-300">
                <span className="font-semibold text-slate-200">Supported Environment Variables:</span>
                <ul className="list-disc list-inside space-y-1 text-slate-400 font-mono text-[11px]">
                  <li><code>CONDUIT_URL</code>: Base URL for Conduit (default: <code>http://127.0.0.1:4242</code>)</li>
                  <li><code>CONDUIT_API_KEY</code>: Optional API token for secured clusters (e.g. <code>cnd_...</code>)</li>
                  <li><code>CONDUIT_NODE_NAME</code>: Explicit node identifier (e.g. <code>main - general</code>)</li>
                  <li><code>CONDUIT_DEBUG</code>: Enable verbose debugging</li>
                </ul>
              </div>
            </div>

            <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-4">
              <h3 className="text-sm font-bold text-slate-200 uppercase tracking-wider">Continuous Queue Cleaner Engine</h3>
              <p className="text-xs text-slate-300 leading-relaxed">
                Conduit's background <strong className="text-slate-100">FileSync Engine</strong> regularly sweeps staging directories, prunes empty or imported folders older than the configured threshold (<strong className="text-amber-400">Clean Queue Days</strong>), and maintains clean storage partitions.
              </p>
            </div>
          </section>
        )}

        {/* 6. Plex & Trakt Sync */}
        {activeSection === 'plex_trakt' && (
          <section className="space-y-6">
            <div>
              <span className="text-xs font-bold text-rose-400 uppercase tracking-wider">Ecosystem Synchronization</span>
              <h1 className="text-3xl font-extrabold text-slate-100 mt-1">Plex & Trakt Watch Sync</h1>
              <p className="text-sm text-slate-400 mt-2 leading-relaxed">
                Connect multiple Plex Media Servers and Trakt.tv accounts for automated metadata refresh and watch scrobbling.
              </p>
            </div>

            <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-4">
              <h3 className="text-sm font-bold text-slate-200 uppercase tracking-wider">Features</h3>
              <div className="space-y-3 text-xs text-slate-300">
                <div className="p-3 rounded-xl bg-slate-950/60 border border-slate-800 space-y-1">
                  <strong className="text-slate-100">Multi-Plex Server Refresh:</strong> Trigger library section scans across multiple Plex servers on demand or automatically after file sync.
                </div>
                <div className="p-3 rounded-xl bg-slate-950/60 border border-slate-800 space-y-1">
                  <strong className="text-slate-100">Trakt OAuth Scrobbler:</strong> Sync watch states, scrobbles, and history between Trakt.tv and your media library.
                </div>
              </div>
            </div>
          </section>
        )}

        {/* 7. Encrypted Vault & Security */}
        {activeSection === 'security' && (
          <section className="space-y-6">
            <div>
              <span className="text-xs font-bold text-emerald-400 uppercase tracking-wider">Cryptography & Protection</span>
              <h1 className="text-3xl font-extrabold text-slate-100 mt-1">Encrypted Database & Vault</h1>
              <p className="text-sm text-slate-400 mt-2 leading-relaxed">
                Conduit protects your credentials, API keys, and session data with enterprise-grade encryption.
              </p>
            </div>

            <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-4">
              <h3 className="text-sm font-bold text-slate-200 uppercase tracking-wider">Security Architecture</h3>
              <ul className="list-disc list-inside space-y-2 text-xs text-slate-300 leading-relaxed">
                <li><strong className="text-emerald-400">SQLCipher Database:</strong> The entire SQLite database (<code className="text-slate-200">data/db.sqlite</code>) is encrypted at rest using 256-bit AES-GCM.</li>
                <li><strong className="text-emerald-400">Encrypted Vault Exports:</strong> Configuration backups are encrypted using Argon2id key derivation + AES-256-GCM.</li>
                <li><strong className="text-emerald-400">Live Database Rekeying:</strong> Rotate your database passphrase at any time from <strong className="text-brand-400">Conduit Settings &rarr; Vault & Backup</strong>.</li>
                <li><strong className="text-emerald-400">Scoped API Tokens:</strong> Generate granular tokens (<code className="text-slate-200">cnd_...</code>) with specific permissions for automation.</li>
              </ul>
            </div>
          </section>
        )}
      </main>
    </div>
  );
};
