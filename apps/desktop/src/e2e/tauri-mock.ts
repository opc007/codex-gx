/**
 * Browser E2E mock — only active with ?e2e=1 (no real Tauri IPC).
 */
import * as core from "@tauri-apps/api/core";
import * as events from "@tauri-apps/api/event";

const MOCK_LICENSE = {
  status: { kind: "unactivated" as const },
  last_validated_at: 0,
  offline: true,
};

const MOCK_TIERS = [
  {
    tier: "monthly",
    displayName: "月卡",
    durationDays: 30,
    priceYuan: 9.9,
    features: ["基础 chat"],
    recommended: false,
  },
];

const handlers: Record<string, (args?: unknown) => unknown> = {
  ping: () => "AgentShell Rust backend v1.6.0 (e2e-mock)",
  license_status: () => MOCK_LICENSE,
  license_tiers: () => MOCK_TIERS,
  license_refresh: () => MOCK_LICENSE,
  license_activate: () => MOCK_LICENSE,
  license_deactivate: () => undefined,
  license_demo_code: () => "MOCK-DEMO-CODE",
  check_update: () => ({
    currentVersion: "1.6.0",
    latestVersion: "1.6.0",
    updateAvailable: false,
    releaseUrl: null,
    releaseNotes: null,
  }),
  bug_report_record: () => "crash_mock_1",
  bug_report_list: () => [],
  bug_report_clear: () => undefined,
  marketplace_get_index_url: () => "https://example.com/index.json",
  marketplace_list_installed: () => [],
  marketplace_fetch_index: () => [],
  routing_get_strategy: () => ({ id: "default", name: "Default" }),
  routing_decide: () => ({ model: "auto", reason: "e2e" }),
  local_discover: () => ({ ollama: [], llamacpp: [] }),
  local_list_models: () => [],
  lint_run_summary: () => ({ issues: 0, files: 0 }),
  queue_list: () => [],
  queue_clear_finished: () => 0,
  p2p_list_peers: () => [],
  learning_get: () => ({
    signals: { total_messages: 0, total_tools: 0, positive_feedback: 0, negative_feedback: 0 },
    preferences: {},
  }),
  list_skills_grouped: () => ({ builtin: [], custom: [], disabled: [] }),
  tts_detect: () => ({ available: false, engines: [] }),
  tts_get_config: () => ({ engine: "system", voice: null, rate: 1 }),
  sync_list: () => [],
  plugin_list: () => [],
  vault_list_encrypted: () => [],
  list_providers: () => [{ id: "mock", name: "Mock", models: ["mock-model"] }],
  list_tools: () => [],
  list_mcp_servers: () => [],
  workspace_changed_broadcast: () => undefined,
};

const origInvoke = core.invoke;
// @ts-expect-error E2E mock overrides invoke
core.invoke = async (cmd: string, args?: unknown) => {
  // #region agent log
  fetch("http://127.0.0.1:7530/ingest/e08fdb9a-c53f-4edd-8510-a7e289a0c763", {
    method: "POST",
    headers: { "Content-Type": "application/json", "X-Debug-Session-Id": "ceb105" },
    body: JSON.stringify({
      sessionId: "ceb105",
      location: "e2e/tauri-mock.ts:invoke",
      message: "e2e_invoke",
      data: { cmd, hasHandler: cmd in handlers },
      timestamp: Date.now(),
      hypothesisId: "E2E",
      runId: "e2e-ui",
    }),
  }).catch(() => {});
  // #endregion
  const h = handlers[cmd];
  if (h) return h(args);
  console.warn(`[e2e-mock] unhandled invoke: ${cmd}`, args);
  return undefined;
};

const origListen = events.listen;
// @ts-expect-error E2E mock overrides listen
events.listen = async () => () => {};

void origInvoke;
void origListen;
console.info("[e2e] Tauri mock active");
