# Changelog

Changes a lsty user can see, newest first.

<!-- markdownlint-configure-file { "MD013": false, "MD024": { "siblings_only": true } } -->

## 0.10.1 (2026-11-26)

### Changed

- **docs:** Rewrite the README: why lsty, the todo.txt tags lsty adds, an agent's loop, the keys for moving tasks, adding below and undo, and usage tips.

## 0.10.0 (2026-11-25)

### Changed

- **tui:** Show a done task in its own colours, struck through, in place of one colour; archived tasks look as before.
- **tui:** Count in the first number of a Projects or Contexts row only the open rows selecting it would show with the current hiding: a project whose only task waits for a date reads `0/1`.
- **core:** The `lsty-core` library's file API changed (`fsio::read` returns the file's mode with its contents and `fsio::write` takes it; `rolled_counts` takes a filter), hence 0.10.

### Fixed

- **core:** Complete tasks that wait on one another in one `do` or `x`: a prerequisite done in the same call counts as met.
- **tui:** Show the same hint when a tag or date prompt is refused as the `:` line does (`"work" is not a tag — use +name, @name or key:value`).
- **tui:** Drop a selected project or context once an outside edit leaves no task with it, so the list neither keeps its old rows nor empties to "nothing matches".
- **tui:** Name the right row in an `undid:` or `redid:` message when another program wrote an identical line, and in the message of a press made on rows another program has since moved.
- **core:** Never follow a link swapped in while a list is saved, and keep each file's permissions even if it is replaced between read and write.

### Faster

- **tui:** Stop rereading the themes folder or a `file:` folder on every key while the `:` line is open.

## 0.9.0 (2026-11-23)

### Breaking

- **tui:** Number list rows by their place in the list, from 1, in place of their line in the file; `{n}G`, `{n}gg` and `:N` go to place n. A bare number in any other `:` command is a place too, so `:do 3` completes the row numbered 3; todo.sh's line numbers stay for the command line (`lsty do 3`) and in the detail panel, which shows the line as `nr`.

### Added

- **tui:** Show the cursor position on the status line, left of the version: the task's place over the rows in the list (` 3|40 `), and in INSERT the column over the line's length as well (` 3|40│12|58 `).
- **tui:** Name the id of the task shown in the detail panel title, `[2]─Detail "id:65f9"`.

### Changed

- **tui:** Name tasks by their place in the list in every message, as the numbers on screen show them: `3 moved to 1`, `1 done: A; archived to done.txt`, `undid: 3 moved back from 1`. The detail panel's `blocked by` row, the `:ls` and `:show` popups and the `dep:` completion name places too. A hint's command to copy keeps the todo.sh form.
- **tui:** Lean the status line: a one-letter mode (` N `, ` V `, ` I `, ` C `), the sort as `sort(manual)`, and no file name. A missing file shows `[New]` in the list title instead.
- **tui:** Quote the path in the list title, `[1]─"~/todo.txt"`, with no padding spaces, like every other box title.
- **tui:** Say `task` or `tasks` after every count in a message (`2 tasks done; 1 task already done`), and say `moved 3 tasks` when a paste of cut tasks moved them all.
- **cli:** Say `task` or `tasks` after the counts of `lsty archive` and `lsty next` too: `archived 2 tasks to done.txt`, `nothing ready (2 tasks blocked, 1 task later)`.

### Fixed

- **tui:** End a list title cut short on a whole filter chip and `…`, never on the gap after one; a click on the `…` acts on that chip.
- **theme:** Colour a due date as due soon only within `upcoming_days`, as the `Upcoming` filter lists it, also when it is set below 6; dates up to 6 days away no longer always count as soon.
- **tui:** Name the task in the message of a refused link restore after a paste (`a dependent of a changed since; dep:a not restored`), in place of a number that was no longer right.

## 0.8.0 (2026-11-19)

### Breaking

- **keys:** `gp` now raises the priority and `gP` lowers it. `+` and `-` no longer change the priority. `<Right>` and `l` raise it and `<Left>` and `h` lower it.
- **tui:** List wrap is replaced by `list_rows`, the most rows a task takes in the list (3 by default).

