# Security policy

## Reporting a vulnerability

Please report security problems **privately** using GitHub's security advisories:
<https://github.com/KeremV1/OmniMark/security/advisories/new>

Do not open a public issue for a vulnerability. Include the affected version, the platform, and
steps or a sample file that reproduces the problem. You can expect an initial response within a
few days. This is a spare-time project, so fixes are best effort.

## Scope

OmniMark converts documents locally. It sends no document data anywhere; the only network calls are
the update check (can be disabled in Settings) and optional model downloads you start yourself.
Parser crashes, path traversal in archive handling, or unsafe rendering of converted content are
all in scope.

## Supported versions

Only the latest release receives fixes.
