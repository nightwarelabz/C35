//! World persistence: the save file, and the rules
//! around it.
//!
//! DeBrief persists at city scale. Corpses persist —
//! discovery is a system, so a body left in an alley is
//! still there when the next agent walks past. A dead
//! operator's career ends, and a new agent inherits the
//! persistent city, not a fresh one. And the Archive —
//! the compendium — files entries append-only at
//! debrief and keeps them across dead agents.
//!
//! This module is the engine layer serving all of that.
//! It serializes state; it does not own any system:
//!
//! - The host assembles a [`Snapshot`] from its systems
//!   (city generation, actors, the op record, the agent
//!   lifecycle) and calls [`Snapshot::save`]. Bytes come
//!   back deterministic: same snapshot, same bytes.
//! - The host stores those bytes wherever it stores
//!   files — paths, atomic writes (temp file + rename),
//!   backups, and rotation are host jobs, not engine
//!   jobs. The engine never touches the filesystem.
//! - [`Snapshot::load`] refuses anything malformed,
//!   corrupt, truncated, or from a future version, with
//!   a plain error. It never returns a partial snapshot
//!   and never silently upgrades or coerces: a future
//!   version is a refusal, and the host decides what to
//!   tell the player.
//!
//! The journal/Archive split is structural, not a
//! convention:
//!
//! - The journal is the running op record: appended
//!   during play, filed whole at debrief by
//!   [`Snapshot::file_journal`].
//! - The Archive is append-only by construction — its
//!   entry list is private, with append and read as the
//!   only operations. There is no edit path and no
//!   delete path, so a filed entry cannot be rewritten
//!   from anywhere, including the host.
//! - The career boundary ([`Snapshot::close_career`])
//!   files the journal, ends the career with the host's
//!   reason word, opens the next career with the next
//!   agent number, and carries the city — world state
//!   and entities — over untouched.
//!
//! What this module deliberately does NOT do:
//!
//! - Compression: a snapshot is small plain data; if
//!   snapshots ever grow large enough to matter, a
//!   compression layer wraps `save` output, it does not
//!   change the format.
//! - Encryption or authentication: the checksum catches
//!   corruption, not tampering. Anyone can recompute
//!   it. A signed or sealed save is a later layer.
//! - Cloud sync, conflict merges, or multi-device
//!   reconciliation: one file, one writer, host-owned.
//! - Migration between versions: a loader that refuses
//!   unknown versions plainly is more honest than one
//!   that guesses. When version 2 exists, it will carry
//!   its own explicit migration, not a silent coercion.
//! - Incremental or delta saves: every save is a whole
//!   snapshot. Simplicity is the corruption resistance.

pub mod error;
pub mod format;
pub mod model;

pub use error::PersistError;
pub use format::{crc32, FORMAT_VERSION, MAGIC};
pub use model::{
    Archive, ArchiveEntry, Career, CareerStatus, EntityState, JournalEntry, Snapshot, WorldDelta,
    WorldState,
};
