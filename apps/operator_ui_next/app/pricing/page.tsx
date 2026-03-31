import { PLAN_ORDER, PLAN_SPECS, formatFreePlayLimit, formatMonthlyPrice } from "@/lib/plans";

export default function PricingPage() {
  return (
    <div className="landing">
      <section className="landing-hero">
        <p className="landing-eyebrow">Pricing</p>
        <h1>Plans aligned to execution volume and team needs</h1>
        <p>
          Free Play includes capped request quotas. Paid mode removes the monthly request limit for
          each plan.
        </p>
      </section>
      <section className="landing-grid">
        {PLAN_ORDER.map((plan) => (
          <article className="landing-card" key={plan}>
            <h2>{plan}</h2>
            <p>{PLAN_SPECS[plan].headline}</p>
            <p>{formatMonthlyPrice(plan)}</p>
            <p>Free Play: {formatFreePlayLimit(plan)}</p>
            <p>Paid mode: {PLAN_SPECS[plan].paid_mode} requests/month</p>
            <ul className="simple-list">
              {PLAN_SPECS[plan].features.map((feature) => (
                <li key={feature}>{feature}</li>
              ))}
            </ul>
          </article>
        ))}
      </section>
    </div>
  );
}
