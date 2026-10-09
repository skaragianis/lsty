# Run from the repository root: just build && vhs assets/demo.tape
# Only a temporary copy of demo.todo.txt is changed. Personal configuration
# and session state are isolated, and all writers use the same defaults.

Require ./target/release/lsty
Require bash
Require mktemp
Require cp

Output assets/demo.gif
# Output assets/demo.mp4

Set Shell bash
Set FontFamily "DejaVu Sans Mono"
Set FontSize 18
Set Columns 80
Set Rows 24
Set Padding 16
Set Theme "Catppuccin Mocha"
Set Framerate 12
Set TypingSpeed 70ms
Set CursorBlink false

# Prepare the list, environment, and cleanup without recording the shell.
Hide
Type@1ms `set -eu; demo=$(mktemp -d); agent=; trap 'test -z "$agent" || kill "$agent" 2>/dev/null || :; rm -rf "$demo"' EXIT; cp assets/demo.todo.txt "$demo/todo.txt"; export HOME="$demo" XDG_CONFIG_HOME="$demo/config" XDG_STATE_HOME="$demo/state" PATH="$PWD/target/release:$PATH" TZ=UTC LSTY_TODAY=2026-10-07 LSTY_NOW=2026-10-07T1000Z TERM=xterm-256color COLORTERM=truecolor; unset TODO_FILE TODO_DIR DONE_FILE LSTY_OWNER LSTY_CONFIG LSTY_NONINTERACTIVE NO_COLOR; clear`
Enter
Sleep 500ms

# A second process claims a task, and completes it just after the final gg
# (its sleeps track the tape's length: lengthen the tape, raise them).
Type@1ms `(sleep 4; lsty start id:review --owner demo-agent --file "$demo/todo.txt" >/dev/null; sleep 20; lsty do id:review --owner demo-agent --file "$demo/todo.txt" >/dev/null) & agent=$!`
Enter
Type@1ms `lsty --file "$demo/todo.txt" --no-config; wait "$agent"`
Enter
Wait+Screen /NORMAL/
Show
Sleep 2s

# Navigate, then add a task with a relative due date.
Type@350ms "jj"
Sleep 800ms
Type "o"
Sleep 500ms
Type@40ms "Prepare launch checklist +lsty @computer due:+2d"
Enter
Sleep 1500ms
# Enter opened the next new task; Escape leaves it unwritten.
Escape
Sleep 1s

# Narrow the list with live search, then clear it.
Type "/"
Type@150ms "launch"
Enter
Sleep 2s
Type "/"
Ctrl+U
Enter
Sleep 1s

# Complete the new task, then undo the change.
Type "x"
Sleep 2s
Type "u"
Sleep 1s

# Toggle a filter on the @home context, which takes in @home.study, then
# off; Space keeps the focus on the panel.
Ctrl+W
Type "3"
Sleep 500ms
Type@300ms "jj"
Sleep 500ms
Space
Sleep 2500ms
Space
Sleep 800ms
Ctrl+W
Type "1"
Sleep 500ms

# Return to the agent's task. The state field stays visible in the compact
# detail panel; the claim description can be clipped below it.
Type "gg"
Wait+Screen /state +done/
Sleep 3s

# Hide the shell and temporary-directory cleanup from the recording.
Hide
Type "q"
Sleep 1s
Type@1ms "exit"
Enter