### Added

- **theme:** Add the `lsty-80s` theme, and a `background` slot that fills the whole screen under every other colour; a theme without it keeps the terminal's background.
- **theme:** Let a theme restyle slots on the highlighted row alone, with `[cursorline_slots]`; `lsty-80s` uses it for a white cursor row.
- **theme:** Colour each part of a task (priority, project, context, code, link, due date, claim) the same wherever it shows: the list, the sidebar, the detail panel, completion, the link picker, the INSERT field and the `:` line.
- **config:** Layer a project's `.lsty/config.toml` over the home config, to keep a theme per project etc. It is found by walking up from the launch folder and applies only when its folder holds the todo file in use. The theme picker saves to the file that set the theme; a project config that is a link is not saved, the pick lasts the session and the status says why.
- **tui:** Open the search with the selected projects and contexts written as filter text. `Enter` moves them back into their panels, and `Esc` or `Ctrl-l` put each panel back as it was before the last search that changed it.
- **tui:** Complete the value after a key in the search, such as `is:` or `dep:`.
- **tui:** Let `+` and `-` take one more or one less row for a task in the list, up to the height of the list, and `0` set it back to `list_rows`; the highlighted task stays on its screen row.
- **tui:** Start the sidebar panels at a third of the height each, and keep that while each fits its entries.

### Changed

- **tui:** Make `gi` move the detail panel to the next place with room: right, below, hidden (below, right, hidden with `detail_position = "bottom"`).
- **tui:** Mark the first selected project or context in a panel with `=` in place of `>`.
- **show:** Read `blocked` for the state of a blocked task in `lsty show`, the detail panel and `:show`; `--json` keeps `state` and its blocked flag.

### Fixed

- **tui:** Show archived tasks when archive is on, whatever done says.
- **tui:** Show the distance in the gutter of an archived row, and size the number column for archived rows.
- **tui:** Show the cut-off projects and contexts of a task cut at its edge in a one-row list.
- **tui:** Keep a panel you changed by hand when `Esc` clears the search.
- **fsio:** Keep a private todo or config file private while it is saved: the file written before it replaces the old one has the old one's permissions from the start.

## 0.7.2 (2026-11-13)

### Changed

- **install:** Need Rust 1.100 or newer to build from crates.io

### Fixed

- **tui:** Give a yank its links back when its paste is undone

### Faster

- **tui:** Borrow the name pool and the distinct names
- **tui:** Count projects and contexts once per reading
- **cli:** Stream the tasks of ls --json to the output
- **cli:** Write the lines of ls as they are made
- **cli:** Escape json strings by runs

## 0.7.1 (2026-11-12)

### Changed

- **tui:** Answer a dim key from the list on screen, without reading the files first
- **tui:** Show a warning, refusal or error first on the status line

### Fixed

- **tui:** Keep the cursor on its line after a write that reads a change made outside lsty
- **tui:** Keep the row in sight when a key says to look again
- **tui:** Keep a selection on its lines when it loses its anchor, or when a write that changes nothing reads a newer list
- **tui:** Check every row a selection writes, and only the rows a count writes
- **tui:** Say a prompt changed nothing when it did not, and leave what `.` repeats alone
- **tui:** Keep the window of `t` through a dim key or a typed count
- **tui:** Judge a key with a count, such as `3gx`, by its count
- **tui:** Let a key other than a motion act on the sticky rows drawn
- **core:** Say a move past an identical line moved nothing, and leave the file alone
- **core:** Name the accepted forms of a refused value once, the same on the command line, in JSON and in the TUI
- **core:** Warn when a completion key in keys.toml is hidden by an insert key
- **core:** Let many writers at once wait their turn instead of saying the file is busy

### Faster

- **tui:** Show the list the core read at once after a look again

## 0.7.0 (2026-11-06)

### Added

