# Threat model

> **Status:** Draft · Companion to [`SECURITY.md`](../SECURITY.md) and
> [spec §1.4](../spec/AGENT-HOOKS-0.1.md#14-trust-model-and-non-goals).
> Every threat row names its mitigation (spec clause) and how that
> mitigation is verified today. Rows marked **GAP** are known-untested;
> they are collected in [§4](#4-gaps) with the roadmap items that track
> them. Honest GAP marking is the point of this document.

## 1. Assets and trust boundaries

| Asset | Description |
| --- | --- |
| Agent actions | Tool invocations, model calls, emitted output — the things a verdict permits or halts |
| `AgentContext` payloads | May contain user PII, secrets in tool arguments, model output |
| Interception records | The audit trail: verdicts + identities + ordering (`session_id`, `sequence`, `decided_by`) |
| Context identities | SHA-256 bindings used for approval (§9, §10.2) |

Trust boundaries (normative statement: spec §1.4):

- **Host: trusted.** Every guarantee is a MUST on the host; a
  non-cooperative host voids the contract and is out of scope.
- **Interceptors and the approval resolver: trusted.** In-process,
  full data access, registration grants write authority over every
  action (§1.4).
- **Adversary: untrusted data** flowing through the trusted host —
  external input, model output, tool results — and the supply chain
  around the artefacts themselves.

## 2. Threat catalog

Verification key: `AH-CTK-NNN` = conformance vector
(`conformance/vectors/`); file paths = unit/integration tests;
`golden` = `conformance/golden/identity.json` asserted in all five
SDKs; **GAP** = no automated verification exists.

| ID | Threat | STRIDE | Scope | Mitigation | Verification |
| --- | --- | --- | --- | --- | --- |
| TM-01 | Prompt-injection-driven tool abuse: untrusted content steers the model into a harmful tool call | E | In | Interceptor `deny`/`transform` at `pre_tool_call` (§3, §6); block propagation §6.2 | AH-CTK-010 (deny halts tool), AH-CTK-020 (transform rewrites args), AH-CTK-011/012 (deny at input/output) |
| TM-02 | Verdict forgery / reserved-reason spoofing: an interceptor emits `host_error:*` or a malformed verdict to impersonate host failures or smuggle state | S, T | In | §5 validation gate: `reason` MUST NOT start `host_error:`; transform-body shape rules; every interceptor and resolver return crosses the gate (§7, §9) | `sdk/rust/core/src/verdict.rs` from_wire tests; `sdk/rust/core/src/types.rs` verdict_validate_tests (NOW-06 regression); `sdk/python/tests/test_types.py` |
| TM-03 | Transform escaping `$target`: a transform path rooted elsewhere rewrites the snapshot, envelope, or host state | T, E | In | §5.2: path MUST be `$target`-rooted; foreign roots fail closed `host_error:transform_target_forbidden`; §4.3 forbids transform at startup/shutdown | `sdk/rust/core/src/path.rs` tests (foreign_root_forbidden); `sdk/python/tests/test_path.py`; AH-CTK-021 (alias), AH-CTK-022 (forbidden point) |
| TM-04 | Approval replay / identity tampering: a resolution bound to a different action is accepted, or the approved action drifts before execution | S, T | In | §9: resolution `context_identity` MUST equal the request's, else `host_error:approval_action_mismatch`; §10.2 identity computed pre-dispatch | Happy paths: AH-CTK-030/031/032. Mismatch/negative path: **GAP** — the CTK vector grammar cannot yet script a resolver returning a wrong identity (NOW-10) |
| TM-05 | TOCTOU via interceptor mutation: an interceptor mutates the context object it received to alter enforcement without returning a transform | T | In | §7: each interceptor receives its own copy; mutation MUST NOT affect enforcement (N05). `input_identity` computed before dispatch | Code-level in all five emitters (deep copy per interceptor). Dedicated adversarial vector: **GAP** (NOW-10) |
| TM-06 | Fail-open on interceptor crash or hang | D, E | In | §6.3: raise/timeout/non-conformant → `deny` with `host_error:interceptor_failed`/`interceptor_timeout` | **GAP** on both halves: no AH-CTK-07x fault-injection vectors exist (grammar cannot script interceptor faults — NOW-10), and no emitter enforces the §7 timeout, so `interceptor_timeout` is unreachable (NOW-09) |
| TM-07 | Zero-interceptor bypass: an emitter with nothing registered silently allows everything | E | In | §7: `enforce`-mode emission with zero interceptors fails closed `host_error:no_interceptor` | AH-CTK-061 |
| TM-08 | Identity collision via canonicalization divergence: two SDKs (or two values) canonicalize differently, breaking approval binding and audit correlation | S, R | In | §10.1 RFC 8785 via single Rust core; §10.2 closed L0+L1 preimage; all bindings delegate | `golden` (11 fixtures asserted in Rust/Python/TS/.NET/Go); `sdk/rust/core/src/canonical.rs` JCS unit tests. Residual: values outside I-JSON (ints >2⁵³, lone surrogates) — open, P-002 |
| TM-09 | Audit-record payload leakage: records or failure messages exfiltrate context data into audit storage | I | In | Identity-only records (no `transformed_target`, Q5/Q6); failure verdicts carry exception *type* only (NOW-05) | Record shape: `spec/schema/interception-record.schema.json` + `sdk/python/tests/test_decided_by.py`; NOW-05 by code review — dedicated leak test: **GAP** |
| TM-10 | Exfiltration/SSRF via `evidence.verification_pointers`: attacker-supplied URIs dereferenced by host or audit tooling | I | In (host obligation) | §5.3/§14: host MUST NOT dereference; propagate opaque | **GAP** — prose only; no test, no scheme allow-list guidance (arch-review X03) |
| TM-11 | Streaming egress before interception: partial model output reaches the caller before `output` (or `post_model_call`) is evaluated | E, I | In (partially open) | §12.1 covers model→host streaming (assemble before `post_model_call`, else fail closed `host_error:streaming_unsupported`). Host→caller egress before `output`: undefined | §12.1 negative path: **GAP** (no vector). Host→caller egress: **GAP** — design open (2026-07-07 arch-review X05; no proposal filed yet) |
| TM-12 | Resource exhaustion: unbounded `target`/`messages` canonicalized, hashed, and deep-copied per interceptor per emission | D | In | None normative — no payload size or depth bounds in the spec | **GAP** (arch-review X12/NOW-13-adjacent; no bounds, no benchmark gate) |
| TM-13 | Label-flow loss: `result_labels` from superseded permit verdicts discarded by the §7.1 fold, or not persisted per §5.4 | I | In | §5.4 persistence obligations | **GAP** — no vector exercises §5.4; fold discard tracked as NOW-15 |
| TM-14 | Supply-chain compromise of the artefacts: squatted names, mutable CI actions, unpinned deps | T, S | In | Distribution `agent-hooks-sdk` published on PyPI/crates.io (squatted `agent-hooks` avoided); GitHub Actions pinned by commit SHA; `Cargo.lock` committed; CodeQL + Dependabot enabled | Name claims live (registry state); pins in `.github/workflows/*.yml`. SBOM/signing/provenance: **GAP** (arch-review X15) |
| TM-15 | Host bypass of interception points: framework code paths (direct tool execution, plugins, background tasks) never reach an emitter | E | **Out** | §1.4: no complete-mediation claim; CLAIMS.md requires production-path attestation | Explicitly disclaimed; unverifiable by the CTK by design |
| TM-16 | Malicious or compromised interceptor/resolver: rewrites actions, exfiltrates context | T, I, E | **Out** | §1.4: registration = write authority; interceptors fully trusted; authentication out of scope | Explicitly disclaimed |
| TM-17 | Hostile host: skips points, ignores verdicts, misreports mode | All | **Out** | §1.4: host inside trust boundary; cooperative contract | Explicitly disclaimed |

## 3. Out of scope

Mirroring spec §1.4 — this contract does **not** defend against or
provide:

- A hostile or buggy host (TM-17): no mechanism detects skipped
  interception points, ignored verdicts, or misreported enforcement
  mode.
- Malicious interceptors or resolvers (TM-16): they are inside the
  trust boundary.
- Complete mediation (TM-15): coverage of the eight points depends on
  the host adapter.
- Sandboxing, process isolation, interceptor authentication, or
  registration authorization.
- Security certification via conformance: the CTK is cooperative-path
  testing against mocks, not adversarial testing.

## 4. Gaps

Every **GAP** above, with the tracking item:

| Gap | Threat rows | Tracked by |
| --- | --- | --- |
| CTK cannot script interceptor/resolver faults (crash, malformed return, wrong approval identity), so §6.3 fail-closed and the §9 mismatch path are untested at the conformance bar | TM-04, TM-05, TM-06 | NOW-10 |
| No emitter enforces the §7 interceptor/resolver timeout; `host_error:interceptor_timeout` unreachable | TM-06 | NOW-09 |
| Non-I-JSON values (ints >2⁵³, lone surrogates, NaN/Infinity pre-marshalling) undermine identity injectivity/determinism | TM-08 | P-002 (decision pending) |
| No dedicated test that failure verdicts stay payload-free | TM-09 | follow-up to NOW-05 |
| `verification_pointers` no-dereference is prose-only; no test or scheme guidance | TM-10 | arch-review X03 |
| §12.1 streaming fail-closed path untested; host→caller egress before `output` undefined | TM-11 | arch-review X05 (design open) |
| No payload size/depth bounds; canonicalization/copy cost unbounded | TM-12 | arch-review X12 |
| §5.4 `result_labels` persistence untested; fold discards labels from superseded permits | TM-13 | NOW-15 |
| No SBOM, artefact signing, or build provenance for the five SDK release channels | TM-14 | arch-review X15 |
| §14 redaction MUST (`extensions.<host>.redacted`) is untestable prose in tension with §13.1's single bar | (governance) | NOW-16 remainder / AR-01-002 |
