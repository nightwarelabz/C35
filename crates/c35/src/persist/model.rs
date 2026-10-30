//! The snapshot: what a save file holds, and the rules
//! around it.
//!
//! A snapshot is plain data the host assembles from its
//! systems. The engine defines the shape and the rules;
//! it does not reach into any system to fill it:
//!
//! - `WorldState`: the city itself — its generation seed,
//!   the day count, and persistent deltas (a moved body,
//!   a broken shutter, a barricade — the host names each
//!   delta by key and carries its payload as a string).
//! - `EntityState`: one thing standing in the world — an
//!   actor, a body, a placed prop. Position, facing, a
//!   host-vocabulary state word (`"alive"`, `"dead"`,
//!   `"hidden"` — the engine does not judge it), and a
//!   free payload string for the rest.
//! - `JournalEntry`: the running op record. Appended
//!   during play, mutable only in the sense that the
//!   journal grows; entries are filed, never rewritten.
//! - `Archive`: the filed record. Append-only by
//!   construction: the entry list is private, and the
//!   only operations are append and read. There is no
//!   edit path and no delete path anywhere in this file.
//! - `Career`: whose career this is, and whether it is
//!   still running. A career ends exactly once, at the
//!   boundary operation below.
//!
//! Field order in the encoding is fixed per struct and
//! mirrored exactly by the decoder. Adding a field to a
//! struct in a future version means a new format version
//! and an explicit migration — never a silent reinterpret.

use super::error::PersistError;
use super::format::{Reader, Writer};

/// One persistent change to the generated city, named by
/// the host. The engine never interprets `key` or `value`;
/// it only carries them, in insertion order.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldDelta {
    /// Host-chosen key, e.g. `"body.moved.12"`.
    pub key: String,
    /// Host payload for the delta, as text.
    pub value: String,
}

/// The city that persists across careers.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldState {
    /// Generation seed the city was built from.
    pub seed: u64,
    /// Days elapsed on the city timeline.
    pub day: u32,
    /// Persistent modifications, in the order recorded.
    pub deltas: Vec<WorldDelta>,
}

/// One thing standing in the world.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityState {
    /// Stable host id, e.g. `"operator"`, `"body.7"`.
    pub id: String,
    /// Host kind word, e.g. `"operator"`, `"body"`, `"prop"`.
    pub kind: String,
    /// Position in world units.
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Facing, radians.
    pub yaw: f32,
    /// Host-vocabulary state word. The engine stores it,
    /// never judges it.
    pub state: String,
    /// Free payload for everything else the host needs.
    pub data: String,
}

/// One line of the running op record.
#[derive(Debug, Clone, PartialEq)]
pub struct JournalEntry {
    /// Monotonic sequence within the career.
    pub seq: u64,
    /// Day the entry was written.
    pub day: u32,
    /// The entry text.
    pub text: String,
    /// Host tags, e.g. `["crossing", "witnessed"]`.
    pub tags: Vec<String>,
}

/// One filed record. Identical in shape to a journal
/// entry, plus the career and day it was filed under —
/// filing stamps the record, it never rewrites it.
#[derive(Debug, Clone, PartialEq)]
pub struct ArchiveEntry {
    /// Sequence carried over from the journal entry.
    pub seq: u64,
    /// Day the entry was filed at debrief.
    pub filed_day: u32,
    /// Career number the entry belongs to.
    pub career: u32,
    /// The entry text, exactly as written.
    pub text: String,
    /// Tags, exactly as written.
    pub tags: Vec<String>,
}

/// The filed record: append-only by construction.
///
/// `entries` is private. The only operations are
/// `append` and read (`iter`, `len`, `is_empty`). There
/// is no edit path, no delete path, no mutable access —
/// a filed entry cannot be rewritten from anywhere in
/// the engine or the host, because no such call exists.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Archive {
    entries: Vec<ArchiveEntry>,
}

impl Archive {
    pub fn new() -> Self {
        Archive {
            entries: Vec::new(),
        }
    }

    /// File one record. The only way an entry enters.
    pub fn append(&mut self, entry: ArchiveEntry) {
        self.entries.push(entry);
    }

