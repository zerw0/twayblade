#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
umask 077
matrix_context="${MATRIX_TEST_DOCKER_CONTEXT:-$(docker context show)}"
matrix_endpoint="$(docker context inspect "$matrix_context" --format '{{.Endpoints.docker.Host}}')"
case "$matrix_endpoint" in
    unix://*|npipe://*) ;;
    *) printf 'Use a local Docker context for this fixture.\n' >&2; exit 1 ;;
esac
compose=(docker --context "$matrix_context" compose -f tests/integration/compose.yaml)
case "${1:-test}" in
    up|test)
        mkdir -p test-data/local/integration
        "${compose[@]}" run --rm --no-deps --entrypoint python synapse /bootstrap.py generate
        "${compose[@]}" up -d --force-recreate --wait --wait-timeout 90
        "${compose[@]}" exec -T synapse python /bootstrap.py register
        if [[ "${1:-test}" == test ]]; then
            # Keep encrypted fixture data for inspection; stop only our test service.
            trap '"${compose[@]}" stop' EXIT
            cargo test --locked -p matrix-client-core encrypted_delivery_profile_and_same_device_store_restore -- --ignored --nocapture
        fi
        ;;
    down) "${compose[@]}" down ;;
    status) "${compose[@]}" ps ;;
    *) printf 'Usage: bash tests/integration/run.sh [up|test|down|status]\n' >&2; exit 1 ;;
esac
