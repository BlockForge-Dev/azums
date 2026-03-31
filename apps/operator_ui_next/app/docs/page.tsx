export default function PublicDocsPage() {
  return (
    <div className="landing">
      <section className="landing-hero">
        <p className="landing-eyebrow">Public Docs</p>
        <h1>Quickstart and platform concepts</h1>
        <p>For signed-in operators and developers, open the in-app docs at `/app/docs`.</p>
      </section>
      <section className="landing-grid">
        <article className="landing-card">
          <h2>Quickstart</h2>
          <p>Sign up, create an API key, run a Playground request, and inspect the receipt.</p>
        </article>
        <article className="landing-card">
          <h2>Execution model</h2>
          <p>States, classifications, retry model, replay lineage, callback records.</p>
        </article>
        <article className="landing-card">
          <h2>Operator model</h2>
          <p>Replay controls, intake audits, callback diagnostics, and activity logs.</p>
        </article>
      </section>
    </div>
  );
}
