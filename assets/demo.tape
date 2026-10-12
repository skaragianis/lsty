# Run from the repository root: just build && vhs assets/demo.tape
# lsty runs in the upper tmux pane; demo-agent.sh types an agent's commands
# into the lower one. Only a temporary copy of demo.todo.txt is changed.
# Personal configuration and session state are isolated, and all writers use
# the same defaults.

Require ./target/release/lsty
Require bash
Require mktemp
Require cp
Require tmux

Output assets/demo.gif
# Output assets/demo.mp4

Set Shell bash
Set FontFamily "DejaVu Sans Mono"
Set FontSize 14
Set Columns 100
Set Rows 36
Set Padding 16
Set Theme "Catppuccin Mocha"
Set Framerate 12
Set TypingSpeed 70ms
Set CursorBlink false

# Prepare the list, environment, and cleanup without recording the shell.
Hide
Type@1ms `set -eu; demo=$(mktemp -d); trap 'tmux -L lsty-demo kill-server 2>/dev/null || :; rm -rf "$demo"' EXIT; cp assets/demo.todo.txt "$demo/todo.txt"; export HOME="$demo" XDG_CONFIG_HOME="$demo/config" XDG_STATE_HOME="$demo/state" PATH="$PWD/target/release:$PATH" TZ=UTC LSTY_NOW=2026-10-07T1000Z TERM=xterm-256color COLORTERM=truecolor TODO_FILE="$demo/todo.txt"; unset TODO_DIR DONE_FILE LSTY_FILE LSTY_OWNER LSTY_CONFIG LSTY_NONINTERACTIVE NO_COLOR TMUX; clear`
Enter
Sleep 500ms

# The agent starts typing a few seconds in; its pauses track the tape.
Type@1ms `assets/demo-agent.sh & tmux -L lsty-demo -f assets/demo.tmux.conf new-session -s demo -x 100 -y 36 lsty \; select-pane -T "human: lsty" \; split-window -v -l 5 'PS1="$ " exec bash --norc --noprofile' \; select-pane -T "agent: lsty CLI" \; select-pane -t 0`
Enter
Wait+Screen /NORMAL/
Show
Sleep 2s

# Navigate, then add a task with a relative due date. The agent claims the
# top task meanwhile.
Type@350ms "jj"
Sleep 800ms
Type "o"
Sleep 500ms
Type@40ms "Prepare launch checklist @computer due:+2d"
Enter
Sleep 2500ms

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

# Add a personal task, completing the nested context with Tab.
Type "G"
Type "k"
Sleep 500ms
Type "o"
Sleep 500ms
Type@40ms "Read chapter 4 @home.s"
Sleep 1s
Tab
Sleep 800ms
Type@40ms " due:sat"
Enter
Sleep 2s

# Filter on @home, which takes in @home.study, then narrow to .study, then
# clear both; x keeps the focus on the panel.
Ctrl+W
Type "4"
Sleep 500ms
Type@300ms "jj"
Sleep 500ms
Type "x"
Sleep 2500ms
Type "j"
Sleep 300ms
Type "x"
Sleep 2500ms
Type "x"
Type "k"
Type "x"
Sleep 800ms

# Show only what is due today; x keeps the focus on the panel.
Ctrl+W
Type "5"
Sleep 500ms
Type@300ms "jjjj"
Sleep 500ms
Type "x"
Sleep 1s

# The agent finishes and leaves a review due today, which the filter shows.
# "due today" is only in the list: the agent types "due:today".
Wait+Screen /due today/
Sleep 2s
Ctrl+W
Type "1"
Sleep 1s
Type "x"
Sleep 2s

# Back to everything: the agent's task and the review are both done.
Ctrl+W
Type "5"
Type "x"
Ctrl+W
Type "1"
Sleep 4s

# Hide the shells and temporary-directory cleanup from the recording.
Hide
Type "q"
Sleep 500ms
Type@1ms "exit"
Enter
Sleep 500ms
Type@1ms "exit"
Enter
