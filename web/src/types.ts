export const WEB_DTO_VERSION = 1 as const;

export type RunProvenance = {
  policy_id: string;
  model_hash: string;
  backend_id: string;
  expertise: string;
};

export type Scenario = {
  id: string;
  version: number;
};

export type Agent = {
  id: {
    index: number;
    generation: number;
  };
  position: [number, number];
};

export type SnapshotV1 = {
  schema_version: number;
  run_id: string;
  seed: string;
  scenario: Scenario;
  tick: string;
  state_hash: string;
  run_provenance: RunProvenance;
  grid: {
    width: number;
    height: number;
    tiles: number[];
  };
  agents: Agent[];
  event_seq: string;
};

export type CapabilitiesV1 = {
  schema_version: number;
  run_id: string;
  run_provenance: RunProvenance;
  dto_versions: Record<string, number>;
  scenario: Scenario;
  commands: Array<{
    type: string;
    ticks: { min: number; max: number };
  }>;
  limits: {
    command_queue: string;
    event_ring: string;
  };
};

export type CommandV1 = {
  schema_version: 1;
  run_id: string;
  command_id: string;
  expected_tick: string;
  command: { type: 'step'; ticks: 1 };
};

export type CommandResultV1 = {
  schema_version: number;
  run_id: string;
  command_id: string;
  applied_tick: string;
  state_hash: string;
  event_seq: string;
};

export type ErrorCode =
  | 'idempotency_conflict'
  | 'tick_conflict'
  | 'malformed'
  | 'schema'
  | 'run_conflict'
  | 'snapshot_required';

export type ErrorV1 = {
  schema_version: number;
  code: ErrorCode;
  message: string;
};

export type EventV1 = {
  schema_version: number;
  run_id: string;
  event_seq: string;
  tick: string;
  state_hash: string;
  event_type: string;
  payload?: Record<string, unknown>;
};

export type ConnectionState = 'connecting' | 'live' | 'disconnected' | 'gap';
export type CommandState =
  | { kind: 'idle' }
  | { kind: 'pending'; commandId: string }
  | { kind: 'ok'; result: CommandResultV1 }
  | { kind: 'error'; error: ErrorV1 | { code: string; message: string } };
