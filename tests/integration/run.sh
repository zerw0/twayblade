#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
umask 077
# Prefer an existing dedicated local fixture VM; never change global Docker context.
if [[ -z "${MATRIX_TEST_DOCKER_CONTEXT:-}" ]] && docker --context colima-twayblade-test info >/dev/null 2>&1; then
    matrix_context=colima-twayblade-test
else
    matrix_context="${MATRIX_TEST_DOCKER_CONTEXT:-$(docker context show)}"
fi
matrix_endpoint="$(docker context inspect "$matrix_context" --format '{{.Endpoints.docker.Host}}')"
case "$matrix_endpoint" in
    unix://*|npipe://*) ;;
    *) printf 'Use a local Docker context for this fixture.\n' >&2; exit 1 ;;
esac
compose=(docker --context "$matrix_context" compose -f tests/integration/compose.yaml)
case "${1:-test}" in
    call)
        mkdir -p test-data/local/integration
        "${compose[@]}" run --rm --no-deps --entrypoint python synapse /bootstrap.py generate
        "${compose[@]}" run --rm --no-deps --entrypoint python synapse /bootstrap.py call-config
        trap '"${compose[@]}" --profile call stop element-call call-proxy lk-jwt-service livekit synapse' EXIT
        "${compose[@]}" --profile call up -d --wait --wait-timeout 90 synapse livekit lk-jwt-service element-call call-proxy
        "${compose[@]}" exec -T synapse python /bootstrap.py register-pair
        MATRIX_TEST_CALL_CONTEXT="$matrix_context" cargo test --locked -p twayblade-core --all-features "${MATRIX_TEST_CALL_FILTER:-native_matrixrtc}" -- --ignored --nocapture --test-threads=1
        ;;
    presence)
        mkdir -p test-data/local/integration
        "${compose[@]}" run --rm --no-deps --entrypoint python synapse /bootstrap.py generate
        trap '"${compose[@]}" stop synapse' EXIT
        for mode in enabled disabled; do
            setting=presence-on
            if [[ "$mode" == disabled ]]; then setting=presence-off; fi
            "${compose[@]}" run --rm --no-deps --entrypoint python synapse /bootstrap.py "$setting"
            "${compose[@]}" up -d --force-recreate --wait --wait-timeout 90 synapse
            "${compose[@]}" exec -T synapse python /bootstrap.py register-pair
            MATRIX_TEST_PRESENCE="$mode" cargo test --locked -p twayblade-core --all-features presence_enabled_and_disabled_server_evidence -- --ignored --nocapture
            "${compose[@]}" stop synapse
        done
        # Leave the shared local fixture's normal default restored while stopped.
        "${compose[@]}" run --rm --no-deps --entrypoint python synapse /bootstrap.py presence-on
        ;;
    recovery)
        mkdir -p test-data/local/integration test-data/local/recovery
        "${compose[@]}" run --rm --no-deps --entrypoint python synapse /bootstrap.py generate
        "${compose[@]}" --profile recovery run --rm --no-deps --entrypoint python recovery-synapse /bootstrap.py generate
        "${compose[@]}" --profile recovery run --rm --no-deps --entrypoint python recovery-synapse /bootstrap.py recovery-config
        trap '"${compose[@]}" --profile recovery stop synapse recovery-synapse' EXIT
        "${compose[@]}" --profile recovery up -d --force-recreate --wait --wait-timeout 90 synapse recovery-synapse
        "${compose[@]}" exec -T synapse python /bootstrap.py register
        "${compose[@]}" --profile recovery exec -T recovery-synapse python /bootstrap.py register
        cargo test --locked -p twayblade-core --all-features revoked_device_and_soft_logout_recovery -- --ignored --nocapture
        ;;
    oauth|sso)
        mkdir -p test-data/local/oauth
        "${compose[@]}" --profile oauth run --rm --no-deps --entrypoint python mas-synapse /bootstrap.py generate
        "${compose[@]}" --profile oauth run --rm --no-deps mas config generate --output /data/mas.generated.yaml
        "${compose[@]}" --profile oauth run --rm --no-deps --entrypoint python mas-synapse /bootstrap.py oauth-config
        trap '"${compose[@]}" --profile oauth --profile sso stop sso-synapse mas mas-synapse mas-db' EXIT
        "${compose[@]}" --profile oauth up -d --force-recreate --wait --wait-timeout 90 mas mas-synapse
        if [[ "$1" == sso ]]; then
            mkdir -p test-data/local/sso
            "${compose[@]}" --profile sso run --rm --no-deps --entrypoint python sso-synapse /bootstrap.py generate
            "${compose[@]}" --profile sso run --rm --no-deps --entrypoint python sso-synapse /bootstrap.py sso-config
            "${compose[@]}" --profile sso up -d --force-recreate --wait --wait-timeout 90 sso-synapse
            cargo test --locked -p twayblade-core --all-features sso_local_oidc_token_exchange_and_restore -- --ignored --nocapture
        else
            cargo test --locked -p twayblade-core --all-features oauth_cancel_and_wrong_state_discard_checkpoint -- --ignored --nocapture
            cargo test --locked -p twayblade-core --all-features oauth_keychain_lifecycle_across_processes -- --ignored --nocapture
        fi
        ;;
    up|test|keychain|services|security|media|threads|spaces|viewer|search|directory|notifications|notification-delivery|room-open)
        mkdir -p test-data/local/integration
        "${compose[@]}" run --rm --no-deps --entrypoint python synapse /bootstrap.py generate
        if [[ "${1:-test}" == spaces ]]; then
            "${compose[@]}" run --rm --no-deps --entrypoint python synapse /bootstrap.py spaces-config
        fi
        if [[ "${1:-test}" == directory ]]; then
            "${compose[@]}" run --rm --no-deps --entrypoint python synapse /bootstrap.py directory-config
        fi
        "${compose[@]}" up -d --force-recreate --wait --wait-timeout 90
        "${compose[@]}" exec -T synapse python /bootstrap.py register
        if [[ "${1:-test}" != up ]]; then
            # Keep encrypted fixture data for inspection; stop only our test service.
            trap '"${compose[@]}" stop' EXIT
            if [[ "${1:-test}" == room-open ]]; then
                "${compose[@]}" exec -T synapse python /bootstrap.py register-security
                cargo test --locked -p twayblade-core --all-features auth::room_open:: -- --ignored --nocapture --test-threads=1
            elif [[ "${1:-test}" == directory ]]; then
                cargo test --locked -p twayblade-core --all-features public_directory_pagination_filter_links_preview_and_knock -- --ignored --nocapture
            elif [[ "${1:-test}" == search ]]; then
                "${compose[@]}" exec -T synapse python /bootstrap.py register-security
                cargo test --locked -p twayblade-core --all-features search_server_cached_encryption_pagination_and_event_context -- --ignored --nocapture
            elif [[ "${1:-test}" == notification-delivery ]]; then
                cargo test --locked -p twayblade-core --all-features encrypted_notification_delivery_and_private_preview -- --ignored --nocapture
            elif [[ "${1:-test}" == notifications ]]; then
                cargo test --locked -p twayblade-core --all-features notification_rules_round_trip_and_second_client_changes -- --ignored --nocapture
            elif [[ "${1:-test}" == viewer ]]; then
                "${compose[@]}" exec -T synapse python /bootstrap.py register-security
                cargo test --locked -p twayblade-core --all-features image_viewer_plain_encrypted_animation_limits_and_corrupt -- --ignored --nocapture
            elif [[ "${1:-test}" == spaces ]]; then
                "${compose[@]}" exec -T synapse python /bootstrap.py register-pair
                cargo test --locked -p twayblade-core --all-features spaces_hierarchy_invites_creation_restricted_join_and_selective_leave -- --ignored --nocapture
            elif [[ "${1:-test}" == threads ]]; then
                cargo test --locked -p twayblade-core --all-features threads_send_paginate_receipts_and_closed_unread -- --ignored --nocapture
            elif [[ "${1:-test}" == media ]]; then
                "${compose[@]}" exec -T synapse python /bootstrap.py register-security
                cargo test --locked -p twayblade-core --all-features attachment_plain_encrypted_cancel_restart_round_trip -- --ignored --nocapture
            elif [[ "${1:-test}" == security ]]; then
                "${compose[@]}" exec -T synapse python /bootstrap.py register-security
                cargo test --locked -p twayblade-core --all-features secure_backup_fresh_store_recovery_and_sas -- --ignored --nocapture
            elif [[ "${1:-test}" == keychain ]]; then
                cargo test --locked --workspace --all-features keychain_session_lifecycle_across_processes -- --ignored --nocapture
            elif [[ "${1:-test}" == services ]]; then
                for test_name in encrypted_message_editing_sources_navigation_and_threads encrypted_delivery_profile_and_same_device_store_restore timeline_metadata_and_send_queue_actions members_typing_and_read_receipts pagination_preserves_message_identity room_list_service_tracks_recency_and_details; do
                    cargo test --locked -p twayblade-core --all-features "$test_name" -- --ignored --nocapture
                done
            else
                cargo test --locked -p twayblade-core encrypted_delivery_profile_and_same_device_store_restore -- --ignored --nocapture
            fi
        fi
        ;;
    down) "${compose[@]}" down ;;
    status) "${compose[@]}" ps ;;
    *) printf 'Usage: bash tests/integration/run.sh [up|test|keychain|services|presence|oauth|sso|recovery|security|media|threads|spaces|viewer|search|notifications|notification-delivery|room-open|down|status]\n' >&2; exit 1 ;;
esac
