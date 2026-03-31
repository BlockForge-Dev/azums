# Azums Monitoring Overlay

This overlay adds optional Prometheus-operator resources:

- `postgres-exporter` with custom SQL metrics for:
  - dispatch queue lag
  - dispatch ready backlog
  - callback retry backlog
  - callback terminal failures
- `ServiceMonitor` objects for:
  - reverse-proxy
  - ingress-api
  - status-api
  - postgres-exporter
- `PrometheusRule` alerts for:
  - ingress/status/execution-worker crash loops
  - dispatch queue lag
  - callback retry backlog
  - callback terminal failures

## Prerequisites

- Prometheus Operator CRDs installed (`ServiceMonitor`, `PrometheusRule`)
- kube-state-metrics available for crash-loop rules

## Apply

```bash
kubectl apply -k deployments/k8s/monitoring
```

## Validate

```bash
kubectl -n azums get deploy,svc | grep postgres-exporter
kubectl -n azums get servicemonitor
kubectl -n azums get prometheusrule
```

If you are not running Prometheus Operator, use `scripts/check_platform_health.ps1` on a schedule as a fallback.
