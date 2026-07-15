# Security policy

## Supported versions

Before the first public tag, security fixes are made on `main`. After release,
the latest 1.x release receives security fixes; superseded and pre-1.0 builds
are not supported unless a maintainer states otherwise in a release note.

## Reporting a vulnerability

Do not disclose a suspected vulnerability in a public issue, discussion, pull
request, log, or screenshot.

Use GitHub's private **Report a vulnerability** control on the repository's
Security tab when it is available. Include:

- the affected version or commit;
- the supported environment used to reproduce it;
- the smallest safe reproduction or proof of concept;
- the expected impact and required attacker capabilities;
- whether terminal content, a local process, clipboard data, configuration, or
  release infrastructure is involved;
- any suggested remediation or embargo constraints.

If private vulnerability reporting is not enabled, open a public issue that
contains only a request for private maintainer contact. Do not include technical
details until a private channel is established.

Maintainers will validate reachability, coordinate a fix and release when
warranted, and credit reporters who request credit. This project does not
currently operate a paid bug-bounty program or promise a response-time SLA.

## Security boundaries

Reports are especially useful for:

- escape-sequence parsing that can escape the terminal surface or exhaust
  resources;
- clipboard or paste behavior that can execute or alter input without the
  documented confirmation boundary;
- shell-integration metadata injection;
- process, PTY, tab, or worker lifecycle failures that affect other sessions;
- unsafe configuration or filesystem handling;
- release artifact substitution, workflow-token exposure, or dependency
  compromise.

A terminal intentionally displays output from local programs and sends user
input to the active PTY. Visual spoofing that requires a program already
running in that PTY may be a terminal-behavior bug rather than a privilege
boundary break, but it is still worth reporting when controls, bidi text,
metadata, or persistent UI can mislead the user.
