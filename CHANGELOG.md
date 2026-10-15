# Changelog

Changes a user of lsty can see, newest first.

<!-- markdownlint-configure-file { "MD013": false, "MD024": { "siblings_only": true } } -->

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
- **demo:** Wait for the list row of the review, not the agent's typing
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
