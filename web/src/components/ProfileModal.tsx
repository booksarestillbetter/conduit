// web/src/components/ProfileModal.tsx
import React, { useState, useEffect } from 'react';
import {
  X,
  User,
  Shield,
  KeyRound,
  QrCode,
  Check,
  Copy,
  AlertCircle,
  CheckCircle2,
  Lock,
  Smartphone,
  ShieldCheck,
  ShieldAlert,
  Loader2,
} from 'lucide-react';
import qrcodegen from 'qrcode-generator';
import { UserRecord, Setup2FaResponse } from '../types';
import {
  updateProfile,
  changePassword,
  setup2Fa,
  verify2Fa,
  disable2Fa,
  fetchMe,
  createMobilePairToken,
} from '../services/api';

interface ProfileModalProps {
  user: UserRecord;
  onClose: () => void;
  onUserUpdated: (user: UserRecord) => void;
}

// Offline QR Code generation (0 network calls) via the standards-compliant `qrcode-generator`
// library — a hand-rolled encoder previously lived here, but it drew finder/timing/alignment
// patterns without real Reed-Solomon error correction or format-info encoding, so it produced
// something QR-*shaped* that didn't actually scan with any real decoder.
const generateQRMatrix = (text: string): boolean[][] => {
  const qr = qrcodegen(0, 'M'); // 0 = auto-select the smallest type number that fits
  qr.addData(text);
  qr.make();

  const count = qr.getModuleCount();
  const matrix: boolean[][] = [];
  for (let row = 0; row < count; row++) {
    const r: boolean[] = [];
    for (let col = 0; col < count; col++) {
      r.push(qr.isDark(row, col));
    }
    matrix.push(r);
  }
  return matrix;
};

const QRCodeDisplay: React.FC<{ url: string; size?: number }> = ({ url, size = 180 }) => {
  const matrix = React.useMemo(() => generateQRMatrix(url), [url]);
  // A real QR quiet zone (blank margin) is baked into the SVG itself — most decoders (including
  // phone cameras) need a few modules of clear space around the symbol to reliably lock onto it.
  const quietZoneModules = 4;
  const totalModules = matrix.length + quietZoneModules * 2;
  const cellSize = size / totalModules;

  return (
    <div className="flex flex-col items-center justify-center p-3 rounded-2xl bg-white border border-slate-700 shadow-inner">
      <svg
        width={size}
        height={size}
        viewBox={`0 0 ${size} ${size}`}
        className="rounded-lg bg-white"
        xmlns="http://www.w3.org/2000/svg"
        shapeRendering="crispEdges"
      >
        <rect width="100%" height="100%" fill="#ffffff" />
        {matrix.map((row, r) =>
          row.map((cell, c) =>
            cell ? (
              <rect
                key={`${r}-${c}`}
                x={(c + quietZoneModules) * cellSize}
                y={(r + quietZoneModules) * cellSize}
                width={cellSize}
                height={cellSize}
                fill="#0f172a"
              />
            ) : null
          )
        )}
      </svg>
      <span className="text-[10px] font-mono text-slate-800 mt-1.5 font-semibold">
        Scan with Conduit Mobile App or Authenticator
      </span>
    </div>
  );
};

