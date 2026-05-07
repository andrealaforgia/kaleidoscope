//! Codex corpus regenerator — maintainer ritual per ADR-0023.
//!
//! Reads the v0.27 resource-class constants from
//! `opentelemetry_semantic_conventions::resource::*` (with the
//! `semconv_experimental` feature enabled, so the experimental
//! attributes the v0 corpus needs are visible) and emits the
//! checked-in artefact at
//! `crates/codex/src/generated/semconv_0_27.rs`.
//!
//! ## Mechanism
//!
//! The binary's source enumerates the upstream constants it consumes
//! via real Rust `use` imports — every entry below is a literal
//! reference to a `&'static str` constant defined in the upstream
//! crate. If upstream renames or removes any of these, the xtask
//! fails to compile, which is the audit signal we want: the
//! corpus regeneration step cannot silently drop attributes.
//!
//! ## Invocation
//!
//! ```sh
//! cargo run --package regenerate-codex-corpus --bin regenerate-codex-corpus
//! ```
//!
//! The binary writes the regenerated file in place, runs
//! alphabetical sort on the entries (so the diff on regeneration
//! is minimal when one attribute is added or removed), and prints
//! a one-line summary so the maintainer can sanity-check before
//! committing.
//!
//! ## Scope discipline (per slice-02-otel-semconv-corpus.md)
//!
//! v0 cares about *resource* attributes only. The constants below
//! are the upstream `resource` module's re-exports plus the legacy
//! `attribute::DEPLOYMENT_ENVIRONMENT` (which is not re-exported
//! from `resource` in 0.27 but is the canonical short-form name
//! Spark composes today). Trace, metric, and log scoped attributes
//! are out of scope.
//!
//! ## Generated-file shape (per ADR-0023 §2)
//!
//! ```rust,ignore
//! pub(crate) const SEMCONV_0_27: &[BlessedAttribute] = &[
//!     BlessedAttribute::Exact("deployment.environment"),
//!     BlessedAttribute::Exact("host.arch"),
//!     // ... (alphabetically sorted)
//! ];
//! ```
//!
//! The catalogue constructor (`SchemaCatalogue::new` in
//! `crates/codex/src/catalogue.rs`) concatenates this slice with
//! the hand-maintained house-attributes slice. The two slices are
//! kept separate at v0 so a regeneration that misbehaves cannot
//! accidentally clobber the house attributes.

#![forbid(unsafe_code)]
// `DEPLOYMENT_ENVIRONMENT` was deprecated upstream in favour of
// `DEPLOYMENT_ENVIRONMENT_NAME`. We still bless the legacy short
// form because Spark composes Resources with `deployment.environment`
// today; the day Spark migrates to the new name we drop this allow
// and remove the const.
#![allow(deprecated)]

use std::fs;
use std::path::PathBuf;

// ----------------------------------------------------------------------
// Resource-class semconv 0.27 constants the v0 corpus blesses.
//
// Each entry is a real upstream `&'static str` constant. The Rust
// compiler verifies these references at xtask build time; if upstream
// drops or renames any of them, this binary fails to compile and the
// maintainer sees the audit signal in their PR.
//
// Order below mirrors the upstream `resource.rs` module's source order
// for ease of cross-referencing during PR review; the emitted file is
// sorted alphabetically by attribute string value (per ADR-0023 §2).
// ----------------------------------------------------------------------

use opentelemetry_semantic_conventions::attribute::DEPLOYMENT_ENVIRONMENT;

