# CRiMe

A single-user Rust CRM with a terminal interface, command line, and stdio MCP server. Stores contacts, companies, deals, follow-up tasks, and activity notes in a local DuckDB file. Crossterm handles terminal input; Ratatui and TachyonFX render the interface; Tokio drives the MCP transport.

Requires Rust (edition 2024), a C++ compiler for bundled DuckDB, Python 3 for installation, and optionally `just`. The first build compiles DuckDB and can take several minutes.

```sh
cargo test --locked
just build
just install                 # ~/.local/bin/crime
just install /opt/my-prefix   # /opt/my-prefix/bin/crime
```

Add the selected prefix's `bin` directory to PATH. Installation prints the MCP server configuration. No administrator access is required for the default prefix. For a prefix you do not own, choose a writable directory or arrange its permissions first.

Run `crime` or `crime tui` to open the terminal. Records appear as named entries and labeled details, including readable tags, nested fields, multiline notes, and linked contact/company names. JSON remains available in CLI/MCP output and advanced commands.

Use `C-n` / `C-p` (Ctrl-N / Ctrl-P) to move, Tab / Shift-Tab to switch categories, or `M-1` through `M-6` to jump directly to All, Contacts, Companies, Deals, Tasks, or Notes, and `C-s` or `/` for live search. Press `n` to open a new-record form, `e` to edit all fields in the selected record, `s` to define that category’s reusable schema, and `M-x` (Alt-X) for a searchable action palette. `:` keeps the advanced command input. With the records panel focused, plain `d` immediately moves the selected record to trash; the category and search filter stay in place. `?`, F1, or `C-h` opens the scrollable key guide. `C-x o` switches focus between records and details; `C-v` / `M-v` or Page Down / Up scroll details. `C-g` cancels the current action or clears a filter. `C-x C-c` or `q` quits from browse mode. Standalone `C-c` is reserved for future application bindings and preserves the current action and input. Escape cancels; it does not quit.

`M-x` opens a searchable command palette in the bottom quarter of the terminal, keeping the current screen visible above it, with action descriptions and shortcuts. Search for import, trash, schema, a theme name, or a custom category; Enter runs the selected action. `:` remains available for advanced commands.

Press `r` to browse records linked to the current record in either direction, including custom types. Enter follows the selected relationship. `B` returns to the previous record with its category, search, saved view, and scroll position restored; repeated jumps keep a back stack.

Use Space to select/unselect a record, `A` to select all visible records, and `C` to clear the selection. Checkmarks identify selected entries; selections persist across filters and categories. `U` opens bulk actions: `u` updates a named field (or `name`), `d` moves the selection to trash after `y` confirmation, and `e` exports the selected records to a new JSON file. Text values can be typed directly; numbers, booleans and lists use JSON syntax. Updates and trash operations are atomic and validate every selected record before committing. Existing export files are preserved.

Select the record you want to keep and press `M` to merge a duplicate of the same type. Choose the duplicate, press Enter to review fields and conflicts, then F10 to commit. The survivor keeps its name and conflicting values; missing values are filled and lists are combined. Links in active records and trash recovery copies point to the survivor. Activity history is retained, and the duplicate's original record is archived separately from Trash. `crime run merges` shows these original snapshots. Merge is not undone by `u`; review the preview first. These actions are also searchable in the bottom command palette.

```sh
crime related RECORD_ID
crime merge SURVIVOR_ID DUPLICATE_ID --preview
crime merge SURVIVOR_ID DUPLICATE_ID
crime bulk update '["ID1","ID2"]' --data '{"status":"active"}'
crime bulk export '["ID1","ID2"]'
crime bulk trash '["ID1","ID2"]'
```

MCP/shared commands use `related ID`, `merge-preview SURVIVOR_ID SOURCE_ID`, `merge SURVIVOR_ID SOURCE_ID`, `merges`, and `bulk update|trash|export IDS_JSON [PATCH_JSON]`. Record-only exports omit the merge archive; database backups include it.

