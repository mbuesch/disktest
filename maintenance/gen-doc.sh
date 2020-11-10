#!/bin/sh
#
# Generate documentation
#


basedir="$(dirname "$0")"
[ "$(echo "$basedir" | cut -c1)" = '/' ] || basedir="$PWD/$basedir"

srcdir="$basedir/.."


die()
{
	echo "$*" >&2
	exit 1
}

gen()
{
	local rst="$1"
	local docname="$(basename "$rst" .rst)"
	local dir="$(dirname "$rst")"
	local html="$dir/$docname.html"
	local md="$dir/$docname.md"

	echo "Generating $(realpath --relative-to="$srcdir" "$html") from $(realpath --relative-to="$srcdir" "$rst") ..."
	python3 -m readme_renderer -o "$html" "$rst" ||\
		die "Failed to generate HTML."

	echo "Generating $(realpath --relative-to="$srcdir" "$md") from $(realpath --relative-to="$srcdir" "$rst") ..."
	pandoc -s -o "$md" "$rst" ||\
		die "Failed to generate MD."
}

for i in $(find "$srcdir" \( -name release-archives -prune \) -o \( -name target -prune \) -o \( -name '*.rst' -print \)); do
	gen "$i"
done

cd "$srcdir" || die "cd failed."
cargo doc || die "cargo doc failed."

exit 0
