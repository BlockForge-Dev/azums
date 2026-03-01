# Adapter Contract

## Objective
Ensure every domain adapter integrates into one uniform platform semantics model.

## Adapter Input Contract
| Input | Required | Description |
|---|---|---|
| Normalized intent | Yes | Platform command with tenant, kind, payload, metadata |
| Execution context | Yes | Attempt/retry context and correlation details |
| Correlation identifiers | Yes | Traceability across components |
| Secure dependencies | Optional | Tokens/signers/providers via dependency injection |

## Adapter Output Contract
| Output | Required | Description |
|---|---|---|
| Structured outcome | Yes | Domain result mapped to stable categories |
| Retryability signal | Yes | Whether core can safely retry |
| Failure class/reason | On failure | Adapter/provider/caller/system category and reason codes |
| Domain metadata | Optional | Signature, provider response, simulation details |

## Required Adapter Methods
| Method | Purpose |
|---|---|
| `validate(intent)` | Adapter-specific payload validation |
| `execute(intent, context)` | Main domain execution |
| `resume(intent, context)` | Resume/re-attempt behavior if applicable |
| `fetch_status(handle)` | Domain status reconciliation if asynchronous |

## Adapter Must Never
| Prohibition | Reason |
|---|---|
| Redefine platform states | Core owns canonical lifecycle |
| Mutate unrelated jobs | Isolation and integrity |
| Bypass durable core lifecycle | Prevent hidden side-channel truth |
| Send final user-facing truth directly | Truth-before-notify invariant |
| Own global replay/retry policy | Policy belongs to core |

## Solana Adapter Notes
| Capability | Expected Behavior |
|---|---|
| Input validation | Validate required Solana fields in normalized payload |
| Preflight/simulation | Run when configured and return structured outcomes |
| Submission/finality metadata | Return signature, blockhash, landed/finalized evidence |
| Error normalization | Map RPC/provider errors to stable platform classes |
| Security | Avoid secret leakage; prevent accidental double-submit behavior |

