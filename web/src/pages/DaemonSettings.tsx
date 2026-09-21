// web/src/pages/DaemonSettings.tsx
import React, { useState, useEffect } from 'react';
import { Sliders, CheckCircle2, XCircle, Gauge, Radio, Folder, Shield, Save } from 'lucide-react';
import { fetchNodes, fetchNodeSession, updateNodeSession, testNodePort } from '../services/api';
import { useNodeCapabilities } from '../hooks/useNodeCapabilities';
import type { NodeStats } from '../types';

export const DaemonSettings: React.FC = () => {
  const { can } = useNodeCapabilities();
  const [nodes, setNodes] = useState<NodeStats[]>([]);
  const [selectedNode, setSelectedNode] = useState('');
  const [session, setSession] = useState<any>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [portStatus, setPortStatus] = useState<boolean | null>(null);
  const [testingPort, setTestingPort] = useState(false);
  const [statusMsg, setStatusMsg] = useState('');

  useEffect(() => {
    fetchNodes().then((nodeList) => {
      setNodes(nodeList);
      if (nodeList.length > 0) {
        setSelectedNode(nodeList[0].node);
      }
      setLoading(false);
    });
  }, []);


  useEffect(() => {
    if (selectedNode) {
      loadNodeSession(selectedNode);
    } else {
      setSession(null);
      setLoading(false);
    }
  }, [selectedNode]);

  const loadNodeSession = async (name: string) => {
    setLoading(true);
    setPortStatus(null);
    try {
      const sess = await fetchNodeSession(name);
      setSession(sess);
    } catch (e) {
      setSession(null);
    } finally {
      setLoading(false);
    }
  };

  const handleSave = async () => {
    if (!selectedNode || !session) return;
    setSaving(true);
    setStatusMsg('');
    try {
      // Only send what this node reported, so a daemon is never told about settings it doesn't have.
      const numeric = ['speed-limit-down', 'speed-limit-up', 'alt-speed-down', 'alt-speed-up', 'peer-limit-global', 'peer-limit-per-torrent', 'peer-port'];
      const passthrough = ['download-dir', 'speed-limit-down-enabled', 'speed-limit-up-enabled', 'alt-speed-enabled', 'dht-enabled', 'pex-enabled', 'lpd-enabled', 'utp-enabled', 'synapse-dht-read-only', 'synapse-zeroconf', 'synapse-announce-ip'];
      const payload: Record<string, unknown> = {};
      numeric.forEach((k) => { if (session[k] !== undefined) payload[k] = Number(session[k]); });
      passthrough.forEach((k) => { if (session[k] !== undefined) payload[k] = session[k]; });
      await updateNodeSession(selectedNode, payload);
      setStatusMsg('Daemon settings updated successfully!');
      setTimeout(() => setStatusMsg(''), 3000);
    } catch (err: any) {
      setStatusMsg(`Error: ${err.message}`);
    } finally {
      setSaving(false);
    }
  };

  const handleTestPort = async () => {
    if (!selectedNode) return;
    setTestingPort(true);
    setPortStatus(null);
    try {
      const open = await testNodePort(selectedNode);
      setPortStatus(open);
    } catch (e) {
      setPortStatus(false);
    } finally {
      setTestingPort(false);
    }
  };

  return (
    <div className="flex-1 overflow-auto p-8 space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-slate-100">Fetcher Daemon Settings</h1>
          <p className="text-sm text-slate-400 mt-1">Configure remote RPC daemon options, limits, and peer port tests.</p>
        </div>

        <div className="flex items-center space-x-4">
          <select
            value={selectedNode}
            onChange={(e) => setSelectedNode(e.target.value)}
            className="rounded-xl border border-slate-700 bg-slate-800 py-2 px-4 text-sm font-semibold text-slate-200 focus:border-brand-500 focus:outline-none"
          >
            {nodes.map((n) => (
              <option key={n.node} value={n.node}>
                Node: {n.node}{n.client_type === 'synapse' ? ' (Synapse)' : ''}
              </option>
            ))}
          </select>

          <button
            onClick={handleSave}
            disabled={saving || !session}
            className="flex items-center space-x-2 rounded-xl bg-brand-600 px-5 py-2 text-sm font-semibold text-white shadow-lg shadow-brand-500/20 hover:bg-brand-500 disabled:opacity-50"
          >
            <Save className="h-4 w-4" />
            <span>{saving ? 'Saving...' : 'Apply to Daemon'}</span>
          </button>
        </div>
      </div>

      {statusMsg && (
        <div className="rounded-xl bg-emerald-500/10 border border-emerald-500/20 p-3 text-sm text-emerald-400">
          {statusMsg}
        </div>
      )}

      {loading ? (
        <div className="p-8 text-slate-400">Loading daemon options...</div>
      ) : !session ? (
        <div className="rounded-2xl border border-rose-900/40 bg-rose-950/20 p-6 text-rose-400">
          Could not communicate with node "{selectedNode}". Verify host and port in Conduit Settings.
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6 max-w-5xl">
          {/* Bandwidth Limits */}
          <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-4">
            <h2 className="text-base font-bold text-slate-200 flex items-center space-x-2">
              <Gauge className="h-5 w-5 text-brand-400" />
              <span>Bandwidth Limits (KB/s)</span>
            </h2>

            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <label className="flex items-center space-x-2 text-sm text-slate-300">
                  <input
                    type="checkbox"
                    checked={session['speed-limit-down-enabled']}
                    onChange={(e) => setSession({ ...session, 'speed-limit-down-enabled': e.target.checked })}
                    className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                  />
                  <span>Limit Download Speed</span>
                </label>
                <input
                  type="number"
                  value={session['speed-limit-down']}
                  onChange={(e) => setSession({ ...session, 'speed-limit-down': e.target.value })}
                  className="w-28 rounded-lg border border-slate-700 bg-slate-800 py-1 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                />
              </div>

              <div className="flex items-center justify-between">
                <label className="flex items-center space-x-2 text-sm text-slate-300">
                  <input
                    type="checkbox"
                    checked={session['speed-limit-up-enabled']}
                    onChange={(e) => setSession({ ...session, 'speed-limit-up-enabled': e.target.checked })}
                    className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                  />
                  <span>Limit Upload Speed</span>
                </label>
                <input
                  type="number"
                  value={session['speed-limit-up']}
                  onChange={(e) => setSession({ ...session, 'speed-limit-up': e.target.value })}
                  className="w-28 rounded-lg border border-slate-700 bg-slate-800 py-1 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                />
              </div>

              <div className="border-t border-slate-800 pt-3">
                <label className="flex items-center space-x-2 text-sm text-slate-300 mb-3">
                  <input
                    type="checkbox"
                    checked={session['alt-speed-enabled']}
                    onChange={(e) => setSession({ ...session, 'alt-speed-enabled': e.target.checked })}
                    className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                  />
                  <span className="font-semibold text-amber-400">Alternate Speed Limits (Turtle Mode)</span>
                </label>

                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Alt Down (KB/s)</label>
                    <input
                      type="number"
                      value={session['alt-speed-down']}
                      onChange={(e) => setSession({ ...session, 'alt-speed-down': e.target.value })}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                    />
                  </div>
                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Alt Up (KB/s)</label>
                    <input
                      type="number"
                      value={session['alt-speed-up']}
                      onChange={(e) => setSession({ ...session, 'alt-speed-up': e.target.value })}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                    />
                  </div>
                </div>
              </div>
            </div>
          </div>

          {/* Peer & Network Config */}
          <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-4">
            <h2 className="text-base font-bold text-slate-200 flex items-center space-x-2">
              <Radio className="h-5 w-5 text-sky-400" />
              <span>Peers & Network Ports</span>
            </h2>

            <div className="space-y-4">
              <div>
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                  Default Download Directory
                </label>
                <input
                  type="text"
                  value={session['download-dir']}
                  onChange={(e) => setSession({ ...session, 'download-dir': e.target.value })}
                  className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                />
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Global Peer Limit</label>
                  <input
                    type="number"
                    value={session['peer-limit-global']}
                    onChange={(e) => setSession({ ...session, 'peer-limit-global': e.target.value })}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                </div>
                <div>
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">Per Torrent Limit</label>
                  <input
                    type="number"
                    value={session['peer-limit-per-torrent']}
                    onChange={(e) => setSession({ ...session, 'peer-limit-per-torrent': e.target.value })}
                    className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                </div>
              </div>

              {session['peer-port'] !== undefined && (
              <div className="border-t border-slate-800 pt-3">
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                  Incoming Peer Port
                </label>
                <div className="flex space-x-3">
                  <input
                    type="number"
                    value={session['peer-port']}
                    onChange={(e) => setSession({ ...session, 'peer-port': e.target.value })}
                    className="w-32 rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                  <button
                    onClick={handleTestPort}
                    disabled={testingPort || !can(selectedNode, 'test_port')}
                    title={can(selectedNode, 'test_port') ? undefined : "This node's daemon cannot test its port"}
                    className="rounded-lg bg-slate-800 px-4 py-1.5 text-xs font-semibold text-sky-400 hover:bg-slate-700 disabled:opacity-40 disabled:cursor-not-allowed"
                  >
                    {testingPort ? 'Testing...' : 'Test Port'}
                  </button>
                  {portStatus !== null && (
                    <div className="flex items-center space-x-1.5 text-xs font-semibold">
                      {portStatus ? (
                        <span className="text-emerald-400 flex items-center space-x-1">
                          <CheckCircle2 className="h-4 w-4" />
                          <span>Open</span>
                        </span>
                      ) : (
                        <span className="text-rose-400 flex items-center space-x-1">
                          <XCircle className="h-4 w-4" />
                          <span>Closed</span>
                        </span>
                      )}
                    </div>
                  )}
                </div>
              </div>
              )}
            </div>
          </div>

          {/* Peer discovery & transport: only the switches this node actually has */}
          {DISCOVERY_TOGGLES.some((t) => session[t.key] !== undefined) && (
            <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-4 md:col-span-2">
              <h2 className="text-base font-bold text-slate-200 flex items-center space-x-2">
                <Shield className="h-5 w-5 text-emerald-400" />
                <span>Peer Discovery & Transport</span>
              </h2>
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                {DISCOVERY_TOGGLES.filter((t) => session[t.key] !== undefined).map((t) => (
                  <label key={t.key} className="flex items-start space-x-2 text-sm text-slate-300">
                    <input
                      type="checkbox"
                      checked={!!session[t.key]}
                      onChange={(e) => setSession({ ...session, [t.key]: e.target.checked })}
                      className="mt-0.5 rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                    />
                    <span>
                      <span className="font-semibold">{t.label}</span>
                      <span className="block text-xs text-slate-500">{t.hint}</span>
                    </span>
                  </label>
                ))}
              </div>
              {session['synapse-announce-ip'] !== undefined && (
                <div>
                  <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                    Announce address
                  </label>
                  <input
                    type="text"
                    placeholder="Let the tracker decide"
                    value={session['synapse-announce-ip']}
                    onChange={(e) => setSession({ ...session, 'synapse-announce-ip': e.target.value })}
                    className="w-64 rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                  <p className="mt-1 text-xs text-slate-500">The address trackers are told to reach this node at (VPN / NAT setups). Leave empty for automatic.</p>
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
};

/** Switches shown when the selected node reports them (Synapse-only ones are prefixed `synapse-`). */
const DISCOVERY_TOGGLES: { key: string; label: string; hint: string }[] = [
  { key: 'dht-enabled', label: 'DHT', hint: 'Find peers through the distributed hash table (public torrents only).' },
  { key: 'pex-enabled', label: 'Peer exchange (PEX)', hint: 'Learn peers from the peers we are connected to.' },
  { key: 'lpd-enabled', label: 'Local peer discovery', hint: 'Find peers on the local network.' },
  { key: 'utp-enabled', label: 'uTP transport', hint: 'Use uTP (BEP 29) alongside TCP.' },
  { key: 'synapse-dht-read-only', label: 'DHT read-only', hint: 'Query the DHT without answering queries or being added to other nodes\' tables (BEP 43).' },
  { key: 'synapse-zeroconf', label: 'Zeroconf (mDNS)', hint: 'Advertise and discover peers on the LAN with multicast DNS (BEP 26).' },
];
