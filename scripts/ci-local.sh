#!/usr/bin/env bash
# Runs .github/workflows/ci.yml locally with act, one job at a time per act process, and prints
# one line per job. Run on a Linux machine with Docker and act, inside `nix develop`.
#
#   scripts/ci-local.sh                 # every job
#   scripts/ci-local.sh clippy ui       # some jobs (matrix names of `check`, `kubernetes` or
#                                       # `kubernetes-l2`)
#
# Why not a plain `act push`:
# - act reports every entry of a matrix as failed once one of them fails, even when all its steps
#   succeeded. Each job here runs in its own act process, so each result is its own.
# - Parallel act processes that share one action cache race while cloning actions, so each job
#   has its own action and cache directories under ~/.local/state/engawa-ci-local.
# - Every job installs Nix and the dev shell from the internet, and a stalled download can hang a
#   job for good. A Nix binary cache serving this machine's /nix/store (harmonia) is offered to
#   the jobs as an extra substituter, and a job whose log stops growing for STALL_MINUTES is
#   stopped and run once more.
# - A full disk fails jobs with no error of their own: a log cut off mid-line, a k3s node that
#   stops scheduling Pods. Jobs build in their containers (several GB each at once), act's cache
#   server keeps every saved cache entry, and a failed job leaves its volumes. So each job's cache
#   is dropped once it grows past CI_LOCAL_CACHE_MB, a job's volumes go with its container, and
#   the run refuses to start with less than CI_LOCAL_MIN_FREE_GB free. The cluster jobs
#   (kubernetes, kubernetes-l2), the largest, run last, one after the other, and alone unless
#   Docker has CI_LOCAL_SHARE_FREE_GB free.
# - kubernetes-l2 builds Genkan from a local checkout, GENKAN_SRC (default ../genkan), which is
#   mounted read-only into its job container (scripts/e2e-l2.sh picks the pinned commit).
#
# Settings (environment): CI_LOCAL_PARALLEL (default 3), STALL_MINUTES (default 15),
# CI_LOCAL_NO_CACHE=1 (no local substituter), CI_LOCAL_DIR (default target/ci-local),
# CI_LOCAL_CACHE_MB (default 1500), CI_LOCAL_MIN_FREE_GB (default 20),
# CI_LOCAL_SHARE_FREE_GB (default 50).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
workflow="$root/.github/workflows/ci.yml"
out="${CI_LOCAL_DIR:-$root/target/ci-local}"
parallel="${CI_LOCAL_PARALLEL:-3}"
stall=$((${STALL_MINUTES:-15} * 60))
cache_mb="${CI_LOCAL_CACHE_MB:-1500}"
min_free_gb="${CI_LOCAL_MIN_FREE_GB:-20}"
share_free_gb="${CI_LOCAL_SHARE_FREE_GB:-50}"
state="${XDG_STATE_HOME:-$HOME/.local/state}/engawa-ci-local"
mkdir -p "$out" "$state"

for tool in act docker nix; do
  command -v "$tool" >/dev/null || { echo "error: $tool is not on PATH" >&2; exit 1; }
done