use opentelemetry_semantic_conventions::resource::{
    ANDROID_OS_API_LEVEL, AWS_ECS_CLUSTER_ARN, AWS_ECS_CONTAINER_ARN, AWS_ECS_LAUNCHTYPE,
    AWS_ECS_TASK_ARN, AWS_ECS_TASK_FAMILY, AWS_ECS_TASK_ID, AWS_ECS_TASK_REVISION,
    AWS_EKS_CLUSTER_ARN, AWS_LOG_GROUP_ARNS, AWS_LOG_GROUP_NAMES, AWS_LOG_STREAM_ARNS,
    AWS_LOG_STREAM_NAMES, BROWSER_BRANDS, BROWSER_LANGUAGE, BROWSER_MOBILE, BROWSER_PLATFORM,
    CLOUDFOUNDRY_APP_ID, CLOUDFOUNDRY_APP_NAME, CLOUDFOUNDRY_ORG_ID, CLOUDFOUNDRY_ORG_NAME,
    CLOUDFOUNDRY_PROCESS_ID, CLOUDFOUNDRY_PROCESS_TYPE, CLOUDFOUNDRY_SPACE_ID,
    CLOUDFOUNDRY_SPACE_NAME, CLOUDFOUNDRY_SYSTEM_ID, CLOUDFOUNDRY_SYSTEM_INSTANCE_ID,
    CLOUD_ACCOUNT_ID, CLOUD_AVAILABILITY_ZONE, CLOUD_PLATFORM, CLOUD_PROVIDER, CLOUD_REGION,
    CLOUD_RESOURCE_ID, CONTAINER_COMMAND, CONTAINER_COMMAND_ARGS, CONTAINER_COMMAND_LINE,
    CONTAINER_ID, CONTAINER_IMAGE_ID, CONTAINER_IMAGE_NAME, CONTAINER_IMAGE_REPO_DIGESTS,
    CONTAINER_IMAGE_TAGS, CONTAINER_LABEL, CONTAINER_NAME, CONTAINER_RUNTIME,
    DEPLOYMENT_ENVIRONMENT_NAME, DEVICE_ID, DEVICE_MANUFACTURER, DEVICE_MODEL_IDENTIFIER,
    DEVICE_MODEL_NAME, FAAS_INSTANCE, FAAS_MAX_MEMORY, FAAS_NAME, FAAS_VERSION,
    GCP_CLOUD_RUN_JOB_EXECUTION, GCP_CLOUD_RUN_JOB_TASK_INDEX, GCP_GCE_INSTANCE_HOSTNAME,
    GCP_GCE_INSTANCE_NAME, HEROKU_APP_ID, HEROKU_RELEASE_COMMIT, HEROKU_RELEASE_CREATION_TIMESTAMP,
    HOST_ARCH, HOST_CPU_CACHE_L2_SIZE, HOST_CPU_FAMILY, HOST_CPU_MODEL_ID, HOST_CPU_MODEL_NAME,
    HOST_CPU_STEPPING, HOST_CPU_VENDOR_ID, HOST_ID, HOST_IMAGE_ID, HOST_IMAGE_NAME,
    HOST_IMAGE_VERSION, HOST_IP, HOST_MAC, HOST_NAME, HOST_TYPE, K8S_CLUSTER_NAME, K8S_CLUSTER_UID,
    K8S_CONTAINER_NAME, K8S_CONTAINER_RESTART_COUNT, K8S_CONTAINER_STATUS_LAST_TERMINATED_REASON,
    K8S_CRONJOB_NAME, K8S_CRONJOB_UID, K8S_DAEMONSET_NAME, K8S_DAEMONSET_UID, K8S_DEPLOYMENT_NAME,
    K8S_DEPLOYMENT_UID, K8S_JOB_NAME, K8S_JOB_UID, K8S_NAMESPACE_NAME, K8S_NODE_NAME, K8S_NODE_UID,
    K8S_POD_ANNOTATION, K8S_POD_LABEL, K8S_POD_NAME, K8S_POD_UID, K8S_REPLICASET_NAME,
    K8S_REPLICASET_UID, K8S_STATEFULSET_NAME, K8S_STATEFULSET_UID, OCI_MANIFEST_DIGEST,
    OS_BUILD_ID, OS_DESCRIPTION, OS_NAME, OS_TYPE, OS_VERSION, OTEL_SCOPE_NAME, OTEL_SCOPE_VERSION,
    PROCESS_COMMAND, PROCESS_COMMAND_ARGS, PROCESS_COMMAND_LINE, PROCESS_EXECUTABLE_NAME,
    PROCESS_EXECUTABLE_PATH, PROCESS_OWNER, PROCESS_PARENT_PID, PROCESS_PID,
    PROCESS_RUNTIME_DESCRIPTION, PROCESS_RUNTIME_NAME, PROCESS_RUNTIME_VERSION,
    SERVICE_INSTANCE_ID, SERVICE_NAME, SERVICE_NAMESPACE, SERVICE_VERSION, TELEMETRY_DISTRO_NAME,
    TELEMETRY_DISTRO_VERSION, TELEMETRY_SDK_LANGUAGE, TELEMETRY_SDK_NAME, TELEMETRY_SDK_VERSION,
    USER_AGENT_ORIGINAL, WEBENGINE_DESCRIPTION, WEBENGINE_NAME, WEBENGINE_VERSION,
};

