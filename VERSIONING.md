# Versioning

| Artefact | Scheme | Tag prefix | Bump rule |
| --- | --- | --- | --- |
| Spec | `MAJOR.MINOR` | `spec/vX.Y.0` | MINOR = additive optional fields, new L2/L3, new vectors. MAJOR = L0/L1 change, interception-point add/remove, verdict-shape change. |
| Conformance vectors | tracks spec | shipped in spec tag tarball | Additive within a spec MINOR. |
| SDKs | semver | `<lang>/vX.Y.Z` | Independent per language. MAJOR on spec MAJOR or breaking API. |

Each SDK exports `SPEC_VERSION = "agent-hooks/X.Y"` matching the
`AgentContext.spec` value it emits and validates.

A conformance claim is the tuple
`(<framework>, <adapter-version>, agent-hooks/<spec-version>, Level <N>, <sdk-lang>@<sdk-version>)`.
