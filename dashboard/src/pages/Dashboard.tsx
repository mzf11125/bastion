import { useState, useEffect, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import { WalletMultiButton } from '@solana/wallet-adapter-react-ui';
import { useTheme } from '../context/ThemeContext';
import { Navbar } from '../components/Navbar';

interface AuditEntry {
  id: string;
  timestamp: number;
  decision: 'ALLOWED' | 'BLOCKED' | 'PENDING';
  account: string;
  intent: string;
  reason: string;
}

interface Policy {
  maxSolPerTx: number;
  maxBalanceDrain: number;
  rateLimit: number;
  allowedPrograms: string[];
  blockedAddresses: string[];
}

const DEFAULT_POLICY: Policy = {
  maxSolPerTx: 1,
  maxBalanceDrain: 100000000,
  rateLimit: 10,
  allowedPrograms: [
    '11111111111111111111111111111111',
    'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA',
    'JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4',
  ],
  blockedAddresses: [],
};

const DECISION_COLORS = {
  ALLOWED: { text: '#22c55e', border: '#22c55e' },
  BLOCKED: { text: '#ef4444', border: '#ef4444' },
  PENDING: { text: '#f59e0b', border: '#f59e0b' },
};

export default function Dashboard() {
  const { theme, toggle } = useTheme();
  const navigate = useNavigate();
  const [activeTab, setActiveTab] = useState<'pending' | 'logs' | 'policy'>('pending');
  const [pending, setPending] = useState<AuditEntry[]>([]);
  const [logs, setLogs] = useState<AuditEntry[]>([]);
  const [policy, setPolicy] = useState<Policy>(DEFAULT_POLICY);
  const [stats, setStats] = useState({ total: 0, allowed: 0, blocked: 0 });
  const [isPaused, setIsPaused] = useState(false);
  const isDark = theme === 'dark';

  useEffect(() => {
    const mockLogs: AuditEntry[] = [
      { id: '1', timestamp: Date.now() / 1000,       decision: 'ALLOWED', account: '7xFX...3kN9', intent: 'Swap 0.5 SOL to USDC via Jupiter', reason: 'Policy passed' },
      { id: '2', timestamp: Date.now() / 1000 - 60,  decision: 'BLOCKED', account: '9mB2...7pL4', intent: 'Transfer 10 SOL to unknown address',  reason: 'Exceeds max SOL per tx' },
      { id: '3', timestamp: Date.now() / 1000 - 120, decision: 'ALLOWED', account: '3fR5...8kM2', intent: 'Mint NFT via Metaplex',                reason: 'Whitelisted program' },
    ];
    setLogs(mockLogs);
    setStats({ total: 156, allowed: 142, blocked: 14 });
  }, []);

  const handleAllow = useCallback((id: string) => {
    setPending(prev => prev.filter(p => p.id !== id));
    setLogs(prev => [{ id, timestamp: Date.now() / 1000, decision: 'ALLOWED' as const, account: '', intent: 'Override: Allow', reason: 'Human approved' }, ...prev]);
    setStats(s => ({ ...s, total: s.total + 1, allowed: s.allowed + 1 }));
  }, []);

  const handleReject = useCallback((id: string) => {
    setPending(prev => prev.filter(p => p.id !== id));
    setLogs(prev => [{ id, timestamp: Date.now() / 1000, decision: 'BLOCKED' as const, account: '', intent: 'Override: Reject', reason: 'Human rejected' }, ...prev]);
    setStats(s => ({ ...s, total: s.total + 1, blocked: s.blocked + 1 }));
  }, []);

  const TABS = [
    { key: 'pending' as const, label: 'Pending Approvals' },
    { key: 'logs'    as const, label: 'Audit Logs' },
    { key: 'policy'  as const, label: 'Policy' },
  ];

  return (
    <div className="min-h-screen" style={{ background: 'var(--bg)' }}>

      {/* Shared Navbar (includes theme toggle) */}
      <Navbar />

      <main className="max-w-6xl mx-auto px-6 pb-20">

        {/* Page header */}
        <div className="flex items-center justify-between mb-8">
          <div>
            <h1 className="font-serif text-3xl font-normal" style={{ color: 'var(--text-primary)', letterSpacing: '-0.5px' }}>
              Firewall Dashboard
            </h1>
            <p className="font-sans text-sm mt-1" style={{ color: 'var(--text-muted)' }}>
              AI Agent Firewall for Solana — v0.3.0
            </p>
          </div>

          <div className="flex items-center gap-3">
            <span
              className="px-3 py-1 rounded-full text-xs font-sans font-semibold border"
              style={isPaused
                ? { background: 'rgba(239,68,68,0.1)', color: '#ef4444', borderColor: 'rgba(239,68,68,0.3)' }
                : { background: 'rgba(34,197,94,0.1)', color: '#22c55e', borderColor: 'rgba(34,197,94,0.3)' }
              }
            >
              {isPaused ? 'PAUSED' : 'LIVE'}
            </span>
            <WalletMultiButton />
          </div>
        </div>

        {/* Stats */}
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-4 mb-8">
          {[
            { label: 'Total Audits',  value: stats.total,                                                                                color: 'var(--text-primary)' },
            { label: 'Allowed',       value: stats.allowed,                                                                             color: '#22c55e' },
            { label: 'Blocked',       value: stats.blocked,                                                                             color: '#ef4444' },
            { label: 'Block Rate',    value: stats.total > 0 ? `${((stats.blocked / stats.total) * 100).toFixed(1)}%` : '0%',          color: '#f59e0b' },
          ].map(stat => (
            <div
              key={stat.label}
              className="rounded-xl p-4"
              style={{ background: 'var(--card-bg)', border: '1px solid var(--card-border)', boxShadow: 'var(--shadow)' }}
            >
              <p className="font-sans text-xs uppercase tracking-wider mb-1" style={{ color: 'var(--text-muted)' }}>
                {stat.label}
              </p>
              <p className="font-mono text-2xl font-bold tabular-nums" style={{ color: stat.color }}>
                {stat.value}
              </p>
            </div>
          ))}
        </div>

        {/* Tabs */}
        <div
          className="flex gap-1 mb-6 p-1 rounded-xl w-fit"
          style={{ background: 'var(--bg-subtle)', border: '1px solid var(--border)' }}
          role="tablist"
          aria-label="Dashboard sections"
        >
          {TABS.map(tab => (
            <button
              key={tab.key}
              role="tab"
              aria-selected={activeTab === tab.key}
              onClick={() => setActiveTab(tab.key)}
              className="px-4 py-2 rounded-lg font-sans text-sm font-medium transition-all duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
              style={activeTab === tab.key
                ? { background: 'var(--accent)', color: '#ffffff' }
                : { background: 'transparent', color: 'var(--text-muted)' }
              }
            >
              {tab.label}
            </button>
          ))}
        </div>

        {/* Tab panels */}
        <div
          className="rounded-2xl p-6"
          style={{ background: 'var(--card-bg)', border: '1px solid var(--card-border)', boxShadow: 'var(--shadow)' }}
          role="tabpanel"
        >

          {/* Pending */}
          {activeTab === 'pending' && (
            <div className="space-y-4">
              {pending.length === 0 ? (
                <p className="font-sans text-center py-12" style={{ color: 'var(--text-muted)' }}>
                  No transactions awaiting approval.
                </p>
              ) : (
                pending.map(item => (
                  <div
                    key={item.id}
                    className="p-4 rounded-xl border-l-4"
                    style={{ background: 'var(--bg-subtle)', borderLeftColor: '#f59e0b', border: '1px solid var(--border)', borderLeft: '4px solid #f59e0b' }}
                  >
                    <div className="flex justify-between mb-2">
                      <span className="font-sans font-semibold text-sm" style={{ color: '#f59e0b' }}>Pending Approval</span>
                      <span className="font-mono text-xs" style={{ color: 'var(--text-muted)' }}>{new Date(item.timestamp * 1000).toLocaleTimeString()}</span>
                    </div>
                    <p className="font-sans text-sm mb-1" style={{ color: 'var(--text-primary)' }}>{item.intent}</p>
                    <p className="font-sans text-xs mb-4" style={{ color: 'var(--text-muted)' }}>{item.reason}</p>
                    <div className="flex gap-2">
                      <button onClick={() => handleAllow(item.id)} className="btn-primary flex-1 text-sm">Allow</button>
                      <button onClick={() => handleReject(item.id)} className="btn-danger flex-1 text-sm">Reject</button>
                    </div>
                  </div>
                ))
              )}
            </div>
          )}

          {/* Logs */}
          {activeTab === 'logs' && (
            <div className="space-y-2">
              {logs.length === 0 ? (
                <p className="font-sans text-center py-12" style={{ color: 'var(--text-muted)' }}>No audit entries yet.</p>
              ) : (
                logs.map(log => (
                  <div
                    key={log.id}
                    className="p-3 rounded-lg"
                    style={{
                      background: 'var(--bg-subtle)',
                      border: `1px solid var(--border)`,
                      borderLeft: `3px solid ${DECISION_COLORS[log.decision].border}`,
                    }}
                  >
                    <div className="flex justify-between items-center">
                      <span className="font-mono text-xs font-semibold" style={{ color: DECISION_COLORS[log.decision].text }}>
                        {log.decision}
                      </span>
                      <span className="font-mono text-xs" style={{ color: 'var(--text-muted)' }}>
                        {new Date(log.timestamp * 1000).toLocaleTimeString()}
                      </span>
                    </div>
                    <p className="font-sans text-sm mt-1" style={{ color: 'var(--text-primary)' }}>{log.intent}</p>
                    {log.account && (
                      <p className="font-mono text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>{log.account}</p>
                    )}
                  </div>
                ))
              )}
            </div>
          )}

          {/* Policy */}
          {activeTab === 'policy' && (
            <div className="space-y-6 max-w-lg">
              <div>
                <label htmlFor="max-sol" className="block font-sans text-sm font-medium mb-2" style={{ color: 'var(--text-primary)' }}>
                  Max SOL per Transaction
                </label>
                <input
                  id="max-sol"
                  type="number"
                  min="0"
                  value={policy.maxSolPerTx}
                  onChange={e => setPolicy(p => ({ ...p, maxSolPerTx: Number(e.target.value) }))}
                  className="w-full p-3 rounded-lg font-mono text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
                  style={{ background: 'var(--bg-subtle)', border: '1px solid var(--border)', color: 'var(--text-primary)' }}
                />
              </div>

              <div>
                <label htmlFor="rate-limit" className="block font-sans text-sm font-medium mb-2" style={{ color: 'var(--text-primary)' }}>
                  Rate Limit (transactions per minute)
                </label>
                <input
                  id="rate-limit"
                  type="number"
                  min="1"
                  value={policy.rateLimit}
                  onChange={e => setPolicy(p => ({ ...p, rateLimit: Number(e.target.value) }))}
                  className="w-full p-3 rounded-lg font-mono text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
                  style={{ background: 'var(--bg-subtle)', border: '1px solid var(--border)', color: 'var(--text-primary)' }}
                />
              </div>

              <div>
                <p className="block font-sans text-sm font-medium mb-2" style={{ color: 'var(--text-primary)' }}>
                  Allowed Programs
                </p>
                <div className="space-y-1">
                  {policy.allowedPrograms.map((prog, i) => (
                    <code
                      key={i}
                      className="block font-mono text-xs p-2 rounded"
                      style={{ background: 'var(--bg-subtle)', border: '1px solid var(--border)', color: 'var(--text-muted)' }}
                    >
                      {prog.slice(0, 12)}...{prog.slice(-8)}
                    </code>
                  ))}
                </div>
              </div>

              <button
                onClick={() => setIsPaused(!isPaused)}
                className="w-full py-3 rounded-xl font-sans font-semibold text-sm transition-all duration-150 hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-offset-2"
                style={isPaused
                  ? { background: '#22c55e', color: '#ffffff', focusVisible: { ringColor: '#22c55e' } }
                  : { background: '#dc2626', color: '#ffffff' }
                }
              >
                {isPaused ? 'Resume Protocol' : 'Pause Protocol (Emergency)'}
              </button>
            </div>
          )}
        </div>
      </main>
    </div>
  );
}
