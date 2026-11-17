# Reporting a security vulnerability

Use [GitHub's private vulnerability report form](https://github.com/hongymagic/moon/security/advisories/new). Include the Moon release tag or commit, Windows version, affected component, and what access the issue requires or grants. A minimal example using a test account and synthetic data helps maintainers investigate.

Moon's installer, UIAccess window manager, ordinary-permission Settings app and per-logon IPC endpoints run with different permissions. If your report involves one of these boundaries, identify which process or account started each side. Keep credentials, personal configuration and real window titles out of the report.

Use the [public issue tracker](https://github.com/hongymagic/moon/issues) for ordinary bugs and feature requests. Keep vulnerability details in the private report until a coordinated disclosure.
