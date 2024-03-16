#!/bin/sh
#
# Generate documentation
#

basedir="$(realpath "$0" | xargs dirname)"
srcdir="$basedir/.."

die()
{
	echo "$*" >&2
	exit 1
}

gen()
{
	local md="$1"
	local docname="$(basename "$md" .md)"
	local dir="$(dirname "$md")"
	local html="$dir/$docname.html"

	echo "Generating $(realpath --relative-to="$srcdir" "$html") from $(realpath --relative-to="$srcdir" "$md") ..."
	python3 -m readme_renderer -o "$html" "$md" ||\
		die "Failed to generate HTML."
}

for i in $(find "$srcdir" \( -name release-archives -prune \) -o \( -name target -prune \) -o \( -name '*.md' -print \)); do
	gen "$i"
done

cd "$srcdir" || die "cd failed."
cargo doc || die "cargo doc failed."

exit 0
