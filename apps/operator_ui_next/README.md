# Operator UI Next

Modern Next.js + TypeScript frontend for Azums product surfaces.

## Prerequisites

- Node.js 20+
- Existing Rust `operator_ui` backend running (default `http://127.0.0.1:8083`)

## Setup

```bash
cd apps/operator_ui_next
cp .env.example .env.local
npm install
npm run dev
```

Open `http://localhost:3000`.

## UX surfaces

- `/` marketing/landing
- `/app/*` customer-facing application (dashboard, Playground, requests, callbacks, usage)
- `/ops` deep operator console (replay, audits, raw diagnostics)
- `/admin` compatibility alias for operator console

## Auth and billing mode

- Signup/login, team, API keys, billing, and invite acceptance are backed by the Rust `operator_ui`
  backend APIs.
- Durable execution truth (requests, receipts, callbacks, replay) comes from backend APIs.
- This frontend is a product surface; it does not invent lifecycle state.

## Backend proxy

The frontend proxies all API requests through Next route handlers:

- Browser -> `/api/ui/*` (Next app)
- Next app -> `${OPERATOR_UI_BACKEND_ORIGIN}/api/ui/*`

This avoids browser CORS issues while keeping auth/header behavior in the Rust backend.

Password reset UX toggle:

- `NEXT_PUBLIC_PASSWORD_RESET_ENABLED=false` hides/disables forgot-password flows in the UI.

## Scripts

- `npm run dev`
- `npm run build`
- `npm run start`
- `npm run typecheck`

## Playground and integration flow

Customer Playground requests post through:

- Browser -> `/api/ui/ingress/requests` (Next proxy route)
- Next -> `operator_ui` backend
- `operator_ui` -> ingress API (`/api/requests`) with configured headers/token

Backend services and inbound webhooks use the same ingress contract directly.
