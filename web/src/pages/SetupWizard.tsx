// web/src/pages/SetupWizard.tsx
import React, { useState } from 'react';
import { Server, KeyRound, User, Check, ArrowRight, ShieldCheck } from 'lucide-react';
import { completeSetup } from '../services/api';
import { UserRecord } from '../types';

interface SetupWizardProps {
  onComplete: (user: UserRecord) => void;
}

export const SetupWizard: React.FC<SetupWizardProps> = ({ onComplete }) => {
  const [step, setStep] = useState(1);
  const [username, setUsername] = useState('admin');
  const [password, setPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  
  // Initial Node
  const [addNode, setAddNode] = useState(true);
  const [nodeName, setNodeName] = useState('main');
  const [nodeHost, setNodeHost] = useState('localhost');
  const [nodePort, setNodePort] = useState(9091);
  const [nodeRpcPath, setNodeRpcPath] = useState('/transmission/rpc');
  const [nodeUser, setNodeUser] = useState('');
  const [nodePass, setNodePass] = useState('');

  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const handleFinish = async () => {
    if (password !== confirmPassword) {
      setError('Passwords do not match');
      return;
    }
    if (password.length < 12) {
      setError('Password must be at least 12 characters');
      return;
    }

    setLoading(true);
    setError('');

    try {
      const payload: any = {
        admin_username: username,
        admin_password: password,
      };

      if (addNode && nodeHost.trim()) {
        payload.initial_node = {
          name: nodeName.trim() || 'main',
          host: nodeHost.trim(),
          port: Number(nodePort) || 9091,
          rpc_path: nodeRpcPath.trim() || '/transmission/rpc',
          username: nodeUser.trim() || undefined,
          password: nodePass.trim() || undefined,
          use_ssl: false,
          enabled: true,
          fetcher_only: false,
          auto_purge_enabled: true,
        };
      }

      const res = await completeSetup(payload);
      onComplete(res.user);
    } catch (err: any) {
      setError(err.message || 'Setup failed');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex min-h-screen items-center justify-center bg-slate-950 p-6">
      <div className="w-full max-w-xl rounded-3xl border border-slate-800 bg-slate-900/90 p-8 shadow-2xl backdrop-blur-xl">
        {/* Header */}
        <div className="text-center mb-8">
          <div className="inline-flex h-16 w-16 items-center justify-center rounded-2xl bg-brand-500/10 border border-brand-500/30 text-3xl mb-3">
            🐕
          </div>
          <h1 className="text-2xl font-bold text-slate-100">Welcome to Conduit</h1>
          <p className="text-sm text-slate-400 mt-1">
            Let's configure your media command center and fetcher nodes.
          </p>
        </div>

        {/* Stepper */}
        <div className="flex items-center justify-center space-x-4 mb-8">
          <div className={`flex h-8 w-8 items-center justify-center rounded-full text-xs font-bold ${
            step >= 1 ? 'bg-brand-600 text-white' : 'bg-slate-800 text-slate-400'
          }`}>
            1
          </div>
          <div className={`h-0.5 w-12 ${step >= 2 ? 'bg-brand-600' : 'bg-slate-800'}`} />
          <div className={`flex h-8 w-8 items-center justify-center rounded-full text-xs font-bold ${
            step >= 2 ? 'bg-brand-600 text-white' : 'bg-slate-800 text-slate-400'
          }`}>
            2
          </div>
        </div>

        {error && (
          <div className="mb-6 rounded-xl bg-rose-500/10 border border-rose-500/20 p-4 text-sm text-rose-400">
            {error}
          </div>
        )}

        {/* Step 1: Admin Account */}
        {step === 1 && (
          <div className="space-y-4">
            <h2 className="text-base font-semibold text-slate-200 flex items-center space-x-2">
              <ShieldCheck className="h-5 w-5 text-brand-400" />
              <span>Create Administrator Account</span>
            </h2>

            <div>
              <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1.5">
                Username
              </label>
              <div className="relative">
                <User className="absolute left-3 top-2.5 h-4 w-4 text-slate-400" />
                <input
                  type="text"
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  className="w-full rounded-xl border border-slate-700 bg-slate-800 py-2 pl-9 pr-4 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                />
              </div>
            </div>

            <div>
              <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1.5">
                Password
              </label>
              <div className="relative">
                <KeyRound className="absolute left-3 top-2.5 h-4 w-4 text-slate-400" />
                <input
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder="At least 12 characters"
                  className="w-full rounded-xl border border-slate-700 bg-slate-800 py-2 pl-9 pr-4 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                />
              </div>
            </div>

            <div>
              <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1.5">
                Confirm Password
              </label>
              <div className="relative">
                <KeyRound className="absolute left-3 top-2.5 h-4 w-4 text-slate-400" />
                <input
                  type="password"
                  value={confirmPassword}
                  onChange={(e) => setConfirmPassword(e.target.value)}
                  className="w-full rounded-xl border border-slate-700 bg-slate-800 py-2 pl-9 pr-4 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                />
              </div>
            </div>

            <button
              type="button"
              onClick={() => {
                if (!password || password !== confirmPassword) {
                  setError('Passwords must match and be non-empty');
                  return;
                }
                setError('');
                setStep(2);
              }}
              className="w-full flex items-center justify-center space-x-2 rounded-xl bg-brand-600 py-2.5 text-sm font-semibold text-white hover:bg-brand-500 transition-colors shadow-lg shadow-brand-500/20 mt-6"
            >
              <span>Next: Connect Fetcher</span>
              <ArrowRight className="h-4 w-4" />
            </button>
          </div>
        )}

        {/* Step 2: Fetcher Node (Transmission-RPC-shaped for now — other backends can be added afterward in Conduit Settings) */}
        {step === 2 && (
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <h2 className="text-base font-semibold text-slate-200 flex items-center space-x-2">
                <Server className="h-5 w-5 text-brand-400" />
                <span>Connect First Fetcher Node</span>
              </h2>
              <label className="flex items-center space-x-2 text-xs text-slate-400 cursor-pointer">
                <input
                  type="checkbox"
                  checked={addNode}
                  onChange={(e) => setAddNode(e.target.checked)}
                  className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500"
                />
                <span>Configure now</span>
              </label>
            </div>

            {addNode && (
              <div className="space-y-3 rounded-2xl border border-slate-800 bg-slate-800/40 p-4">
                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                      Node Name
                    </label>
                    <input
                      type="text"
                      value={nodeName}
                      onChange={(e) => setNodeName(e.target.value)}
                      placeholder="e.g. main, seedbox"
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                    />
                  </div>
                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                      Host
                    </label>
                    <input
                      type="text"
                      value={nodeHost}
                      onChange={(e) => setNodeHost(e.target.value)}
                      placeholder="localhost or 192.168.1.100"
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                    />
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                      Port
                    </label>
                    <input
                      type="number"
                      value={nodePort}
                      onChange={(e) => setNodePort(Number(e.target.value))}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                    />
                  </div>
                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                      RPC Path
                    </label>
                    <input
                      type="text"
                      value={nodeRpcPath}
                      onChange={(e) => setNodeRpcPath(e.target.value)}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                    />
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                      RPC Username (Optional)
                    </label>
                    <input
                      type="text"
                      value={nodeUser}
                      onChange={(e) => setNodeUser(e.target.value)}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                    />
                  </div>
                  <div>
                    <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
                      RPC Password (Optional)
                    </label>
                    <input
                      type="password"
                      value={nodePass}
                      onChange={(e) => setNodePass(e.target.value)}
                      className="w-full rounded-lg border border-slate-700 bg-slate-800 py-1.5 px-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                    />
                  </div>
                </div>
                <p className="text-[11px] text-slate-500">
                  This form connects a Transmission-RPC node. Synapse, Deluge, and qBittorrent
                  nodes can be added afterward in Conduit Settings → Retrievers.
                </p>
              </div>
            )}

            <div className="flex items-center space-x-3 pt-4">
              <button
                type="button"
                onClick={() => setStep(1)}
                className="w-1/3 rounded-xl border border-slate-700 py-2.5 text-sm font-semibold text-slate-300 hover:bg-slate-800"
              >
                Back
              </button>
              <button
                type="button"
                onClick={handleFinish}
                disabled={loading}
                className="w-2/3 flex items-center justify-center space-x-2 rounded-xl bg-brand-600 py-2.5 text-sm font-semibold text-white hover:bg-brand-500 transition-colors shadow-lg shadow-brand-500/20 disabled:opacity-50"
              >
                <Check className="h-4 w-4" />
                <span>{loading ? 'Finalizing Setup...' : 'Complete & Launch Conduit'}</span>
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
