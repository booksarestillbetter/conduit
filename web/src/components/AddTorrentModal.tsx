// web/src/components/AddTorrentModal.tsx
import React, { useEffect, useState } from 'react';
import { X, Upload, Link, Folder, Server } from 'lucide-react';
import { NodeStats } from '../types';

interface AddTorrentModalProps {
  isOpen: boolean;
  onClose: () => void;
  nodes: NodeStats[];
  activeNode: string;
  onAdd: (payload: {
    node: string;
    magnet_or_url?: string;
    metainfo_base64?: string;
    download_dir?: string;
    paused: boolean;
    sequential_download?: boolean;
  }) => Promise<void>;
}

export const AddTorrentModal: React.FC<AddTorrentModalProps> = ({
  isOpen,
  onClose,
  nodes,
  activeNode,
  onAdd,
}) => {
  const [selectedNode, setSelectedNode] = useState(activeNode !== 'all' ? activeNode : nodes[0]?.node || '');

  // `nodes` is frequently still empty at the instant this modal first mounts (the initial stats
  // poll/WS message hasn't landed yet); without this, selectedNode stayed '' forever and the
  // <select> — which has no matching empty option — would visually show a node the submitted
  // payload didn't actually reference. Keep the selection valid whenever the node list changes.
  useEffect(() => {
    if (nodes.length === 0) return;
    const stillValid = nodes.some((n) => n.node === selectedNode);
    if (!stillValid) {
      const preferred = activeNode !== 'all' && nodes.some((n) => n.node === activeNode) ? activeNode : nodes[0].node;
      setSelectedNode(preferred);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nodes, activeNode]);
  const [mode, setMode] = useState<'url' | 'file'>('url');
  const [urlOrMagnet, setUrlOrMagnet] = useState('');
  const [fileBase64, setFileBase64] = useState<string | null>(null);
  const [fileName, setFileName] = useState('');
  const [downloadDir, setDownloadDir] = useState('');
  const [startPaused, setStartPaused] = useState(false);
  const [sequentialDownload, setSequentialDownload] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  if (!isOpen) return null;

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) {
      setFileName(file.name);
      const reader = new FileReader();
      reader.onload = () => {
        const result = reader.result as string;
        // Strip data:application/x-bittorrent;base64, prefix
        const base64 = result.split(',')[1] || result;
        setFileBase64(base64);
      };
      reader.readAsDataURL(file);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError('');
    setLoading(true);

    try {
      if (!selectedNode) {
        throw new Error('Please select a target node');
      }
      if (mode === 'url' && !urlOrMagnet.trim()) {
        throw new Error('Please enter a valid Magnet URI or torrent URL');
      }
      if (mode === 'url' && !/^(magnet:\?|https?:\/\/)/i.test(urlOrMagnet.trim())) {
        throw new Error('Expected a magnet: link or an http(s):// URL');
      }
      if (mode === 'file' && !fileBase64) {
        throw new Error('Please select a .torrent file');
      }

      await onAdd({
        node: selectedNode,
        magnet_or_url: mode === 'url' ? urlOrMagnet.trim() : undefined,
        metainfo_base64: mode === 'file' ? fileBase64 || undefined : undefined,
        download_dir: downloadDir.trim() ? downloadDir.trim() : undefined,
        paused: startPaused,
        sequential_download: sequentialDownload,
      });

      onClose();
    } catch (err: any) {
      setError(err.message || 'Failed to add torrent');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4">
      <div className="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-900 p-6 shadow-2xl animate-in zoom-in-95 duration-200">
        <div className="flex items-center justify-between border-b border-slate-800 pb-4">
          <h2 className="text-lg font-bold text-slate-100">Add New Fetcher</h2>
          <button
            onClick={onClose}
            className="rounded-lg p-1 text-slate-400 hover:bg-slate-800 hover:text-slate-200"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="mt-5 space-y-4">
          {error && (
            <div className="rounded-lg bg-rose-500/10 border border-rose-500/20 p-3 text-xs text-rose-400">
              {error}
            </div>
          )}

          {/* Target Node */}
          <div>
            <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1">
              Target Node
            </label>
            <div className="relative">
              <Server className="absolute left-3 top-2.5 h-4 w-4 text-slate-400 pointer-events-none" />
              <select
                value={selectedNode}
                onChange={(e) => setSelectedNode(e.target.value)}
                className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 pl-9 pr-3 text-sm text-slate-200 focus:border-brand-500 focus:outline-none"
                required
              >
                {nodes.map((n) => (
                  <option key={n.node} value={n.node}>
                    {n.node} ({n.connected ? `${n.free_space_gb.toFixed(1)} GB free` : 'offline'})
                  </option>
                ))}
              </select>
            </div>
          </div>

          {/* Mode Toggle */}
          <div className="flex rounded-lg bg-slate-800/80 p-1 border border-slate-700">
            <button
              type="button"
              onClick={() => setMode('url')}
              className={`flex flex-1 items-center justify-center space-x-2 rounded-md py-1.5 text-xs font-semibold transition-colors ${
                mode === 'url' ? 'bg-brand-600 text-white shadow' : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              <Link className="h-3.5 w-3.5" />
              <span>Magnet / URL</span>
            </button>
            <button
              type="button"
              onClick={() => setMode('file')}
              className={`flex flex-1 items-center justify-center space-x-2 rounded-md py-1.5 text-xs font-semibold transition-colors ${
                mode === 'file' ? 'bg-brand-600 text-white shadow' : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              <Upload className="h-3.5 w-3.5" />
              <span>Upload .torrent</span>
            </button>
          </div>

          {mode === 'url' ? (
            <div>
              <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1.5">
                Magnet URI or Torrent Link
              </label>
              <textarea
                value={urlOrMagnet}
                onChange={(e) => setUrlOrMagnet(e.target.value)}
                rows={3}
                placeholder="magnet:?xt=urn:btih:... or https://..."
                className="w-full rounded-lg border border-slate-700 bg-slate-800 p-3 text-sm font-mono text-slate-200 placeholder-slate-500 focus:border-brand-500 focus:outline-none"
              />
            </div>
          ) : (
            <div>
              <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1.5">
                Upload .torrent
              </label>
              <label className="flex flex-col items-center justify-center border-2 border-dashed border-slate-700 rounded-lg p-6 bg-slate-800/50 hover:bg-slate-800 cursor-pointer transition-colors">
                <Upload className="h-8 w-8 text-brand-400 mb-2" />
                <span className="text-sm font-medium text-slate-300">
                  {fileName || 'Click or drag .torrent file here'}
                </span>
                <input type="file" accept=".torrent" onChange={handleFileChange} className="hidden" />
              </label>
            </div>
          )}

          {/* Download Directory */}
          <div>
            <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1.5">
              Custom Download Directory (Optional)
            </label>
            <div className="relative">
              <Folder className="absolute left-3 top-2.5 h-4 w-4 text-slate-400" />
              <input
                type="text"
                value={downloadDir}
                onChange={(e) => setDownloadDir(e.target.value)}
                placeholder="Default node download dir"
                className="w-full rounded-lg border border-slate-700 bg-slate-800 py-2 pl-9 pr-4 text-sm text-slate-200 placeholder-slate-500 focus:border-brand-500 focus:outline-none"
              />
            </div>
          </div>

          {/* Toggles: Start Paused & Sequential Download */}
          <div className="grid grid-cols-2 gap-3 pt-1">
            <div className="flex items-center space-x-2">
              <input
                type="checkbox"
                id="startPaused"
                checked={startPaused}
                onChange={(e) => setStartPaused(e.target.checked)}
                className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500 cursor-pointer"
              />
              <label htmlFor="startPaused" className="text-xs text-slate-300 select-none cursor-pointer">
                Start download paused
              </label>
            </div>

            <div className="flex items-center space-x-2">
              <input
                type="checkbox"
                id="sequentialDownload"
                checked={sequentialDownload}
                onChange={(e) => setSequentialDownload(e.target.checked)}
                className="rounded border-slate-700 bg-slate-800 text-indigo-500 focus:ring-indigo-500 cursor-pointer"
              />
              <label htmlFor="sequentialDownload" className="text-xs text-indigo-300 font-medium select-none cursor-pointer" title="Download pieces in order for streaming / media preview">
                🎬 Sequential download
              </label>
            </div>
          </div>

          {/* Actions */}
          <div className="flex items-center justify-end space-x-3 pt-4 border-t border-slate-800">
            <button
              type="button"
              onClick={onClose}
              className="rounded-lg px-4 py-2 text-sm font-medium text-slate-400 hover:bg-slate-800 hover:text-white"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={loading}
              className="rounded-lg bg-brand-600 px-5 py-2 text-sm font-semibold text-white shadow-lg shadow-brand-500/20 hover:bg-brand-500 disabled:opacity-50"
            >
              {loading ? 'Adding...' : 'Add Torrent'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