/// The full set of v0.27 resource-class semconv attribute names that
/// the Codex catalogue blesses at v0. Each entry is a `&'static str`
/// pulled directly from the upstream crate.
const SEMCONV_RESOURCE_NAMES: &[&str] = &[
    // Legacy short-form `deployment.environment` (deprecated upstream
    // in favour of `deployment.environment.name` but still the
    // canonical name Spark composes today). Pulled directly from
    // `attribute::DEPLOYMENT_ENVIRONMENT` because `resource::*` only
    // re-exports the new `_NAME`-suffixed form in 0.27.
    DEPLOYMENT_ENVIRONMENT,
    // The rest mirror the `resource::*` re-export set verbatim. If
    // upstream drops or renames any of these, this xtask fails to
    // compile and the maintainer sees the audit signal.
    ANDROID_OS_API_LEVEL,
    AWS_ECS_CLUSTER_ARN,
    AWS_ECS_CONTAINER_ARN,
    AWS_ECS_LAUNCHTYPE,
    AWS_ECS_TASK_ARN,
    AWS_ECS_TASK_FAMILY,
    AWS_ECS_TASK_ID,
    AWS_ECS_TASK_REVISION,
    AWS_EKS_CLUSTER_ARN,
    AWS_LOG_GROUP_ARNS,
    AWS_LOG_GROUP_NAMES,
    AWS_LOG_STREAM_ARNS,
    AWS_LOG_STREAM_NAMES,
    BROWSER_BRANDS,
    BROWSER_LANGUAGE,
    BROWSER_MOBILE,
    BROWSER_PLATFORM,
    CLOUD_ACCOUNT_ID,
    CLOUD_AVAILABILITY_ZONE,
    CLOUD_PLATFORM,
    CLOUD_PROVIDER,
    CLOUD_REGION,
    CLOUD_RESOURCE_ID,
    CLOUDFOUNDRY_APP_ID,
    CLOUDFOUNDRY_APP_NAME,
    CLOUDFOUNDRY_ORG_ID,
    CLOUDFOUNDRY_ORG_NAME,
    CLOUDFOUNDRY_PROCESS_ID,
    CLOUDFOUNDRY_PROCESS_TYPE,
    CLOUDFOUNDRY_SPACE_ID,
    CLOUDFOUNDRY_SPACE_NAME,
    CLOUDFOUNDRY_SYSTEM_ID,
    CLOUDFOUNDRY_SYSTEM_INSTANCE_ID,
    CONTAINER_COMMAND,
    CONTAINER_COMMAND_ARGS,
    CONTAINER_COMMAND_LINE,
    CONTAINER_ID,
    CONTAINER_IMAGE_ID,
    CONTAINER_IMAGE_NAME,
    CONTAINER_IMAGE_REPO_DIGESTS,
    CONTAINER_IMAGE_TAGS,
    CONTAINER_LABEL,
    CONTAINER_NAME,
    CONTAINER_RUNTIME,
    DEPLOYMENT_ENVIRONMENT_NAME,
    DEVICE_ID,
    DEVICE_MANUFACTURER,
    DEVICE_MODEL_IDENTIFIER,
    DEVICE_MODEL_NAME,
    FAAS_INSTANCE,
    FAAS_MAX_MEMORY,
    FAAS_NAME,
    FAAS_VERSION,
    GCP_CLOUD_RUN_JOB_EXECUTION,
    GCP_CLOUD_RUN_JOB_TASK_INDEX,
    GCP_GCE_INSTANCE_HOSTNAME,
    GCP_GCE_INSTANCE_NAME,
    HEROKU_APP_ID,
    HEROKU_RELEASE_COMMIT,
    HEROKU_RELEASE_CREATION_TIMESTAMP,
    HOST_ARCH,
    HOST_CPU_CACHE_L2_SIZE,
    HOST_CPU_FAMILY,
    HOST_CPU_MODEL_ID,
    HOST_CPU_MODEL_NAME,
    HOST_CPU_STEPPING,
    HOST_CPU_VENDOR_ID,
    HOST_ID,
    HOST_IMAGE_ID,
    HOST_IMAGE_NAME,
    HOST_IMAGE_VERSION,
    HOST_IP,
    HOST_MAC,
    HOST_NAME,
    HOST_TYPE,
    K8S_CLUSTER_NAME,
    K8S_CLUSTER_UID,
    K8S_CONTAINER_NAME,
    K8S_CONTAINER_RESTART_COUNT,
    K8S_CONTAINER_STATUS_LAST_TERMINATED_REASON,
    K8S_CRONJOB_NAME,
    K8S_CRONJOB_UID,
    K8S_DAEMONSET_NAME,
    K8S_DAEMONSET_UID,
    K8S_DEPLOYMENT_NAME,
    K8S_DEPLOYMENT_UID,
    K8S_JOB_NAME,
    K8S_JOB_UID,
    K8S_NAMESPACE_NAME,
    K8S_NODE_NAME,
    K8S_NODE_UID,
    K8S_POD_ANNOTATION,
    K8S_POD_LABEL,
    K8S_POD_NAME,
    K8S_POD_UID,
    K8S_REPLICASET_NAME,
    K8S_REPLICASET_UID,
    K8S_STATEFULSET_NAME,
    K8S_STATEFULSET_UID,
    OCI_MANIFEST_DIGEST,
    OS_BUILD_ID,
    OS_DESCRIPTION,
    OS_NAME,
    OS_TYPE,
    OS_VERSION,
    OTEL_SCOPE_NAME,
    OTEL_SCOPE_VERSION,
    PROCESS_COMMAND,
    PROCESS_COMMAND_ARGS,
    PROCESS_COMMAND_LINE,
    PROCESS_EXECUTABLE_NAME,
    PROCESS_EXECUTABLE_PATH,
    PROCESS_OWNER,
    PROCESS_PARENT_PID,
    PROCESS_PID,
    PROCESS_RUNTIME_DESCRIPTION,
    PROCESS_RUNTIME_NAME,
    PROCESS_RUNTIME_VERSION,
    SERVICE_INSTANCE_ID,
    SERVICE_NAME,
    SERVICE_NAMESPACE,
    SERVICE_VERSION,
    TELEMETRY_DISTRO_NAME,
    TELEMETRY_DISTRO_VERSION,
    TELEMETRY_SDK_LANGUAGE,
    TELEMETRY_SDK_NAME,
    TELEMETRY_SDK_VERSION,
    USER_AGENT_ORIGINAL,
    WEBENGINE_DESCRIPTION,
    WEBENGINE_NAME,
    WEBENGINE_VERSION,
];

