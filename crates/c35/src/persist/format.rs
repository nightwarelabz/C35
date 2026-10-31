//! The save envelope: magic, version, length, body, checksum.
//!
//! Layout, all integers little-endian:
//!
//! ```text
//! magic     4 bytes  "C35S"
//! version   u16      FORMAT_VERSION (1)
//! body_len  u32      length of the body that follows
//! body      bytes    the snapshot, field by field, fixed order
//! crc       u32      CRC-32 (IEEE) over everything before it
//! ```
//!
//! Load order is deliberate: magic, then version, then
//! length, then checksum, then strict body decode. A file
//! stamped with a future version is refused as a version
//! error even if its checksum is stale — the version gate
//! fires first, so the refusal names the real problem.
//!
//! The checksum catches corruption and truncation. It is
//! NOT authentication: anyone can recompute it, so a
//! tampered file with a fresh checksum still loads. Signing
//! or encryption is a later layer, named as left out in
//! the module docs, never implied here.
//!
//! Determinism: the body is a fixed field order, lengths
//! are explicit, floats are written by bit pattern (NaN
//! canonicalized, so a NaN always encodes the same way),
//! and no hash map is ever serialized — order is always
//! insertion order. Same snapshot, same bytes, every time.

use super::error::PersistError;

/// Magic bytes opening every save file.
pub const MAGIC: [u8; 4] = *b"C35S";

/// The only version this loader writes and reads.
pub const FORMAT_VERSION: u16 = 1;

/// CRC-32 (IEEE, polynomial 0xEDB88320) over `data`.
///
/// Computed from a table built at compile time — no
/// dependency, no runtime table construction, no state.
pub fn crc32(data: &[u8]) -> u32 {
    const fn build_table() -> [u32; 256] {
        let mut table = [0u32; 256];
        let mut i = 0usize;
        while i < 256 {
            let mut c = i as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
                k += 1;
            }
            table[i] = c;
            i += 1;
        }
        table
    }
    const TABLE: [u32; 256] = build_table();
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = TABLE[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

/// Canonical quiet-NaN bit pattern used when writing any
/// float, so a NaN always encodes identically.
const CANON_NAN_F32: u32 = 0x7FC0_0000;
const CANON_NAN_F64: u64 = 0x7FF8_0000_0000_0000;

/// Append-only byte writer for the body.
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Writer { buf: Vec::new() }
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn bool(&mut self, v: bool) {
        self.buf.push(v as u8);
    }

    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn f32(&mut self, v: f32) {
        let bits = if v.is_nan() {
            CANON_NAN_F32
        } else {
            v.to_bits()
        };
        self.buf.extend_from_slice(&bits.to_le_bytes());
    }

    pub fn f64(&mut self, v: f64) {
        let bits = if v.is_nan() {
            CANON_NAN_F64
        } else {
            v.to_bits()
        };
        self.buf.extend_from_slice(&bits.to_le_bytes());
    }

    /// A string as a u32 length prefix plus UTF-8 bytes.
    pub fn string(&mut self, v: &str) {
        self.u32(v.len() as u32);
        self.buf.extend_from_slice(v.as_bytes());
    }

    /// Raw bytes as a u32 length prefix plus the bytes.
    pub fn bytes(&mut self, v: &[u8]) {
        self.u32(v.len() as u32);
        self.buf.extend_from_slice(v);
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

/// Bounds-checked cursor over a body slice.
///
/// Nothing here allocates from an untrusted length: vectors
/// and strings are built by pushing checked pieces, so a
/// hostile length prefix yields `Overrun`, never a huge
/// allocation.
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    /// Bytes consumed so far; must equal the body length
    /// when decoding finishes, or the body has trailing data.
    pub fn pos(&self) -> usize {
        self.pos
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], PersistError> {
        let end = self.pos.checked_add(n).ok_or(PersistError::Overrun)?;
        if end > self.buf.len() {
            return Err(PersistError::Overrun);
        }
        let out = &self.buf[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    pub fn u8(&mut self) -> Result<u8, PersistError> {
        Ok(self.take(1)?[0])
    }

    pub fn bool(&mut self) -> Result<bool, PersistError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            d => Err(PersistError::InvalidEnum(d)),
        }
    }

    pub fn u16(&mut self) -> Result<u16, PersistError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn u32(&mut self) -> Result<u32, PersistError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn u64(&mut self) -> Result<u64, PersistError> {
        let b = self.take(8)?;
        let mut out = [0u8; 8];
        out.copy_from_slice(b);
        Ok(u64::from_le_bytes(out))
    }

    pub fn f32(&mut self) -> Result<f32, PersistError> {
        Ok(f32::from_bits(self.u32()?))
    }

    pub fn f64(&mut self) -> Result<f64, PersistError> {
        Ok(f64::from_bits(self.u64()?))
    }

    pub fn string(&mut self) -> Result<String, PersistError> {
        let len = self.u32()? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| PersistError::InvalidUtf8)
    }

    pub fn bytes(&mut self) -> Result<Vec<u8>, PersistError> {
        let len = self.u32()? as usize;
        Ok(self.take(len)?.to_vec())
    }
}

/// Wrap a body in the envelope: magic, version, length,
/// body, checksum.
pub fn wrap(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 2 + 4 + body.len() + 4);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    let crc = crc32(&out);
    out.extend_from_slice(&crc.to_le_bytes());
    out
}

/// Unwrap an envelope, checking magic, version, length,
/// and checksum in that order. Returns the body slice on
/// success; the caller decodes it strictly and must confirm
/// the cursor ends exactly at the body end.
pub fn unwrap(data: &[u8]) -> Result<&[u8], PersistError> {
    // Smallest possible file: magic + version + len + crc.
    if data.len() < 14 {
        return Err(PersistError::Truncated);
    }
    if data[0..4] != MAGIC {
        return Err(PersistError::BadMagic);
    }
    let version = u16::from_le_bytes([data[4], data[5]]);
    if version != FORMAT_VERSION {
        return Err(PersistError::UnsupportedVersion(version));
    }
    let body_len = u32::from_le_bytes([data[6], data[7], data[8], data[9]]) as usize;
    // Total must be exactly header + body + checksum.
    let total = 10usize
        .checked_add(body_len)
        .and_then(|t| t.checked_add(4))
        .ok_or(PersistError::Overrun)?;
    if data.len() < total {
        return Err(PersistError::Truncated);
    }
    if data.len() > total {
        return Err(PersistError::TrailingData);
    }
    let body = &data[10..10 + body_len];
    let stored = u32::from_le_bytes([
        data[10 + body_len],
        data[11 + body_len],
        data[12 + body_len],
        data[13 + body_len],
    ]);
    if crc32(&data[..10 + body_len]) != stored {
        return Err(PersistError::ChecksumMismatch);
    }
    Ok(body)
}