`d` still removes the current record immediately from the active list, but now `u` restores the most recent deletion. `b` opens Trash, where Enter or `r` restores a chosen record with its original ID, fields, schema version, timestamps, and links. Removing a recovery copy with `d` in Trash requires `y` confirmation. Activity history remains even after a trash copy is removed. Record-only export excludes trash; database-file backups include it.

Press `c` or choose **New record type** in the palette to create a category such as Projects or Vendors. Give it a display name and stable lowercase ID; its schema editor opens next. Custom types work in forms, links, views, CSV imports, CLI and MCP. Tab cycles all categories, `M-1` through `M-9` select the first nine (including All), and the palette can select any category.

In the schema editor, `F6` opens field rules: Required, dropdown choices (Text fields, one choice per line using `C-j`), and descriptions shown during data entry. `F7` / `F9` move the field up/down. Types, rules, descriptions and order persist with the schema version. A required field may have no default; users must fill it before saving. Existing records keep their applied schema until explicitly migrated. New records created through CLI/MCP receive schema defaults and validation too.

Press `i` or choose **Import CSV** in the palette. Choose a file and record type, review automatic mappings, and press Enter on a column to change its destination or ignore it. One column must map to Name/title. Choose a duplicate match field and a policy: Skip, Merge mapped nonblank values, or Keep both. `F10` first shows a preview with counts, sample values and row errors; `F10` on the preview commits the import only when all rows validate. Escape returns to mappings. Dates use YYYY-MM-DD, yes/no values accept yes/no, true/false or 1/0, list cells use semicolons, and links match existing record names or IDs. Quoted CSV fields and multiline values are supported. Imports are atomic; no partial rows are saved on failure. Merge preserves unmapped existing values; ambiguous matches are rejected.

```sh
crime types
crime type-add project Projects
crime schema project
crime trash
crime undo
crime restore RECORD_ID
crime import-csv people.csv --kind contact --preview
crime import-csv people.csv --kind contact --duplicates merge --match-field email
crime import-csv people.csv --kind contact --name-column 'Full Name' --map 'Email Address=email'
```

The shared MCP/`run` commands include `types`, `type-add ID LABEL`, `schema KIND`, `schema-save KIND JSON`, `trash`, `undo`, `restore ID`, `trash-purge ID`, and `import-preview PATH KIND CONFIG_JSON` / `import-csv PATH KIND CONFIG_JSON`. A schema document contains `version` (expected current version), `template`, `hints`, and `controls`; read `schema KIND` before modifying it. CSV config contains `kind`, `mapping` (zero-based column indexes to `name` or `/data/field` paths), `duplicate_field`, and `duplicates` (`skip`, `merge`, `keep_both`). CLI `--map 'Header=-'` ignores a column; full `/data/...` paths map nested fields. Import previews are recomputed for CLI/MCP requests; the TUI checks that records/schema have not changed before committing its preview.

The TUI opens on **Today**. Press `g` or `M-0` to return to it. It lists overdue and due-today tasks, the next seven days, tasks without dates, invalid dates to fix, open deals without a follow-up task, stale deals, and contacts needing attention. Completed/cancelled tasks and won/lost deals are excluded. `x` completes the selected task; Enter or `e` edits it. `M-1` through `M-6` still open the ordinary record categories.

Press `l` on a contact (or another record) for its unified timeline: linked notes, tasks, deals, open follow-ups, and recorded changes, including deleted linked records. `a` logs a linked activity note; `f` schedules a linked follow-up. These open regular forms and return to the parent timeline on save. Notes can use an `occurred_on` date. Today counts linked notes and completed follow-ups when checking contact activity; `last_contacted`, `next_contact`, and `follow_up_days` can override the default 30-day follow-up cadence. Audit history starts with this version; pre-existing records show a creation fallback, not invented historical edits. Activity history is retained when a record is deleted.

