use std::convert::TryInto;
use std::fmt;
use std::io::Write;

use byteorder::{LittleEndian, WriteBytesExt};
use bytes::{Buf, Bytes};

use crate::{Error, Result};

/// The representation of a RIFF chunk
#[derive(Clone, PartialEq)]
pub struct RiffChunk {
    id: [u8; 4],
    content: RiffContent,
}

// The contents of a RIFF chunk
#[derive(Clone, PartialEq)]
pub enum RiffContent {
    List {
        kind: Option<[u8; 4]>,
        subchunks: Vec<RiffChunk>,
    },
    Data(Bytes),
}

#[allow(clippy::len_without_is_empty)]
impl RiffChunk {
    /// Construct a new RIFF chunk.
    #[inline]
    pub fn new(id: [u8; 4], content: RiffContent) -> RiffChunk {
        RiffChunk { id, content }
    }

    /// Create a new `RiffChunk` from a Reader.
    ///
    /// # Errors
    ///
    /// This method fails if reading fails or if the first chunk doesn't have
    /// an id of "RIFF"
    #[inline]
    pub fn from_bytes(mut b: Bytes) -> Result<RiffChunk> {
        RiffChunk::from_bytes_impl(&mut b, true)
    }

    pub(crate) fn from_bytes_impl(b: &mut Bytes, check_riff_id: bool) -> Result<RiffChunk> {
        let mut id = [0u8; 4];
        b.copy_to_slice(&mut id);

        if check_riff_id && id != *b"RIFF" {
            return Err(Error::NoRiffHeader);
        }

        let content = RiffContent::from_bytes(b, id)?;
        Ok(RiffChunk::new(id, content))
    }

    /// Get the id of this `RiffChunk`
    #[inline]
    pub fn id(&self) -> [u8; 4] {
        self.id
    }

    /// Get the content of this `RiffChunk`
    #[inline]
    pub fn content(&self) -> &RiffContent {
        &self.content
    }

    /// Get a mutable reference to the content of this `RiffChunk`
    #[inline]
    pub fn content_mut(&mut self) -> &mut RiffContent {
        &mut self.content
    }

    /// Get the total size of this `RiffChunk` once it is encoded.
    ///
    /// The size is the sum of:
    ///
    /// - The chunk id (4 bytes).
    /// - The size field (4 bytes).
    /// - The size of the content + a single padding byte if the size is odd.
    pub fn len(&self) -> u32 {
        let mut len = 4 + 4 + self.content.len();

        // RIFF chunks with an uneven number of bytes have an extra 0x00 padding byte
        len += len % 2;

        len
    }

    /// Encode this `RiffChunk` and write it to a Writer.
    pub fn write_to(&self, w: &mut dyn Write) -> Result<()> {
        w.write_all(&self.id)?;
        w.write_u32::<LittleEndian>(self.content.len())?;
        self.content.write_to(w)
    }
}

#[allow(clippy::len_without_is_empty)]
impl RiffContent {
    fn from_bytes(b: &mut Bytes, id: [u8; 4]) -> Result<RiffContent> {
        let len = b.get_u32_le();
        let mut content = b.split_to(len as usize);

        if has_subchunks(id) {
            let kind = if has_kind(id) {
                let mut buf = [0u8; 4];
                content.copy_to_slice(&mut buf);

                Some(buf)
            } else {
                None
            };

            let mut subchunks = Vec::new();
            while !content.is_empty() {
                let subchunk = RiffChunk::from_bytes_impl(&mut content, false)?;
                subchunks.push(subchunk);
            }

            Ok(RiffContent::List { kind, subchunks })
        } else {
            // RIFF chunks with an uneven number of bytes have an extra 0x00 padding byte
            b.advance((len % 2) as usize);

            Ok(RiffContent::Data(content))
        }
    }

    /// Get the total size of this `RiffContent` once it is encoded.
    ///
    /// If this `RiffContent` is a `List` the size is the sum of:
    ///
    /// - The kind (4 bytes) if this `List` has a kind.
    /// - The sum of the size of every `subchunk`.
    ///
    /// If this `RiffContent` is `Data` the size is the length of the data.
    pub fn len(&self) -> u32 {
        match self {
            RiffContent::List { kind, subchunks } => {
                let mut len = 0;

                if kind.is_some() {
                    len += 4;
                }

                len += subchunks.iter().map(|subchunk| subchunk.len()).sum::<u32>();
                len
            }
            RiffContent::Data(data) => data.len().try_into().unwrap(),
        }
    }

    /// Get `kind` and `subchunks` of this `RiffContent` if it is a `List`.
    ///
    /// Returns `None` if it is `Data`.
    pub fn list(&self) -> Option<(&Option<[u8; 4]>, &Vec<RiffChunk>)> {
        match self {
            RiffContent::List {
                ref kind,
                ref subchunks,
            } => Some((kind, subchunks)),
            RiffContent::Data(_) => None,
        }
    }

    /// Get the `data` of this `RiffContent` if it is `Data`.
    ///
    /// Returns `None` if it is a `List`.
    pub fn data(&self) -> Option<Bytes> {
        match self {
            RiffContent::List { .. } => None,
            RiffContent::Data(data) => Some(data.clone()),
        }
    }

    /// Encode this `RiffContent` and write it to a Writer.
    pub fn write_to(&self, w: &mut dyn Write) -> Result<()> {
        match self {
            RiffContent::List { kind, subchunks } => {
                if let Some(kind) = kind {
                    w.write_all(kind)?;
                }

                for chunk in subchunks {
                    chunk.write_to(w)?;
                }
            }
            RiffContent::Data(data) => {
                w.write_all(data)?;

                if data.len() % 2 != 0 {
                    w.write_u8(0x00)?;
                }
            }
        };

        Ok(())
    }
}

impl fmt::Debug for RiffChunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RiffChunk").field("id", &self.id).finish()
    }
}

fn has_subchunks(id: [u8; 4]) -> bool {
    match &id {
        b"RIFF" | b"LIST" | b"seqt" => true,
        _ => false,
    }
}

fn has_kind(id: [u8; 4]) -> bool {
    match &id {
        b"RIFF" | b"LIST" => true,
        _ => false,
    }
}
