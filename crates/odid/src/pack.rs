use crate::{MAX_PACK_MESSAGES, MESSAGE_SIZE};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PackError {
    TooShort,
    NotAPack,
    BadMessageSize(u8),
    TooManyMessages(u8),
    Truncated,
    Empty,
    InvalidContent,
}

pub struct PackIter<'a>(core::slice::Iter<'a, [u8; MESSAGE_SIZE]>);

impl<'a> Iterator for PackIter<'a> {
    type Item = &'a [u8; MESSAGE_SIZE];
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

/// Per-type limits from `checkPackContent` in the reference opendroneid.c.
const MAX_PER_TYPE: [u8; 6] = [2, 1, 16, 1, 1, 1];

pub fn parse_pack(pack: &[u8]) -> Result<PackIter<'_>, PackError> {
    let [header, size, n, rest @ ..] = pack else {
        return Err(PackError::TooShort);
    };
    if header >> 4 != 0xF {
        return Err(PackError::NotAPack);
    }
    if usize::from(*size) != MESSAGE_SIZE {
        return Err(PackError::BadMessageSize(*size));
    }
    if usize::from(*n) > MAX_PACK_MESSAGES {
        return Err(PackError::TooManyMessages(*n));
    }
    if *n == 0 {
        return Err(PackError::Empty);
    }
    let msgs = rest
        .as_chunks::<MESSAGE_SIZE>()
        .0
        .get(..usize::from(*n))
        .ok_or(PackError::Truncated)?;
    let mut counts = [0u8; 6];
    for m in msgs {
        let t = usize::from(m[0] >> 4);
        let (Some(c), Some(max)) = (counts.get_mut(t), MAX_PER_TYPE.get(t)) else {
            return Err(PackError::InvalidContent);
        };
        *c = c.saturating_add(1);
        if *c > *max {
            return Err(PackError::InvalidContent);
        }
    }
    Ok(PackIter(msgs.iter()))
}
