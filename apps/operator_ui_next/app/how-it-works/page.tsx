export default function HowItWorksPage() {
  return (
    <div className="landing">
      <section className="landing-hero">
        <p className="landing-eyebrow">How It Works</p>
        <h1>API / Webhook or Agent Gateway → Ingress → Core → Receipt</h1>
        <p>
          Direct API and webhook traffic, plus agent gateway traffic, all converge into the same
          normalized intent, execution core, and receipt model.
        </p>
      </section>
      <section className="landing-grid">
        <article className="landing-card">
          <h2>Direct integration</h2>
          <p>Backend systems and webhook senders submit supported work directly into ingress.</p>
        </article>
        <article className="landing-card">
          <h2>Agent gateway</h2>
          <p>Runtimes authenticate, resolve agent identity, compile requests, and then hand off to the same control flow.</p>
        </article>
        <article className="landing-card">
          <h2>Shared lifecycle</h2>
          <p>Retries, replay, receipts, reconciliation, and exceptions still come from one shared core underneath both paths.</p>
        </article>
      </section>
    </div>
  );
}
