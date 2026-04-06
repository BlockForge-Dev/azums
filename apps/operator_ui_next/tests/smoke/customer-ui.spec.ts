import { expect, test, type APIRequestContext, type Page } from "@playwright/test";

async function warmRoute(path: string, request: APIRequestContext) {
  await request.get(path, {
    failOnStatusCode: false,
    timeout: 120_000,
  });
}

async function fillStable(locator: ReturnType<Page["getByTestId"]>, value: string) {
  await locator.fill(value);
  await expect(locator).toHaveValue(value, { timeout: 30_000 });
}

async function waitForJsonGet(page: Page, pathFragment: string) {
  return page.waitForResponse(
    (response) =>
      response.request().method() === "GET" &&
      response.url().includes(pathFragment) &&
      response.status() < 500,
    { timeout: 120_000 }
  );
}

async function waitForJsonPost(page: Page, pathFragment: string) {
  return page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      response.url().includes(pathFragment) &&
      response.status() < 400,
    { timeout: 120_000 }
  );
}

async function loginAsDemo(page: Page, nextPath: string) {
  await page.goto(`/login?next=${encodeURIComponent(nextPath)}`, {
    timeout: 120_000,
    waitUntil: "domcontentloaded",
  });
  await waitForJsonGet(page, "/api/ui/account/session");

  const emailInput = page.getByTestId("login-email");
  const passwordInput = page.getByTestId("login-password");
  await fillStable(emailInput, "demo@azums.dev");
  await fillStable(passwordInput, "dev-password");
  const loginResponse = waitForJsonPost(page, "/api/ui/account/login");
  await page.getByTestId("login-submit").click({ noWaitAfter: true });
  await loginResponse;
}

test("customer UI smoke covers login, workspace switch, Playground replay, API keys, and webhooks", async ({
  page,
  request,
}) => {
  test.setTimeout(300_000);

  await warmRoute("/login?next=%2Fapp%2Fworkspaces", request);
  await warmRoute("/app/workspaces", request);
  await warmRoute("/app/playground", request);
  await warmRoute("/app/api-keys", request);
  await warmRoute("/app/webhooks", request);
  await warmRoute("/api/ui/account/session", request);

  await loginAsDemo(page, "/app/workspaces");

  await Promise.all([
    waitForJsonGet(page, "/api/ui/account/session"),
    page.goto("/app/workspaces", {
      timeout: 120_000,
      waitUntil: "domcontentloaded",
    }),
  ]);
  await expect(page.getByText("Manage your workspace and environment access.")).toBeVisible();

  const switchResponse = waitForJsonPost(page, "/api/ui/account/workspaces/switch");
  await page.getByTestId("workspace-switch-workspace_sandbox").click({
    noWaitAfter: true,
  });
  await switchResponse;
  await expect(page.getByTestId("workspace-card-workspace_sandbox")).toContainText("Current");
  await expect(page.getByTestId("workspace-card-workspace_sandbox")).toContainText(
    "Sandbox Workspace"
  );

  await Promise.all([
    waitForJsonGet(page, "/api/ui/account/session"),
    page.goto("/app/playground", {
      timeout: 120_000,
      waitUntil: "domcontentloaded",
    }),
  ]);
  await page.getByTestId("playground-demo-scenario").selectOption("terminal_failure");
  const submitResponse = waitForJsonPost(page, "/api/ui/ingress/requests");
  await page.getByTestId("playground-submit").click({ noWaitAfter: true });
  await submitResponse;

  await expect(page.getByText(/Submitted .* on devnet Playground\./)).toBeVisible();
  await page.getByTestId("playground-tab-replay").click();
  await page.getByTestId("playground-confirm-replay").click();
  const replayResponse = waitForJsonPost(page, "/api/ui/status/requests/");
  await page.getByTestId("playground-replay-now").click({ noWaitAfter: true });
  await replayResponse;
  await expect(page.getByText("Replay Result")).toBeVisible();
  await expect(page.getByText("succeeded", { exact: true }).first()).toBeVisible();

  await page.goto("/app/api-keys", {
    timeout: 120_000,
    waitUntil: "domcontentloaded",
  });
  await expect(page.getByTestId("api-keys-name")).toBeEnabled({ timeout: 120_000 });
  await page.getByTestId("api-keys-name").fill("smoke-service");
  const createKeyResponse = waitForJsonPost(page, "/api/ui/account/api-keys");
  await page.getByTestId("api-keys-create").click({ noWaitAfter: true });
  await createKeyResponse;
  await expect(page.getByText("Copy this key now. It is shown once.")).toBeVisible();

  await page.goto("/app/webhooks", {
    timeout: 120_000,
    waitUntil: "domcontentloaded",
  });
  await expect(page.getByTestId("webhooks-issue-key")).toBeEnabled({ timeout: 120_000 });
  await page.getByTestId("webhooks-source").fill("github");
  const issueWebhookResponse = waitForJsonPost(page, "/api/ui/account/webhook-keys");
  await page.getByTestId("webhooks-issue-key").click({ noWaitAfter: true });
  await issueWebhookResponse;
  await expect(page.getByText("Copy this secret now. It is shown once.")).toBeVisible();
});

test("operator smoke covers Paystack exception investigation", async ({ page, request }) => {
  test.setTimeout(300_000);

  await warmRoute("/login?next=%2Fops%2Fexceptions", request);
  await warmRoute("/ops/exceptions", request);
  await warmRoute("/api/ui/account/session", request);
  await warmRoute("/api/ui/config", request);
  await warmRoute("/api/ui/status/exceptions?limit=80&offset=0", request);

  await loginAsDemo(page, "/ops/exceptions");

  const configResponse = waitForJsonGet(page, "/api/ui/config");
  const indexResponse = waitForJsonGet(page, "/api/ui/status/exceptions");
  await page.goto("/ops/exceptions", {
    timeout: 120_000,
    waitUntil: "domcontentloaded",
  });
  await Promise.all([configResponse, indexResponse]);

  await expect(
    page.getByText("Paystack settlement amount did not match the protected execution request.")
  ).toBeVisible({ timeout: 120_000 });
  await expect(page.getByText("Fiat rail investigation")).toBeVisible({ timeout: 120_000 });
  await expect(page.getByText("Evidence rows")).toBeVisible({ timeout: 120_000 });
  await expect(page.getByText("1 execution / 1 webhook")).toBeVisible({ timeout: 120_000 });

  const webhookExcerpt = page.getByText("Webhook payload excerpt");
  await expect(webhookExcerpt).toBeVisible();
  await webhookExcerpt.click();

  await expect(page.getByText("event:refund.processed")).toBeVisible();
  await expect(page.getByText("provider_status:success")).toBeVisible();
  await expect(page.getByText("gateway:Refund processed by Paystack")).toBeVisible();
});
