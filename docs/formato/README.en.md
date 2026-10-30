# The `.bivar` file format, version 1 (draft)

> **Status:** draft, written in phase 0.6 from the prototype in `poc/crdt-armazenamento/prototipo/arquivo`. It becomes the official specification in delivery N.2. Decision: [ADR-0023](../adr/0023.md) (Portuguese). Portuguese version: [README.md](README.md).

This specification is enough to write a reader in any language without looking at Bivar's code. Two record kinds carry the binary format of the Loro library, which is open but belongs to Loro (section 5).

## 1. Overview

- **A single file,** with no side files.
- **A 64-byte header,** followed by a sequence of **records**.
- **Records are only ever appended.** A written record never changes.
- **Every record carries a SHA-256 digest** of its kind, length and payload, so a cut or altered record is detected.
- **Compaction** writes a new file (same format) and puts it in place of the old one in one rename.

All numbers are unsigned integers, **little-endian**.

## 2. Header (64 bytes)

| Offset | Size | Content |
| --- | --- | --- |
| 0 | 8 | ASCII `BIVARDOC` |
| 8 | 4 | format version: `1` |
| 12 | 4 | flags: `0`. Bit 0 is **reserved for encryption**, postponed by the owner. A reader refuses any unknown bit |
| 16 | 16 | reserved: zeros (future encryption parameters) |
| 32 | 32 | SHA-256 of bytes 0 to 31 |

**A reader refuses the file:**

- if it does not start with `BIVARDOC` ("not a Bivar file");
- if the header digest does not match ("damaged header");
- if the version is higher than the reader knows ("written by a newer Bivar"). **It never tries to "repair".**

## 3. Records

| Field | Size | Content |
| --- | --- | --- |
| kind | 1 | one of the kinds in section 4 |
| length | 8 | payload length in bytes; at most the limit of its kind (section 3.1) |
| payload | `length` | depends on the kind |
| digest | 32 | SHA-256 of `kind ‖ length ‖ payload` |

### 3.1 Limits ([ADR-0024](../adr/0024.md), Portuguese)

A reader checks these limits **before** reading or reserving memory for a payload. A file above a limit is refused with a message naming the limit. The user may raise a limit in the settings, only explicitly and with a warning.

| What | Limit |
| --- | --- |
| one snapshot or updates record | 256 MiB |
| all snapshot and updates records together (the document data) | 1 GiB |
| one large-file chunk | 32 bytes + 4 MiB |
| one large-file index | 8 MiB (about 260,000 chunks, 1 TB per large file) |
| one save mark | 8 bytes |
| number of records | 50 million |

**Large files** (image, video, audio) **do not count in the 1 GiB:** they have their own chunk and index limits. The total file size has no limit.

### 3.2 Reading

**How to read:**

1. Read the records in order from byte 64.
2. If the file ends inside a record, or if the **last** record has a wrong digest, that record is a **cut write** (for example, from a crash). Drop it; everything before it stands. A reader that may write removes those bytes from the end.
3. If a record that is **not the last** has a wrong digest, an unknown kind or a length above the limit, the file is **damaged** from there on. Use only what came before, open **read-only**, and tell the user.

## 4. Record kinds

| Kind | Name | Payload |
| --- | --- | --- |
| 1 | Snapshot | the whole document in Loro's snapshot format. Always the first record. Compaction writes a *shallow* snapshot, which keeps history only from a given point |
| 2 | Updates | document changes since the previous updates record, in Loro's updates format. Applied in order after the snapshot |
| 3 | Chunk | 32 bytes: the chunk id (SHA-256 of the data); then up to 4 MiB of a large file's data (image, audio, video). An identical chunk is never stored twice. On opening, a reader may read only the id and skip the data (except in the last record, always checked in full); the data is checked against the id when read |
| 4 | Large-file index | 32 bytes: SHA-256 of the whole file; 8 bytes: total size; then 32 bytes per chunk (the ids, in order) |
| 5 | Save mark | 8 bytes: seconds since 1970, or 0 (no date, in the copy to send). "The user saved here." Updates after the last mark are the "edits since the last save", which the program offers to keep or drop |

**An unknown kind** inside version 1 is treated as damage (section 3). A new kind requires a new format version.

## 5. The document inside the records

Snapshot and updates payloads use the binary format of **Loro 1.17.1** (open library, MIT license, [loro.dev](https://loro.dev)). This dependency is recorded: reading the document requires Loro, or a compatible implementation of its format. For a readable export, the program can write the history as JSON (Loro's `export_json_updates`).

**The document layout inside Loro** ([ADR-0021](../adr/0021.md)):

- **map `meta`:**
  - `doc_id`: the document identity, random (`doc-` and 32 hex digits; it says nothing about who created it or when);
  - `schema`: the layout version, now `2` (version 2 stores the removal mark as entries, with the removal time). A document of another version is refused, never read "as well as possible".
- **tree `tree`:** the nodes. Children order is the stack order. Each node's map holds:
  - `kind`;
  - parameters as entries `p:<name>#<counter>.<copy>` (16 hex digits each number). The parameter's value is the one with the **highest** key;
  - the removal mark as entries `removed#<counter>.<copy>`: a number (seconds since 1970, when removed) means "removed", `false` means "restored";
  - `text`, for text nodes.
- **map `edges`:** one connection per key, as `from␟port␟to␟port` (␟ is character 0x1F).
- **map `copies`:** `<copy>#vv` (the version that copy has seen) and `<copy>#at` (when, in seconds).
- **map `checkpoints`:** time checkpoints, keyed `<seconds>.<copy>.<ops>`, with the version at that moment.
- **map `milestones`:** named milestones, with the name and the version. **Never cleaned.**

## 6. Writing

- **Edits are appended** as updates records while the user works.
- **Forcing data to disk** (`fsync`/`sync_data`) happens:
  - at most **1 second** after an edit;
  - on save;
  - on closing the program;
  - on compaction.

  On a power loss, at most the last second is lost.
- **A program that opens the file for writing prevents another from writing at the same time:**
  - on Windows, by opening it with read-only sharing;
  - elsewhere, with a lock.

  The second program opens read-only, and copying the file still works.
- **Compaction:**
  1. writes the new file as `<name>.compact-tmp` and forces it to disk;
  2. renames the old file to the safety copy `<name>.anterior`;
  3. renames `.compact-tmp` to `<name>`;
  4. opens the new file to check it;
  5. deletes `.anterior`. If the new file does not open well, `.anterior` goes back in place.
- **On opening:**
  - if `<name>` is missing or damaged and `<name>.anterior` exists, the reader offers to recover from it. The damaged file is never deleted: it is kept as `<name>.estragado`;
  - if `<name>` opens well and an `.anterior` is left (compaction stopped after the swap), the `.anterior` is deleted;
  - a leftover `.compact-tmp` is deleted.
- **"Salvar cópia para enviar" (save a copy to send) and "Salvar como" (save as)** first write `<name>.parcial`, force it to disk, and only then rename it to `<name>`. They never overwrite an existing file. A `.parcial` left by a crash is never read as a document nor deleted without asking: the next save under that name stops and warns, and opening `<name>` warns that it is there.
- **The copy to send is an ordinary file in this format,** with a new document identity and a history that starts with it. Nothing in the file says it is a copy.

## 7. Not part of the format

- **Scripts:** opening a file never runs code (`CLAUDE.md` §4).
- **Encryption:** postponed; space is reserved in the header.
