# Tags

Open **Tags** above the history list to create tags, choose colors, rename, delete, or merge tags. **Merge into…** selects the destination; **Merge** moves that row's assignments into it and removes the source tag. Deleting a tag keeps the clips.

**Assign / Unassign** operates on the focused history clip. For bulk operations, select a row and click **Select for tagging**, repeat for other rows, then open the manager. **Clear selection** returns to the focused clip. Assignments are transactional: a missing, expired or sensitive clip rejects the entire operation.

**Filter tags** supports all-selected and any-selected matching. Combine it with a category and a text query; the background worker searches the complete active history. `tag:work` also searches persistent tags. Saved searches retain tag filter IDs, so renaming does not break them. Deleting a tag leaves filters referring to it unmatched until cleared.

Tags use separate `tags` and `clip_tags` tables on both SQLite and DuckDB. Names are trimmed, lowercased and unique; limits are 64 UTF-8 bytes per name, 256 tags, 32 assignments per clip, 1,000 clips per bulk command, and 65,536 assignments per database. Existing clip and frozen export formats are unchanged. The SQLite-to-DuckDB importer copies and verifies both tables. Removing clips removes their assignments.

Tags belong to the currently opened history database. Independent user profiles and groups are not implemented by this change. Existing JSON clip exports do not include tags; a full portable backup format with tag restoration remains separate work. Memory-only and sensitive clips cannot receive persistent tags.
