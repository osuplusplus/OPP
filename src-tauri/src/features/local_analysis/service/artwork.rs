//! 谱面封面（背景图）定位与随机抽样：为封面墙提供去重键与均匀样本。
use super::*;
use std::hash::BuildHasher;

/// 生成背景图的去重键，作为封面抽样的身份标识。
/// `Stable` 用谱面文件所在目录拼上背景文件名（同目录多个难度共享一张背景时
/// 得到同一键）；`Lazer` 用 Realm 登记的内容哈希（同名不同内容可区分）。
/// 非谱面条目或未声明背景文件时返回 `None`。
pub(super) fn background_key(client: LocalClient, entry: &IndexedEntry) -> Option<String> {
    let IndexedData::Beatmap { detail, .. } = &entry.data else {
        return None;
    };
    let name = detail.background_file.trim();
    if name.is_empty() {
        return None;
    }
    match client {
        LocalClient::Stable => entry.physical_path.parent().map(|parent| {
            format!(
                "stable:{}",
                parent.join(name.replace('\\', "/")).to_string_lossy()
            )
        }),
        LocalClient::Lazer => entry
            .lazer_files
            .as_ref()?
            .iter()
            .find(|file| file.filename.eq_ignore_ascii_case(name))
            .map(|file| format!("lazer:{}", file.hash)),
    }
}

/// 有界抽样：最多保留 24 个不同的背景引用。键为（随机排名, 身份键），
/// 排名由调用方传入的哈希函数给出；同一背景的重复难度因身份键相同而去重，
/// 不会在样本中占据多个名额，内存占用也恒定。
fn sample_artwork(
    candidates: impl Iterator<Item = (String, LocalArtwork)>,
    rank: impl Fn(&str) -> u64,
) -> Vec<LocalArtwork> {
    // Keep only 24 references. Repeated difficulties with the same background
    // share a rank and cannot bias the sample or inflate its memory use.
    let mut sample = BTreeMap::new();
    for (identity, artwork) in candidates {
        let key = (rank(&identity), identity);
        if sample.len() < 24
            || sample
                .last_key_value()
                .is_some_and(|(last, _)| &key <= last)
        {
            sample.insert(key, artwork);
            if sample.len() > 24 {
                sample.pop_last();
            }
        }
    }
    sample.into_values().collect()
}

impl LocalAnalysisService {
    /// 生成封面墙抽样：候选来自两个客户端当前索引中声明了背景的谱面，
    /// 来源已变更的索引不参与；样本最多 24 个并按随机哈希均匀抽取。
    /// 结果缓存在内存中，仅当非空时写入，避免首次扫描未完成时把
    /// 空结果长期缓存。
    pub fn artwork_sample(&self) -> CommandResult<Vec<LocalArtwork>> {
        let mut cached = self
            .artwork_sample
            .lock()
            .map_err(|_| CommandError::new("LOCAL_INDEX_STATE_ERROR", "本地背景缓存不可用"))?;
        if let Some(sample) = cached.as_ref() {
            return Ok(sample.clone());
        }
        let mut indexes = Vec::new();
        for client in [LocalClient::Stable, LocalClient::Lazer] {
            if let Some(index) = self.current_index(client)?
                && source_matches(&self.sources.resolve(client)?, &index.source_root)
            {
                indexes.push((client, index));
            }
        }
        let candidates = indexes.iter().flat_map(|(client, index)| {
            index.entries.iter().filter_map(move |entry| {
                let IndexedData::Beatmap { summary, .. } = &entry.data else {
                    return None;
                };
                Some((
                    background_key(*client, entry)?,
                    LocalArtwork {
                        client: *client,
                        resource_id: summary.resource.resource_id.clone(),
                    },
                ))
            })
        });
        let random = std::collections::hash_map::RandomState::new();
        let sample = sample_artwork(candidates, |key| random.hash_one(key));
        // An empty library may still be finishing its first scan.
        if !sample.is_empty() {
            *cached = Some(sample.clone());
        }
        Ok(sample)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn random_sampling_is_bounded_deduplicated_and_not_limited_to_the_first_page() {
        let candidates = || {
            (0..10_000).flat_map(|i| {
                (0..3).map(move |difficulty| {
                    (
                        format!("art-{i}"),
                        LocalArtwork {
                            client: LocalClient::Stable,
                            resource_id: format!("{i}:{difficulty}"),
                        },
                    )
                })
            })
        };
        let sample = sample_artwork(candidates(), |key| {
            10_000 - key[4..].parse::<u64>().unwrap()
        });
        assert_eq!(sample.len(), 24);
        assert!(sample.iter().all(|item| {
            item.resource_id
                .split(':')
                .next()
                .unwrap()
                .parse::<u64>()
                .unwrap()
                >= 9976
        }));
        assert_eq!(
            sample
                .iter()
                .map(|item| item.resource_id.split(':').next().unwrap())
                .collect::<BTreeSet<_>>()
                .len(),
            24
        );
        let other = sample_artwork(candidates(), |key| key[4..].parse().unwrap());
        assert_ne!(sample[0].resource_id, other[0].resource_id);
    }
}
