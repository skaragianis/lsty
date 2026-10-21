#!/usr/bin/env bash
# The agent of assets/demo.tape: types lsty commands into the lower pane.
set -eu
type() {
    local i
    for ((i = 0; i < ${#1}; i++)); do
        tmux -L lsty-demo send-keys -t demo:0.1 -l -- "${1:i:1}"
        sleep 0.02
    done
    sleep 0.4
    tmux -L lsty-demo send-keys -t demo:0.1 Enter
}
sleep 4
type 'export LSTY_OWNER=agent-7f3a'
type 'lsty next --filter @agent'
sleep 1
type 'lsty start id:r7kx'
sleep 24
type "lsty add '(A) Review the installation fixes +lsty @human due:today'"
sleep 1
type 'lsty do id:r7kx'
# Wait for the human's review, which the publish task depends on, then look
# again: wait itself tells the script when to go on.
sleep 1
type 'lsty wait id:h4qe'
lsty wait --timeout 2m id:h4qe >/dev/null
sleep 1.5
type 'lsty next --filter @agent'