export const ProfileModal: React.FC<ProfileModalProps> = ({
  user,
  onClose,
  onUserUpdated,
}) => {
  const [activeTab, setActiveTab] = useState<'profile' | 'password' | '2fa' | 'mobile'>('profile');

  // Mobile pairing state
  const [mobilePairData, setMobilePairData] = useState<{
    pair_code: string;
    qr_payload: string;
    server_url?: string;
    expires_in_secs: number;
  } | null>(null);
  const [mobilePairLoading, setMobilePairLoading] = useState(false);
  const [mobilePairError, setMobilePairError] = useState<string | null>(null);
  const [copiedPairCode, setCopiedPairCode] = useState(false);

  // Profile state
  const [username, setUsername] = useState(user.username);
  const [profileSaving, setProfileSaving] = useState(false);
  const [profileMessage, setProfileMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  // Password state
  const [currentPassword, setCurrentPassword] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [passwordSaving, setPasswordSaving] = useState(false);
  const [passwordMessage, setPasswordMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  // 2FA state
  const [is2FaEnabled, setIs2FaEnabled] = useState(!!user.totp_enabled);
  const [setupData, setSetupData] = useState<Setup2FaResponse | null>(null);
  const [totpCode, setTotpCode] = useState('');
  const [disablePassword, setDisablePassword] = useState('');
  const [isSettingUp2Fa, setIsSettingUp2Fa] = useState(false);
  const [isDisabling2Fa, setIsDisabling2Fa] = useState(false);
  const [twoFaLoading, setTwoFaLoading] = useState(false);
  const [twoFaMessage, setTwoFaMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);
  const [copiedSecret, setCopiedSecret] = useState(false);

  useEffect(() => {
    // Refresh user profile details
    fetchMe()
      .then((u) => {
        setIs2FaEnabled(!!u.totp_enabled);
        setUsername(u.username);
      })
      .catch(() => {});
  }, []);

  const handleUpdateProfile = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!username.trim()) return;

    setProfileSaving(true);
    setProfileMessage(null);
    try {
      await updateProfile(username.trim());
      setProfileMessage({ type: 'success', text: 'Username updated successfully.' });
      const updated = await fetchMe();
      onUserUpdated(updated);
    } catch (err: any) {
      setProfileMessage({ type: 'error', text: err.message || 'Failed to update username' });
    } finally {
      setProfileSaving(false);
    }
  };

  const handleChangePassword = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!currentPassword || !newPassword) return;

    if (newPassword !== confirmPassword) {
      setPasswordMessage({ type: 'error', text: 'New passwords do not match.' });
      return;
    }

    if (newPassword.length < 12) {
      setPasswordMessage({ type: 'error', text: 'New password must be at least 12 characters.' });
      return;
    }

    setPasswordSaving(true);
    setPasswordMessage(null);
    try {
      await changePassword(currentPassword, newPassword);
      setPasswordMessage({ type: 'success', text: 'Password changed successfully.' });
      setCurrentPassword('');
      setNewPassword('');
      setConfirmPassword('');
    } catch (err: any) {
      setPasswordMessage({ type: 'error', text: err.message || 'Failed to change password' });
    } finally {
      setPasswordSaving(false);
    }
  };

  const handleStart2FaSetup = async () => {
    setTwoFaLoading(true);
    setTwoFaMessage(null);
    try {
      const data = await setup2Fa();
      setSetupData(data);
      setIsSettingUp2Fa(true);
      setIsDisabling2Fa(false);
    } catch (err: any) {
      setTwoFaMessage({ type: 'error', text: err.message || 'Failed to initialize 2FA' });
    } finally {
      setTwoFaLoading(false);
    }
  };

  const handleVerifyAndEnable2Fa = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!totpCode.trim()) return;

    setTwoFaLoading(true);
    setTwoFaMessage(null);
    try {
      await verify2Fa(totpCode.trim());
      setTwoFaMessage({ type: 'success', text: 'Two-Factor Authentication is now active!' });
      setIs2FaEnabled(true);
      setIsSettingUp2Fa(false);
      setSetupData(null);
      setTotpCode('');
      const updated = await fetchMe();
      onUserUpdated(updated);
    } catch (err: any) {
      setTwoFaMessage({ type: 'error', text: err.message || 'Invalid verification code' });
    } finally {
      setTwoFaLoading(false);
    }
  };

  const handleDisable2Fa = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!disablePassword) return;

    setTwoFaLoading(true);
    setTwoFaMessage(null);
    try {
      await disable2Fa(disablePassword);
      setTwoFaMessage({ type: 'success', text: 'Two-Factor Authentication has been disabled.' });
      setIs2FaEnabled(false);
      setIsDisabling2Fa(false);
      setDisablePassword('');
      const updated = await fetchMe();
      onUserUpdated(updated);
    } catch (err: any) {
      setTwoFaMessage({ type: 'error', text: err.message || 'Failed to disable 2FA' });
    } finally {
      setTwoFaLoading(false);
    }
  };

  const handleCopySecret = () => {
    if (!setupData?.secret) return;
    navigator.clipboard.writeText(setupData.secret);
    setCopiedSecret(true);
    setTimeout(() => setCopiedSecret(false), 2000);
  };

  const handleGenerateMobilePair = async () => {
    setMobilePairLoading(true);
    setMobilePairError(null);
    try {
      const data = await createMobilePairToken();
      setMobilePairData(data);
    } catch (e: any) {
      setMobilePairError(e.message || 'Failed to generate mobile pairing code');
    } finally {
      setMobilePairLoading(false);
    }
  };

  const handleCopyPairCode = () => {
    if (!mobilePairData?.pair_code) return;
    navigator.clipboard.writeText(mobilePairData.pair_code);
    setCopiedPairCode(true);
    setTimeout(() => setCopiedPairCode(false), 2000);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/75 p-4 backdrop-blur-md animate-fadeIn">
      <div className="flex h-auto max-h-[90vh] w-full max-w-2xl flex-col rounded-2xl border border-slate-800 bg-slate-900 shadow-2xl overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-slate-800/80 bg-slate-950/70 px-6 py-4">
          <div className="flex items-center space-x-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-brand-500/10 text-brand-400 border border-brand-500/20">
              <Shield className="h-5 w-5" />
            </div>
            <div>
              <h2 className="text-base font-bold text-slate-100">Account Profile & Security</h2>
              <p className="text-xs text-slate-400">Manage your credentials, password, and two-factor authentication.</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="rounded-xl p-2 text-slate-400 hover:bg-slate-800 hover:text-slate-100 transition-colors"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        {/* Tab Navigation */}
        <div className="flex border-b border-slate-800/80 bg-slate-950/30 px-6">
          <button
            onClick={() => setActiveTab('profile')}
            className={`flex items-center space-x-2 border-b-2 py-3 px-3 text-xs font-semibold transition-all ${
              activeTab === 'profile'
                ? 'border-brand-500 text-brand-400'
                : 'border-transparent text-slate-400 hover:text-slate-200'
            }`}
          >
            <User className="h-4 w-4" />
            <span>Profile</span>
          </button>
          <button
            onClick={() => setActiveTab('password')}
            className={`flex items-center space-x-2 border-b-2 py-3 px-3 text-xs font-semibold transition-all ${
              activeTab === 'password'
                ? 'border-brand-500 text-brand-400'
                : 'border-transparent text-slate-400 hover:text-slate-200'
            }`}
          >
            <KeyRound className="h-4 w-4" />
            <span>Password</span>
          </button>
          <button
            onClick={() => setActiveTab('2fa')}
            className={`flex items-center space-x-2 border-b-2 py-3 px-3 text-xs font-semibold transition-all ${
              activeTab === '2fa'
                ? 'border-brand-500 text-brand-400'
                : 'border-transparent text-slate-400 hover:text-slate-200'
            }`}
          >
            <Smartphone className="h-4 w-4" />
            <span>2-Step Verification</span>
            {is2FaEnabled && (
              <span className="rounded-full bg-emerald-500/20 px-1.5 py-0.2 text-[9px] font-bold text-emerald-400 border border-emerald-500/30">
                ACTIVE
              </span>
            )}
          </button>
          <button
            onClick={() => {
              setActiveTab('mobile');
              if (!mobilePairData) {
                handleGenerateMobilePair();
              }
            }}
            className={`flex items-center space-x-2 border-b-2 py-3 px-3 text-xs font-semibold transition-all ${
              activeTab === 'mobile'
                ? 'border-brand-500 text-brand-400'
                : 'border-transparent text-slate-400 hover:text-slate-200'
            }`}
          >
            <QrCode className="h-4 w-4" />
            <span>📱 Pair Mobile App</span>
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {/* TAB 1: PROFILE */}
          {activeTab === 'profile' && (
            <form onSubmit={handleUpdateProfile} className="space-y-5">
              {profileMessage && (
                <div
                  className={`flex items-center space-x-2 rounded-xl p-3 text-xs border ${
                    profileMessage.type === 'success'
                      ? 'bg-emerald-500/10 text-emerald-300 border-emerald-500/30'
                      : 'bg-rose-500/10 text-rose-300 border-rose-500/30'
                  }`}
                >
                  {profileMessage.type === 'success' ? (
                    <CheckCircle2 className="h-4 w-4 shrink-0 text-emerald-400" />
                  ) : (
                    <AlertCircle className="h-4 w-4 shrink-0 text-rose-400" />
                  )}
                  <span>{profileMessage.text}</span>
                </div>
              )}

              <div className="space-y-1.5">
                <label className="block text-xs font-semibold text-slate-300">Username</label>
                <input
                  type="text"
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  className="w-full rounded-xl border border-slate-700 bg-slate-800/80 px-3.5 py-2.5 text-xs text-slate-100 placeholder-slate-500 focus:border-brand-500 focus:outline-none"
                  required
                />
              </div>

              <div className="grid grid-cols-2 gap-4 rounded-xl border border-slate-800 bg-slate-950/40 p-4 text-xs">
                <div>
                  <span className="text-slate-400">Account Role:</span>
                  <div className="mt-1 font-bold text-slate-200 flex items-center space-x-1.5">
                    <ShieldCheck className="h-3.5 w-3.5 text-brand-400" />
                    <span>{user.is_admin ? 'Administrator' : 'Standard User'}</span>
                  </div>
                </div>
                <div>
                  <span className="text-slate-400">User ID:</span>
                  <div className="mt-1 font-mono text-slate-300 truncate">{user.id}</div>
                </div>
              </div>

              <div className="flex justify-end pt-2">
                <button
                  type="submit"
                  disabled={profileSaving}
                  className="flex items-center space-x-2 rounded-xl bg-brand-600 px-5 py-2.5 text-xs font-semibold text-white hover:bg-brand-500 disabled:opacity-50 transition-colors shadow-md"
                >
                  {profileSaving && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
                  <span>Save Profile Changes</span>
                </button>
              </div>
            </form>
          )}

          {/* TAB 2: PASSWORD */}
          {activeTab === 'password' && (
            <form onSubmit={handleChangePassword} className="space-y-4">
              {passwordMessage && (
                <div
                  className={`flex items-center space-x-2 rounded-xl p-3 text-xs border ${
                    passwordMessage.type === 'success'
                      ? 'bg-emerald-500/10 text-emerald-300 border-emerald-500/30'
                      : 'bg-rose-500/10 text-rose-300 border-rose-500/30'
                  }`}
                >
                  {passwordMessage.type === 'success' ? (
                    <CheckCircle2 className="h-4 w-4 shrink-0 text-emerald-400" />
                  ) : (
                    <AlertCircle className="h-4 w-4 shrink-0 text-rose-400" />
                  )}
                  <span>{passwordMessage.text}</span>
                </div>
              )}

              <div className="space-y-1.5">
                <label className="block text-xs font-semibold text-slate-300">Current Password</label>
                <input
                  type="password"
                  value={currentPassword}
                  onChange={(e) => setCurrentPassword(e.target.value)}
                  placeholder="Enter existing password"
                  className="w-full rounded-xl border border-slate-700 bg-slate-800/80 px-3.5 py-2.5 text-xs text-slate-100 placeholder-slate-500 focus:border-brand-500 focus:outline-none"
                  required
                />
              </div>

              <div className="space-y-1.5">
                <label className="block text-xs font-semibold text-slate-300">New Password</label>
                <input
                  type="password"
                  value={newPassword}
                  onChange={(e) => setNewPassword(e.target.value)}
                  placeholder="Enter new password (minimum 12 characters)"
                  className="w-full rounded-xl border border-slate-700 bg-slate-800/80 px-3.5 py-2.5 text-xs text-slate-100 placeholder-slate-500 focus:border-brand-500 focus:outline-none"
                  required
                />
              </div>

              <div className="space-y-1.5">
                <label className="block text-xs font-semibold text-slate-300">Confirm New Password</label>
                <input
                  type="password"
                  value={confirmPassword}
                  onChange={(e) => setConfirmPassword(e.target.value)}
                  placeholder="Re-type new password"
                  className="w-full rounded-xl border border-slate-700 bg-slate-800/80 px-3.5 py-2.5 text-xs text-slate-100 placeholder-slate-500 focus:border-brand-500 focus:outline-none"
                  required
                />
              </div>

              <div className="flex justify-end pt-3">
                <button
                  type="submit"
                  disabled={passwordSaving}
                  className="flex items-center space-x-2 rounded-xl bg-brand-600 px-5 py-2.5 text-xs font-semibold text-white hover:bg-brand-500 disabled:opacity-50 transition-colors shadow-md"
                >
                  {passwordSaving && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
                  <span>Update Password</span>
                </button>
              </div>
            </form>
          )}

          {/* TAB 3: TWO-FACTOR AUTHENTICATION */}
          {activeTab === '2fa' && (
            <div className="space-y-5">
              {twoFaMessage && (
                <div
                  className={`flex items-center space-x-2 rounded-xl p-3 text-xs border ${
                    twoFaMessage.type === 'success'
                      ? 'bg-emerald-500/10 text-emerald-300 border-emerald-500/30'
                      : 'bg-rose-500/10 text-rose-300 border-rose-500/30'
                  }`}
                >
                  {twoFaMessage.type === 'success' ? (
                    <CheckCircle2 className="h-4 w-4 shrink-0 text-emerald-400" />
                  ) : (
                    <AlertCircle className="h-4 w-4 shrink-0 text-rose-400" />
                  )}
                  <span>{twoFaMessage.text}</span>
                </div>
              )}

              {/* Current Status Card */}
              <div className="rounded-2xl border border-slate-800 bg-slate-950/40 p-5 space-y-4">
                <div className="flex items-center justify-between">
                  <div className="flex items-center space-x-3">
                    <div
                      className={`flex h-10 w-10 items-center justify-center rounded-xl border ${
                        is2FaEnabled
                          ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20'
                          : 'bg-amber-500/10 text-amber-400 border-amber-500/20'
                      }`}
                    >
                      {is2FaEnabled ? <ShieldCheck className="h-5 w-5" /> : <ShieldAlert className="h-5 w-5" />}
                    </div>
                    <div>
                      <h4 className="text-xs font-bold uppercase tracking-wider text-slate-200">
                        {is2FaEnabled ? '2-Step Verification is Active' : '2-Step Verification is Disabled'}
                      </h4>
                      <p className="text-xs text-slate-400 mt-0.5">
                        {is2FaEnabled
                          ? 'Your account requires a 6-digit TOTP code when logging in.'
                          : 'Protect your account by requiring an authenticator code in addition to your password.'}
                      </p>
                    </div>
                  </div>

                  {!is2FaEnabled && !isSettingUp2Fa && (
                    <button
                      onClick={handleStart2FaSetup}
                      disabled={twoFaLoading}
                      className="flex items-center space-x-2 rounded-xl bg-brand-600 px-4 py-2 text-xs font-semibold text-white hover:bg-brand-500 transition-colors shadow-md"
                    >
                      <QrCode className="h-3.5 w-3.5" />
                      <span>Set Up 2FA</span>
                    </button>
                  )}

                  {is2FaEnabled && !isDisabling2Fa && (
                    <button
                      onClick={() => setIsDisabling2Fa(true)}
                      className="rounded-xl border border-rose-500/30 bg-rose-500/10 px-4 py-2 text-xs font-semibold text-rose-400 hover:bg-rose-500/20 transition-colors"
                    >
                      Disable 2FA
                    </button>
                  )}
                </div>
              </div>

              {/* 2FA SETUP WIZARD */}
              {isSettingUp2Fa && setupData && (
                <form
                  onSubmit={handleVerifyAndEnable2Fa}
                  className="rounded-2xl border border-brand-500/30 bg-brand-950/10 p-6 space-y-6 animate-fadeIn"
                >
                  <div className="flex items-center justify-between border-b border-slate-800 pb-3">
                    <h4 className="text-sm font-bold text-slate-100 flex items-center space-x-2">
                      <Smartphone className="h-4 w-4 text-brand-400" />
                      <span>Set up Authenticator App</span>
                    </h4>
                    <button
                      type="button"
                      onClick={() => {
                        setIsSettingUp2Fa(false);
                        setSetupData(null);
                      }}
                      className="text-xs text-slate-400 hover:text-slate-200"
                    >
                      Cancel
                    </button>
                  </div>

                  <div className="grid grid-cols-1 md:grid-cols-2 gap-6 items-center">
                    {/* QR Code */}
                    <div className="flex flex-col items-center space-y-2">
                      <QRCodeDisplay url={setupData.otpauth_url} size={170} />
                      <span className="text-[11px] text-slate-400 text-center">
                        Step 1: Scan this QR code with Google Authenticator, Authy, or 1Password.
                      </span>
                    </div>

                    {/* Manual Secret Key */}
                    <div className="space-y-4 text-xs">
                      <div>
                        <span className="text-slate-400 font-semibold">Or enter this key manually:</span>
                        <div className="mt-1.5 flex items-center space-x-2">
                          <code className="flex-1 rounded-xl bg-slate-950 px-3 py-2 font-mono text-xs font-bold text-brand-300 border border-slate-800 select-all">
                            {setupData.secret}
                          </code>
                          <button
                            type="button"
                            onClick={handleCopySecret}
                            className="flex items-center space-x-1 rounded-xl bg-slate-800 px-3 py-2 text-slate-200 hover:bg-slate-700 transition-colors"
                          >
                            {copiedSecret ? <Check className="h-3.5 w-3.5 text-emerald-400" /> : <Copy className="h-3.5 w-3.5" />}
                            <span>{copiedSecret ? 'Copied' : 'Copy'}</span>
                          </button>
                        </div>
                      </div>

                      <div className="space-y-1.5">
                        <label className="block font-semibold text-slate-300">
                          Step 2: Enter 6-digit code from app:
                        </label>
                        <input
                          type="text"
                          maxLength={6}
                          placeholder="123456"
                          value={totpCode}
                          onChange={(e) => setTotpCode(e.target.value.replace(/\D/g, ''))}
                          className="w-full rounded-xl border border-brand-500/50 bg-slate-950 px-4 py-2.5 text-center font-mono text-lg tracking-widest text-slate-100 placeholder-slate-700 focus:border-brand-500 focus:outline-none"
                          required
                          autoFocus
                        />
                      </div>

                      <button
                        type="submit"
                        disabled={twoFaLoading || totpCode.length !== 6}
                        className="w-full flex items-center justify-center space-x-2 rounded-xl bg-emerald-600 px-5 py-2.5 font-semibold text-white hover:bg-emerald-500 disabled:opacity-50 transition-colors shadow-lg"
                      >
                        {twoFaLoading && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
                        <span>Confirm & Activate 2FA</span>
                      </button>
                    </div>
                  </div>
                </form>
              )}

              {/* DISABLE 2FA FORM */}
              {isDisabling2Fa && (
                <form
                  onSubmit={handleDisable2Fa}
                  className="rounded-2xl border border-rose-500/30 bg-rose-950/10 p-5 space-y-4 animate-fadeIn"
                >
                  <div className="flex items-center justify-between border-b border-slate-800 pb-2">
                    <h4 className="text-xs font-bold text-rose-300 flex items-center space-x-2">
                      <Lock className="h-4 w-4 text-rose-400" />
                      <span>Confirm Deactivation of 2-Step Verification</span>
                    </h4>
                    <button
                      type="button"
                      onClick={() => setIsDisabling2Fa(false)}
                      className="text-xs text-slate-400 hover:text-slate-200"
                    >
                      Cancel
                    </button>
                  </div>

                  <div className="space-y-1.5">
                    <label className="block text-xs font-semibold text-slate-300">
                      Enter Account Password to Confirm:
                    </label>
                    <input
                      type="password"
                      value={disablePassword}
                      onChange={(e) => setDisablePassword(e.target.value)}
                      placeholder="Account password"
                      className="w-full rounded-xl border border-slate-700 bg-slate-800/80 px-3.5 py-2.5 text-xs text-slate-100 placeholder-slate-500 focus:border-rose-500 focus:outline-none"
                      required
                    />
                  </div>

                  <div className="flex justify-end space-x-2 pt-2">
                    <button
                      type="button"
                      onClick={() => setIsDisabling2Fa(false)}
                      className="rounded-xl px-4 py-2 text-xs font-semibold text-slate-400 hover:text-slate-200"
                    >
                      Cancel
                    </button>
                    <button
                      type="submit"
                      disabled={twoFaLoading || !disablePassword}
                      className="rounded-xl bg-rose-600 px-4 py-2 text-xs font-semibold text-white hover:bg-rose-500 disabled:opacity-50 transition-colors shadow-md"
                    >
                      Disable 2FA Now
                    </button>
                  </div>
                </form>
              )}
            </div>
          )}

          {/* TAB 4: MOBILE APP PAIRING */}
          {activeTab === 'mobile' && (
            <div className="space-y-5">
              <div className="rounded-2xl border border-brand-500/30 bg-brand-500/5 p-5 space-y-3 shadow-inner">
                <div className="flex items-center space-x-3">
                  <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-brand-500/20 text-brand-400">
                    <Smartphone className="h-5 w-5" />
                  </div>
                  <div>
                    <h3 className="text-sm font-bold text-slate-100">Conduit Flutter Mobile App Pairing</h3>
                    <p className="text-xs text-slate-400">
                      Instantly connect and authenticate your iOS or Android device by scanning this QR code.
                    </p>
                  </div>
                </div>

                <div className="text-xs text-slate-300 space-y-1.5 bg-slate-950/60 p-3.5 rounded-xl border border-slate-800/80">
                  <div className="font-semibold text-brand-300">How to Pair:</div>
                  <ol className="list-decimal list-inside space-y-1 text-slate-400">
                    <li>Open the <strong>Conduit Mobile App</strong> (Flutter) on your phone.</li>
                    <li>Tap <strong>Scan QR Code to Connect</strong> on the setup screen.</li>
                    <li>Point your device camera at the QR code below.</li>
                  </ol>
                </div>
              </div>

              {mobilePairLoading && (
                <div className="flex flex-col items-center justify-center py-10 space-y-3">
                  <Loader2 className="h-8 w-8 animate-spin text-brand-400" />
                  <span className="text-xs font-medium text-slate-400">Generating secure pairing key...</span>
                </div>
              )}

              {mobilePairError && (
                <div className="flex items-center space-x-2 rounded-xl p-3 text-xs border bg-rose-500/10 text-rose-300 border-rose-500/30">
                  <AlertCircle className="h-4 w-4 shrink-0 text-rose-400" />
                  <span>{mobilePairError}</span>
                </div>
              )}

              {mobilePairData && !mobilePairLoading && (
                <div className="flex flex-col items-center space-y-4 rounded-2xl border border-slate-800 bg-slate-950/40 p-6">
                  {(() => {
                    let computedPayload = mobilePairData.qr_payload;
                    const origin = window.location.origin;
                    try {
                      const parsed = JSON.parse(mobilePairData.qr_payload);
                      parsed.conduit_url = origin;
                      computedPayload = JSON.stringify(parsed);
                    } catch {
                      // Fallback
                    }
                    return (
                      <>
                        <QRCodeDisplay url={computedPayload} size={220} />
                        <div className="text-center">
                          <span className="text-[11px] font-medium text-slate-400">Target Server: </span>
                          <span className="text-[11px] font-mono font-semibold text-brand-300">{origin}</span>
                        </div>
                      </>
                    );
                  })()}

                  <div className="w-full max-w-sm space-y-2">
                    <div className="flex items-center justify-between text-xs text-slate-400">
                      <span>Manual One-Time Pairing Code:</span>
                      <button
                        onClick={handleCopyPairCode}
                        className="flex items-center space-x-1 text-brand-400 hover:text-brand-300 font-medium"
                      >
                        {copiedPairCode ? <Check className="h-3.5 w-3.5 text-emerald-400" /> : <Copy className="h-3.5 w-3.5" />}
                        <span>{copiedPairCode ? 'Copied' : 'Copy'}</span>
                      </button>
                    </div>
                    <div className="rounded-xl border border-slate-700 bg-slate-900 p-2.5 text-center font-mono text-xs font-bold text-amber-300 break-all select-all">
                      {mobilePairData.pair_code}
                    </div>
                    <p className="text-[11px] text-center text-slate-500">
                      Code expires in {Math.round(mobilePairData.expires_in_secs / 60)} minutes. Single-use token.
                    </p>
                  </div>

                  <button
                    type="button"
                    onClick={handleGenerateMobilePair}
                    className="flex items-center space-x-2 rounded-xl bg-slate-800 hover:bg-slate-700 px-4 py-2 text-xs font-semibold text-slate-200 transition-colors border border-slate-700"
                  >
                    <QrCode className="h-4 w-4 text-brand-400" />
                    <span>Generate Fresh QR Code</span>
                  </button>
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
