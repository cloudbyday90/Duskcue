#!/usr/bin/env bash
# Duskcue — Self-hosted media streaming server
# Copyright (C) 2026-2026 Duskcue Contributors
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program. If not, see <https://www.gnu.org/licenses/>.

set -Eeuo pipefail

if [[ "${1:-start}" != "start" ]]; then
    exec "$@"
fi

PUID="${PUID:-1000}"
PGID="${PGID:-1000}"
DATA_DIR="${DUSKCUE_DATA_DIR:-/data}"
CACHE_DIR="${DUSKCUE_CACHE_DIR:-/cache}"
PGDATA="${DUSKCUE_EMBEDDED_PGDATA:-$DATA_DIR/postgres}"
PG_RUN="${DUSKCUE_EMBEDDED_PG_RUN:-/var/run/postgresql}"
PG_LOG_DIR="$DATA_DIR/logs"
PG_LOG="$PG_LOG_DIR/postgres.log"
PUBLIC_BIND="${DUSKCUE_BIND_ADDRESS:-0.0.0.0}"
PUBLIC_PORT="${DUSKCUE_PORT:-48027}"
API_BIND="${DUSKCUE_INTERNAL_BIND_ADDRESS:-127.0.0.1}"
API_PORT="${DUSKCUE_INTERNAL_API_PORT:-48028}"
API_URL="${DUSKCUE_INTERNAL_API_URL:-http://127.0.0.1:$API_PORT}"
WEB_ROOT="${DUSKCUE_WEB_ROOT:-/opt/duskcue/web}"
POSTGRES_STARTED=0
API_PID=""
WEB_PID=""

embedded_pg_enabled() {
    [[ -z "${DUSKCUE_DATABASE_URL:-}" ]]
}

run_as_user() {
    if [[ "$(id -u)" == "0" ]]; then
        su-exec "$PUID:$PGID" "$@"
    else
        "$@"
    fi
}

prepare_nss_wrapper() {
    if [[ "$(id -u)" != "0" || ! -f /usr/lib/libnss_wrapper.so ]]; then
        return
    fi

    cat > /tmp/passwd <<EOF
duskcue:x:$PUID:$PGID:Duskcue:$DATA_DIR:/sbin/nologin
EOF
    cat > /tmp/group <<EOF
duskcue:x:$PGID:
EOF
    export NSS_WRAPPER_PASSWD=/tmp/passwd
    export NSS_WRAPPER_GROUP=/tmp/group
    export LD_PRELOAD=/usr/lib/libnss_wrapper.so
}

prepare_directories() {
    mkdir -p "$DATA_DIR" "$CACHE_DIR"
    if embedded_pg_enabled; then
        mkdir -p "$PG_RUN"
    fi

    if [[ "$(id -u)" == "0" ]]; then
        chown "$PUID:$PGID" "$DATA_DIR" "$CACHE_DIR"
        if embedded_pg_enabled; then
            chown "$PUID:$PGID" "$PG_RUN" 2>/dev/null || true
        fi
    fi

    run_as_user mkdir -p \
        "$DATA_DIR/config" \
        "$DATA_DIR/metadata/artwork" \
        "$DATA_DIR/metadata/thumbnails" \
        "$DATA_DIR/transcode" \
        "$DATA_DIR/backups" \
        "$PG_LOG_DIR" \
        "$CACHE_DIR/hls" \
        "$CACHE_DIR/images" \
        "$CACHE_DIR/storyboards" \
        "$CACHE_DIR/search"

    if embedded_pg_enabled; then
        run_as_user mkdir -p "$PGDATA" "$PG_RUN"
    fi

    if [[ "$(id -u)" == "0" ]]; then
        chown -R "$PUID:$PGID" "$DATA_DIR" "$CACHE_DIR" 2>/dev/null || true
        if embedded_pg_enabled; then
            chown -R "$PUID:$PGID" "$PG_RUN" 2>/dev/null || true
        fi
    fi

    if embedded_pg_enabled; then
        run_as_user chmod 700 "$PGDATA"
        run_as_user chmod 770 "$PG_RUN"
    fi
}

init_postgres() {
    if run_as_user test -f "$PGDATA/PG_VERSION"; then
        return
    fi

    echo "Initializing embedded PostgreSQL at $PGDATA"
    run_as_user initdb -D "$PGDATA" --auth=trust --encoding=UTF8 --data-checksums --username=duskcue
    run_as_user tee -a "$PGDATA/postgresql.conf" >/dev/null <<EOF
listen_addresses = ''
unix_socket_directories = '$PG_RUN'
logging_collector = on
log_destination = 'stderr'
log_directory = '$PG_LOG_DIR'
log_filename = 'postgres.log'
log_rotation_age = 1d
log_rotation_size = 100MB
shared_buffers = 128MB
EOF
}