- **core:** Add won't do
- **tui:** Make t start and stop and T mark won't do
- **tui:** Refuse t on a done task instead of reopening it
- **tui:** Put a claim back on a second t
- **tui:** Decide every writing key by one rule
- **tui:** Dim the hints whose key would change nothing
- **tui:** Dim the hints of the keys that do not write tasks
- **tui:** Show a typed count in the status line and make the hints follow it

### Fixed

- **core:** Make the messages that count agree with their number
- **tui:** Dim a hint only when its key would do nothing
- **tui:** Read the files again when a dim key is pressed
- **theme:** Draw built-in themes in the terminal's own colours at 16 colours
- **tui:** Name the place tab leaves for in the detail hint
- **tui:** Show x done first in the detail hints
- **tui:** Keep a sticky row for what starts from it
- **tui:** Dim the panel and focus hints whose key would do nothing
- **tui:** Act on the rows as drawn when a key comes before they change
- **tui:** Drop keys when the cursor leaves their task and jump to the true ends
- **tui:** Keep what a selection lights and forget keys a reading cannot place
- **tui:** Write nothing from a prompt a reading cannot place and say so
- **tui:** Refuse a key whose line changed since the list was drawn
- **tui:** Alert on a dim press too and cover every part of the line check
- **tui:** Forget keys lost in a popup once the next one ends them
- **tui:** Refuse a prompt or a selection whose lines changed
- **tui:** Forget the refusal when an empty list ends a selection
- **tui:** Refuse a field whose line changed under it
- **tui:** Refuse a dep: whose line changed and say how to save
- **tui:** Put the warning first in the refusal of a dep:
- **tui:** Ask the rule again from where a lost line last stood
- **tui:** Refuse an undo or redo on a line that changed since
- **tui:** Leave a dependent out of a paste when an outside edit moved it
- **core:** Put the blank lines a move crossed back with an undo
- **tui:** Keep the cursor on its row unless an outside edit left its line alone
- **tui:** Keep the selection and the repeat through a dim press
- **tui:** Say some are archived when nothing is selected
- **tui:** Dim the hint of a key with no candidate to move to
- **tui:** Dim the delete keys of a field with nothing to delete
- **tui:** Let x and T pass over an archived task in a press
- **tui:** Show the next panel key before the fold keys
- **tui:** Show the way out of VISUAL right after its first key
- **tui:** Close the command line when the list it aimed at changed
- **tui:** Reopen the command line only for a busy lock
- **tui:** Say reopen for x on archived tasks
- **core:** Say reopened as after a restore
- **tui:** Say x reopen over done and archived tasks
- **tui:** Make T mark won't do only

## 0.6.1 (2026-10-29)

