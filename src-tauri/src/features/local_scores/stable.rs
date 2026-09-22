use super::LocalScore;
use crate::error::{CommandError, CommandResult};
use sha2::{Digest, Sha256};

fn invalid() -> CommandError {
    CommandError::new("INVALID_SCORES_DB", "成绩数据库格式无效或不完整")
}
struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl Reader<'_> {
    fn take(&mut self, length: usize) -> CommandResult<&[u8]> {
        let end = self.offset.checked_add(length).ok_or_else(invalid)?;
        let bytes = self.bytes.get(self.offset..end).ok_or_else(invalid)?;
        self.offset = end;
        Ok(bytes)
    }
    fn u8(&mut self) -> CommandResult<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> CommandResult<u16> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().map_err(|_| invalid())?,
        ))
    }
    fn u32(&mut self) -> CommandResult<u32> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().map_err(|_| invalid())?,
        ))
    }
    fn i64(&mut self) -> CommandResult<i64> {
        Ok(i64::from_le_bytes(
            self.take(8)?.try_into().map_err(|_| invalid())?,
        ))
    }
    fn string(&mut self) -> CommandResult<String> {
        match self.u8()? {
            0 => return Ok(String::new()),
            11 => {}
            _ => return Err(invalid()),
        }
        let mut len = 0usize;
        for shift in (0..28).step_by(7) {
            let byte = self.u8()?;
            len |= ((byte & 127) as usize) << shift;
            if byte & 128 == 0 {
                if len > 1_048_576 {
                    return Err(invalid());
                }
                return String::from_utf8(self.take(len)?.to_vec()).map_err(|_| invalid());
            }
        }
        Err(invalid())
    }
    fn count(&mut self) -> CommandResult<usize> {
        let n = self.u32()? as usize;
        if n > 1_000_000 { Err(invalid()) } else { Ok(n) }
    }
}

pub(super) fn accuracy(mode: u8, h: [u16; 6]) -> Option<f64> {
    let [great, ok, meh, perfect, good, miss] = h.map(f64::from);
    let (earned, total) = match mode {
        0 => (
            300.0 * great + 100.0 * ok + 50.0 * meh,
            300.0 * (great + ok + meh + miss),
        ),
        1 => (2.0 * great + ok, 2.0 * (great + ok + miss)),
        2 => (great + ok + meh, great + ok + meh + good + miss),
        3 => (
            300.0 * (great + perfect) + 200.0 * good + 100.0 * ok + 50.0 * meh,
            300.0 * (great + perfect + good + ok + meh + miss),
        ),
        _ => return None,
    };
    (total > 0.0).then_some(earned / total)
}

fn mods(bits: u32) -> String {
    let names = [
        "NF", "EZ", "TD", "HD", "HR", "SD", "DT", "RX", "HT", "NC", "FL", "AT", "SO", "AP", "PF",
        "4K", "5K", "6K", "7K", "8K", "FI", "RD", "CN", "TP", "9K", "CO", "1K", "3K", "2K", "V2",
        "MR",
    ];
    names
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            bits & (1 << i) != 0
                && !(*i == 6 && bits & (1 << 9) != 0 || *i == 5 && bits & (1 << 14) != 0)
        })
        .map(|(_, name)| *name)
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn parse(bytes: &[u8]) -> CommandResult<Vec<LocalScore>> {
    let mut r = Reader { bytes, offset: 0 };
    let version = r.u32()?;
    if version < 20140721 {
        return Err(CommandError::new(
            "UNSUPPORTED_SCORES_DB",
            "请使用较新版本 osu!stable 保存成绩数据库",
        ));
    }
    let count = r.count()?;
    let mut result = Vec::new();
    for _ in 0..count {
        let folder_hash = r.string()?;
        for _ in 0..r.count()? {
            if result.len() >= 1_000_000 {
                return Err(invalid());
            }
            let start = r.offset;
            let mode = r.u8()?;
            let _version = r.u32()?;
            let hash = r.string()?;
            if hash != folder_hash || mode > 3 {
                return Err(invalid());
            }
            let player = r.string()?;
            let _replay_hash = r.string()?;
            let hits = [r.u16()?, r.u16()?, r.u16()?, r.u16()?, r.u16()?, r.u16()?];
            let score = u64::from(r.u32()?);
            let combo = Some(u32::from(r.u16()?));
            r.u8()?;
            let bits = r.u32()?;
            r.string()?;
            let ticks = r.i64()?;
            if r.u32()? != u32::MAX {
                return Err(invalid());
            }
            r.i64()?;
            if bits & (1 << 23) != 0 {
                r.take(8)?;
            }
            let id = format!("stable:{:x}", Sha256::digest(&bytes[start..r.offset]));
            let played_at = ticks
                .checked_sub(621355968000000000)
                .and_then(|t| {
                    chrono::DateTime::from_timestamp(
                        t.div_euclid(10_000_000),
                        (t.rem_euclid(10_000_000) * 100) as u32,
                    )
                })
                .map(|t| t.to_rfc3339());
            result.push(LocalScore {
                id,
                source: "stable".into(),
                player,
                ruleset: ["osu", "taiko", "fruits", "mania"][mode as usize].into(),
                scoring: if bits & (1 << 29) != 0 {
                    "score_v2"
                } else {
                    "stable"
                }
                .into(),
                beatmap_hash: hash,
                score,
                accuracy: accuracy(mode, hits),
                combo,
                mods: mods(bits),
                played_at,
                note: String::new(),
            });
        }
    }
    if r.offset != bytes.len() {
        return Err(invalid());
    }
    Ok(result)
}

