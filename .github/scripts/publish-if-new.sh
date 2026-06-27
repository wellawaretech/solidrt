#!/usr/bin/env bash
# Publish a package with `bun publish`, unless that name@version is already on
# the npm registry. This makes the release job idempotent: if a run fails partway
# through (e.g. one package errors), re-running it skips the already-published
# packages instead of 403-ing on "cannot publish over existing version".
#
# Reads VERSION, TAG_FLAG and DRY_RUN from the environment.
#   $1  package directory to publish (e.g. packages/core)
set -eu

dir="$1"
cd "$dir"

name=$(node -p "require('./package.json').name")
# Scope slash must be percent-encoded for the registry path.
encoded=$(printf '%s' "$name" | sed 's#/#%2f#')
status=$(curl -s -o /dev/null -w '%{http_code}' "https://registry.npmjs.org/${encoded}/${VERSION}")
if [ "$status" = "200" ]; then
  echo "::notice::${name}@${VERSION} already published; skipping"
  exit 0
fi

dry=""
if [ "${DRY_RUN}" = "true" ]; then dry="--dry-run"; fi
echo ">> publishing ${name}@${VERSION}"
bun publish --access public ${TAG_FLAG} $dry