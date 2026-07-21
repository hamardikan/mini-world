import { useEffect, useRef } from 'react';
import type { Agent, SnapshotV1 } from './types';

type MapViewProps = {
  snapshot: SnapshotV1;
  selectedAgent: number;
  onSelectAgent: (index: number) => void;
};

const tileColors = ['#18232d', '#263b43', '#394a43', '#4a3d36', '#344534'];

export function CanvasMap({ snapshot }: { snapshot: SnapshotV1 }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    let context: CanvasRenderingContext2D | null;
    try {
      context = canvas.getContext('2d');
    } catch {
      return;
    }
    if (!context) return;
    const { width, height, tiles } = snapshot.grid;
    const scale = 32;
    canvas.width = width * scale;
    canvas.height = height * scale;
    context.clearRect(0, 0, canvas.width, canvas.height);
    context.fillStyle = '#10171e';
    context.fillRect(0, 0, canvas.width, canvas.height);
    for (let y = 0; y < height; y += 1) {
      for (let x = 0; x < width; x += 1) {
        context.fillStyle = tileColors[tiles[y * width + x] ?? 0] ?? tileColors[0];
        context.fillRect(x * scale + 1, y * scale + 1, scale - 2, scale - 2);
      }
    }
    for (const [agentIndex, agent] of snapshot.agents.entries()) {
      const [x, y] = agent.position;
      const centerX = x * scale + scale / 2;
      const centerY = y * scale + scale / 2;
      context.fillStyle = '#d8f3dc';
      context.fillRect(centerX - 7, centerY - 7, 14, 14);
      context.strokeStyle = '#0b1116';
      context.lineWidth = 2;
      context.strokeRect(centerX - 7, centerY - 7, 14, 14);
      context.fillStyle = '#17313c';
      context.font = '600 10px ui-monospace, monospace';
      context.textAlign = 'center';
      context.textBaseline = 'middle';
      context.fillText(String(agentIndex + 1), centerX, centerY);
    }
  }, [snapshot]);

  return (
    <canvas
      ref={canvasRef}
      className="world-canvas"
      role="img"
      aria-label="Visual 16 by 16 village map. Use the agent table for accessible map details."
    />
  );
}

function AgentRow({ agent, agentIndex, selected, onSelect }: { agent: Agent; agentIndex: number; selected: boolean; onSelect: () => void }) {
  return (
    <tr className={selected ? 'agent-row is-selected' : 'agent-row'}>
      <td>
        <button
          type="button"
          className="agent-select"
          aria-label={`Select agent ${agent.id.index} generation ${agent.id.generation}`}
          aria-pressed={selected}
          onClick={onSelect}
          onKeyDown={(event) => {
            const isArrow = event.key === 'ArrowDown' || event.key === 'ArrowUp';
            const isTab = event.key === 'Tab';
            if (!isArrow && !isTab) return;
            event.preventDefault();
            const rows = Array.from(document.querySelectorAll<HTMLButtonElement>('.agent-select'));
            const direction = event.key === 'ArrowUp' || (isTab && event.shiftKey) ? -1 : 1;
            const next = (agentIndex + direction + rows.length) % rows.length;
            rows[next]?.focus();
            rows[next]?.click();
          }}
        >
          <span className="selection-mark" aria-hidden="true">{selected ? '▣' : '□'}</span>
          <span>{agent.id.index}:{agent.id.generation}</span>
        </button>
      </td>
      <td className="mono">{agent.position[0]}, {agent.position[1]}</td>
      <td className="selection-state">{selected ? 'selected' : 'available'}</td>
    </tr>
  );
}

export function AgentMirror({ snapshot, selectedAgent, onSelectAgent }: MapViewProps) {
  return (
    <section className="mirror-panel" aria-labelledby="agent-list-heading">
      <div className="section-heading-row">
        <div>
          <h2 id="agent-list-heading">Agents</h2>
          <p className="section-note">Keyboard focus and selection mirror the map.</p>
        </div>
        <span className="count-badge">{snapshot.agents.length} visible</span>
      </div>
      <div className="table-wrap">
        <table className="agent-table">
          <caption className="visually-hidden">Accessible agent positions and selection state</caption>
          <thead>
            <tr><th scope="col">Identity</th><th scope="col">Position</th><th scope="col">State</th></tr>
          </thead>
          <tbody>
            {snapshot.agents.map((agent, index) => (
              <AgentRow
                key={`${agent.id.index}:${agent.id.generation}`}
                agent={agent}
                agentIndex={index}
                selected={index === selectedAgent}
                onSelect={() => onSelectAgent(index)}
              />
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

export function MapView(props: MapViewProps) {
  return (
    <div className="map-layout">
      <section className="map-panel" aria-labelledby="map-heading">
        <div className="section-heading-row">
          <div>
            <h2 id="map-heading">World map</h2>
            <p className="section-note">Canvas projection. The table is the accessible source of truth.</p>
          </div>
          <span className="map-dimensions mono">{props.snapshot.grid.width} × {props.snapshot.grid.height}</span>
        </div>
        <div className="canvas-frame"><CanvasMap snapshot={props.snapshot} /></div>
        <p className="map-legend"><span className="legend-swatch" aria-hidden="true" /> Agents are marked with a numbered square.</p>
      </section>
      <AgentMirror {...props} />
    </div>
  );
}
