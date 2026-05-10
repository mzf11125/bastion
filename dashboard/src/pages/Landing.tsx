import { useNavigate } from 'react-router-dom';
import { useWallet } from '@solana/wallet-adapter-react';
import { useWalletModal } from '@solana/wallet-adapter-react-ui';
import { Navbar } from '../components/Navbar';
import { VideoBackground } from '../components/VideoBackground';

const FEATURES = [
  {
    title: 'Transaction Simulation',
    description:
      'Every transaction is simulated against live Solana state via Helius before signing. Balance drain, error codes, and compute units are checked in real time.',
    tag: 'Core',
  },
  {
    title: 'On-Chain Audit Trail',
    description:
      'Every decision — allowed, blocked, or pending — is recorded as an immutable Anchor PDA on Solana. Verifiable on-chain by anyone, at any time.',
    tag: 'Unique',
  },
  {
    title: 'Policy Engine',
    description:
      'Configurable SOL caps, rate limits, program allowlists, and Blockint security checks. Policy runs before simulation; nothing reaches the chain without passing both layers.',
    tag: 'Core',
  },
  {
    title: 'Circuit Breaker',
    description:
      'One command pauses all transaction processing across your agent fleet. Designed for emergencies. Resumes instantly when the threat passes.',
    tag: 'Safety',
  },
  {
    title: 'Agent Identity Registry',
    description:
      'Every agent gets an on-chain identity PDA tied to its authority key. Reputation compounds across sessions. The first agent registry on Solana.',
    tag: 'Unique',
  },
  {
    title: 'Human Override',
    description:
      'Blocked transactions surface in the dashboard for human review. Approve or reject with one click. Full audit log preserved regardless of decision.',
    tag: 'Control',
  },
];

const TAG_COLORS: Record<string, string> = {
  Core:    'bg-blue-500/10 text-blue-400 border border-blue-500/20',
  Unique:  'bg-sky-500/10 text-sky-400 border border-sky-500/20',
  Safety:  'bg-red-500/10 text-red-400 border border-red-500/20',
  Control: 'bg-amber-500/10 text-amber-400 border border-amber-500/20',
};

const FLOW_STEPS = [
  { label: 'AI Agent', sub: 'Constructs transaction' },
  { label: 'Bastion', sub: 'Policy + simulation check' },
  { label: 'Helius', sub: 'Live state simulation' },
  { label: 'Solana', sub: 'On-chain audit recorded' },
];

