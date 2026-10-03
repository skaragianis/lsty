#!/usr/bin/env bash
set -euo pipefail

VERSION=2.14.0
URL="https://github.com/todotxt/todo.txt-cli/releases/download/v${VERSION}/todo.txt_cli-${VERSION}.tar.gz"
FIXED_DATE=2026-09-29

out=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

curl -fsSL "$URL" | tar -xz -C "$work"
todo="$work/todo.txt_cli-${VERSION}/todo.sh"

mkdir "$work/bin" "$work/data"
cat >"$work/bin/date" <<SH
#!/bin/sh
case "\$*" in
  *%Y-%m-%d*) echo $FIXED_DATE ;;
  *) exec /bin/date "\$@" ;;
esac
SH
chmod +x "$work/bin/date"
export PATH="$work/bin:$PATH"

cat >"$work/todo.cfg" <<CFG
export TODO_DIR="$work/data"
export TODO_FILE="\$TODO_DIR/todo.txt"
export DONE_FILE="\$TODO_DIR/done.txt"
export REPORT_FILE="\$TODO_DIR/report.txt"
export TODOTXT_PRESERVE_LINE_NUMBERS=1
export TODOTXT_DATE_ON_ADD=1
export TODOTXT_FORCE=1
export TODOTXT_AUTO_ARCHIVE=0
CFG

t() { "$todo" -d "$work/todo.cfg" "$@" >/dev/null; }

t add "Call client +work @phone due:2026-09-29"
t add "Buy milk @store"
t add "Write report +work t:2026-10-01"
t add "Water plants rec:1w"
t add "Pay rent +home due:2026-10-01"
t add "Old task to delete"
t add "Fix bug +work"
t add "Email Bob @computer"
t pri 1 A
t pri 3 B
t pri 7 C
t append 2 "from the corner shop"
t do 2
t do 8
t del 6
cp "$work/data/todo.txt" "$out/before-archive.txt"
t archive
cp "$work/data/todo.txt" "$out/todo.txt"
cp "$work/data/done.txt" "$out/done.txt"
