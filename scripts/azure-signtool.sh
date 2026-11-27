# shellcheck shell=bash
# Sourced by the release scripts that sign Windows files, in Git Bash on a Windows runner.
#
#   azure_signtool <work dir>
#
# Sets the array `sign` to the SignTool command line that signs a file (append its Windows path)
# with Azure Artifact Signing, from AZURE_SIGNING_ENDPOINT, AZURE_SIGNING_ACCOUNT and
# AZURE_SIGNING_PROFILE and the Azure CLI login of azure/login. Without AZURE_SIGNING_ENDPOINT it
# warns and leaves `sign` empty. Run the command with MSYS2_ARG_CONV_EXCL="*": SignTool takes /fd,
# /tr and the like literally, and Git Bash must not turn them into paths.
azure_signtool() {
  local work=$1
  sign=()
  if [[ -z "${AZURE_SIGNING_ENDPOINT:-}" ]]; then
    echo "::warning::no Azure Artifact Signing account: the Windows files are not signed"
    return
  fi
  # Microsoft's signtool plug-in for Azure Artifact Signing, pinned by version and digest.
  local dlib_version=1.0.151
  local dlib_sha256=35375a4bc4e1102da729fc3d8d9925f774c3baae6b8b7ff6398d948ca9ab0d8a
  curl --fail --silent --show-error --location --retry 3 --output "$work/dlib.nupkg" \
    "https://api.nuget.org/v3-flatcontainer/microsoft.artifactsigning.client/$dlib_version/microsoft.artifactsigning.client.$dlib_version.nupkg"
  echo "$dlib_sha256  $work/dlib.nupkg" | sha256sum --check --status
  unzip -q "$work/dlib.nupkg" -d "$work/dlib"
  jq -n --arg endpoint "$AZURE_SIGNING_ENDPOINT" --arg account "$AZURE_SIGNING_ACCOUNT" \
    --arg profile "$AZURE_SIGNING_PROFILE" \
    '{Endpoint: $endpoint, CodeSigningAccountName: $account, CertificateProfileName: $profile}' \
    >"$work/metadata.json"
  local signtool
  signtool=$(find "/c/Program Files (x86)/Windows Kits/10/bin" -path '*/x64/signtool.exe' | sort -V | tail -n 1)
  # shellcheck disable=SC2034 # the caller runs it
  sign=("$(cygpath -w "$signtool")" sign /v /fd SHA256 /tr http://timestamp.acs.microsoft.com /td SHA256
    /dlib "$(cygpath -w "$work/dlib/bin/x64/Azure.CodeSigning.Dlib.dll")"
    /dmdf "$(cygpath -w "$work/metadata.json")")
}
