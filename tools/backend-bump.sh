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
# The repository path of the group: org/ankiquest
GROUP_PATH=${GROUP//.//}
# New artefact names first; the names carried before the rename fall back,
# so versions published before it keep working.
ANDROID="ankiquest-backend-android anki-backend-android"
CORE="ankiquest-backend-core anki-backend-core"
TESTING="ankiquest-android-backend-testing anki-android-backend-testing"
# How long `wait` polls; the check runs it with 0
WAIT_MINUTES=${WAIT_MINUTES:-20}

pom_url() { # <coordinate> <version> -> prints the pom's URL
  echo "https://repo1.maven.org/maven2/$GROUP_PATH/$1/$2/$1-$2.pom"
}

versions() { # <coordinate> -> prints every version its metadata lists
  curl -sf "https://repo1.maven.org/maven2/$GROUP_PATH/$1/maven-metadata.xml" \
    | sed -n 's:.*<version>\(.*\)</version>.*:\1:p' || true
}

served() { # <version> -> succeeds when all three artefacts' poms are served
  for fam in "$ANDROID" "$CORE" "$TESTING"; do
    ok=0
    for coord in $fam; do
      if curl -sfI "$(pom_url "$coord" "$1")" > /dev/null; then
        ok=1
        break
      fi
    done
    [ "$ok" = 1 ] || return 1
  done
}

case "${1:-}" in
  latest)
    # Newest first; a version missing an artefact (a cancelled publish
    # run) is skipped
    for coord in $ANDROID; do
      versions "$coord"
    done | sort -uVr | while read -r version; do
      if served "$version"; then
        echo "$version"
        break
      fi
    done
    ;;
  wait)
    version=$2
    deadline=$(( $(date +%s) + WAIT_MINUTES * 60 ))
    while true; do
      if served "$version"; then
        echo "Maven Central serves $version"
        exit 0
      fi
      if [ "$(date +%s)" -ge "$deadline" ]; then
        echo "::error::Maven Central does not serve $version after $WAIT_MINUTES minutes" >&2
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