    /// Read the filed record, oldest first.
    pub fn iter(&self) -> std::slice::Iter<'_, ArchiveEntry> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Whether a career is still running, and how it ended.
#[derive(Debug, Clone, PartialEq)]
pub enum CareerStatus {
    /// The agent is still operating.
    Active,
    /// The career ended; the reason is the host's word,
    /// e.g. `"killed"`, `"retired"`, `"extracted"`.
    Ended(String),
}

/// Whose career this snapshot belongs to.
#[derive(Debug, Clone, PartialEq)]
pub struct Career {
    /// Career number, starting at 1, rising by one at
    /// every boundary. Never reused, never reset.
    pub agent_no: u32,
    /// The agent's display name for this career.
    pub agent_name: String,
    pub status: CareerStatus,
}

/// Everything one save file holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub world: WorldState,
    pub entities: Vec<EntityState>,
    pub journal: Vec<JournalEntry>,
    pub archive: Archive,
    pub career: Career,
}

impl Snapshot {
    /// A fresh snapshot: a city at `day`, an active first
    /// career, an empty journal, an empty archive.
    pub fn new(world_seed: u64, day: u32, agent_name: &str) -> Self {
        Snapshot {
            world: WorldState {
                seed: world_seed,
                day,
                deltas: Vec::new(),
            },
            entities: Vec::new(),
            journal: Vec::new(),
            archive: Archive::new(),
            career: Career {
                agent_no: 1,
                agent_name: agent_name.to_string(),
                status: CareerStatus::Active,
            },
        }
    }

    /// Append one line to the running op record.
    pub fn journal_push(&mut self, seq: u64, day: u32, text: &str, tags: &[&str]) {
        self.journal.push(JournalEntry {
            seq,
            day,
            text: text.to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
        });
    }

    /// File the whole journal into the Archive, in order.
    /// Each entry is stamped with the career it belongs
    /// to and the day it was filed; the journal is then
    /// empty. Filing never rewrites an entry's text or
    /// tags — it only adds the filing stamp.
    ///
    /// Returns the number of entries filed.
    pub fn file_journal(&mut self, filed_day: u32) -> usize {
        let career = self.career.agent_no;
        let count = self.journal.len();
        for entry in self.journal.drain(..) {
            self.archive.append(ArchiveEntry {
                seq: entry.seq,
                filed_day,
                career,
                text: entry.text,
                tags: entry.tags,
            });
        }
        count
    }

    /// File one record directly, outside the journal —
    /// for findings that reach the Archive without passing
    /// through the op record. Still append-only.
    pub fn archive_append(&mut self, seq: u64, filed_day: u32, text: &str, tags: &[&str]) {
        self.archive.append(ArchiveEntry {
            seq,
            filed_day,
            career: self.career.agent_no,
            text: text.to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
        });
    }

    /// The career boundary, as pure state transitions:
    ///
    /// 1. The running journal is filed into the Archive
    ///    (stamped with the ending career and the current
    ///    world day), so the Archive grows by exactly the
    ///    journal's length.
    /// 2. The ending career is marked `Ended(reason)`.
    /// 3. A new career opens with the next agent number
    ///    and an empty journal.
    /// 4. The city — world state and entities — is carried
    ///    over untouched: the new agent inherits the
    ///    persistent city, bodies and all.
    ///
    /// No file IO happens here; the host saves the
    /// returned snapshot with `save` when it chooses.
    /// Returns the new career's agent number.
    pub fn close_career(&mut self, reason: &str, next_agent_name: &str) -> u32 {
        // Step 1: file the journal. `file_journal`
        // returns the count filed; the demo asserts the
        // Archive grew by exactly that many.
        self.file_journal(self.world.day);
        self.career.status = CareerStatus::Ended(reason.to_string());
        let next_no = self.career.agent_no + 1;
        self.career = Career {
            agent_no: next_no,
            agent_name: next_agent_name.to_string(),
            status: CareerStatus::Active,
        };
        next_no
    }

    /// Serialize this snapshot to deterministic bytes.
    pub fn save(&self) -> Vec<u8> {
        let mut w = Writer::new();
        self.encode(&mut w);
        super::format::wrap(&w.into_bytes())
    }

