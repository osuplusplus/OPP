//! osu!stable `scores.db` 二进制格式的只读解析器。
//! 整体布局：`u32` 版本号 → `u32` 集合数 → 每个集合（谱面 MD5 字符串、
//! `u32` 成绩数、若干成绩条目）；所有多字节整数均为小端。
use super::LocalScore;
use crate::error::{CommandError, CommandResult};
use sha2::{Digest, Sha256};

/// 统一的“格式无效或不完整”错误。
fn invalid() -> CommandError {
    CommandError::new("INVALID_SCORES_DB", "成绩数据库格式无效或不完整")
}
/// 顺序字节读取器；所有读取都先做边界检查，越界即判定格式无效。
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
    /// osu! 变长字符串：`0x00` 表示空串，`0x0B` 后跟 LEB128 长度与 UTF-8 字节；
    /// 长度上限 1MB，防止异常数据触发超大分配。
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
    /// 集合/成绩数量（`u32`）；上限 100 万，防止伪造计数撑爆内存。
    fn count(&mut self) -> CommandResult<usize> {
        let n = self.u32()? as usize;
        if n > 1_000_000 { Err(invalid()) } else { Ok(n) }
    }
}

/// 按各模式官方公式由判定数计算准确率（`h` 为 300/100/50/geki/katu/miss 计数）；
/// 判定总数为零时返回 `None`。
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

/// 把模组位掩码翻译为空格分隔缩写；`NC` 隐含的 `DT`、`PF` 隐含的 `SD` 不再重复输出。
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

/// 解析整份 `scores.db`，返回统一模型的成绩列表。
///
/// 版本号低于 20140721 视为过旧直接拒绝；每条成绩以其字节区间的 `sha256`
/// 摘要生成稳定 ID；结束时必须恰好读完整个文件，否则判定格式无效。
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
    // 外层：每个谱面集合。
    for _ in 0..count {
        // 集合的 .osu 文件 MD5，成绩内的哈希必须与之一致。
        let folder_hash = r.string()?;
        // 内层：该集合下的每条成绩。
        for _ in 0..r.count()? {
            if result.len() >= 1_000_000 {
                return Err(invalid());
            }
            // 记录成绩起始偏移，稍后对该字节区间做内容哈希生成 ID。
            let start = r.offset;
            let mode = r.u8()?;
            let _version = r.u32()?;
            let hash = r.string()?;
            if hash != folder_hash || mode > 3 {
                return Err(invalid());
            }
            let player = r.string()?;
            let _replay_hash = r.string()?;
            // 六个判定计数：300、100、50、geki、katu、miss。
            let hits = [r.u16()?, r.u16()?, r.u16()?, r.u16()?, r.u16()?, r.u16()?];
            let score = u64::from(r.u32()?);
            let combo = Some(u32::from(r.u16()?));
            // Full Combo 标志字节，读取后丢弃。
            r.u8()?;
            let bits = r.u32()?;
            // 常为空的附加字符串，读取后丢弃。
            r.string()?;
            // .NET ticks：自 0001-01-01 起、以 100ns 为单位的时间戳。
            let ticks = r.i64()?;
            // 新版格式的 -1 定界标记，用于确认行结构完整。
            if r.u32()? != u32::MAX {
                return Err(invalid());
            }
            // 线上成绩 ID，读取后丢弃。
            r.i64()?;
            // Target Practice 模组（bit 23）额外附带 8 字节附加数据。
            if bits & (1 << 23) != 0 {
                r.take(8)?;
            }
            let id = format!("stable:{:x}", Sha256::digest(&bytes[start..r.offset]));
            // 先减去 0001-01-01 到 Unix 纪元的 ticks 偏移，再换算为秒 + 纳秒。
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
    // 必须恰好读完整个文件：尾部多余字节说明结构被误读。
    if r.offset != bytes.len() {
        return Err(invalid());
    }
    Ok(result)
}

/// 构造一份最小合法的 `scores.db` 测试数据（两条 osu! 模式成绩）。
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
    // 覆盖准确率公式、计数上限防御与完整解码/任意截断拒绝。
    use super::*;
    // 四种模式的准确率公式与零判定返回 `None`。
    #[test]
    fn all_modes_and_empty_accuracy() {
        assert_eq!(accuracy(0, [1, 1, 1, 0, 0, 1]), Some(450.0 / 1200.0));
        assert_eq!(accuracy(1, [1, 1, 0, 0, 0, 1]), Some(0.5));
        assert_eq!(accuracy(2, [1, 1, 1, 0, 1, 1]), Some(0.6));
        assert_eq!(accuracy(3, [1, 1, 1, 1, 1, 1]), Some(950.0 / 1800.0));
        assert_eq!(accuracy(0, [0; 6]), None);
    }
    // 空数据与伪造的巨大计数都应被拒绝。
    #[test]
    fn rejects_truncation_and_unbounded_counts() {
        assert!(parse(&[]).is_err());
        let mut bytes = 20250101u32.to_le_bytes().to_vec();
        bytes.extend(0u32.to_le_bytes());
        assert!(parse(&bytes).unwrap().is_empty());
        bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse(&bytes).is_err());
    }
    // 完整解码字段正确，且任意长度截断都不得通过。
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
