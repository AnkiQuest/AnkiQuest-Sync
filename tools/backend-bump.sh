#!/usr/bin/env bash
# Logic for backend-bump.yml: finding the newest published backend version,
# waiting for Maven Central to serve it, and the pin edit in the app's
# gradle/libs.versions.toml.
#
# usage:
#   backend-bump.sh latest          newest version served for the artefacts
#   backend-bump.sh wait <version>  poll until that version's poms are served
#   backend-bump.sh bump <version> <toml>   edit ankiBackend in place
set -euo pipefail

GROUP=org.ankiquest
# New artefact names first; the names carried before the rename fall back,
# so versions published before it keep working.
ANDROID="ankiquest-backend-android anki-backend-android"
CORE="ankiquest-backend-core"
TESTING="ankiquest-android-backend-testing anki-android-backend-testing"

pom_url() { # <coordinate> <version> -> prints the pom's URL
  echo "https://repo1.maven.org/maven2/org/$GROUP/$1/$2/$1-$2.pom"
}

metadata() { # <coordinate> -> prints <latest>, empty when not published
  curl -sf "https://repo1.maven.org/maven2/org/$GROUP/$1/maven-metadata.xml" \
    | sed -n 's:.*<latest>\(.*\)</latest>.*:\1:p' || true
}

case "${1:-}" in
  latest)
    shift
    for coord in $ANDROID; do
      found=$(metadata "$coord")
      if [ -n "$found" ]; then
        echo "$found"
        exit 0
      fi
    done
    ;;
  wait)
    version=$2
    deadline=$(( $(date +%s) + 20 * 60 ))
    while true; do
      served=1
      for fam in "$ANDROID" "$CORE" "$TESTING"; do
        ok=0
        for coord in $fam; do
          if curl -sfI "$(pom_url "$coord" "$version")" > /dev/null; then
            ok=1
            break
          fi
        done
        [ "$ok" = 1 ] || served=0
      done
      if [ "$served" = 1 ]; then
        echo "Maven Central serves $version"
        exit 0
      fi
      if [ "$(date +%s)" -ge "$deadline" ]; then
        echo "::error::Maven Central does not serve $version after 20 minutes" >&2
        exit 1
      fi
      sleep 15
    done
    ;;
  bump)
    version=$2
    toml=$3
    if [ "$(grep -c "^ankiBackend = '" "$toml")" != 1 ]; then
      echo "no single ankiBackend line in $toml" >&2
      exit 1
    fi
    sed -i "s/^ankiBackend = '[^']*'/ankiBackend = '$version'/" "$toml"
    ;;
  *)
    echo "usage: backend-bump.sh latest | wait <version> | bump <version> <toml>" >&2
    exit 2
    ;;
esac
