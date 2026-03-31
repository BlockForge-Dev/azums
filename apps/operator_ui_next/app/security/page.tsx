export default function SecurityPage() {
  return (
    <div className="landing">
      <section className="landing-hero">
        <p className="landing-eyebrow">Security</p>
        <h1>Tenant isolation and role-based controls</h1>
        <p>
          Azums enforces workspace/tenant context in backend requests. Frontend permissions mirror
          backend role rules and do not fabricate authority.
        </p>
      </section>
      <section className="landing-grid">
        <article className="landing-card">
          <h2>Auth surface</h2>
          <p>Session cookie auth for app routes with protected backend proxy endpoints.</p>
        </article>
        <article className="landing-card">
          <h2>Sensitive data handling</h2>
          <p>API key secrets are one-time reveal only; key list never returns token values.</p>
        </article>
        <article className="landing-card">
          <h2>Execution truth</h2>
          <p>Receipts, history, callbacks, and replay lineage are backend-sourced durable records.</p>
        </article>
      </section>
    </div>
  );
}
