export default function ProductPage() {
  return (
    <div className="landing">
      <section className="landing-hero">
        <p className="landing-eyebrow">Product</p>
        <h1>Durable execution for request-driven workflows</h1>
        <p>
          Azums persists execution truth across ingress, queueing, adapter dispatch, retries,
          callbacks, and replay lineage.
        </p>
      </section>
      <section className="landing-grid">
        <article className="landing-card">
          <h2>Core Objects</h2>
          <p>Intent, Request, Job, Receipt, Callback, Replay, Audit.</p>
        </article>
        <article className="landing-card">
          <h2>Delivery Guarantees</h2>
          <p>Durable state transitions and callback attempt history from backend truth.</p>
        </article>
        <article className="landing-card">
          <h2>Operator Controls</h2>
          <p>Replay with lineage, intake audits, callback diagnostics, and system visibility.</p>
        </article>
      </section>
    </div>
  );
}
