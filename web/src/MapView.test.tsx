import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { MapView } from './MapView';
import type { SnapshotV1 } from './types';

const snapshot: SnapshotV1 = {
  schema_version: 1,
  run_id: 'test-run',
  seed: '18446744073709551615',
  scenario: { id: 'village', version: 1 },
  tick: '120',
  state_hash: '0x0123456789abcdef',
  run_provenance: { policy_id: 'utility-v0', model_hash: 'sha256:test', backend_id: 'rust-utility', expertise: 'capable' },
  grid: { width: 16, height: 16, tiles: Array(256).fill(0) },
  agents: [
    { id: { index: 0, generation: 0 }, position: [8, 8] },
    { id: { index: 7, generation: 2 }, position: [2, 11] },
  ],
  event_seq: '240',
};

describe('MapView', () => {
  it('renders the visual map and authoritative accessible agent mirror', () => {
    render(<MapView snapshot={snapshot} selectedAgent={1} onSelectAgent={() => undefined} />);

    expect(screen.getByRole('img', { name: /visual 16 by 16 village map/i })).toBeInTheDocument();
    expect(screen.getByRole('table', { name: /accessible agent positions/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /agent 7 generation 2/i })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByText('2, 11')).toBeInTheDocument();
    expect(screen.getByText('selected')).toBeInTheDocument();
  });
});