Press `v` for saved views or `V` to save the current category/search as a new view. The guided view editor configures record type, a rolling base filter, search text, multiple AND conditions, sorting, and visible fields. Enter edits a setting, `F2` adds a condition, `F3` removes the selected condition, and `F10` saves. Space toggles fields in the visible-columns chooser. Date conditions accept YYYY-MM-DD or `$today`, which updates daily. Starter views include Overdue tasks, Active deals, and Contacts needing follow-up; they can be edited or deleted. Views store rules, not frozen results, and update when records change. Edits made from a view return to that view.

The same workflows work through the CLI and MCP `crm` tool:

```sh
crime today
crime today --date 2026-11-09
crime timeline RECORD_ID
crime views
crime view 'Overdue tasks'
# Shared MCP/command syntax:
crime run view-save '{"name":"Open tasks","kind":"task","filters":[{"field":"status","op":"equals","value":"open"}],"sort":"due","columns":["due","contact_id"]}'
crime run view-delete 'Open tasks'
```

Dates in Today use the local calendar. Contact follow-ups honor a future `next_contact`/`next_contact_date`; otherwise a contact with no recorded interactions is eligible immediately. Stale deals have had no record update in over 30 days. Missing sort values stay last, and numeric values sort numerically. Database-file backups include views and activity history; record-only JSON export does not include those tables.

Data entry is guided on screen; no JSON or documentation is needed. In All, `n` first asks whether to create a contact, company, deal, task, or note. In a specific category it opens that form directly. Until you save a schema, each type starts with suggested fields and fields discovered in existing data. Once saved, your schema fully controls new-record forms, including defaults and field types. `e` opens the full edit form.

In a form, Tab or `C-n/p` selects a field and Enter edits it. Numbers and dates validate with examples, yes/no and status fields offer choices, and links show record names. Text supports Emacs editing and `C-j` for a new line. `F2` always asks for a custom field name, then its type (text, number, yes/no, date, group, list, or record link). `F4` (or Enter on a list) adds a list item separately. Groups and lists expand into editable child fields; `F3` removes a field or item from the draft. Custom field types are remembered, even when a value is not set.

`F10` or `C-x C-s` saves the complete record. Escape or `C-g` backs out of the current step; leaving a changed form offers Keep editing or Discard changes. Nothing is written until Save. Existing nested objects, arrays, numbers, and unknown fields are preserved while editing other fields. The CLI and MCP interfaces continue to accept flexible JSON.

Press `s` in the records view to edit a reusable schema. All asks which record type; a category opens its schema directly. Add named fields with `F2`, remove them with `F3`, change their type with `F5` (clears the draft default), and use Enter to set defaults. `F6` configures required values, dropdown choices and descriptions; `F7`/`F9` adjusts field order. `F10` or `C-x C-s` publishes a new schema version. Starter fields can be removed, and an empty schema is allowed. Saving a schema does not rewrite any records. Fields added inside a normal record form are specific to that record; add reusable fields in the schema editor instead.

Existing records open exactly as saved. Their edit form shows the applied and current schema versions (`0` means unversioned/starter). `F8` previews migration to the latest saved schema: missing fields receive defaults, compatible values are converted, and extra fields are retained. Incompatible values are listed as conflicts; use `F5` and edit the field to resolve them before retrying. Page Up/Down scrolls the preview. Choose Apply migration to draft, then `F10` to commit; cancelling leaves the stored record unchanged. Schemas, their revisions, defaults, and record version markers live in the DuckDB database and are included in a database-file backup. CLI/MCP record JSON includes `schema_version`; flexible JSON commands remain available.

After `C-x`, the status area immediately lists the available next keys: `b` for category, `o` for focus, `C-c` to quit, and `C-g` or Escape to cancel. The pending hint clears when the sequence finishes. The footer shows control-key guidance for the current focus or minibuffer before you use it.

The minibuffer supports `C-a/e` (line start/end), `C-b/f` (character), `M-b/f` (word), `C-d` (delete), `C-k/u` (kill to end/start), `C-w` / `M-d` (kill word), and `C-y` (yank). `C-p/n` recalls command history for the current session. Arrow keys work too. Terminals may reserve some shortcuts; `:` and `/` provide alternatives to Meta-X and Ctrl-S.

