export default function PublicDocsPage() {
  return (
    <div className="landing">
      <section className="landing-hero">
        <p className="landing-eyebrow">Public Docs</p>
        <h1>Quickstart and platform concepts</h1>
        <p>
          Azums supports two entry paths: direct API or webhooks, and agent gateway integration.
          Both use the same execution truth model underneath.
        </p>
      </section>
      <section className="landing-grid">
        <article className="landing-card">
          <h2>Direct API / webhooks</h2>
          <p>Send typed backend requests or signed webhook traffic straight into ingress and inspect the same receipts later.</p>
        </article>
        <article className="landing-card">
          <h2>Agent gateway</h2>
          <p>Compile free-form or structured runtime input into the same request lifecycle without creating a second executor.</p>
        </article>
        <article className="landing-card">
          <h2>Shared truth</h2>
          <p>Receipts, replay, reconciliation, exceptions, and operator diagnostics remain consistent regardless of entry path.</p>
        </article>
      </section>
    </div>
  );
}