#[cfg(test)]
pub(super) fn fixture() -> Vec<u8> {
    fn string(bytes: &mut Vec<u8>, text: &str) {
        bytes.push(11);
        bytes.push(text.len() as u8);
        bytes.extend(text.as_bytes());
    }
    let mut bytes = 20250101u32.to_le_bytes().to_vec();
    bytes.extend(1u32.to_le_bytes());
    string(&mut bytes, "exact-md5");
    bytes.extend(2u32.to_le_bytes());
    for player in ["Player", "Other"] {
        bytes.push(0);
        bytes.extend(20250101u32.to_le_bytes());
        string(&mut bytes, "exact-md5");
        string(&mut bytes, player);
        string(&mut bytes, "replay-md5");
        for hits in [100u16, 10, 1, 0, 0, 2] {
            bytes.extend(hits.to_le_bytes());
        }
        bytes.extend(876543u32.to_le_bytes());
        bytes.extend(120u16.to_le_bytes());
        bytes.push(0);
        bytes.extend((1u32 << 3).to_le_bytes());
        bytes.push(0);
        bytes.extend(638000000000000000i64.to_le_bytes());
        bytes.extend(u32::MAX.to_le_bytes());
        bytes.extend(123i64.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_modes_and_empty_accuracy() {
        assert_eq!(accuracy(0, [1, 1, 1, 0, 0, 1]), Some(450.0 / 1200.0));
        assert_eq!(accuracy(1, [1, 1, 0, 0, 0, 1]), Some(0.5));
        assert_eq!(accuracy(2, [1, 1, 1, 0, 1, 1]), Some(0.6));
        assert_eq!(accuracy(3, [1, 1, 1, 1, 1, 1]), Some(950.0 / 1800.0));
        assert_eq!(accuracy(0, [0; 6]), None);
    }
    #[test]
    fn rejects_truncation_and_unbounded_counts() {
        assert!(parse(&[]).is_err());
        let mut bytes = 20250101u32.to_le_bytes().to_vec();
        bytes.extend(0u32.to_le_bytes());
        assert!(parse(&bytes).unwrap().is_empty());
        bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse(&bytes).is_err());
    }
    #[test]
    fn decodes_full_database_and_rejects_each_truncation() {
        let bytes = fixture();
        let scores = parse(&bytes).unwrap();
        assert_eq!(scores.len(), 2);
        assert_ne!(scores[0].id, scores[1].id);
        assert_eq!(scores[0].player, "Player");
        assert_eq!(scores[0].score, 876543);
        assert_eq!(scores[0].mods, "HD");
        assert_eq!(scores[0].combo, Some(120));
        assert!(scores[0].played_at.as_ref().unwrap().starts_with("2022-"));
        for length in 0..bytes.len() {
            assert!(
                parse(&bytes[..length]).is_err(),
                "accepted truncated length {length}"
            );
        }
    }
}
