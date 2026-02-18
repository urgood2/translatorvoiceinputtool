/**
 * TypeScript types for Tauri commands.
 *
 * These types match the Rust types defined in src-tauri/src/commands.rs
 * and related modules.
 */

// ============================================================================
// STATE TYPES
// ============================================================================

/** Application state. */
export type AppState =
  | 'idle'
  | 'loading_model'
  | 'recording'
  | 'transcribing'
  | 'error';

/** Application state info returned by get_app_state. */
export interface StateEvent {
  state: AppState;
  enabled: boolean;
  detail?: string;
  timestamp: string;
}

/** Reason why recording cannot start. */
export type CannotRecordReason =
  | { type: 'already_recording' }
  | { type: 'transcribing' }
  | { type: 'model_not_ready' }
  | { type: 'disabled' };

// ============================================================================
// CAPABILITY TYPES
// ============================================================================

/** Display server type. */
export type DisplayServer =
  | { type: 'windows' }
  | { type: 'macos' }
  | { type: 'x11' }
  | { type: 'wayland'; compositor?: string }
  | { type: 'unknown' };

/** Hotkey activation mode. */
export type ActivationMode = 'hold' | 'toggle';

/** Text injection method. */
export type InjectionMethod = 'clipboard_paste' | 'clipboard_only';

/** Effective mode with reason. */
export interface EffectiveMode<T> {
  configured: T;
  effective: T;
  reason?: string;
}

/** Permission state. */
export type PermissionState = 'granted' | 'denied' | 'unknown' | 'not_required';

/** Permission status. */
export interface PermissionStatus {
  microphone: PermissionState;
  accessibility?: PermissionState;
}

/** Platform capabilities. */
export interface Capabilities {
  display_server: DisplayServer;
  hotkey_press_available: boolean;
  hotkey_release_available: boolean;
  keystroke_injection_available: boolean;
  clipboard_available: boolean;
  hotkey_mode: EffectiveMode<ActivationMode>;
  injection_method: EffectiveMode<InjectionMethod>;
  permissions: PermissionStatus;
  diagnostics: string;
}

/** Capability issue for user attention. */
export interface CapabilityIssue {
  id: string;
  severity: 'error' | 'warning' | 'info';
  title: string;
  description: string;
  fix_instructions?: string;
}

// ============================================================================
// CONFIG TYPES
// ============================================================================

/** Hotkey mode setting. */
export type HotkeyMode = 'hold' | 'toggle';

/** Audio configuration. */
export interface AudioConfig {
  device_uid?: string;
  audio_cues_enabled: boolean;
  trim_silence: boolean;
}

/** Hotkey configuration. */
export interface HotkeyConfig {
  primary: string;
  copy_last: string;
  mode: HotkeyMode;
}

/** Injection configuration. */
export interface InjectionConfig {
  paste_delay_ms: number;
  restore_clipboard: boolean;
  suffix: string;
  focus_guard_enabled: boolean;
}

/** Model configuration. */
export interface ModelConfig {
  model_id: string | null;
  device: 'auto' | 'cpu' | 'cuda' | 'mps' | null;
  preferred_device: 'auto' | 'cpu' | 'gpu';
  language: string | null;
}

/** UI configuration. */
export interface UiConfig {
  show_on_startup: boolean;
  window_width: number;
  window_height: number;
  theme: 'system' | 'light' | 'dark';
  onboarding_completed: boolean;
  overlay_enabled: boolean;
  locale: string | null;
  reduce_motion: boolean;
}

/** Text replacement rule kind. */
export type ReplacementKind = 'literal' | 'regex';

/** Text replacement rule origin. */
export type ReplacementOrigin = 'user' | 'preset';

/** Text replacement rule (matches IPC protocol). */
export interface ReplacementRule {
  id: string;
  enabled: boolean;
  kind: ReplacementKind;
  pattern: string;
  replacement: string;
  word_boundary: boolean;
  case_sensitive: boolean;
  description?: string;
  origin?: ReplacementOrigin;
}