    /// Load a snapshot, refusing anything malformed,
    /// corrupt, truncated, or from a future version.
    /// Never returns a partial snapshot.
    pub fn load(data: &[u8]) -> Result<Snapshot, PersistError> {
        let body = super::format::unwrap(data)?;
        let mut r = Reader::new(body);
        let snap = Snapshot::decode(&mut r)?;
        if r.pos() != body.len() {
            return Err(PersistError::TrailingData);
        }
        Ok(snap)
    }

    fn encode(&self, w: &mut Writer) {
        // World state.
        w.u64(self.world.seed);
        w.u32(self.world.day);
        w.u32(self.world.deltas.len() as u32);
        for d in &self.world.deltas {
            w.string(&d.key);
            w.string(&d.value);
        }
        // Entities, in order.
        w.u32(self.entities.len() as u32);
        for e in &self.entities {
            w.string(&e.id);
            w.string(&e.kind);
            w.f32(e.x);
            w.f32(e.y);
            w.f32(e.z);
            w.f32(e.yaw);
            w.string(&e.state);
            w.string(&e.data);
        }
        // Journal, in order.
        w.u32(self.journal.len() as u32);
        for j in &self.journal {
            w.u64(j.seq);
            w.u32(j.day);
            w.string(&j.text);
            w.u32(j.tags.len() as u32);
            for t in &j.tags {
                w.string(t);
            }
        }
        // Archive, oldest first. Append-only on the
        // read side too: decode rebuilds by appending.
        w.u32(self.archive.len() as u32);
        for a in self.archive.iter() {
            w.u64(a.seq);
            w.u32(a.filed_day);
            w.u32(a.career);
            w.string(&a.text);
            w.u32(a.tags.len() as u32);
            for t in &a.tags {
                w.string(t);
            }
        }
        // Career.
        w.u32(self.career.agent_no);
        w.string(&self.career.agent_name);
        match &self.career.status {
            CareerStatus::Active => w.u8(0),
            CareerStatus::Ended(reason) => {
                w.u8(1);
                w.string(reason);
            }
        }
    }

    fn decode(r: &mut Reader) -> Result<Snapshot, PersistError> {
        let seed = r.u64()?;
        let day = r.u32()?;
        let n_deltas = r.u32()? as usize;
        let mut deltas = Vec::new();
        for _ in 0..n_deltas {
            deltas.push(WorldDelta {
                key: r.string()?,
                value: r.string()?,
            });
        }
        let n_entities = r.u32()? as usize;
        let mut entities = Vec::new();
        for _ in 0..n_entities {
            entities.push(EntityState {
                id: r.string()?,
                kind: r.string()?,
                x: r.f32()?,
                y: r.f32()?,
                z: r.f32()?,
                yaw: r.f32()?,
                state: r.string()?,
                data: r.string()?,
            });
        }
        let n_journal = r.u32()? as usize;
        let mut journal = Vec::new();
        for _ in 0..n_journal {
            let seq = r.u64()?;
            let jday = r.u32()?;
            let text = r.string()?;
            let n_tags = r.u32()? as usize;
            let mut tags = Vec::new();
            for _ in 0..n_tags {
                tags.push(r.string()?);
            }
            journal.push(JournalEntry {
                seq,
                day: jday,
                text,
                tags,
            });
        }
        let n_archive = r.u32()? as usize;
        let mut archive = Archive::new();
        for _ in 0..n_archive {
            let seq = r.u64()?;
            let filed_day = r.u32()?;
            let career = r.u32()?;
            let text = r.string()?;
            let n_tags = r.u32()? as usize;
            let mut tags = Vec::new();
            for _ in 0..n_tags {
                tags.push(r.string()?);
            }
            archive.append(ArchiveEntry {
                seq,
                filed_day,
                career,
                text,
                tags,
            });
        }
        let agent_no = r.u32()?;
        let agent_name = r.string()?;
        let status = match r.u8()? {
            0 => CareerStatus::Active,
            1 => CareerStatus::Ended(r.string()?),
            d => return Err(PersistError::InvalidEnum(d)),
        };
        Ok(Snapshot {
            world: WorldState {
                seed,
                day,
                deltas,
            },
            entities,
            journal,
            archive,
            career: Career {
                agent_no,
                agent_name,
                status,
            },
        })
    }
}
