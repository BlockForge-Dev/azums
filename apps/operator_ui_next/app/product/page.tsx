export default function ProductPage() {
  return (
    <div className="landing">
      <section className="landing-hero">
        <p className="landing-eyebrow">Product</p>
        <h1>Durable execution for request-driven workflows</h1>
        <p>
          Azums supports direct API or webhook integration and agent gateway integration,
          both landing in the same execution truth, receipt, replay, and reconciliation model.
        </p>
      </section>
      <section className="landing-grid">
        <article className="landing-card">
          <h2>Entry Paths</h2>
          <p>Use direct API or signed webhooks, or send runtime traffic through the agent gateway.</p>
        </article>
        <article className="landing-card">
          <h2>Shared Core</h2>
          <p>Both paths converge into one normalized intent model, one execution core, and one receipt surface.</p>
        </article>
        <article className="landing-card">
          <h2>Operator Controls</h2>
          <p>Replay with lineage, intake audits, callback diagnostics, and system visibility.</p>
        </article>
      </section>
    </div>
  );
}
