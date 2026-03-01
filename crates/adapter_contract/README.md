# Adapter Contract

`adapter_contract` provides:

- `AdapterRegistry`, a concrete `AdapterRouter` for `execution_core`
- `DomainAdapter`, a stricter adapter interface for domain executors
- `DomainAdapterExecutor`, which adapts `DomainAdapter` to `execution_core::AdapterExecutor`

Use it to:

- map normalized intent kinds to adapter IDs
- register adapter executors
- register domain adapters with `validate/execute/resume/fetch_status`
- enforce route/executor lookup through a single adapter boundary
- enforce status/outcome contract consistency before returning results to core
