#!/usr/bin/env bash
#
# build_sidecars.sh — operationalize the Cherchi reference-parity oracle.
#
# Builds the two C++ artifacts the new-kernel crates differential-test against:
#   1. mesh_booleans / mesh_booleans_inputcheck  (Cherchi 2022 binary)
#        consumed by cherchi-sidecar-rs + cherchi-rs parity tests
#   2. Indirect_Predicates  (Marco Attene, LGPL-2.1, bundled in the repo above)
#        FFI-linked by indirect-predicates-sidecar-rs
#
# A single recursive clone populates BOTH (Indirect_Predicates lives at
# arrangements/external/Indirect_Predicates inside the Cherchi 2022 repo).
#
# Why a script and NOT a Dockerfile `RUN make`:
#   the build is ~22 min and the artifacts are large; baking it into the image
#   adds that cost to every image rebuild. This script runs once at
#   container-create / first-test instead. It is idempotent — re-running when
#   the binary already exists is a fast no-op.
#
# The default paths below match the crate defaults (cherchi-sidecar-rs
# DEFAULT_BIN_PATH and indirect-predicates-sidecar-rs DEFAULT_SRC), so no env
# vars are required. Override with CHERCHI2022_ROOT if you build elsewhere; then
# also export CHERCHI2022_BIN and INDIRECT_PREDICATES_SRC for the crates.
#
# Reference: docs/sidecar/cherchi2022_build_guide.md
# Roadmap milestone: M0 (docs/yang_functional_roadmap.md §4)
set -euo pipefail

CHERCHI2022_ROOT="${CHERCHI2022_ROOT:-/home/claude/cherchi2022}"
REPO_DIR="${CHERCHI2022_ROOT}/InteractiveAndRobustMeshBooleans"
BUILD_DIR="${REPO_DIR}/build"
BIN="${BUILD_DIR}/mesh_booleans"
IP_SRC="${REPO_DIR}/arrangements/external/Indirect_Predicates"
REPO_URL="https://github.com/gcherchi/InteractiveAndRobustMeshBooleans.git"

log() { printf '[build_sidecars] %s\n' "$*"; }

if [[ -x "${BIN}" && -f "${IP_SRC}/include/indirect_predicates.h" ]]; then
  log "already built — binary at ${BIN}; IP source at ${IP_SRC}. Nothing to do."
  log "  export CHERCHI2022_BIN=${BIN}"
  log "  export INDIRECT_PREDICATES_SRC=${IP_SRC}"
  exit 0
fi

mkdir -p "${CHERCHI2022_ROOT}"

if [[ ! -d "${REPO_DIR}/.git" ]]; then
  log "cloning ${REPO_URL} (recursive — pulls bundled Indirect_Predicates) ..."
  git clone --recursive --depth 1 "${REPO_URL}" "${REPO_DIR}"
else
  log "repo present at ${REPO_DIR}; ensuring submodules are checked out ..."
  git -C "${REPO_DIR}" submodule update --init --recursive --depth 1
fi

if [[ ! -f "${IP_SRC}/include/indirect_predicates.h" ]]; then
  log "ERROR: Indirect_Predicates header not found at ${IP_SRC}/include/ after clone."
  log "       Check the upstream layout in docs/sidecar/cherchi2022_build_guide.md."
  exit 1
fi

log "configuring + building (Release) — this is the ~22 min step ..."
cmake -S "${REPO_DIR}" -B "${BUILD_DIR}" -DCMAKE_BUILD_TYPE=Release
cmake --build "${BUILD_DIR}" -j "$(nproc)"

if [[ ! -x "${BIN}" ]]; then
  log "ERROR: build finished but ${BIN} is missing."
  exit 1
fi

log "done."
log "  mesh_booleans:           ${BIN}"
log "  mesh_booleans_inputcheck: ${BUILD_DIR}/mesh_booleans_inputcheck (if built)"
log "  Indirect_Predicates src:  ${IP_SRC}"
log ""
log "These match the crate default paths, so cargo test will pick them up with no"
log "env vars. To be explicit (or for a non-default CHERCHI2022_ROOT):"
log "  export CHERCHI2022_BIN=${BIN}"
log "  export INDIRECT_PREDICATES_SRC=${IP_SRC}"
log ""
log "NOTE (roadmap M2/M3): the interim LabeledArrangement producer needs a small"
log "C++ patch to mesh_booleans (emit per-triangle labels from customBooleanPipeline)."
log "That patch is applied in a later milestone; this script builds the stock binary."
