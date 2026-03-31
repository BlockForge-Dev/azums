import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "../../../..");

const child = spawn("cargo", ["run", "--manifest-path", "apps/operator_ui/Cargo.toml", "--quiet"], {
  cwd: repoRoot,
  stdio: "inherit",
  env: {
    ...process.env,
    OPERATOR_UI_BIND: "127.0.0.1:43083",
    OPERATOR_UI_STATUS_BASE_URL: "http://127.0.0.1:43082/status",
    OPERATOR_UI_INGRESS_BASE_URL: "http://127.0.0.1:43000",
    OPERATOR_UI_PUBLIC_BASE_URL: "http://127.0.0.1:43110",
    OPERATOR_UI_SESSION_EMAIL: "demo@azums.dev",
    OPERATOR_UI_SESSION_NAME: "Demo User",
    OPERATOR_UI_SESSION_PASSWORD: "dev-password",
    OPERATOR_UI_WORKSPACE_ID: "workspace_demo",
    OPERATOR_UI_WORKSPACE_NAME: "Demo Workspace",
    OPERATOR_UI_WORKSPACE_ROLE: "owner",
    OPERATOR_UI_WORKSPACE_ENVIRONMENT: "staging",
    OPERATOR_UI_EXTRA_WORKSPACES: "workspace_sandbox|Sandbox Workspace|sandbox",
    OPERATOR_UI_REQUIRE_EMAIL_VERIFICATION: "false",
    OPERATOR_UI_PASSWORD_RESET_ENABLED: "true",
  },
});

function shutdown(signal) {
  if (!child.killed) {
    child.kill(signal);
  }
}

process.on("SIGINT", () => shutdown("SIGINT"));
process.on("SIGTERM", () => shutdown("SIGTERM"));
child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 0);
});
