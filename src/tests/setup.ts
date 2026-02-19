/**
 * Vitest test setup file.
 *
 * Sets up:
 * - Testing Library DOM matchers
 * - Tauri API mocks
 * - Global test utilities
 */

import '@testing-library/jest-dom/vitest';
import { vi, beforeEach, afterEach } from 'vitest';

// ============================================================================
// TAURI MOCK INFRASTRUCTURE
// ============================================================================

export type MockInvokeHandler = (cmd: string, args?: unknown) => unknown;

let mockInvokeHandler: MockInvokeHandler = () => undefined;

/**
 * Set a custom handler for invoke calls during tests.
 */
export function setMockInvokeHandler(handler: MockInvokeHandler): void {
  mockInvokeHandler = handler;
}

/**
 * Create a simple mock for a specific command.
 */
export function mockInvoke(cmd: string, response: unknown): void {
  const prevHandler = mockInvokeHandler;
  mockInvokeHandler = (c, args) => {
    if (c === cmd) return response;
    return prevHandler(c, args);
  };
}

/**
 * Create a mock that throws for a specific command.
 */
export function mockInvokeError(cmd: string, error: Error): void {
  const prevHandler = mockInvokeHandler;
  mockInvokeHandler = (c, args) => {
    if (c === cmd) throw error;
    return prevHandler(c, args);
  };
}

// Mock Tauri core module
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string, args?: unknown) => {
    return Promise.resolve(mockInvokeHandler(cmd, args));
  }),
}));

// Track active listeners for cleanup
type ListenerCallback = (event: { payload: unknown }) => void;
const activeListeners: Map<string, Set<ListenerCallback>> = new Map();

/**
 * Emit a mock event to all listeners.
 */
export function emitMockEvent(eventName: string, payload: unknown): void {
  const listeners = activeListeners.get(eventName);
  if (listeners) {
    listeners.forEach((callback) => {
      callback({ payload });
    });
  }
}

// Mock Tauri event module
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((eventName: string, callback: ListenerCallback) => {
    if (!activeListeners.has(eventName)) {
      activeListeners.set(eventName, new Set());
    }
    activeListeners.get(eventName)!.add(callback);

    // Return unlisten function
    const unlisten = () => {
      activeListeners.get(eventName)?.delete(callback);
    };
    return Promise.resolve(unlisten);
  }),
  emit: vi.fn(),
}));

// ============================================================================
// TEST LIFECYCLE HOOKS
// ============================================================================

beforeEach(() => {
  // Reset mock handler before each test
  mockInvokeHandler = () => undefined;

  // Clear all listeners
  activeListeners.clear();
});

afterEach(() => {
  // Clean up any remaining listeners
  activeListeners.clear();

  // Clear all mocks
  vi.clearAllMocks();
});

// ============================================================================
// TEST UTILITIES
// ============================================================================

/**
 * Wait for a condition to be true.
 */
export async function waitFor(
  condition: () => boolean,
  timeout = 1000
): Promise<void> {
  const start = Date.now();
  while (!condition()) {
    if (Date.now() - start > timeout) {
      throw new Error('waitFor timeout');
    }
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}

/**
 * Create a mock audio device.
 */
export function createMockDevice(overrides: Partial<{
  uid: string;
  name: string;
  is_default: boolean;
  sample_rate: number;
  channels: number;
}> = {}) {
  return {
    uid: 'test-device-uid',
    name: 'Test Microphone',
    is_default: false,
    sample_rate: 48000,
    channels: 1,
    ...overrides,
  };
}

/**
 * Create a mock transcript entry.
 */
export function createMockTranscript(overrides: Partial<{
  id: string;
  text: string;
  timestamp: string;
  audio_duration_ms: number;
  transcription_duration_ms: number;
  injection_result:
    | { status: 'injected' }
    | { status: 'clipboard_only'; reason: string }
    | { status: 'error'; message: string };
  timings: Partial<{
    ipc_ms: number;
    transcribe_ms: number;
    postprocess_ms: number;
    inject_ms: number;
    total_ms: number;
  }>;
}> = {}) {
  return {
    id: 'test-transcript-id',
    text: 'Hello, world!',
    timestamp: new Date().toISOString(),
    audio_duration_ms: 2000,
    transcription_duration_ms: 500,
    injection_result: { status: 'injected' as const },
    ...overrides,
  };
}

/**
 * Create a mock model status.
 */
export function createMockModelStatus(overrides: Partial<{
  status: string;
  model_id: string;
  error?: string;
}> = {}) {
  return {
    status: 'ready',
    model_id: 'parakeet-rnnt-1.1b',
    ...overrides,
  };
}

/**
 * Create a mock app config.
 */
export function createMockConfig() {
  return {
    audio: {
      device_uid: null,
      sample_rate: 16000,
      channels: 1,
    },
    hotkeys: {
      primary: 'Ctrl+Shift+Space',
      copy_last: 'Ctrl+Shift+C',
      mode: 'toggle',
    },
    injection: {
      method: 'clipboard',
      paste_delay_ms: 50,
      auto_paste: true,
    },
    model: {
      model_id: 'parakeet-rnnt-1.1b',
    },
    ui: {
      show_tray_icon: true,
      start_minimized: false,
    },
    replacements: [],
  };
}
