# deploy (phase 2, intentionally empty)

Public deployment on t5s k3s: image the Rust service (multi-stage with
harness+docwright checkouts baked for the verifier's pack:check), k8s manifests
following the self-contained reproducible deployment convention, ingress via
*.shrwk.com. v1 runs locally only — see the root README.
