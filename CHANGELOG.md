# Changelog

Changes a user of lsty can see, newest first.

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