/** Presets configuration. */
export interface PresetsConfig {
  enabled_presets: string[];
}

/** Complete application configuration. */
export interface AppConfig {
  schema_version: number;
  audio: AudioConfig;
  hotkeys: HotkeyConfig;
  injection: InjectionConfig;
  model: ModelConfig | null;
  replacements: ReplacementRule[];
  ui: UiConfig;
  presets: PresetsConfig;
}

// ============================================================================
// AUDIO TYPES
// ============================================================================

/** Audio device information. */
export interface AudioDevice {
  uid: string;
  name: string;
  is_default: boolean;
  sample_rate: number;
  channels: number;
}

// ============================================================================
// MODEL TYPES
// ============================================================================

/** Model state. */
export type ModelState =
  | 'missing'
  | 'downloading'
  | 'loading'
  | 'verifying'
  | 'ready'
  | 'error'
  | 'unknown';

/** Download/verification progress. */
export interface Progress {
  current: number;
  total?: number;
  unit: string;
}

/** Model status information. */
export interface ModelStatus {
  seq?: number;
  model_id: string;
  status: ModelState;
  revision?: string;
  cache_path?: string;
  progress?: Progress;
  error?: string;
}

// ============================================================================
// HISTORY TYPES
// ============================================================================

/** Injection result for a transcript. */
export type InjectionResult =
  | { status: 'injected' }
  | { status: 'clipboard_only'; reason: string }
  | { status: 'error'; message: string };

/** Transcript history entry. */
export interface TranscriptEntry {
  id: string;
  text: string;
  timestamp: string;
  audio_duration_ms: number;
  transcription_duration_ms: number;
  injection_result: InjectionResult;
}

// ============================================================================
// HOTKEY TYPES
// ============================================================================

/** Hotkey status information. */
export interface HotkeyStatus {
  primary: string;
  copy_last: string;
  mode: string;
  registered: boolean;
}

// ============================================================================
// PRESET TYPES
// ============================================================================

/** Preset information. */
export interface PresetInfo {
  id: string;
  name: string;
  description: string;
  rule_count: number;
}

// ============================================================================
// SELF-CHECK TYPES
// ============================================================================

/** Check status. */
export type CheckStatus = 'ok' | 'warning' | 'error';

/** Individual check item. */
export interface CheckItem {
  status: CheckStatus;
  message: string;
  detail?: string;
}

/** Self-check result. */
export interface SelfCheckResult {
  hotkey: CheckItem;
  injection: CheckItem;
  microphone: CheckItem;
  sidecar: CheckItem;
  model: CheckItem;
}

// ============================================================================
// DIAGNOSTICS TYPES
// ============================================================================

/** Log entry. */
export interface LogEntry {
  timestamp: string;
  level: string;
  message: string;
}

/** Diagnostics report. */
export interface DiagnosticsReport {
  version: string;
  platform: string;
  capabilities: Capabilities;
  config: AppConfig;
  self_check: SelfCheckResult;
}

// ============================================================================
// ERROR TYPES
// ============================================================================

/** Command error codes. */
export type CommandErrorCode =
  | 'config'
  | 'audio'
  | 'model'
  | 'clipboard'
  | 'hotkey'
  | 'not_implemented'
  | 'internal';

/** Command error. */
export interface CommandError {
  code: CommandErrorCode;
  message: string;
}

// ============================================================================
// EVENT TYPES (Rust → UI)
// ============================================================================

/** Audio level event during mic test. */
export interface AudioLevelEvent {
  rms: number;
  peak: number;
}

/** Model download progress event. */
export interface ModelProgressEvent {
  current: number;
  total?: number;
  unit: string;
}

/** Transcript completed event. */
export interface TranscriptEvent {
  entry: TranscriptEntry;
}

/** Error event. */
export interface ErrorEvent {
  message: string;
  recoverable: boolean;
}
