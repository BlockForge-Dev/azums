export default function Page() {
  return (
    <div className="space-y-6">
      <section className="bg-gradient-to-br from-primary/20 via-card to-card rounded-2xl p-8 border border-primary/20">
        <p className="text-sm font-medium text-primary mb-2">Docs</p>
        <h2 className="text-2xl font-bold text-foreground mb-2">Customer Console Docs</h2>
        <p className="text-muted-foreground">Quickstart, Playground guidance, API references, lifecycle state meanings, and callback signing guidance.</p>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <h3 className="text-lg font-semibold text-foreground mb-4">Quickstart</h3>
        <ol className="list-decimal list-inside space-y-2 text-muted-foreground">
          <li>Finish onboarding and create an API key.</li>
          <li>Configure outbound callback destination.</li>
          <li>Run a sample request from Playground.</li>
          <li>Open Request Detail and inspect the Receipt tab.</li>
          <li>Validate callback delivery history, then wire backend or webhook integrations.</li>
        </ol>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <h3 className="text-lg font-semibold text-foreground mb-4">API reference</h3>
        <ul className="space-y-2 text-muted-foreground">
          <li><code className="px-1.5 py-0.5 bg-muted rounded text-sm font-mono">POST /api/requests</code> accepts supported intent payloads from backend services, inbound webhooks, and Playground.</li>
          <li><code className="px-1.5 py-0.5 bg-muted rounded text-sm font-mono">GET /status/requests/:intent_id</code> fetch current request state.</li>
          <li><code className="px-1.5 py-0.5 bg-muted rounded text-sm font-mono">GET /status/requests/:intent_id/receipt</code> fetch durable receipt entries.</li>
          <li><code className="px-1.5 py-0.5 bg-muted rounded text-sm font-mono">GET /status/requests/:intent_id/history</code> fetch lifecycle transitions.</li>
          <li><code className="px-1.5 py-0.5 bg-muted rounded text-sm font-mono">GET /status/requests/:intent_id/callbacks</code> fetch callback delivery history.</li>
          <li><code className="px-1.5 py-0.5 bg-muted rounded text-sm font-mono">POST /status/requests/:intent_id/replay</code> replay with owner/admin authorization.</li>
        </ul>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <h3 className="text-lg font-semibold text-foreground mb-4">State machine meanings</h3>
        <ul className="space-y-2 text-muted-foreground">
          <li><strong className="text-foreground">received</strong>: ingress accepted request context.</li>
          <li><strong className="text-foreground">validated</strong>: contract checks passed.</li>
          <li><strong className="text-foreground">queued</strong>: ready for worker lease.</li>
          <li><strong className="text-foreground">leased</strong>: worker owns the execution lease.</li>
          <li><strong className="text-foreground">executing</strong>: adapter execution in progress.</li>
          <li><strong className="text-foreground">retry_scheduled</strong>: retryable condition and backoff recorded.</li>
          <li><strong className="text-foreground">succeeded</strong>: terminal success committed.</li>
          <li><strong className="text-foreground">failed_terminal</strong>: terminal failure committed.</li>
          <li><strong className="text-foreground">dead_lettered</strong>: retry budget exhausted.</li>
          <li><strong className="text-foreground">replayed</strong>: replay lineage created from a prior execution.</li>
        </ul>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <h3 className="text-lg font-semibold text-foreground mb-4">Callback signing guide</h3>
        <ul className="space-y-2 text-muted-foreground">
          <li>Configure destination URL, timeout, and host allowlist in <code className="px-1.5 py-0.5 bg-muted rounded text-sm font-mono">Callbacks</code>.</li>
          <li>Keep bearer/signing secrets masked and server-side only.</li>
          <li>Verify callback signatures before trusting payloads.</li>
          <li>Treat callback delivery failures separately from execution truth.</li>
        </ul>
      </section>
    </div>
  );
}
