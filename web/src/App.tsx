import { useEffect, useMemo, useState } from 'react';
import * as Toggle from '@radix-ui/react-toggle';
import { MapView } from './MapView';
import { useSimulation } from './useSimulation';
import type { CommandState, ConnectionState } from './types';

const connectionCopy: Record<ConnectionState, { icon: string; label: string }> = {
  connecting: { icon: '◌', label: 'Connecting' },
  live: { icon: '●', label: 'Live' },
  disconnected: { icon: '×', label: 'Disconnected' },
  gap: { icon: '!', label: 'Snapshot recovery' },
};

function CommandStatus({ command }: { command: CommandState }) {
  if (command.kind === 'idle') return <span className="status-value">Idle</span>;
  if (command.kind === 'pending') return <span className="status-value pending">Pending <span className="mono">{command.commandId.slice(0, 8)}</span></span>;
  if (command.kind === 'ok') return <span className="status-value success">Accepted at tick {command.result.applied_tick}</span>;
  return <span className="status-value error">Error: {command.error.code}. {command.error.message}</span>;
}

function ConnectionStatus({ state }: { state: ConnectionState }) {
  const copy = connectionCopy[state];
  return <span className={`connection-value state-${state}`}><span aria-hidden="true">{copy.icon}</span> {copy.label}</span>;
}

function Provenance({ policy, backend, expertise }: { policy: string; backend: string; expertise: string }) {
  return (
    <section className="provenance" aria-labelledby="provenance-heading">
      <h2 id="provenance-heading">Run provenance</h2>
      <dl className="provenance-grid">
        <div><dt>Policy</dt><dd>{policy}</dd></div>
        <div><dt>Backend</dt><dd>{backend}</dd></div>
        <div><dt>Expertise</dt><dd>{expertise}</dd></div>
      </dl>
    </section>
  );
}

export default function App() {
  const { capabilities, snapshot, connection, command, sendStep } = useSimulation();
  const [selectedAgent, setSelectedAgent] = useState(0);
  const stepEnabled = useMemo(
    () => capabilities?.commands.some((entry) => entry.type === 'step' && entry.ticks.min === 1 && entry.ticks.max === 1) ?? false,
    [capabilities],
  );

  useEffect(() => {
    const handleKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (event.key.toLowerCase() !== 's' || target?.matches('input, textarea, select, button, [contenteditable="true"]')) return;
      event.preventDefault();
      void sendStep();
    };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [sendStep]);

  useEffect(() => {
    if (snapshot && selectedAgent >= snapshot.agents.length) setSelectedAgent(Math.max(snapshot.agents.length - 1, 0));
  }, [snapshot, selectedAgent]);

  return (
    <div className="app-shell">
      <header className="topbar">
        <div className="brand-lockup">
          <span className="brand-mark" aria-hidden="true">MW</span>
          <div><p className="product-name">Mini World</p><p className="product-subtitle">Simulation observatory</p></div>
        </div>
        <div className="run-chip" aria-label={snapshot ? `Run ${snapshot.run_id}` : 'No active run'}>
          <span className="run-label">Run</span><span className="mono">{snapshot?.run_id ?? 'awaiting host'}</span>
        </div>
      </header>

      <main>
        <div className="intro-row">
          <div>
            <p className="eyebrow">Local control surface</p>
            <h1>Observe the village as it runs.</h1>
            <p className="intro-copy">A host-owned projection for deterministic simulation research.</p>
          </div>
          <div className="connection-card" role="status" aria-live="polite">
            <span className="meta-label">Connection</span>
            <ConnectionStatus state={connection} />
          </div>
        </div>

        <section className="status-grid" aria-label="Run status">
          <div className="status-cell"><span className="meta-label">Tick</span><strong className="mono">{snapshot?.tick ?? '...'}</strong></div>
          <div className="status-cell hash-cell"><span className="meta-label">State hash</span><strong className="mono hash-value">{snapshot?.state_hash ?? '...'}</strong></div>
          <div className="status-cell"><span className="meta-label">Event cursor</span><strong className="mono">{snapshot?.event_seq ?? '...'}</strong></div>
          <div className="status-cell command-cell"><span className="meta-label">Last command</span><CommandStatus command={command} /></div>
        </section>

        {!capabilities || !snapshot ? (
          <section className="loading-panel" aria-live="polite">
            <span className="loading-mark" aria-hidden="true">{connection === 'disconnected' ? '!' : '...'}</span>
            <h2>{connection === 'disconnected' ? 'Host unavailable' : 'Loading run projection'}</h2>
            <p>{connection === 'disconnected' ? 'Start the loopback host and reload this page to reconnect.' : 'Capabilities are loaded before any control is exposed.'}</p>
          </section>
        ) : (
          <>
            <section className="control-panel" aria-labelledby="control-heading">
              <div><h2 id="control-heading">Control</h2><p className="section-note">Commands are enabled by the host capability response.</p></div>
              {stepEnabled ? (
                <Toggle.Root
                  className="step-button"
                  type="button"
                  pressed={command.kind === 'pending'}
                  disabled={command.kind === 'pending'}
                  aria-label="Step one simulation tick"
                  onClick={() => void sendStep()}
                >
                  <span aria-hidden="true">›</span> Step one tick <kbd>S</kbd>
                </Toggle.Root>
              ) : <p className="disabled-control">Step is not advertised by this run.</p>}
            </section>
            <MapView snapshot={snapshot} selectedAgent={selectedAgent} onSelectAgent={setSelectedAgent} />
            <Provenance policy={snapshot.run_provenance.policy_id} backend={snapshot.run_provenance.backend_id} expertise={snapshot.run_provenance.expertise} />
          </>
        )}
      </main>
      <div className="sr-status" role="status" aria-live="assertive"><CommandStatus command={command} /></div>
    </div>
  );
}
