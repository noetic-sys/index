# Plan: Global Index Store

Move `.index/` from per-project to a single global store, like `~/.cargo/registry`.
All projects share one index. Package data is already keyed by `(registry, name, version)`
and content-addressed — so sharing is free, there's no per-project data in the store.

Branch: `kyle/global-index-store`

---

## Steps (all complete ✓)

### 1. `src/local/mod.rs` — replace local discovery with global path

- Delete `INDEX_DIR_NAME`, `find_index_root()`, `is_local_mode()`
- Add `get_global_index_dir() -> PathBuf`:
  - Returns `$IDX_DIR` env var if set (override for CI / testing)
  - Otherwise `dirs::data_dir().join("idx")` → `~/Library/Application Support/idx` on macOS,
    `~/.local/share/idx` on Linux
- Replace `get_index_dir() -> Option<PathBuf>` with `get_index_dir() -> PathBuf`
  - Just calls `get_global_index_dir()`, creates dir if missing
  - No more `Option` — global dir always exists (or we bail early with a clear error)
- Update all callers: drop the `.context("No .index directory found...")` unwrap pattern,
  they now always get a `PathBuf`

### 2. `src/local/db.rs` — add project_packages table

New table tracking which project registered each package version:

```sql
CREATE TABLE IF NOT EXISTS project_packages (
    project_id  TEXT NOT NULL,   -- SHA-256 of canonical project root path
    registry    TEXT NOT NULL,
    name        TEXT NOT NULL,
    version     TEXT NOT NULL,
    PRIMARY KEY (project_id, registry, name, version)
)
```

New methods on `LocalDb`:
- `register_project_package(project_id, registry, name, version)` — upsert
- `unregister_project(project_id)` — delete all rows for this project
- `list_project_packages(project_id) -> Vec<(registry, name, version)>` — for clean
- `ref_count(registry, name, version) -> usize` — count distinct project_ids referencing it

Helper in `src/local/mod.rs`:
- `project_id(project_root: &Path) -> String` — SHA-256 hex of `canonicalize(path)`

### 3. `src/commands/init.rs` — record project associations

After successfully indexing each package:
- Compute `project_id` from `self.path.canonicalize()`
- Call `db.register_project_package(project_id, registry, name, version)`

The indexer itself doesn't need to change — `LocalIndexer` just indexes packages.
The `init` command (and `update`, `index`, `watch`) layer the association on top.

> Note: `LocalIndexer` doesn't currently expose the db directly. Add a
> `register_project_package` method to `LocalIndexer` that delegates to its db,
> or expose a `db()` accessor. Probably cleaner to add it to `LocalIndexer`.

### 4. `src/commands/update.rs` + `src/commands/index.rs` + `src/commands/watch.rs`

Same as init: after each successful `index_package` call, register the association.
These commands already have a `project_root` concept (cwd or `self.path`) — use that.

### 5. `src/commands/clean.rs` — ref-counted delete

New behavior:
1. Compute `project_id` from cwd
2. Get `list_project_packages(project_id)` — the packages this project registered
3. For each: check `ref_count(registry, name, version)`
   - If 1 (only this project): delete chunks, blobs, vectors, version row
   - If >1: just unregister this project, data stays for others
4. Call `unregister_project(project_id)` to clean up the associations
5. Print a summary: "Removed N packages, kept M (still used by other projects)"

Remove the old "delete entire .index dir" logic.

### 6. `src/commands/status.rs`

Remove the early-exit "no .index directory found" branch — index dir always exists now.
Show global store path in output.

### 7. `src/manifests/discover.rs`

Remove `".index"` from `SKIP_DIRS` (line 47). It's no longer in project trees.

### 8. Callsite cleanup across all commands

All commands currently do:
```rust
let index_dir = local::get_index_dir().context("No .index directory found. Run `idx init` first.")?;
```

After step 1, `get_index_dir()` returns `PathBuf` (not `Option`), so these all become:
```rust
let index_dir = local::get_index_dir();
```

Commands that should still check for "has this project been initialized" (i.e. has it ever
been indexed?) can check via `db.list_project_packages(project_id).is_empty()` instead.

---

## Files touched

| File | Change |
|------|--------|
| `src/local/mod.rs` | Replace local discovery with global path logic |
| `src/local/db.rs` | Add `project_packages` table + ref-count methods |
| `src/local/indexer.rs` | Add `register_project_package` method |
| `src/commands/init.rs` | Register associations after indexing; create global dir |
| `src/commands/update.rs` | Register associations after indexing |
| `src/commands/index.rs` | Register association after indexing |
| `src/commands/watch.rs` | Register association after indexing |
| `src/commands/clean.rs` | Ref-counted delete instead of rm -rf |
| `src/commands/status.rs` | Remove Option-based index dir check |
| `src/manifests/discover.rs` | Remove `.index` from SKIP_DIRS |

---

## Out of scope

- `--local` flag: skip entirely, use `IDX_DIR` env var for isolation needs
- Migration of existing `.index/` dirs: hard cut, no migration path (early stage)
