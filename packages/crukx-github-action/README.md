# Crukx Release Gate GitHub Action

Run `crukx gate` in your CI pipeline to block releases when reliability, security, latency, or consistency constraints aren't met.

## Usage

```yaml
name: Release Gate

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  gate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Run Crukx Gate
        uses: crukx/crukx-gate@v1
        with:
          policy: crukx.yml
          format: junit
          fail-on-regression: true
```

## Inputs

| Input | Description | Default |
|---|---|---|
| `version` | Version of crukx to install | `latest` |
| `policy` | Path to crukx.yml policy file | `crukx.yml` |
| `format` | Output format (junit, markdown, json) | `markdown` |
| `fail-on-regression` | Exit 1 on regression vs last green gate | `true` |

## Outputs

| Output | Description |
|---|---|
| `decision` | Gate decision (PASS or BLOCK) |
| `vtr` | Verified Trust Rate |

## Example Policy File

```yaml
contracts:
  - id: my-contract
    mode: block
    repeat: 3

constraints:
  vtr_minimum: 0.95
  p95_latency_max_ms: 5000
  security_critical_delta_max: 0
```

## Permissions

The action needs the following permissions:

```yaml
permissions:
  contents: read
  checks: write  # For JUnit output
```

## License

MIT
