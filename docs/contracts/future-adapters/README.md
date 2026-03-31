# Future Adapter Mapping Docs

These documents do not implement the adapters.

They show how each future adapter would plug into:

- execution
- reconciliation
- exception intelligence
- evidence/indexing

Use them with:

- [adapter-integration-playbook.md](../adapter-integration-playbook.md)
- [recon-rule-pack-template.md](../recon-rule-pack-template.md)
- [exception-subcode-guidance.md](../exception-subcode-guidance.md)

## Required Structure

Each future adapter mapping document should cover:

1. first plausible intents
2. execution adapter mapping
3. expected fact map
4. observed fact map
5. mismatch mapping
6. evidence mapping
7. non-goals for v1

These are examples and readiness kits, not product commitments.

The point is to make a future adapter feel like a conforming Azums adapter, not a separate architecture.

## Mapping Set

| File | Adapter family |
|---|---|
| `evm-adapter-mapping.md` | EVM execution and recon |
| `sui-adapter-mapping.md` | Sui execution and recon |
| `http-adapter-mapping.md` | outbound/internal HTTP execution and recon |
| `slack-adapter-mapping.md` | Slack message execution and recon |
| `email-adapter-mapping.md` | Email send execution and recon |
| `stripe-adapter-mapping.md` | Stripe payment execution and recon |
| `paystack-adapter-mapping.md` | Paystack payment execution and recon |
| `flutterwave-adapter-mapping.md` | Flutterwave payment execution and recon |
