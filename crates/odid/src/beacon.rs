use crate::{ASTM_OUI, ODID_OUI_TYPE};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BeaconRid<'a> {
    pub src_mac: [u8; 6],
    pub msg_counter: u8,
    pub pack: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BeaconError {
    TooShort,
    NotBeacon,
    TruncatedIe,
    NotRemoteId,
}

pub fn parse_beacon(mpdu: &[u8]) -> Result<BeaconRid<'_>, BeaconError> {
    let (Some(head), Some(mut ies)) = (mpdu.first_chunk::<36>(), mpdu.get(36..)) else {
        return Err(BeaconError::TooShort);
    };
    if head[0] != 0x80 {
        return Err(BeaconError::NotBeacon);
    }
    let mut src_mac = [0; 6];
    src_mac.copy_from_slice(&head[10..16]);
    while let [id, len, rest @ ..] = ies {
        let (Some(body), Some(next)) =
            (rest.get(..usize::from(*len)), rest.get(usize::from(*len)..))
        else {
            return Err(BeaconError::TruncatedIe);
        };
        // Stops at the first match, so a malformed element after it cannot reject a valid pack.
        if *id == 0xDD
            && let [a, b, c, ty, msg_counter, pack @ ..] = body
            && [*a, *b, *c] == ASTM_OUI
            && *ty == ODID_OUI_TYPE
        {
            return Ok(BeaconRid {
                src_mac,
                msg_counter: *msg_counter,
                pack,
            });
        }
        ies = next;
    }
    if ies.is_empty() {
        Err(BeaconError::NotRemoteId)
    } else {
        Err(BeaconError::TruncatedIe)
    }
}