Press `T` to cycle Tokyo Night, Dracula, Solarized Dark, and Solarized Light. The choice is saved with this CRM database. You can also run `theme dracula` in the minibuffer. To override the saved choice for a session:

```sh
crime tui --theme dracula
crime tui --theme solarized-light
CRIME_THEME=tokyo-night crime tui
```

Theme palettes follow [Dracula](https://github.com/dracula/dracula-theme), [Solarized](https://ethanschoonover.com/solarized/), and [Tokyo Night](https://github.com/folke/tokyonight.nvim). A true-color terminal gives the best color match.

```sh
crime add company 'Analytical Engines' --data '{"website":"https://example.com"}'
crime run add contact 'Ada Lovelace' '{"email":"ada@example.com","tags":["customer"]}'
crime run list contact Ada
crime run add deal 'Engine order' '{"stage":"qualified","value":1200,"currency":"USD"}'
crime run add task 'Call Ada' '{"due":"2026-12-01","status":"open"}'
crime run add note 'Discovery call' '{"body":"Interested in a demonstration"}'
crime run update RECORD_ID '{"status":"done"}'
crime run get RECORD_ID
crime run delete RECORD_ID
crime run export > backup.json
```

Names are required; additional fields are JSON objects. Link records by putting their IDs in `contact_id` or `company_id`. Saved schemas enforce field types, required values, and dropdown choices on new records and edits at the record’s applied schema version. Extra fields remain allowed; links retain IDs without enforcing referential integrity. `update` merges top-level fields and accepts `name` to rename a record. Deleted records are recoverable from trash. Export returns all records as JSON.

The default database uses the platform's application data directory (`~/Library/Application Support/crime/crm.duckdb` on macOS). Override with `--db /absolute/path/crm.duckdb` or `CRIME_DB`. All three interfaces must use the same database path to share records. DuckDB permits one writer process: exit the TUI before running a separate CLI or MCP process against that database. Back up the database while CRiMe is stopped.

## MCP setup

`crime mcp` is the MCP server; there is no separate server executable to install. It reserves stdout for newline-delimited JSON-RPC. Generate configuration after installing:

```sh
crime mcp-config
crime --db /absolute/path/crm.duckdb mcp-config
```

Merge the generated `mcpServers.crime` entry into your MCP client's JSON settings, preserving existing servers. Claude Desktop on macOS uses `~/Library/Application Support/Claude/claude_desktop_config.json`; other clients can import the same server definition or register its command and arguments. Restart the client after editing configuration.

For clients with a command registration interface, register the absolute installed `crime` executable with arguments `--db /absolute/path/crm.duckdb mcp`. The database flag goes before `mcp`. Use the same absolute database path across clients. The server exposes a `crm` tool taking `{"args":["help"]}` or any documented CRM command array. The client controls tool approval; all data remains in your local database.

Run `just check` for formatting and Clippy, `just test` for persistence and protocol integration tests, and `just test-install` to exercise the installed CLI/MCP in a temporary prefix. No background service is needed.

Register the installed server using your client's supported CLI:

```sh
just install-mcp codex
just install-mcp claude
just install-mcp codex /opt/my-prefix
# Optional explicit database:
python3 scripts/install_mcp.py codex --prefix /opt/my-prefix --db /absolute/path/crm.duckdb
```

The Codex recipe calls `codex mcp add crime -- /absolute/prefix/bin/crime mcp`; Claude Code uses `claude mcp add --transport stdio --scope user crime -- ...`. These commands modify the client's user configuration when you run them. Claude Desktop uses the JSON definition printed by `crime mcp-config` instead. See [Codex MCP documentation](https://learn.chatgpt.com/docs/extend/mcp) and [Claude Code MCP documentation](https://code.claude.com/docs/en/mcp).

Direct CLI commands include `crime add contact Ada --data '{"email":"ada@example.com"}'`, `crime list --kind contact --search Ada`, `crime get ID`, `crime update ID --data '{"phone":"123"}'`, `crime delete ID`, and `crime export`. `crime run` remains available for the shared TUI/MCP command syntax.
