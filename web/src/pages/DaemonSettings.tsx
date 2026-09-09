// web/src/pages/DaemonSettings.tsx
import React, { useState, useEffect } from 'react';
import { Sliders, CheckCircle2, XCircle, Gauge, Radio, Folder, Shield, Save, Info } from 'lucide-react';
import { fetchNodes, fetchNodeSession, updateNodeSession, testNodePort } from '../services/api';
import type { NodeStats } from '../types';

export const DaemonSettings: React.FC = () => {
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

  const selectedNodeInfo = nodes.find((n) => n.node === selectedNode);
  // synapse.v2 has no session-settings RPC (bandwidth/alt-speed/peer-limit/blocklist are all
  // Transmission-RPC-shaped concepts synapse doesn't expose a way to change) — this whole form
  // would silently do nothing on save for a synapse node, so it's swapped for an explanatory
  // notice below rather than presenting controls that don't work.
  const isSynapseNode = selectedNodeInfo?.client_type === 'synapse';

  useEffect(() => {
    if (selectedNode && !isSynapseNode) {
      loadNodeSession(selectedNode);
    } else {
      setSession(null);
      setLoading(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedNode, isSynapseNode]);

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
      await updateNodeSession(selectedNode, {
        'download-dir': session['download-dir'],
        'speed-limit-down': Number(session['speed-limit-down']),
        'speed-limit-down-enabled': session['speed-limit-down-enabled'],
        'speed-limit-up': Number(session['speed-limit-up']),
        'speed-limit-up-enabled': session['speed-limit-up-enabled'],
        'alt-speed-down': Number(session['alt-speed-down']),
        'alt-speed-up': Number(session['alt-speed-up']),
        'alt-speed-enabled': session['alt-speed-enabled'],
        'peer-limit-global': Number(session['peer-limit-global']),
        'peer-limit-per-torrent': Number(session['peer-limit-per-torrent']),
        'peer-port': Number(session['peer-port']),
        'dht-enabled': session['dht-enabled'],
        'pex-enabled': session['pex-enabled'],
      });
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
            disabled={saving || !session || isSynapseNode}
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

      {isSynapseNode ? (
        <div className="rounded-2xl border border-sky-900/40 bg-sky-950/20 p-6 text-sky-300 flex items-start space-x-3 max-w-3xl">
          <Info className="h-5 w-5 shrink-0 mt-0.5" />
          <div className="space-y-1">
            <p className="font-semibold text-sky-200">Not available for Synapse nodes</p>
            <p className="text-sm text-sky-300/90">
              Bandwidth limits, alternate speed schedules, peer limits, and blocklists here are all Transmission RPC
              settings — Synapse doesn't have an equivalent settings RPC yet, only live stats. Use the node's own{' '}
              <code className="rounded bg-sky-950/60 px-1 py-0.5 text-xs">synapse.toml</code> to configure rate limits
              and peer settings for this daemon directly.
            </p>
          </div>
        </div>
      ) : loading ? (
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
                    disabled={testingPort}
                    className="rounded-lg bg-slate-800 px-4 py-1.5 text-xs font-semibold text-sky-400 hover:bg-slate-700"
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
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
