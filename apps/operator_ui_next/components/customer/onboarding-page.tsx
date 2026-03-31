"use client";

import Link from "next/link";
import { useEffect, useMemo, useState } from "react";
import {
  listApiKeys,
  markOnboardingStep,
  onboardingProgress,
  readSession,
  type SessionRecord,
} from "@/lib/app-state";
import { apiGet } from "@/lib/client-api";
import type {
  CallbackDestinationResponse,
  JobListResponse,
  UiConfigResponse,
} from "@/lib/types";

type StepId = keyof SessionRecord["onboarding"];

type Step = {
  id: StepId;
  label: string;
  description: string;
  href: string;
  actionLabel: string;
  done: boolean;
};

export function OnboardingPage() {
  const [session, setSession] = useState<SessionRecord | null>(null);
  const [config, setConfig] = useState<UiConfigResponse | null>(null);
  const [jobsCount, setJobsCount] = useState(0);
  const [callbackConfigured, setCallbackConfigured] = useState(false);
  const [activeKeys, setActiveKeys] = useState(0);
  const [copyLabel, setCopyLabel] = useState("Copy example");

  useEffect(() => {
    let cancelled = false;

    void Promise.all([
      readSession(),
      apiGet<UiConfigResponse>("config").catch(() => null),
      listApiKeys(),
      apiGet<JobListResponse>("status/jobs?limit=20&offset=0").catch(() => ({ jobs: [] })),
      apiGet<CallbackDestinationResponse>("status/tenant/callback-destination").catch(() => ({
        configured: false,
      })),
    ])
      .then(([currentSession, cfg, keys, jobs, callback]) => {
        if (cancelled) return;

        setSession(currentSession);
        if (cfg) setConfig(cfg);

        const nextActiveKeys = keys.filter((key) => key.revoked_at_ms == null).length;
        const nextJobsCount = jobs.jobs?.length ?? 0;
        const nextCallbackConfigured = Boolean(callback.configured);

        setActiveKeys(nextActiveKeys);
        setJobsCount(nextJobsCount);
        setCallbackConfigured(nextCallbackConfigured);

        if (nextJobsCount > 0) {
          void markOnboardingStep("submitted_request");
        }

        if (nextCallbackConfigured) {
          void markOnboardingStep("configured_callback");
        }
      })
      .catch(() => void 0);

    return () => {
      cancelled = true;
    };
  }, []);

  const progress = useMemo(
    () => (session ? onboardingProgress(session) : { completed: 0, total: 5, percent: 0 }),
    [session]
  );

  const steps = useMemo<Step[]>(() => {
    const onboarding = session?.onboarding;

    return [
      {
        id: "workspace_created",
        label: "Workspace ready",
        description: "Your workspace is set up and ready to use.",
        href: "/app/workspaces",
        actionLabel: "Open workspaces",
        done: Boolean(onboarding?.workspace_created),
      },
      {
        id: "api_key_generated",
        label: "Create an API key",
        description: "Generate a key for requests from your app or backend.",
        href: "/app/api-keys",
        actionLabel: "Create API key",
        done: Boolean(onboarding?.api_key_generated) || activeKeys > 0,
      },
      {
        id: "submitted_request",
        label: "Send a test request",
        description: "Use Playground to run a safe test request.",
        href: "/app/playground",
        actionLabel: "Open Playground",
        done: Boolean(onboarding?.submitted_request) || jobsCount > 0,
      },
      {
        id: "viewed_receipt",
        label: "Review the result",
        description: "Open the request page and confirm the result.",
        href: "/app/requests",
        actionLabel: "Open requests",
        done: Boolean(onboarding?.viewed_receipt),
      },
      {
        id: "configured_callback",
        label: "Connect callbacks",
        description: "Send request updates back to your app automatically.",
        href: "/app/callbacks",
        actionLabel: "Set up callbacks",
        done: Boolean(onboarding?.configured_callback) || callbackConfigured,
      },
    ];
  }, [activeKeys, callbackConfigured, jobsCount, session?.onboarding]);

  const nextStepIndex = useMemo(() => steps.findIndex((step) => !step.done), [steps]);

  const nextStep = useMemo(() => {
    if (nextStepIndex === -1) return null;
    return steps[nextStepIndex];
  }, [nextStepIndex, steps]);

  const sampleCurl = useMemo(() => {
    const base = (config?.ingress_base_url ?? "http://127.0.0.2:8000").replace(/\/$/, "");
    const tenantId = config?.tenant_id ?? "<TENANT_ID>";

    return `curl -X POST "${base}/api/requests" \\
  -H "x-tenant-id: ${tenantId}" \\
  -H "x-principal-id: backend-service" \\
  -H "x-submitter-kind: api_key_holder" \\
  -H "x-api-key: <API_KEY>" \\
  -H "content-type: application/json" \\
  -d '{
    "intent_kind":"solana.transfer.v1",
    "payload":{
      "intent_id":"intent_onboarding_demo_001",
      "type":"transfer",
      "to_addr":"GK8jAw6oibNGWT7WRwh2PCKSTb1XGQSiuPZdCaWRpqRC",
      "amount":1
    }
  }'`;
  }, [config?.ingress_base_url, config?.tenant_id]);

  async function copyCurl() {
    try {
      await navigator.clipboard.writeText(sampleCurl);
      setCopyLabel("Copied");
      window.setTimeout(() => setCopyLabel("Copy example"), 1400);
    } catch {
      setCopyLabel("Copy failed");
      window.setTimeout(() => setCopyLabel("Copy example"), 1400);
    }
  }

  return (
    <div className="stack onboarding-page">
      <section className="surface hero-surface onboarding-hero">
        <div className="onboarding-hero-copy">
          <p className="eyebrow">Get started</p>
          <h2>Finish setup in a few clear steps.</h2>
          <p>
            Create a key, send a test request, review the result, and connect callbacks
            for your app.
          </p>
        </div>

        <div className="onboarding-hero-side">
          <span className="badge neutral">
            {nextStep ? `Step ${nextStepIndex + 1} of ${steps.length}` : "Setup complete"}
          </span>
          <span className="badge success">{progress.percent}% complete</span>
        </div>
      </section>

      <section className="surface onboarding-progress-card">
        <div className="onboarding-progress-head">
          <div>
            <h3>Progress</h3>
            <p className="panel-subtitle">
              {nextStep ? `Next: ${nextStep.label}` : "Everything is set up."}
            </p>
          </div>
        </div>

        <div className="progress">
          <div className="progress-fill" style={{ width: `${progress.percent}%` }} />
        </div>

        <div className="onboarding-stat-grid">
          <div className="onboarding-stat">
            <span>Completed</span>
            <strong>
              {progress.completed}/{progress.total}
            </strong>
          </div>
          <div className="onboarding-stat">
            <span>Active keys</span>
            <strong>{activeKeys}</strong>
          </div>
          <div className="onboarding-stat">
            <span>Requests</span>
            <strong>{jobsCount}</strong>
          </div>
          <div className="onboarding-stat">
            <span>Callbacks</span>
            <strong>{callbackConfigured ? "Connected" : "Not yet"}</strong>
          </div>
        </div>
      </section>

      <section className="surface">
        <div className="panel-header">
          <div>
            <h3>Setup checklist</h3>
            <p className="panel-subtitle">
              Work through the steps below. The next unfinished step is highlighted.
            </p>
          </div>
        </div>

        <div className="onboarding-step-grid">
          {steps.map((step, index) => {
            const isNext = !step.done && index === nextStepIndex;

            return (
              <article
                key={step.id}
                className={`onboarding-step-card ${step.done ? "done" : ""} ${isNext ? "active" : ""}`}
              >
                <div className="onboarding-step-top">
                  <div className="onboarding-step-index">{step.done ? "✓" : index + 1}</div>
                  <span className={`badge ${step.done ? "success" : isNext ? "warn" : "neutral"}`}>
                    {step.done ? "Completed" : isNext ? "Next" : "Pending"}
                  </span>
                </div>

                <h3>{step.label}</h3>
                <p>{step.description}</p>

                <div className="onboarding-step-actions">
                  <Link href={step.href}>{step.done ? "Open" : step.actionLabel}</Link>
                </div>
              </article>
            );
          })}
        </div>
      </section>

      <section className="surface onboarding-code-card">
        <div className="panel-header">
          <div>
            <h3>Backend example</h3>
            <p className="panel-subtitle">
              Use this example when sending requests from your backend instead of the Playground.
            </p>
          </div>
        </div>

        <pre>{sampleCurl}</pre>

        <div className="onboarding-actions">
          <button className="btn ghost" type="button" onClick={() => void copyCurl()}>
            {copyLabel}
          </button>

          <Link className="btn ghost" href="/app/docs">
            Open docs
          </Link>
        </div>
      </section>
    </div>
  );
}