remove_stale_postmaster_pid() {
    if ! run_as_user test -f "$PGDATA/postmaster.pid"; then
        return
    fi

    if run_as_user pg_ctl -D "$PGDATA" status >/dev/null 2>&1; then
        return
    fi

    echo "Removing stale PostgreSQL postmaster.pid"
    run_as_user rm -f "$PGDATA/postmaster.pid"
}

start_postgres() {
    if ! embedded_pg_enabled; then
        echo "Using external PostgreSQL"
        return
    fi

    init_postgres
    remove_stale_postmaster_pid

    echo "Starting embedded PostgreSQL"
    run_as_user pg_ctl -D "$PGDATA" -l "$PG_LOG" -w start
    POSTGRES_STARTED=1

    echo "Waiting for embedded PostgreSQL"
    for _ in $(seq 1 60); do
        if run_as_user pg_isready -q -h "$PG_RUN" -U duskcue; then
            break
        fi
        sleep 1
    done

    if ! run_as_user pg_isready -q -h "$PG_RUN" -U duskcue; then
        echo "Embedded PostgreSQL did not become ready" >&2
        tail -n 80 "$PG_LOG" >&2 || true
        exit 1
    fi

    run_as_user createdb -h "$PG_RUN" -U duskcue duskcue >/dev/null 2>&1 || true
    export DUSKCUE_DATABASE_URL="postgresql://duskcue@localhost/duskcue?host=$PG_RUN"
    echo "Embedded PostgreSQL ready"
}

wait_for_api_ready() {
    echo "Waiting for Duskcue API readiness"
    for _ in $(seq 1 120); do
        if curl --fail --silent --max-time 2 "$API_URL/health/ready" >/dev/null; then
            return
        fi
        if ! kill -0 "$API_PID" >/dev/null 2>&1; then
            echo "Duskcue API exited before becoming ready" >&2
            wait "$API_PID" || true
            exit 1
        fi
        sleep 1
    done

    echo "Duskcue API did not become ready" >&2
    exit 1
}

start_api() {
    run_as_user rm -f "$DATA_DIR/.duskcue.lock"
    echo "Starting Duskcue API on $API_BIND:$API_PORT"
    run_as_user env \
        DUSKCUE_BIND_ADDRESS="$API_BIND" \
        DUSKCUE_PORT="$API_PORT" \
        /usr/local/bin/duskcue &
    API_PID="$!"
    wait_for_api_ready
}

start_web() {
    echo "Starting Duskcue web surface on $PUBLIC_BIND:$PUBLIC_PORT"
    local web_env=(
        HOST="$PUBLIC_BIND"
        PORT="$PUBLIC_PORT"
        DUSKCUE_INTERNAL_API_URL="$API_URL"
    )
    if [[ -n "${DUSKCUE_PUBLIC_URL:-}" ]]; then
        web_env+=(ORIGIN="$DUSKCUE_PUBLIC_URL")
    fi
    run_as_user env "${web_env[@]}" node "$WEB_ROOT/index.js" &
    WEB_PID="$!"
}

stop_process() {
    local pid="$1"
    local name="$2"

    if [[ -z "$pid" ]] || ! kill -0 "$pid" >/dev/null 2>&1; then
        return
    fi

    echo "Stopping $name"
    kill -TERM "$pid" >/dev/null 2>&1 || true

    for _ in $(seq 1 30); do
        if ! kill -0 "$pid" >/dev/null 2>&1; then
            return
        fi
        sleep 1
    done

    echo "Force stopping $name"
    kill -KILL "$pid" >/dev/null 2>&1 || true
}

stop_postgres() {
    if [[ "$POSTGRES_STARTED" != "1" ]]; then
        return
    fi

    echo "Stopping embedded PostgreSQL"
    run_as_user pg_ctl -D "$PGDATA" -m fast -w stop >/dev/null 2>&1 || true
}

shutdown() {
    trap - TERM INT
    stop_process "$WEB_PID" "Duskcue web"
    stop_process "$API_PID" "Duskcue API"
    stop_postgres
}

trap 'shutdown; exit 143' TERM INT

prepare_nss_wrapper
prepare_directories
start_postgres
start_api
start_web

set +e
wait -n "$API_PID" "$WEB_PID"
EXIT_CODE="$?"
set -e

shutdown
exit "$EXIT_CODE"
