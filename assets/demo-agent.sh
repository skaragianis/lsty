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
type 'lsty next'
sleep 1
type 'lsty start id:install'
sleep 24
type "lsty add '(A) Review the installation fixes +lsty @computer due:today'"
sleep 1
type 'lsty do id:install'
