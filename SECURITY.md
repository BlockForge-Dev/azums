# Security Policy

## Supported Versions

Security fixes are provided for the latest `1.x` release. Users should upgrade to the newest
published patch release before reporting an issue or requesting support.

## Reporting a Vulnerability

Please report suspected vulnerabilities through GitHub's private vulnerability reporting flow:

https://github.com/BlockForge-Dev/azums/security/advisories/new

Do not open a public issue for an unpatched vulnerability. Include affected versions, impact,
reproduction steps, and any suggested mitigation. We will acknowledge a complete report as soon as
practical, coordinate validation and remediation privately, and publish an advisory when users can
upgrade safely.

Never include production credentials, private payloads, or customer data in a report.

## Scope

Relevant reports include authentication or authorization bypasses, unsafe deserialization, secret
exposure, denial-of-service vectors, dependency vulnerabilities with an active Azums code path, and
violations of Azums' documented durability or isolation guarantees that create a security impact.
