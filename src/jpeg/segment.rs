use std::convert::TryInto;
use std::fmt;
use std::io::{self, Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use super::entropy::Entropy;
use super::markers::{self, has_entropy, has_length};
use crate::{Result, EXIF_DATA_PREFIX};

const ICC_DATA_PREFIX: &[u8] = b"ICC_PROFILE\0";

/// The representation of a single segment composing a Jpeg image.
#[derive(Clone, PartialEq)]
pub struct JpegSegment {
    marker: u8,
    contents: Vec<u8>,
    entropy: Option<Entropy>,
}

#[allow(clippy::len_without_is_empty)]
impl JpegSegment {
    /// Construct an empty `JpegSegment`.
    #[inline]
    pub fn new(marker: u8) -> JpegSegment {
        JpegSegment {
            marker,
            contents: Vec::new(),
            entropy: None,
        }
    }

    /// Construct a `JpegSegment` with `contents`.
    #[inline]
    pub fn new_with_contents(marker: u8, contents: Vec<u8>) -> JpegSegment {
        JpegSegment {
            marker,
            contents,
            entropy: None,
        }
    }

    /// Construct a `JpegSegment` with `contents` and `Etropy`.
    #[inline]
    pub fn new_with_entropy(marker: u8, contents: Vec<u8>, entropy: Entropy) -> JpegSegment {
        JpegSegment {
            marker,
            contents,
            entropy: Some(entropy),
        }
    }

    /// Creates an ICC `JpegSegment`
    pub(super) fn new_icc(seqno: u8, num: u8, buf: &[u8]) -> JpegSegment {
        let mut contents = Vec::with_capacity(ICC_DATA_PREFIX.len() + 2 + buf.len());
        contents.extend(ICC_DATA_PREFIX);
        contents.push(seqno);
        contents.push(num);
        contents.extend(buf);

        JpegSegment::new_with_contents(markers::APP2, contents)
    }

    /// Creates an EXIF `JpegSegment`
    pub(super) fn new_exif(buf: &[u8]) -> JpegSegment {
        let mut contents = Vec::with_capacity(EXIF_DATA_PREFIX.len() + buf.len());
        contents.extend(EXIF_DATA_PREFIX);
        contents.extend(buf);

        JpegSegment::new_with_contents(markers::APP1, contents)
    }

    /// Create a `JpegSegment` with a length from a Reader.
    pub fn read(marker: u8, r: &mut dyn Read) -> Result<JpegSegment> {
        let size = r.read_u16::<BigEndian>()? - 2;

        let mut contents = Vec::with_capacity(size as usize);
        r.take(size as u64).read_to_end(&mut contents)?;

        if !has_entropy(marker) {
            Ok(JpegSegment::new_with_contents(marker, contents))
        } else {
            let entropy = Entropy::read(r)?;
            Ok(JpegSegment::new_with_entropy(marker, contents, entropy))
        }
    }

    /// Get the size of this `JpegSegment` once it is encoded excluding
    /// the `Entropy`.
    ///
    /// The size is the sum of:
    ///
    /// - The marker (2 bytes).
    /// - The length (2 bytes) if this marker has a length.
    /// - The size of the content.
    pub fn len(&self) -> usize {
        if has_length(self.marker) {
            // 2 bytes (marker) + 2 bytes (length) + length of the content
            2 + 2 + self.contents.len()
        } else {
            // 2 bytes (marker) + length of the content
            2 + self.contents.len()
        }
    }

    /// Get the size of this `JpegSegment` once it is encoded including
    /// the `Entropy`.
    ///
    /// The size is the sum of:
    ///
    /// - The marker (2 bytes).
    /// - The length (2 bytes) if this marker has a length.
    /// - The size of the content.
    /// - The size of the encoded entropy data.
    pub fn len_with_entropy(&self) -> usize {
        self.len()
            + self
                .entropy
                .as_ref()
                .map(|entropy| entropy.len())
                .unwrap_or(0)
    }

    /// Get the second byte of the marker of this `JpegSegment`.
    #[inline]
    pub fn marker(&self) -> u8 {
        self.marker
    }

    /// Get the content of this `JpegSegment`.
    #[inline]
    pub fn contents(&self) -> &[u8] {
        self.contents.as_slice()
    }

    /// Check if this `JpegSegment` has entropy.
    #[inline]
    pub fn has_entropy(&self) -> bool {
        self.entropy.is_some()
    }

    /// Returns the ICC segment data if this `JpegSegment` is an ICC segment.
    pub(super) fn icc(&self) -> Option<(u8, u8, &[u8])> {
        if self.marker == markers::APP2
            && self.contents.get(0..ICC_DATA_PREFIX.len()) == Some(ICC_DATA_PREFIX)
        {
            // sequence number (between 1 and N. of sequence numbers inclusive)
            let seqno = *self.contents.get(12)?;
            // number of sequences
            let num = *self.contents.get(13)?;

            let icc = self.contents.get(14..)?;

            Some((seqno, num, icc))
        } else {
            None
        }
    }

    /// Returns the EXIF segment data if this `JpegSegment` is an EXIF segment.
    pub(super) fn exif(&self) -> Option<&[u8]> {
        if self.marker == markers::APP1
            && self.contents.get(..EXIF_DATA_PREFIX.len()) == Some(EXIF_DATA_PREFIX)
        {
            self.contents.get(EXIF_DATA_PREFIX.len()..)
        } else {
            None
        }
    }

    /// Encode this `JpegSegment` and write it to a Writer.
    pub fn write_to(&self, w: &mut dyn Write) -> io::Result<()> {
        w.write_u8(markers::P)?;
        w.write_u8(self.marker())?;
        w.write_u16::<BigEndian>((self.len() - 2).try_into().unwrap())?;
        w.write_all(&self.contents)?;

        if let Some(entropy) = &self.entropy {
            entropy.write_to(w)?;
        }

        Ok(())
    }
}

impl fmt::Debug for JpegSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JpegSegment")
            .field("marker", &self.marker)
            .finish()
    }
}
