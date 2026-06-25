# Security Policy

## Supported Versions

Security updates are provided for the latest released version of Cassady. If you are using an older version, please upgrade to the latest release before reporting an issue, unless the issue also affects the latest release.

| Version | Supported |
| ------- | --------- |
| Latest | ✅ |
| Older releases | ❌ |

## Reporting a Vulnerability

Please do **not** report security vulnerabilities in public GitHub issues, discussions, or pull requests.

To report a vulnerability, use GitHub's private vulnerability reporting for this repository:

1. Open the repository on GitHub.
2. Go to **Security** → **Report a vulnerability**.
3. Include as much detail as you can about the issue, impact, affected versions, and steps to reproduce.

## What to Include

Helpful reports include:

- A description of the vulnerability and likely impact.
- Steps to reproduce or a minimal proof of concept.
- The Cassady version, operating system, shell, and terminal environment.
- Relevant configuration details with secrets removed.
- Any known mitigations or workarounds.

Do not include live API keys, tokens, private prompts, or sensitive project files in a report. Redact secrets before sharing logs or configuration.

## Response Expectations

After a report is received, the maintainer will aim to:

- Acknowledge the report within 7 days.
- Confirm whether the issue is in scope and reproducible.
- Provide status updates when there is meaningful progress.
- Coordinate disclosure timing before publishing details publicly.

Security fixes may be released as patch versions when appropriate. Public disclosure should wait until a fix or mitigation is available, unless otherwise coordinated with the maintainer.

## Scope

Examples of in-scope issues include vulnerabilities in Cassady that could:

- Bypass documented access modes, approval prompts, or workspace boundaries.
- Cause unintended file reads, writes, edits, or shell command execution.
- Leak API keys, provider credentials, chat history, or local configuration.
- Corrupt or expose session data stored under `~/.cass`.
- Introduce unsafe behavior in bundled release artifacts.

Out-of-scope issues generally include:

- Vulnerabilities in third-party model providers or APIs not controlled by this project.
- Prompt-injection behavior that does not bypass Cassady's documented safety controls.
- Issues that require a compromised local machine, shell, dependency cache, or provider account.
- Reports against unsupported older versions that are fixed in the latest release.

## Safe Harbor

Good-faith security research is welcome. Please avoid privacy violations, data destruction, service disruption, and accessing data that does not belong to you. If you follow this policy and make a good-faith effort to avoid harm, the maintainer will not pursue legal action for your research.
