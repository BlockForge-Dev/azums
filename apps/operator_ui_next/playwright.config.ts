import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/smoke",
  timeout: 180_000,
  expect: {
    timeout: 20_000,
  },
  retries: 0,
  fullyParallel: false,
  reporter: [["list"]],
  use: {
    baseURL: "http://127.0.0.1:43110",
    actionTimeout: 30_000,
    navigationTimeout: 120_000,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
      },
    },
  ],
  webServer: [
    {
      command: "node tests/smoke/mock-platform.mjs",
      cwd: __dirname,
      url: "http://127.0.0.1:43000/healthz",
      reuseExistingServer: false,
      timeout: 180_000,
    },
    {
      command: "node tests/smoke/run-operator-ui.mjs",
      cwd: __dirname,
      url: "http://127.0.0.1:43083/health",
      reuseExistingServer: false,
      timeout: 180_000,
    },
    {
      command: "npm run dev -- --hostname 127.0.0.1 --port 43110",
      cwd: __dirname,
      env: {
        ...process.env,
        OPERATOR_UI_BACKEND_ORIGIN: "http://127.0.0.1:43083",
        NEXT_PUBLIC_PASSWORD_RESET_ENABLED: "true",
      },
      url: "http://127.0.0.1:43110/login",
      reuseExistingServer: false,
      timeout: 600_000,
    },
  ],
});
