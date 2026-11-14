#!/usr/bin/env bash
# What the runner this runs on has, in the run's summary: the CPU and its architecture, memory,
# disk, whether Docker runs containers, whether /dev/kvm is there, the iscsi_tcp module (the
# catalog's Longhorn e2e loads it) and whether a k3d cluster starts, which the cluster, e2e and
# kubernetes jobs need. Every check reports and goes on; the script fails only when it cannot run.
set -euo pipefail
check() {
  local out
  if out=$("$@" 2>&1); then
    echo "yes: ${out//$'\n'/ }" | cut -c1-120
  else
    echo "no: ${out//$'\n'/ }" | cut -c1-120
  fi
}
cluster() {
  k3d cluster create runner-facts --no-lb --wait --timeout 180s >/dev/null &&
    kubectl --context k3d-runner-facts get nodes --no-headers
}
trap 'k3d cluster delete runner-facts >/dev/null 2>&1 || true' EXIT
{
  echo "### ${RUNNER_LABEL:-this runner}"
  echo "| Fact | Value |"
  echo "|---|---|"
  echo "| Architecture | $(uname -m) |"
  echo "| CPU | $(nproc) × $(lscpu | sed -n 's/^Model name: *//p' | head -n1) |"
  echo "| Memory | $(free -g | awk '/^Mem:/ {print $2}') GiB |"
  echo "| Disk free on / | $(df -h --output=avail / | tail -n1 | tr -d ' ') |"
  echo "| OS | $(sed -n "s/^PRETTY_NAME=//p" /etc/os-release | tr -d '"'), kernel $(uname -r) |"
  echo "| Docker runs a container | $(check docker run --rm busybox:1.37 true) |"
  echo "| /dev/kvm | $(check test -c /dev/kvm) |"
  echo "| iscsi_tcp loads | $(check sudo modprobe iscsi_tcp) |"
  echo "| k3d cluster starts | $(check cluster) |"
} | tee -a "${GITHUB_STEP_SUMMARY:-/dev/null}"
