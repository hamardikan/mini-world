import { useCallback, useEffect, useRef, useState } from 'react';
import { API_BASE, getCapabilities, getSnapshot, postStep } from './api';
import type {
  CapabilitiesV1,
  CommandState,
  ConnectionState,
  EventV1,
  SnapshotV1,
} from './types';

function decimalParts(value: string): [string, number] {
  const normalized = value.replace(/^0+(?=\d)/, '');
  return [normalized || '0', normalized.length];
}

function compareDecimal(left: string, right: string): number {
  const [a, al] = decimalParts(left);
  const [b, bl] = decimalParts(right);
  if (/^\d+$/.test(a) && /^\d+$/.test(b)) {
    if (al !== bl) return al > bl ? 1 : -1;
    return a === b ? 0 : a > b ? 1 : -1;
  }
  return left === right ? 0 : left > right ? 1 : -1;
}

function commandId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `cmd-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

function isError(value: unknown): value is { code: string; message: string } {
  return typeof value === 'object' && value !== null && 'code' in value && 'message' in value;
}

export function useSimulation() {
  const [capabilities, setCapabilities] = useState<CapabilitiesV1 | null>(null);
  const [snapshot, setSnapshot] = useState<SnapshotV1 | null>(null);
  const [connection, setConnection] = useState<ConnectionState>('connecting');
  const [command, setCommand] = useState<CommandState>({ kind: 'idle' });
  const snapshotRef = useRef<SnapshotV1 | null>(null);
  const eventSourceRef = useRef<EventSource | null>(null);
  const reconnectTimerRef = useRef<number | undefined>(undefined);
  const mountedRef = useRef(true);

  const storeSnapshot = useCallback((next: SnapshotV1) => {
    snapshotRef.current = next;
    setSnapshot(next);
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    const controller = new AbortController();
    let initialized = false;

    const recover = async () => {
      if (!mountedRef.current) return;
      setConnection('gap');
      try {
        const nextCapabilities = await getCapabilities(controller.signal);
        if (!mountedRef.current) return;
        setCapabilities(nextCapabilities);
        const next = await getSnapshot(controller.signal);
        if (!mountedRef.current) return;
        storeSnapshot(next);
        setConnection('live');
        connect(next.event_seq);
      } catch {
        if (mountedRef.current) setConnection('disconnected');
      }
    };

    const applyEvent = (event: EventV1) => {
      if (!mountedRef.current) return;
      const current = snapshotRef.current;
      if (!current || event.run_id !== current.run_id) {
        void recover();
        return;
      }
      if (event.event_type === 'snapshot_required') {
        void recover();
        return;
      }
      if (compareDecimal(event.event_seq, current.event_seq) <= 0) return;
      const actor = event.payload?.actor;
      const to = event.payload?.to;
      const agents = current.agents.map((agent) => {
        if (
          event.event_type === 'moved' &&
          typeof actor === 'object' &&
          actor !== null &&
          'index' in actor &&
          'generation' in actor &&
          Array.isArray(to) &&
          to.length === 2 &&
          typeof to[0] === 'number' &&
          typeof to[1] === 'number' &&
          agent.id.index === actor.index &&
          agent.id.generation === actor.generation
        ) {
          return { ...agent, position: [to[0], to[1]] as [number, number] };
        }
        return agent;
      });
      storeSnapshot({ ...current, tick: event.tick, state_hash: event.state_hash, event_seq: event.event_seq, agents });
      setConnection('live');
    };

    const handleMessage = (message: MessageEvent<string>) => {
      try {
        applyEvent(JSON.parse(message.data) as EventV1);
      } catch {
        setConnection('gap');
        void recover();
      }
    };

    function connect(afterSeq: string) {
      if (!mountedRef.current) return;
      eventSourceRef.current?.close();
      const query = afterSeq ? `?after_seq=${encodeURIComponent(afterSeq)}` : '';
      const source = new EventSource(`${API_BASE}/events${query}`);
      eventSourceRef.current = source;
      source.onopen = () => mountedRef.current && setConnection('live');
      source.onerror = () => {
        if (!mountedRef.current) return;
        source.close();
        setConnection('disconnected');
        window.clearTimeout(reconnectTimerRef.current);
        reconnectTimerRef.current = window.setTimeout(() => {
          const cursor = snapshotRef.current?.event_seq ?? '';
          connect(cursor);
        }, 1000);
      };
      source.onmessage = handleMessage;
      for (const type of ['moved', 'interacted', 'spoke', 'rejected', 'command_result', 'tick', 'step_applied', 'snapshot_required']) {
        source.addEventListener(type, handleMessage as EventListener);
      }
    }

    const initialize = async () => {
      try {
        const nextCapabilities = await getCapabilities(controller.signal);
        if (!mountedRef.current) return;
        setCapabilities(nextCapabilities);
        const nextSnapshot = await getSnapshot(controller.signal);
        if (!mountedRef.current) return;
        storeSnapshot(nextSnapshot);
        initialized = true;
        connect(nextSnapshot.event_seq);
      } catch {
        if (mountedRef.current && !initialized) setConnection('disconnected');
      }
    };
    void initialize();

    return () => {
      mountedRef.current = false;
      controller.abort();
      eventSourceRef.current?.close();
      window.clearTimeout(reconnectTimerRef.current);
    };
  }, [storeSnapshot]);

  const sendStep = useCallback(async () => {
    const current = snapshotRef.current;
    const enabled = capabilities?.commands.some(
      (entry) => entry.type === 'step' && entry.ticks.min === 1 && entry.ticks.max === 1,
    );
    if (!current || !enabled || command.kind === 'pending') return;
    const id = commandId();
    setCommand({ kind: 'pending', commandId: id });
    const body = {
      schema_version: 1 as const,
      run_id: current.run_id,
      command_id: id,
      expected_tick: current.tick,
      command: { type: 'step' as const, ticks: 1 as const },
    };
    try {
      const result = await postStep(body);
      if (mountedRef.current) setCommand({ kind: 'ok', result });
    } catch (error) {
      if (!mountedRef.current) return;
      setCommand({
        kind: 'error',
        error: isError(error) ? error : { code: 'network_error', message: 'Command request failed.' },
      });
    }
  }, [capabilities, command.kind]);

  return { capabilities, snapshot, connection, command, sendStep };
}
