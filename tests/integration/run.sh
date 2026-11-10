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
    up|test|keychain|services)
        mkdir -p test-data/local/integration
        "${compose[@]}" run --rm --no-deps --entrypoint python synapse /bootstrap.py generate
        "${compose[@]}" up -d --force-recreate --wait --wait-timeout 90
        "${compose[@]}" exec -T synapse python /bootstrap.py register
        if [[ "${1:-test}" != up ]]; then
            # Keep encrypted fixture data for inspection; stop only our test service.
            trap '"${compose[@]}" stop' EXIT
            if [[ "${1:-test}" == keychain ]]; then
                cargo test --locked --workspace --all-features keychain_session_lifecycle_across_processes -- --ignored --nocapture
            elif [[ "${1:-test}" == services ]]; then
                for test_name in encrypted_delivery_profile_and_same_device_store_restore timeline_metadata_and_send_queue_actions members_typing_and_read_receipts pagination_preserves_message_identity room_list_service_tracks_recency_and_details; do
                    cargo test --locked -p matrix-client-core --all-features "$test_name" -- --ignored --nocapture
                done
            else
                cargo test --locked -p matrix-client-core encrypted_delivery_profile_and_same_device_store_restore -- --ignored --nocapture
            fi
        fi
        ;;
    down) "${compose[@]}" down ;;
    status) "${compose[@]}" ps ;;
    *) printf 'Usage: bash tests/integration/run.sh [up|test|keychain|services|down|status]\n' >&2; exit 1 ;;
esac
