#!/bin/sh

srcdir="$(realpath -e "$0" | xargs dirname)"
srcdir="$srcdir/.."

# Import the makerelease.lib
# https://bues.ch/cgit/misc.git/tree/makerelease.lib
die() { echo "$*"; exit 1; }
for path in $(echo "$PATH" | tr ':' ' '); do
	[ -f "$MAKERELEASE_LIB" ] && break
	MAKERELEASE_LIB="$path/makerelease.lib"
done

[ -f "$MAKERELEASE_LIB" ] && . "$MAKERELEASE_LIB" || die "makerelease.lib not found."
project=disktest-rawio
conf_package=disktest-rawio
conf_notag=1
makerelease "$@"

[ -f "$MAKERELEASE_LIB" ] && . "$MAKERELEASE_LIB" || die "makerelease.lib not found."
project=disktest-lib
conf_package=disktest-lib
conf_notag=1
makerelease "$@"

[ -f "$MAKERELEASE_LIB" ] && . "$MAKERELEASE_LIB" || die "makerelease.lib not found."
project=disktest
conf_package=disktest
conf_notag=0
makerelease "$@"