/// The header comment block emitted at the top of the generated
/// file. Per ADR-0023 §2 the comment names the upstream version,
/// disclaims hand-editing, and points the reader at the regeneration
/// command.
const FILE_HEADER: &str = "//! Codex semconv 0.27 corpus.
//!
//! AUTO-GENERATED by `regenerate-codex-corpus` from upstream
//! `opentelemetry-semantic-conventions = \"=0.27\"` — DO NOT EDIT BY
//! HAND. To regenerate, run:
//!
//! ```sh
//! cargo run --package regenerate-codex-corpus --bin regenerate-codex-corpus
//! ```
//!
//! Diff this file in PR review when the upstream pin moves; the diff
//! is the audit trail of what the catalogue gained or lost (per
//! ADR-0023 §2). The slice is sorted alphabetically by attribute
//! name so a single-attribute upstream change yields a single-line
//! diff here.
//!
//! Scope: resource-class attributes only (per the slice-02 brief).
//! Trace, metric, and log scoped attributes are out of scope at v0.

use crate::catalogue::BlessedAttribute;
";

fn main() {
    // Sort + dedup. The dedup is defensive: the upstream crate has
    // a few aliases (legacy short names alongside the new
    // `*_NAME`-suffixed form) and we want each blessed string to
    // appear exactly once in the catalogue.
    let mut names: Vec<&'static str> = SEMCONV_RESOURCE_NAMES.to_vec();
    names.sort_unstable();
    names.dedup();

    // Emit the file body.
    let mut body = String::new();
    body.push_str(FILE_HEADER);
    body.push('\n');
    body.push_str("pub(crate) const SEMCONV_0_27: &[BlessedAttribute] = &[\n");
    for name in &names {
        body.push_str("    BlessedAttribute::Exact(\"");
        body.push_str(name);
        body.push_str("\"),\n");
    }
    body.push_str("];\n");

    // Resolve the output path relative to the workspace root. The
    // xtask is invoked via `cargo run --package …` from the
    // workspace root, so `CARGO_MANIFEST_DIR` here points at
    // `xtask/regenerate_codex_corpus/`. We walk up two levels to
    // reach the workspace root, then descend into the codex crate.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("xtask Cargo.toml is two directories below the workspace root")
        .to_path_buf();
    let output_path = workspace_root
        .join("crates")
        .join("codex")
        .join("src")
        .join("generated")
        .join("semconv_0_27.rs");

    fs::write(&output_path, &body).expect("write generated semconv_0_27.rs");

    println!(
        "regenerated {count} entries from opentelemetry-semantic-conventions =0.27 → {path}",
        count = names.len(),
        path = output_path.display(),
    );
}
