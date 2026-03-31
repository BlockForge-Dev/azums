export default function ContactPage() {
  return (
    <div className="min-h-screen flex flex-col items-center justify-center p-8">
      <section className="text-center max-w-2xl mb-12">
        <p className="text-sm font-medium text-primary mb-3">Contact</p>
        <h1 className="text-4xl font-bold text-foreground mb-4">Talk to the Azums team</h1>
        <p className="text-lg text-muted-foreground">
          For architecture reviews, enterprise rollout, and managed deployment support, contact the
          platform team.
        </p>
      </section>
      <section className="grid grid-cols-1 md:grid-cols-3 gap-6 w-full max-w-4xl">
        <article className="bg-card rounded-xl border border-border/50 p-6 text-center hover:border-primary/30 transition-colors">
          <h2 className="text-lg font-semibold text-foreground mb-2">Email</h2>
          <p className="text-primary">support@azums.dev</p>
        </article>
        <article className="bg-card rounded-xl border border-border/50 p-6 text-center hover:border-primary/30 transition-colors">
          <h2 className="text-lg font-semibold text-foreground mb-2">Sales</h2>
          <p className="text-primary">sales@azums.dev</p>
        </article>
        <article className="bg-card rounded-xl border border-border/50 p-6 text-center hover:border-primary/30 transition-colors">
          <h2 className="text-lg font-semibold text-foreground mb-2">Security</h2>
          <p className="text-primary">security@azums.dev</p>
        </article>
      </section>
    </div>
  );
}