# The matrix names of the `check` job, in file order, and the other jobs that run on push.
matrix_names() {
  awk '/^  check:/{in_check=1} /^  [a-z]/ && !/^  check:/{in_check=0}
       in_check && /^          - name: /{sub(/^          - name: /, ""); print}' "$workflow"
}
clusters=(-e kubernetes -e kubernetes-l2)
mapfile -t all < <(matrix_names; echo kubernetes; echo kubernetes-l2)
if (($# > 0)); then targets=("$@"); else targets=("${all[@]}"); fi
for t in "${targets[@]}"; do
  printf '%s\n' "${all[@]}" | grep -qx -- "$t" || { echo "error: no job named $t" >&2; exit 1; }
done
# The cluster jobs run last, each once (see the run loop below).
mapfile -t targets < <(printf '%s\n' "${targets[@]}" | grep -vx "${clusters[@]}" || true
  printf '%s\n' "${targets[@]}" | grep -x "${clusters[@]}" | awk '!seen[$0]++' || true)
genkan_src=""
if printf '%s\n' "${targets[@]}" | grep -qx kubernetes-l2; then
  genkan_src="$(cd "${GENKAN_SRC:-$root/../genkan}" && pwd)" || {
    echo "error: kubernetes-l2 needs a Genkan checkout (GENKAN_SRC, default ../genkan)" >&2
    exit 1
  }
fi

# --- Disk -----------------------------------------------------------------------------------
for dir in "$state"/actcache/*/; do
  [[ -d "$dir" ]] || continue
  if (($(du -sm "$dir" | cut -f1) > cache_mb)); then rm -rf "$dir"; fi
done
docker_root=$(docker info --format '{{.DockerRootDir}}')
for dir in "$out" "$state" "$docker_root"; do
  free_gb=$(($(df --output=avail -k "$dir" | tail -1) / 1024 / 1024))
  if ((free_gb < min_free_gb)); then
    echo "error: $free_gb GB free under $dir, need $min_free_gb (CI_LOCAL_MIN_FREE_GB)" >&2
    exit 1
  fi
done
docker_free_gb=$(($(df --output=avail -k "$docker_root" | tail -1) / 1024 / 1024))

# --- Local Nix substituter -------------------------------------------------------------------
cache_args=()
if [[ -z "${CI_LOCAL_NO_CACHE:-}" ]]; then
  key="$state/cache-key.sec"
  if [[ ! -s "$key" ]]; then
    nix key generate-secret --key-name engawa-ci-local-1 >"$key"
    chmod 0600 "$key"
  fi
  public=$(nix key convert-secret-to-public <"$key")
  port=5555
  if ! curl -sf "http://127.0.0.1:$port/nix-cache-info" >/dev/null; then
    printf 'bind = "127.0.0.1:%s"\nsign_key_paths = ["%s"]\n' "$port" "$key" >"$state/harmonia.toml"
    harmonia=$(nix build --no-link --print-out-paths --inputs-from "$root" nixpkgs#harmonia)
    CONFIG_FILE="$state/harmonia.toml" setsid nohup "$harmonia/bin/harmonia-cache" \
      >"$state/harmonia.log" 2>&1 </dev/null &
    for _ in $(seq 1 50); do
      curl -sf "http://127.0.0.1:$port/nix-cache-info" >/dev/null && break
      sleep 0.2
    done
    if ! curl -sf "http://127.0.0.1:$port/nix-cache-info" >/dev/null; then
      echo "error: the local Nix cache did not start; see $state/harmonia.log" >&2
      exit 1
    fi
  fi
  # act runs job containers on the host network, so 127.0.0.1 is this machine.
  cache_args=(--var "NIX_LOCAL_SUBSTITUTER=http://127.0.0.1:$port"
    --var "NIX_LOCAL_PUBLIC_KEY=$public")
fi

# --- One job -------------------------------------------------------------------------------
# Runs `act` for one job with a watchdog; prints PASS, FAIL or STALLED and returns 0 on PASS.
run_once() { # $1 = job name, $2 = log file, $3 = artifact port
  local name="$1" log="$2" port="$3" args
  case "$name" in
    kubernetes)
      args=(-j kubernetes)
      k3d cluster delete engawa >/dev/null 2>&1 || true
      ;;
    kubernetes-l2)
      args=(-j kubernetes-l2 --var GENKAN_SRC=/genkan
        --container-options "--volume $genkan_src:/genkan:ro")
      k3d cluster delete engawa-l2 >/dev/null 2>&1 || true
      ;;
    *) args=(-j check --matrix "name:$name") ;;
  esac
  setsid act push -W "$workflow" "${args[@]}" "${cache_args[@]}" \
    --artifact-server-port "$port" --cache-server-path "$state/actcache/$name" \
    --action-cache-path "$state/actions/$name" --action-offline-mode \
    >"$log" 2>&1 </dev/null &
  local pid=$! size=-1 quiet=0
  while kill -0 "$pid" 2>/dev/null; do
    sleep 10
    local now
    now=$(stat -c %s "$log")
    if ((now == size)); then quiet=$((quiet + 10)); else quiet=0 size=$now; fi
    if ((quiet >= stall)); then
      kill -- "-$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
      docker ps -aq --filter "name=act-CI-$name" | xargs -r docker rm -f >/dev/null
      docker volume ls -q --filter "name=act-CI-$name" | xargs -r docker volume rm >/dev/null
      echo STALLED
      return 1
    fi
  done
  local result=FAIL
  if wait "$pid"; then result=PASS; fi
  # act leaves a job's container running after it ends, and a failed job's volumes too; a dozen
  # runs fill the disk.
  docker ps -aq --filter "name=act-CI-$name" | xargs -r docker rm -f >/dev/null
  docker volume ls -q --filter "name=act-CI-$name" | xargs -r docker volume rm >/dev/null
  case "$name" in
    kubernetes) k3d cluster delete engawa >/dev/null 2>&1 || true ;;
    kubernetes-l2) k3d cluster delete engawa-l2 >/dev/null 2>&1 || true ;;
  esac
  echo "$result"
  [[ "$result" == PASS ]]
}

run_job() { # $1 = job name, $2 = index
  local name="$1" index="$2" started result tries=0
  started=$(date +%s)
  while :; do
    tries=$((tries + 1))
    result=$(run_once "$name" "$out/$name.log" $((34600 + index))) || true
    [[ "$result" == STALLED && $tries -lt 2 ]] || break
    mv "$out/$name.log" "$out/$name.stalled.log"
  done
  printf '%-14s %-8s %5ss  tries=%s  %s\n' "$name" "$result" $(($(date +%s) - started)) \
    "$tries" "$out/$name.log" >"$out/$name.result"
}

running() { # how many of this run's jobs are still running
  local count=0 pid
  for pid in "${pids[@]}"; do kill -0 "$pid" 2>/dev/null && count=$((count + 1)); done
  echo "$count"
}

echo "running ${#targets[@]} job(s), $parallel at a time; logs in $out"
# Wait only for the jobs: the cache server started above keeps running for the next run.
index=0
pids=()
cluster_pid=""
for name in "${targets[@]}"; do
  while (($(running) >= parallel)); do sleep 2; done
  if printf '%s\n' "$name" | grep -qx "${clusters[@]}"; then
    # One cluster job at a time: each builds several images and runs a k3s node, and the
    # container clean-up above matches `act-CI-kubernetes*` names.
    if [[ -n "$cluster_pid" ]]; then wait "$cluster_pid" || true; fi
    # The kubernetes job builds twice and runs a k3s node; next to the ui and no-ui builds it
    # used up to 21 GB more and ran a 62 GB disk out of space. With less than
    # CI_LOCAL_SHARE_FREE_GB free for Docker it waits until it can run alone.
    if ((docker_free_gb < share_free_gb)); then
      while (($(running) > 0)); do sleep 2; done
    fi
  fi
  run_job "$name" "$index" &
  pids+=($!)
  if printf '%s\n' "$name" | grep -qx "${clusters[@]}"; then cluster_pid=$!; fi
  index=$((index + 1))
done
for pid in "${pids[@]}"; do wait "$pid" || true; done

status=0
for name in "${targets[@]}"; do
  cat "$out/$name.result"
  grep -q ' PASS ' "$out/$name.result" || status=1
done
exit $status
