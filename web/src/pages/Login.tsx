// web/src/pages/Login.tsx
import React, { useState } from 'react';
import { KeyRound, User, LogIn, Smartphone, ArrowLeft, Loader2 } from 'lucide-react';
import { login } from '../services/api';
import { UserRecord } from '../types';

interface LoginProps {
  onLoginSuccess: (user: UserRecord) => void;
}

export const Login: React.FC<LoginProps> = ({ onLoginSuccess }) => {
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [totpCode, setTotpCode] = useState('');
  const [requires2Fa, setRequires2Fa] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!username || !password) return;

    setLoading(true);
    setError('');

    try {
      const res = await login(username, password, requires2Fa ? totpCode : undefined);
      if (res.requires_2fa && !res.token) {
        setRequires2Fa(true);
        setLoading(false);
        return;
      }

      if (res.user) {
        onLoginSuccess(res.user);
      }
    } catch (err: any) {
      setError(err.message || 'Invalid credentials or verification code');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex min-h-screen items-center justify-center bg-slate-950 p-6">
      <div className="w-full max-w-md rounded-3xl border border-slate-800 bg-slate-900/90 p-8 shadow-2xl backdrop-blur-xl">
        <div className="text-center mb-8">
          <div className="inline-flex h-16 w-16 items-center justify-center rounded-2xl bg-brand-500/10 border border-brand-500/30 text-3xl mb-3">
            🐕
          </div>
          <h1 className="text-2xl font-bold text-slate-100">
            {requires2Fa ? '2-Step Verification' : 'Sign In to Conduit'}
          </h1>
          <p className="text-sm text-slate-400 mt-1">
            {requires2Fa
              ? 'Enter the 6-digit passcode from your Authenticator app'
              : 'Unified Media Fetching Commander & Automation'}
          </p>
        </div>

        {error && (
          <div className="mb-6 rounded-xl bg-rose-500/10 border border-rose-500/20 p-3 text-sm text-rose-400">
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit} className="space-y-4">
          {!requires2Fa ? (
            <>
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
                    required
                    autoFocus
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
                    required
                    className="w-full rounded-xl border border-slate-700 bg-slate-800 py-2 pl-9 pr-4 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                  />
                </div>
              </div>
            </>
          ) : (
            <div className="space-y-3 animate-fadeIn">
              <div className="rounded-xl border border-brand-500/30 bg-brand-950/20 p-4 text-center">
                <Smartphone className="h-8 w-8 text-brand-400 mx-auto mb-2" />
                <span className="text-xs text-slate-300">
                  Logging in as <strong className="text-white font-mono">{username}</strong>
                </span>
              </div>

              <div>
                <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1.5">
                  Authenticator Code
                </label>
                <input
                  type="text"
                  maxLength={6}
                  value={totpCode}
                  onChange={(e) => setTotpCode(e.target.value.replace(/\D/g, ''))}
                  placeholder="123456"
                  required
                  autoFocus
                  className="w-full rounded-xl border border-brand-500/50 bg-slate-950 py-3 px-4 text-center font-mono text-xl tracking-widest text-slate-100 placeholder-slate-700 focus:border-brand-500 focus:outline-none"
                />
              </div>
            </div>
          )}

          <button
            type="submit"
            disabled={loading || (requires2Fa && totpCode.length !== 6)}
            className="w-full flex items-center justify-center space-x-2 rounded-xl bg-brand-600 py-2.5 text-sm font-semibold text-white hover:bg-brand-500 transition-colors shadow-lg shadow-brand-500/20 disabled:opacity-50 mt-6"
          >
            {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : <LogIn className="h-4 w-4" />}
            <span>{loading ? 'Verifying...' : requires2Fa ? 'Verify Passcode' : 'Sign In'}</span>
          </button>

          {requires2Fa && (
            <button
              type="button"
              onClick={() => {
                setRequires2Fa(false);
                setTotpCode('');
                setError('');
              }}
              className="w-full flex items-center justify-center space-x-1.5 py-2 text-xs text-slate-400 hover:text-slate-200 transition-colors"
            >
              <ArrowLeft className="h-3.5 w-3.5" />
              <span>Back to password</span>
            </button>
          )}
        </form>
      </div>
    </div>
  );
};