Pull requests are now welcome, see [CONTRIBUTING.md](https://github.com/skaragianis/lsty/blob/main/CONTRIBUTING.md).

### Fixed

- **tui:** Never rewrite a session state file that could not be read

## 0.6.0 (2026-10-28)

### Added

- **tui:** Search with the filter language of `--filter`
- **tui:** Auto-complete names and states in the search
- **tui:** Auto-complete filter terms on the command line as the search does
- **core:** Hint at the spaces when a term starts or ends with a bar
- **tui:** Take back what the last search moved when it is cleared
- **tui:** Hint how to clear a search and what enter does to it
- **tui:** Say what enter does in the due and tag prompts

### Fixed

- **tui:** Auto-complete states only after is: and nothing inside a phrase
- **tui:** Auto-complete file paths in filter terms
- **core:** Hint whenever a term starts or ends with a bar

## 0.5.0 (2026-10-21)

### Added

- **cli:** Add wait to block until a task changes
- **cli:** End wait at once when a task is already done
- **cli:** Show several tasks at once
- **keys:** Bind the readline editing keys in insert and command mode
- **list:** Add a done sort, newest first
- **ops:** Give every new task an id
- **tui:** Drop the owner column from the list
- **tui:** Draw the human's claim with a hollow marker
- **tui:** Add an id column to the list
- **tui:** Show cut projects and contexts after the ellipsis
- **ops:** Refuse an id an agent chooses
- **theme:** Paint the id column in its own slot
- **ops:** Make replace keep only what lsty writes and take a typed date

### Fixed

- **tui:** Show a repeated project or context once after the ellipsis
- **core:** Keep the reference as given in hints that name a command
- **core:** Count every line of the done file as done
- **ops:** Take a date prepended to an undated task as its creation date
- **cli:** Put a blank line between the tasks show prints
- **ops:** Make del empty a line of only white space
- **cli:** Report json done for every task in the done state

### Faster

- **tui:** Wrap only the words that reach the third row

## 0.4.0 (2026-10-19)

### Added

- **tui:** Hint the stop key after the cycle key
- **tui:** Show the version at the end of the status row
- **tui:** Draw the active border plain while the terminal is unfocused
- **core:** Hide done tasks at launch by default
- **tui:** Give the detail panel three tenths of the screen

### Fixed

- **cli:** Bundle the time zone database for the tests
- **tui:** Watch every folder again on reload where that is harmless
- **tui:** Keep the view still when a task written above the cursor lands
- **tui:** Land an undo by the rows the record names in the shown reading
- **tui:** Keep a click on the border of a tall field off its text
- **tui:** Draw nothing in a buffer with no width or no height
- **core:** Keep a space between replaced text and a touching token
- **tui:** Focus the pressed box and draw queued input once

## 0.3.0 (2026-10-15)

### Breaking

- **tui:** Make space a second key for x and the leader a backslash
- **tui:** Show projects and contexts in the text and drop their column
- **cli:** Call items tasks in json fields, error codes, options and key actions, json version 3
- **cli:** Read the config file and variables of todo.sh
- **cli:** Read a word with one dash after the command as a term or text

### Added

- **core:** Stamp ended when a started item is completed
- **keys:** Follow vim for the keys of the text input
- **tui:** Run a command by a unique prefix of its name
- **clip:** Write the clipboard with a program and the tmux buffer
- **keys:** Close help with f1 and quit from help and the pickers with ctrl-q
- **core:** Read a word with a scheme and two slashes as text
- **core:** List urls among an item's links
- **tui:** Open urls with the links of an item
- **cli:** Open urls with the links of an item
- **tui:** Save on enter when the word typed is the candidate
- **core:** Try each clipboard program in order
- **cli:** List urls with the files in the json item
- **cli:** Raise the json output version to 2
- **tui:** Add + and - as priority keys
- **tui:** Leave the priority column out when no item has one
- **tui:** Use short forms in the date column
- **tui:** Mark a stale duration with a bang
- **tui:** Give the item text most of the row
- **tui:** Keep the end of a narrowed owner
- **tui:** Shorten projects and contexts before the column narrows
- **tui:** Limit the owner column to 12 cells
- **tui:** Wrap the field by words
- **tui:** Show today in the date column only for a todo due today
- **tui:** Clear every filter with gc
- **tui:** List the labels column in natural order
- **tui:** Say filters cleared after gc
- **add:** Add --blocks to make an item wait for the new one
- **tui:** Add go and gO to add an item linked to the cursor todo
- **tui:** Let the dates of a line be edited in insert mode
- **edit:** Let the editor change the dates of a line
- **tui:** Keep a date typed in front of a new todo as its creation date
- **tui:** Wrap long items in the list
- **tui:** Give a todo changed meanwhile its id in place of refusing
- **tui:** Keep a typed date as the creation date with include_creation off
- **cli:** Keep a date typed in front of an added todo as its creation date
- **tui:** Keep wrapped rows to the text column
- Call a line of the file a task on the screen, in messages and in help
- **cli:** Add shorthelp, command, listaddons and report
- **cli:** Take todo.sh's short options before the command
- **tui:** Draw the title of a box without focus in the muted slot
- **tui:** Wrap long tasks by default
- **theme:** Make catppuccin-frappe the default theme

### Fixed

- **tui:** Click after a file reload only the line that was pressed
- **tui:** Refuse arguments to the quit commands
- **tui:** Shorten the owner in the claim column, not the state and duration
- **tui:** Refuse arguments to commands that take none
- **tui:** Let tab accept the word typed and keep typing
- **tui:** Leave the blank before a link without its style
- **tui:** Leave the blank before the stale count without its style
- **tui:** Narrow the claim and tags columns before dropping one
- **open:** Fail the opener when there is no display or browser
- **tui:** Clear every status message after two seconds
- **tui:** Make a click in a list row only select the row
- **tui:** Start a new item only from the highlighted item
- **core:** Show the duration of a done item beside its ended time
- **tui:** Shorten the empty list line to nothing matches
- **tui:** Cope with the file changing under a draft that gives an id
- **cli:** Draw control characters in printed text as pictures
- **tui:** Open the key help at the section of where it was opened
- **help:** Say project or context where help and readme said name
- **help:** Say box, select and blank lines where help and readme used other words
- **help:** Head the key help group of the boxes Boxes
- **cli:** Make listaddons exit 0 like todo.sh
- **cli:** Use a letter that is still unknown in the generic hint test
- **core:** Work out the report file only in report
- **core:** Refuse a config value that bash would read differently
- **core:** Warn about any unreadable line that mentions a variable lsty reads
- **core:** Stop reading the config file at a line that goes on

## 0.2.0 (2026-10-12)

### Added

- **core:** Read nothing from text between backticks
- **cli:** List actions and default keys in help keys
- **cli:** Describe custom themes in help themes
- **tui:** Share the motions across the list, panels and popups
- **tui:** Quit with :q!, :qa, :qa! and ZQ
- **tui:** Add the vim scrolling keys ctrl-f/b/e/y, zz, H, M and L
- **tui:** Add the detail_position option, right or bottom
- **tui:** Show the item number in the detail panel
- **tui:** Start a new item with the projects of the item beside it
- **tui:** Quit with :quit, :wqa, :xa and their vim spellings
- **tui:** Add the scrolling keys zt and zb
- **tui:** Show the delete keys of insert mode in the hint row
- **tui:** Complete a name typed right before other text
- **tui:** Show id and dep tags in the list, with the show_ids option
- **tui:** Go to an item by its number with :N
- **tui:** Show the path of the file in the list title
- **tui:** Start a new item with the contexts of the item beside it, with the inherit_context option
- **core:** Start a new name at a sigil inside a project or context word
- **tui:** Move the cursor to the next item after completing one
- **tui:** Act on the item shown while the detail panel has the focus
- **keys:** Make the detail panel follow the normal keys
- **keys:** Make x change a panel entry and enter only go to the list
- **tui:** Choose several names in the project and context panels with connectives
- **tui:** Refilter the list 50 ms after the last panel step
- **keys:** Bind plain keys beside the vim keys

### Fixed

- **tui:** Move between the list and a detail panel below it with ctrl-w j/k
- **core:** End a project or context name before trailing punctuation
- **core:** Skip an empty level in a project or context name
- **tui:** Complete names that hold the typed text anywhere
- **tui:** Put the cursor on the item below in the view after a delete
- **tui:** Hide a panel name whose items are all done and hidden
- **tui:** Move sideways to the detail panel only when it is beside the list
- **tui:** Inherit projects only when nothing is chosen in the sidebar
- **tui:** Keep a panel name while any of its items is left
- **tui:** Inherit nothing while a search or filter is active
- **tui:** Put the cursor on the changed item after undo and redo
- **core:** Cut only trailing sentence marks off a name
- **core:** Keep a name with a doubled separator as one flat name
- **tui:** Keep the cursor on the item whose tag was removed
- **core:** List finished items in the built-in filters
- **tui:** Keep completing a name typed before other text
- **core:** List only open items in the upcoming filter
- **tui:** Show no overdue note on a finished item
- **tui:** Keep the cursor when a command archives another item
- **tui:** Keep the cursor on the last row when a command completes it
- **tui:** Release the rows held in view at the next key or the end of the notice
- **tui:** Count a move over the rows that stay when rows are held
- **tui:** Keep held rows in place while a mouse button is down

## 0.1.0 (2026-10-11)

First release.
