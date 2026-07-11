# Versioning

| Artefact | Scheme | Tag prefix | Bump rule |
| --- | --- | --- | --- |
| Spec | `MAJOR.MINOR` | `spec/vX.Y.0` | MINOR = additive optional/namespaced fields, new vectors, new (optional) composition profiles. MAJOR = required/conditional field change, interception-point add/remove, verdict-shape or composition-semantics change. |
| Conformance vectors | tracks spec | shipped in spec tag tarball | Additive within a spec MINOR. |
| SDKs | semver | `<lang>/vX.Y.Z` | Independent per language. MAJOR on spec MAJOR or breaking API. |

Each SDK exports `SPEC_VERSION = "agent-hooks/X.Y"` matching the
`AgentContext.spec` value it emits and validates.

A conformance claim is the tuple
`(<framework>, <adapter-version>, agent-hooks/<spec-version>, <capabilities>, <profiles>, <identity-provider>, <sdk-lang>@<sdk-version>)`
plus the attached CTK per-part report (spec §13.3). There are no
conformance levels or tiers.
