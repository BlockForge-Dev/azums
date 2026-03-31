export default function HowItWorksPage() {
  return (
    <div className="landing">
      <section className="landing-hero">
        <p className="landing-eyebrow">How It Works</p>
        <h1>Ingress → Queue → Core → Adapter → Receipt → Callback</h1>
        <p>
          The platform records each transition and classification so frontend views always reflect
          backend execution truth.
        </p>
      </section>
      <section className="landing-grid">
        <article className="landing-card">
          <h2>Flow A</h2>
          <p>Inbound execution from request intake to terminal state and callback delivery.</p>
        </article>
        <article className="landing-card">
          <h2>Flow B</h2>
          <p>Retry flow with scheduled backoff and timeline continuity.</p>
        </article>
        <article className="landing-card">
          <h2>Flow C / D</h2>
          <p>Terminal failure classification and controlled replay with full lineage.</p>
        </article>
      </section>
    </div>
  );
}