export default function Landing() {
  const { connected } = useWallet();
  const { setVisible } = useWalletModal();
  const navigate = useNavigate();

  function handleCTA() {
    if (connected) {
      navigate('/dashboard');
    } else {
      setVisible(true);
    }
  }

  return (
    <div className="relative min-h-screen w-full overflow-x-hidden" style={{ background: 'var(--bg)' }}>

      {/* ── Video background ── */}
      <VideoBackground />

      {/* ── Navbar ── */}
      <Navbar />

      {/* ── Hero Section ── */}
      <section
        className="relative z-10 flex flex-col items-center justify-center text-center px-6 pb-40"
        style={{ paddingTop: 'calc(8rem - 75px)' }}
        aria-labelledby="hero-headline"
      >
        <h1
          id="hero-headline"
          className="animate-fade-rise font-serif max-w-5xl"
          style={{
            fontSize: 'clamp(2.75rem, 8vw, 5.5rem)',
            lineHeight: '0.95',
            letterSpacing: '-2.46px',
            fontWeight: 400,
            color: 'var(--text-primary)',
          }}
        >
          Beyond security,{' '}
          <em style={{ color: 'var(--text-muted)', fontStyle: 'italic' }}>we build</em>{' '}
          the verifiable.
        </h1>

        <p
          className="animate-fade-rise-delay font-sans mt-8 max-w-2xl text-base sm:text-lg leading-relaxed"
          style={{ color: 'var(--text-muted)' }}
        >
          Building infrastructure for brilliant agents, fearless developers, and decentralized
          protocols. Through the noise, we craft a firewall for pure execution.
        </p>

        <button
          onClick={handleCTA}
          className="animate-fade-rise-delay-2 mt-12 rounded-full px-14 py-5 text-base font-medium font-sans transition-transform duration-150 hover:scale-[1.03] active:scale-[0.98] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)] focus-visible:ring-offset-2"
          style={{ background: 'var(--text-primary)', color: 'var(--bg)' }}
        >
          Go to Dashboard
        </button>
      </section>

      {/* ── Architecture Flow ── */}
      <section className="relative z-10 max-w-4xl mx-auto px-6 py-24" aria-label="How Bastion works">
        <h2
          className="font-serif text-center mb-16"
          style={{
            fontSize: 'clamp(1.75rem, 4vw, 2.5rem)',
            letterSpacing: '-1px',
            fontWeight: 400,
            color: 'var(--text-primary)',
          }}
        >
          How it works
        </h2>

        <div className="flex flex-col sm:flex-row items-center justify-center gap-0">
          {FLOW_STEPS.map((step, i) => (
            <div key={step.label} className="flex flex-col sm:flex-row items-center">
              {/* Step box */}
              <div
                className="flex flex-col items-center text-center px-6 py-4 rounded-xl"
                style={{ background: 'var(--card-bg)', border: '1px solid var(--card-border)', minWidth: '140px' }}
              >
                <span className="font-sans font-semibold text-sm" style={{ color: 'var(--text-primary)' }}>
                  {step.label}
                </span>
                <span className="font-sans text-xs mt-1" style={{ color: 'var(--text-muted)' }}>
                  {step.sub}
                </span>
              </div>

              {/* Arrow */}
              {i < FLOW_STEPS.length - 1 && (
                <div
                  className="w-8 h-px sm:h-px sm:w-8 my-2 sm:my-0 sm:mx-1 flex-shrink-0"
                  style={{ background: 'var(--border)' }}
                  aria-hidden="true"
                >
                  <span className="sr-only">to</span>
                </div>
              )}
            </div>
          ))}
        </div>
      </section>

      {/* ── Bento Feature Grid ── */}
      <section className="relative z-10 max-w-6xl mx-auto px-6 pb-40" aria-labelledby="features-heading">
        <h2
          id="features-heading"
          className="font-serif text-center mb-16"
          style={{
            fontSize: 'clamp(1.75rem, 4vw, 2.5rem)',
            letterSpacing: '-1px',
            fontWeight: 400,
            color: 'var(--text-primary)',
          }}
        >
          Every layer, defended.
        </h2>

        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
          {FEATURES.map((feature) => (
            <article
              key={feature.title}
              className="group relative rounded-2xl p-6 transition-shadow duration-200"
              style={{
                background: 'var(--card-bg)',
                border: '1px solid var(--card-border)',
                boxShadow: 'var(--shadow)',
              }}
            >
              <div className="flex items-start justify-between mb-4">
                <h3
                  className="font-sans font-semibold text-base pr-4"
                  style={{ color: 'var(--text-primary)' }}
                >
                  {feature.title}
                </h3>
                <span className={`text-xs font-mono font-medium px-2 py-0.5 rounded-full flex-shrink-0 ${TAG_COLORS[feature.tag]}`}>
                  {feature.tag}
                </span>
              </div>
              <p className="font-sans text-sm leading-relaxed" style={{ color: 'var(--text-muted)' }}>
                {feature.description}
              </p>
            </article>
          ))}
        </div>
      </section>

      {/* ── Final CTA ── */}
      <section
        className="relative z-10 flex flex-col items-center text-center px-6 pb-32"
        aria-label="Call to action"
      >
        <h2
          className="font-serif max-w-2xl"
          style={{
            fontSize: 'clamp(1.75rem, 4vw, 2.75rem)',
            letterSpacing: '-1px',
            fontWeight: 400,
            color: 'var(--text-primary)',
          }}
        >
          Your agents deserve a firewall.
        </h2>
        <p
          className="font-sans mt-4 max-w-md text-base leading-relaxed"
          style={{ color: 'var(--text-muted)' }}
        >
          Open source. MIT licensed. Built on Solana.
        </p>
        <button
          onClick={handleCTA}
          className="mt-8 rounded-full px-12 py-4 text-base font-medium font-sans transition-transform duration-150 hover:scale-[1.03] active:scale-[0.98] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)] focus-visible:ring-offset-2"
          style={{ background: 'var(--text-primary)', color: 'var(--bg)' }}
        >
          Go to Dashboard
        </button>
      </section>

      {/* ── Footer ── */}
      <footer
        className="relative z-10 border-t px-8 py-8 max-w-7xl mx-auto flex flex-col sm:flex-row justify-between items-center gap-4"
        style={{ borderColor: 'var(--border)' }}
      >
        <span className="font-serif text-xl" style={{ color: 'var(--text-primary)' }}>
          Bastion<sup className="text-xs align-super">®</sup>
        </span>
        <span className="font-sans text-sm" style={{ color: 'var(--text-muted)' }}>
          Bastion v0.3.0 — AI Agent Firewall for Solana
        </span>
        <a
          href="https://github.com/bastion-defend/bastion"
          target="_blank"
          rel="noopener noreferrer"
          className="font-sans text-sm transition-opacity hover:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)] rounded"
          style={{ color: 'var(--text-muted)', opacity: 0.7 }}
        >
          GitHub
        </a>
      </footer>
    </div>
  );
}
