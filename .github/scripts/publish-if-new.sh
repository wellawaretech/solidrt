#!/usr/bin/env bash
# Publish a package with `bun publish`, unless that name@version is already on
# the npm registry. This makes the release job idempotent: if a run fails partway
# through (e.g. one package errors), re-running it skips the already-published
# packages instead of 403-ing on "cannot publish over existing version".
#
# A failed publish is retried a few times, and the registry is asked again
# before each retry: the registry has answered a publish with a transient
# 404 (0.0.62), and a publish that errored after the upload landed must not
# be repeated.
#
# Reads VERSION, TAG_FLAG and DRY_RUN from the environment.
#   $1  package directory to publish (e.g. packages/core)
set -eu

# Publish attempts before giving up, and the pause between them.
ATTEMPTS=3
RETRY_DELAY_S=20

dir="$1"
cd "$dir"

name=$(node -p "require('./package.json').name")
# Scope slash must be percent-encoded for the registry path.
encoded=$(printf '%s' "$name" | sed 's#/#%2f#')

published() {
  local status
  status=$(curl -s -o /dev/null -w '%{http_code}' "https://registry.npmjs.org/${encoded}/${VERSION}")
  [ "$status" = "200" ]
}

dry=""
if [ "${DRY_RUN}" = "true" ]; then dry="--dry-run"; fi
for i in $(seq 1 "$ATTEMPTS"); do
  if published; then
    echo "::notice::${name}@${VERSION} already published; skipping"
    exit 0
  fi
  echo ">> publishing ${name}@${VERSION} (attempt $i)"
  bun publish --access public ${TAG_FLAG} $dry && exit 0
  if [ "$i" -lt "$ATTEMPTS" ]; then
    echo "publish failed, retrying in ${RETRY_DELAY_S}s"
    sleep "$RETRY_DELAY_S"
  fi
done
echo "::error::${name}@${VERSION} did not publish after ${ATTEMPTS} attempts"
exit 1