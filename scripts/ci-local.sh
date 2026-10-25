#!/usr/bin/env bash
# Runs CI's jobs locally, each with scripts/ci.sh as GitHub runs it, and prints one line per job.
# Run on a Linux machine with Nix and Docker (Docker only for the cluster jobs), not on a
# workstation you care about: the cluster jobs create k3d clusters, images and volumes.
#
#   scripts/ci-local.sh                 # every job
#   scripts/ci-local.sh clippy ui       # some jobs (scripts/ci.sh --list)
#
# As on GitHub, each job starts from its own copy of this checkout (uncommitted changes included,
# build output and node_modules left out), with a build directory of its own, so jobs neither see
# nor disturb each other's output. CI_LOCAL_PARALLEL jobs (default 3) run at once; the cluster
# jobs run one after the other, after the rest, since each runs a k3s node and builds images.
# Logs and results are in target/ci-local (CI_LOCAL_DIR): <job>.log and <job>.result; a job's
# copy is deleted when it passes and kept under work/<job> when it fails.
#
# Unlike GitHub's fresh runners, the jobs still run in this machine's environment, so what an
# earlier run left (clusters, images, tools installed by hand) can make a job pass here and fail
# there: on a VM, snapshot its clean state and restore it after every run.
#
# A full disk fails jobs with no error of their own (a log cut off mid-line, a k3s node that
# stops scheduling pods), so the run refuses to start with less than CI_LOCAL_MIN_FREE_GB
# (default 25) free; each running job's build directory takes several GB. The cluster jobs fetch
# Zeabur Identity themselves (scripts/dev-cluster.sh), or build ZEABUR_IDENTITY_SRC when it is set.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="${CI_LOCAL_DIR:-$root/target/ci-local}"
parallel="${CI_LOCAL_PARALLEL:-3}"
min_free_gb="${CI_LOCAL_MIN_FREE_GB:-25}"
mkdir -p "$out"

mapfile -t all < <("$root/scripts/ci.sh" --list)
if (($# > 0)); then targets=("$@"); else targets=("${all[@]}"); fi
for t in "${targets[@]}"; do
  printf '%s\n' "${all[@]}" | grep -qx -- "$t" || { echo "error: no job named $t" >&2; exit 1; }
done
# In the order of the list, which puts the cluster jobs last.
mapfile -t targets < <(printf '%s\n' "${all[@]}" | grep -xF -f <(printf '%s\n' "${targets[@]}"))
is_cluster() { [[ "$1" == kubernetes || "$1" == kubernetes-l2 ]]; }

dirs=("$out")
for t in "${targets[@]}"; do
  if is_cluster "$t"; then
    command -v docker >/dev/null || { echo "error: the cluster jobs need Docker" >&2; exit 1; }
    dirs+=("$(docker info --format '{{.DockerRootDir}}')")
    break
  fi
done
for dir in "${dirs[@]}"; do
  free_gb=$(($(df --output=avail -k "$dir" | tail -1) / 1024 / 1024))
  if ((free_gb < min_free_gb)); then
    echo "error: $free_gb GB free under $dir, need $min_free_gb (CI_LOCAL_MIN_FREE_GB)" >&2
    exit 1
  fi
done

# A copy is relative to nothing next to it, so Zeabur Identity's checkout is named outright.
if [[ -n "${ZEABUR_IDENTITY_SRC:-}" ]]; then
  ZEABUR_IDENTITY_SRC="$(cd "$ZEABUR_IDENTITY_SRC" && pwd)"
  export ZEABUR_IDENTITY_SRC
fi

run_job() { # $1 = job name
  local name="$1" work="$out/work/$1" started result=PASS
  started=$(date +%s)
  rm -rf "$work"
  mkdir -p "$work"
  rsync -a --exclude /target --exclude node_modules --exclude /web/dist --exclude /.direnv \
    "$root/" "$work/"
  "$work/scripts/ci.sh" "$name" >"$out/$name.log" 2>&1 </dev/null || result=FAIL
  [[ "$result" == PASS ]] && rm -rf "$work"
  printf '%-14s %-5s %5ss  %s\n' "$name" "$result" $(($(date +%s) - started)) "$out/$name.log" \
    >"$out/$name.result"
  cat "$out/$name.result"
}

echo "running ${#targets[@]} job(s), $parallel at a time; logs in $out"
rm -f "$out"/*.result
running=0
for name in "${targets[@]}"; do
  if is_cluster "$name"; then
    # The cluster jobs wait for everything before them, then run alone, one after the other.
    wait
    running=0
    run_job "$name"
    continue
  fi
  if ((running >= parallel)); then
    wait -n || true
    running=$((running - 1))
  fi
  run_job "$name" &
  running=$((running + 1))
done
wait

status=0
for name in "${targets[@]}"; do
  grep -q ' PASS ' "$out/$name.result" || status=1
done
exit $status
