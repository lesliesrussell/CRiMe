# CRiMe

Keep track of people, companies, deals, and the next thing you promised to do—all from your terminal.

CRiMe is a personal CRM with guided forms, linked records, follow-up tasks, and a daily action list. Your records live in a local database. You can use the terminal interface without writing JSON, use the command line for scripts, or connect an AI assistant through MCP.

**Start here:** [Install](#install) → [Create your first contact](#your-first-five-minutes) → [Build a daily routine](#your-daily-routine).

## Install

### What you need

The instructions below use a macOS or Linux shell. Install these tools before building:

- **Git** to download the source.
- **Current stable Rust and Cargo** to compile CRiMe. If you do not have them, use the installer at [rustup.rs](https://rustup.rs), then reopen your terminal.
- **A C/C++ compiler and platform build tools** because CRiMe builds DuckDB from source. On macOS, run `xcode-select --install`. On Debian/Ubuntu, install `build-essential` with your package manager.
- **Python 3** for the installation script.

Check that the tools are available:

```sh
git --version
rustc --version
cargo --version
c++ --version
python3 --version
```

### Build and install from source

```sh
git clone https://github.com/lesliesrussell/CRiMe.git
cd CRiMe
cargo build --release --locked
python3 scripts/install.py "$HOME/.local"
```

The first build can take several minutes while DuckDB compiles. The installer places the executable at `~/.local/bin/crime` and prints optional MCP configuration; you can ignore that configuration if you only want the terminal app.

Add the install directory to your current shell's PATH:

```sh
export PATH="$HOME/.local/bin:$PATH"
crime --version
crime
```

To keep that PATH setting after reopening your terminal, add the `export` line to `~/.zshrc` for Zsh or `~/.bashrc` for Bash. On macOS with Bash, use `~/.bash_profile`.

If you already use [just](https://github.com/casey/just), `just install` builds and installs to the same location. A custom prefix is supported too:

```sh
just install "$HOME/apps/crime"
# Then add $HOME/apps/crime/bin to PATH.
```

You can also try the app without installing it:

```sh
./target/release/crime
```

### Update or uninstall

To update, close CRiMe, return to your source checkout, and run:

```sh
git pull --ff-only
cargo build --release --locked
python3 scripts/install.py "$HOME/.local"
```

To uninstall the default installation, remove `~/.local/bin/crime`. This leaves your database in place. If you registered an MCP server, remove that registration in your assistant's settings as well.

## Your first five minutes

Run `crime`. It opens **Today**, your list of tasks and relationships needing attention. A new database starts empty.

### 1. Create a company

1. Press **Tab** until **Companies** is selected.
2. Press **n** to open a new-company form.
3. Select the name field, press **Enter**, type `Analytical Engines`, and press **Enter** to apply the text.
4. Fill in any other fields you want. Use **Tab** or the arrow keys to move through the form and **Enter** to edit a field.
5. Press **F10** to save. You can also use **Ctrl-X**, then **Ctrl-S**.

### 2. Add a contact and link the company

1. Switch to **Contacts** with **Tab** or **Shift-Tab**, then press **n**.
2. Set the name to `Ada Lovelace` and add an email or phone number.
3. Select the company link field and press **Enter**. Choose **Analytical Engines** by name.
4. Save with **F10**.

You now have two records connected by a relationship. You do not need to copy an ID or type JSON to link them.

### 3. Record a conversation and schedule the next step

With Ada selected in the record list:

1. Press **a** to add an activity note. Give it a title and put your conversation notes in the body, then save.
2. Press **f** to create a follow-up task linked to Ada. Give it a title such as `Send proposal`, enter a due date as `YYYY-MM-DD`, and save.
3. Press **Esc** to leave the timeline, then **g** to open Today and find the task. Tasks due in the next seven days appear there; tasks further away are available in **Tasks**.
4. When it is finished, select the task and press **x** to mark it done.

Press **l** on Ada to see the relationship's timeline, including linked activity and changes. Press **Esc** to return to the record browser. From there, **r** browses related records, **Enter** follows one, and uppercase **B** goes back.

### 4. Find your way around

Press **Alt-X** to open the searchable action palette in the bottom quarter of the terminal. Type an action such as `import`, `schema`, `trash`, or `theme`, then press **Enter**. You can also find custom categories this way.

Press **?** whenever you need the full key guide. **Esc** cancels an action or backs out of a screen. In the record browser, **q** quits.

## Essential keys

Uppercase and lowercase keys do different things. `Ctrl-X`, then `Ctrl-S` means press those two combinations in sequence. The on-screen guide abbreviates Ctrl as `C-` and Alt/Option as `M-`.

| What you want to do | Key |
| --- | --- |
| Move through records or choices | Up / Down, or Ctrl-P / Ctrl-N |
| Change category | Tab / Shift-Tab |
| Search the current list | `/` or Ctrl-S |
| Open the action palette | Alt-X |
| Create / edit a record | `n` / `e` or Enter |
| Save a form | F10, or Ctrl-X then Ctrl-S |
| Cancel / go back | Esc or Ctrl-G |
| Return to Today | `g` |
| Add a note / follow-up for the selected record | `a` / `f` |
| Complete the selected task | `x` |
| View activity / related records | `l` / `r` |
| Return after following a relationship | `B` |
| Move a record to trash / undo the last deletion | `d` / `u` |
| Open Trash | `b` |
| Show the key guide | `?` or F1 |
| Quit from the record browser | `q`, or Ctrl-X then Ctrl-C |

Ctrl-C by itself is reserved; it does not quit. On terminals that reserve Alt shortcuts, use the single-letter keys or the key guide. If function keys control brightness or volume, use your keyboard's Fn key, or use Ctrl-X then Ctrl-S to save.

## Your daily routine

### Start with Today

Press `g` to see overdue tasks, today's tasks, the next seven days, and tasks without dates. Today also highlights open deals without a follow-up, stale deals, and contacts needing attention. Completed or cancelled tasks and won or lost deals are excluded.

Use `a` to log a conversation and `f` to schedule a next step while a contact or company is selected. Linked notes and completed follow-ups help CRiMe decide when a contact needs attention. A future `next_contact` date postpones that prompt; `follow_up_days` changes the default 30-day interval. Dates follow your local calendar.

### Search, inspect, and follow links

Switch to a category and press `/` to search names and field values as you type. Ctrl-G clears the search. Press `e` to edit the selected record.

Press `r` to see records linked in either direction. Follow a company, contact, task, note, deal, or custom record with Enter. `B` returns to the previous location, including its category, filter, saved view, and scroll position.

Ctrl-X then `o` switches focus between the record list and details. Page Up / Page Down scroll details. Press `l` for the activity timeline; deleted linked records can still appear in its history.

### Save a view you use often

Press `V` to turn the current category and search into a saved view. In the view editor, add conditions with F2, choose sorting and visible fields, then save with F10. Conditions are combined with AND. Date conditions can use `$today` so they stay current.

Press `v` to open saved views. Starter views include **Overdue tasks**, **Active deals**, and **Contacts needing follow-up**. Views save rules and update as your records change.

### Recover a deleted record

With the record list focused, `d` immediately moves the selected record to trash. Press `u` to restore the most recent deletion, or `b` to choose a record from Trash and restore it with Enter or `r`.

Restoration keeps the original ID, fields, links, and timestamps. In Trash, `d` followed by `y` removes the recovery copy permanently. Activity history remains.

## Make the CRM fit your work

### Add fields or your own categories

Built-in categories are Contacts, Companies, Deals, Tasks, and Notes. Press `c` to create a category such as Projects or Vendors. Give it a display name and a stable lowercase ID, such as `project`; the schema editor opens next.

A **schema** is the reusable set of fields and defaults for a category. Press `s` in that category to edit it:

| In the schema editor | Key |
| --- | --- |
| Add a named field | F2 |
| Remove a field | F3 |
| Change a field's type | F5 |
| Set a default value | Enter |
| Set required, dropdown choices, or a description | F6 |
| Move a field up / down | F7 / F9 |
| Save a new schema version | F10 |

Fields can hold text, numbers, yes/no values, dates, groups, lists, or links to records. Dropdown choices require a Text field; enter one choice per line using Ctrl-J. Required fields can have no default, but must be filled before a record can be saved.

You can also add a field while editing an individual record with F2. That changes just that record; use the schema editor for fields you want on every new record. F4 adds a list item. Ctrl-J inserts a newline in text.

Schema changes apply to new records. Existing records keep their applied schema version until you explicitly migrate them: open the record, press F8, review the changes, apply them to the draft, and save with F10. Cancel to leave the stored record unchanged.

### Import a spreadsheet as CSV

Export your spreadsheet as a CSV file with column headers. For example:

```csv
Name,Email,Phone,Tags
Ada Lovelace,ada@example.com,555-0100,customer;priority
Grace Hopper,grace@example.com,555-0101,partner
```

In CRiMe:

1. Press `i` and enter the CSV file path.
2. Choose the record category.
3. Review the proposed column mappings. Press Enter on a column to change its destination or ignore it. One column must map to the record's name/title.
4. Choose a duplicate match field, such as email, and a policy: **Skip**, **Merge mapped nonblank values**, or **Keep both**.
5. Press F10 to preview the counts, sample values, and row errors.
6. Fix any errors, then press F10 on the preview to import. Esc returns to the mappings.

The preview does not write records. An import commits only when all rows validate. CSV merge preserves existing values in unmapped columns. Ambiguous duplicate matches are rejected.

Use `YYYY-MM-DD` dates, semicolons between list items, and yes/no, true/false, or 1/0 for boolean fields. Links can use an existing record's name or ID. Quoted fields and multiline values are supported.

### Change several records at once

Space selects or unselects a record. `A` selects all visible records; `C` clears the selection. Checkmarks show what is selected. Selections persist when you change filters or categories.

Press `U` to open bulk actions:

- `u`: enter a field name and its new value. Use `name` to rename records. Plain text works directly; numbers, booleans, and lists use JSON syntax.
- `d`: move the selection to trash after confirming with `y`.
- `e`: export only the selected records to a new JSON file. Existing files are preserved.

Updates and trash operations are atomic: if a selected record fails validation, the operation rolls back.

### Merge duplicate records

Select the record you want to **keep**, press `M`, and choose a duplicate of the same category. Enter opens a preview; F10 commits the reviewed merge.

The survivor keeps its name and conflicting values. Missing values are filled from the duplicate, and lists are combined. Links in active records and trash copies point to the survivor. Both records' activity history is retained, and the duplicate's original values are archived.

A merge is not undone by `u`. Review the preview before committing. Use `crime run merges` to inspect the archived source records later.

### Choose a theme

Press `T` to cycle Tokyo Night, Dracula, Solarized Dark, and Solarized Light. The choice is saved for this database. You can also search for a theme in the action palette or choose one at launch:

```sh
crime tui --theme dracula
crime tui --theme solarized-light
```

## Use the command line

Close the terminal app before using a separate CLI process against the same database. Commands return JSON; record creation returns an `id` that you can use in later commands.

```sh
crime add contact 'Ada Lovelace' --data '{"email":"ada@example.com"}'
crime list --kind contact --search Ada
crime add task 'Send proposal' --data '{"due":"2026-12-01","status":"open"}'
crime today
crime views
crime view 'Overdue tasks'
```

Replace `RECORD_ID` below with the ID returned by `add` or `list`:

```sh
crime get RECORD_ID
crime update RECORD_ID --data '{"phone":"555-0100"}'
crime timeline RECORD_ID
crime related RECORD_ID
crime delete RECORD_ID
crime undo
crime export > records.json
```

To link a task or note to a contact from the CLI, put the contact's ID in its `contact_id` field. Company links use `company_id`. Updates merge top-level fields; extra fields are allowed, while saved schemas enforce their field types and rules.

Preview a CSV import before running it without `--preview`:

```sh
crime import-csv people.csv --kind contact --preview
crime import-csv people.csv --kind contact --duplicates merge --match-field email
crime import-csv people.csv --kind contact --name-column 'Full Name' --map 'Email Address=email'
```

Use `crime --help` or `crime COMMAND --help` for flags. `crime run help` lists the shared command syntax used by the advanced terminal input (`:`) and MCP. For example:

```sh
crime run add contact 'Grace Hopper' '{"email":"grace@example.com"}'
crime run list contact Grace
```

## Connect an AI assistant with MCP

CRiMe includes a stdio MCP server in the same executable. Your assistant launches it when needed; you do not need to start a background service yourself.

1. Install CRiMe first.
2. Close the TUI so the assistant can open the same database.
3. Generate a server definition:

   ```sh
   crime mcp-config
   ```

4. Merge the printed `mcpServers.crime` entry into your MCP client's configuration, preserving its other servers. It contains the absolute executable and database paths. If your client uses a server setup form, use that executable as the command and the printed `args` array as its arguments.
5. Reload or restart the client, enable the `crime` server, and try: “Show my overdue tasks” or “Add Ada Lovelace as a contact with email ada@example.com.”

For Claude Desktop on macOS, the configuration file is `~/Library/Application Support/Claude/claude_desktop_config.json`. For clients that accept the registration commands used by the included script, run one of these from your source checkout:

```sh
python3 scripts/install_mcp.py codex
python3 scripts/install_mcp.py claude
```

These register the installed executable in the client's user configuration; `claude` here means Claude Code, not Claude Desktop. For a custom installation or database, use `--prefix` and `--db`:

```sh
python3 scripts/install_mcp.py codex --prefix "$HOME/apps/crime" --db /absolute/path/crm.duckdb
```

The MCP tool is named `crm` and accepts an `args` array. `{"args":["help"]}` lists available commands. Your assistant can read and change records through this tool according to the approvals you grant in its client. Records are stored locally, but any content you ask an assistant to read is also shared with that assistant.

## Where your data lives

CRiMe creates its database automatically on first use. By default it uses your operating system's application data directory; on macOS that is:

```text
~/Library/Application Support/crime/crm.duckdb
```

To see the exact absolute path on your system, run `crime mcp-config` and read the database path in `args`. You do not need an MCP client to run that command.

Choose a different database with either of these:

```sh
crime --db "$HOME/Documents/my-crm.duckdb"
CRIME_DB="$HOME/Documents/my-crm.duckdb" crime
```

Use the same absolute database path in the TUI, scripts, and MCP configuration if you want them to share records. Different paths create separate CRMs. A convenient way to explore safely is to launch a separate practice database:

```sh
crime --db "$HOME/Documents/crime-practice.duckdb"
```

DuckDB allows only one writer process for a database. Exit the TUI before running a CLI command or letting an MCP client launch another server against that file. Stop the MCP server in your client before reopening the TUI.

### Back up your records

For a complete backup, stop all CRiMe processes and copy the database file to a safe location. A cleanly closed database includes records, custom categories, schemas and revisions, views, preferences, trash, activity history, and merge archives. To restore, keep a copy of your current database, then replace it with the backup while CRiMe is stopped.

`crime export > records.json` exports active records only. It is useful for inspecting or moving record data, but is **not a full backup**: it omits schemas, views, trash, history, preferences, and merge archives. CSV import does not restore a JSON database export. Backup and restore are currently manual.

## Troubleshooting

| Problem | What to try |
| --- | --- |
| `crime: command not found` | Run `export PATH="$HOME/.local/bin:$PATH"`, or launch `~/.local/bin/crime` directly. If you used another prefix, use its `bin` directory. |
| A build reports a missing compiler or linker | Install the platform C/C++ build tools, reopen the terminal, and run `cargo build --release --locked` again. |
| Rust is too old for a dependency | Update your stable toolchain with `rustup update stable`, then rebuild. |
| The first build takes a long time | DuckDB compiles from source. Allow the build to finish; subsequent builds usually reuse it. |
| “Cannot open CRM database” or a lock error | Close the TUI, other CLI processes, or the MCP server using that database. |
| Records appear to be missing | Check the selected category and search filter, then verify that you are using the same database path. Look in Trash with `b` if you deleted a record. |
| Alt shortcuts do not work | Your terminal may reserve them. Use Tab, single-letter actions, and `?` for alternatives. On macOS, Option may need to be configured as Meta/Alt in your terminal. |
| F10 changes volume or brightness | Hold Fn while pressing F10, or save with Ctrl-X then Ctrl-S. |
| The terminal says it is too small | Enlarge the window; the main browser needs at least 45 columns by 10 rows. Import and other tools need more space. |
| A record will not save | Check required fields, dropdown choices, and date format. To leave a changed form, press Esc and choose whether to discard the draft. |
| The assistant cannot start CRiMe | Regenerate `crime mcp-config`, check its absolute executable path, and make sure the database is not already open in the TUI. |

## Development

With `just` installed:

```sh
just check          # formatting and Clippy
just test           # unit and CLI/MCP integration tests
just build          # release executable
just test-install   # temporary-prefix installation and MCP registration checks
```

The application uses Rust, Ratatui/Crossterm, and DuckDB. The installation check uses a temporary directory and does not change your real assistant settings.

## License

CRiMe is available under the [MIT License](LICENSE). You may use, modify, and redistribute it under that license's terms.
