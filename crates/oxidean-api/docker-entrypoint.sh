#!/bin/sh
# oxidean-api container entrypoint.
#
# Runtime CLI feed fetch (D-CLI-DIST): when OXIDEAN_CLI_DIST_URL is set, fetch
# the release feed's full ox target matrix at boot — the same curl +
# sha256sum -c + retry contract the Dockerfile's build-arg path uses — into a
# per-URL cache dir, then serve it via OXIDEAN_CLI_DIST_DIR. Cloud deploy
# tooling pins the URL per environment (scripts/railway-*-deploy*.sh); self-
# host builds keep the build-arg fetch (baked into /usr/local/share/oxidean-cli)
# and leave this var unset — a no-op entrypoint.
#
# Cache: /var/cli-dist/<sha256(url)[0:16]> — on Oxidean Cloud /var is the
# forge-data volume, so redeploys and restarts skip the refetch when the URL
# is unchanged; elsewhere it degrades to the container layer (correct either
# way). The dir is populated by an atomic rename so concurrent replicas never
# observe a partially fetched feed.
#
# Fail-closed parity with the build-arg path: unreachable/incomplete feed or a
# checksum mismatch exits before the api starts — the container never reports
# healthy and the platform keeps the previous replica (same shape as failed
# auto-migrations). latest.json stays best-effort (prerelease feeds may not
# carry one; /cli/latest tolerates its absence).

set -eu

# Only fetch ahead of the api binary — `docker run … sh` etc. skips it.
if [ "$#" -eq 0 ]; then
  exit 0
fi
if [ "$1" != "oxidean-api" ] && [ "$1" != "/usr/local/bin/oxidean-api" ]; then
  exec "$@"
fi

if [ -z "${OXIDEAN_CLI_DIST_URL:-}" ]; then
  exec "$@"
fi

url="$OXIDEAN_CLI_DIST_URL"
key=$(printf '%s' "$url" | sha256sum | cut -c1-16)

# /var is the forge-data volume on Cloud; fall back to the container layer.
if [ -d /var ] && [ -w /var ]; then
  root=/var/cli-dist
else
  root=/tmp/oxidean-cli-dist
fi
dir="$root/$key"

if [ ! -f "$dir/.complete" ]; then
  part="$dir.part.$$"
  trap 'rm -rf "$part"' EXIT INT TERM
  mkdir -p "$part"

  echo "cli-dist: fetching ox matrix from $url"
  ok=0
  i=1
  while [ "$i" -le 20 ]; do
    if curl -fsSL "$url/ox-targets.txt" -o "$part/ox-targets.txt"; then
      ok=1
      break
    fi
    echo "cli-dist: ox-targets.txt not ready (attempt $i/20); sleeping 15s"
    i=$((i + 1))
    sleep 15
  done
  if [ "$ok" != 1 ]; then
    echo "cli-dist: ERROR: could not fetch ox-targets.txt from $url" >&2
    exit 1
  fi

  while read -r t || [ -n "$t" ]; do
    [ -z "$t" ] && continue
    if curl -fsSL "$url/ox-$t" -o "$part/ox-$t" \
      && curl -fsSL "$url/ox-$t.sha256" -o "$part/ox-$t.sha256" \
      && (cd "$part" && sha256sum -c "ox-$t.sha256"); then
      :
    else
      echo "cli-dist: ERROR: failed to stage ox-$t from $url" >&2
      exit 1
    fi
  done < "$part/ox-targets.txt"

  if ! curl -fsSL "$url/SOURCE_COMMIT" -o "$part/SOURCE_COMMIT"; then
    echo "cli-dist: ERROR: SOURCE_COMMIT missing at $url" >&2
    exit 1
  fi
  if ! curl -fsSL "$url/latest.json" -o "$part/latest.json"; then
    echo "cli-dist: no latest.json at $url — /cli/latest reports unpublished"
    rm -f "$part/latest.json"
  fi

  touch "$part/.complete"
  # Atomic publish: a same-root rename. If a concurrent replica already
  # published, discard ours and use theirs (identical content — same URL).
  mv -T "$part" "$dir" 2>/dev/null || rm -rf "$part"
  trap - EXIT INT TERM
fi

export OXIDEAN_CLI_DIST_DIR="$dir"
echo "cli-dist: serving feed from $dir (url $url)"
ls -lh "$dir" || true

exec "$@"
