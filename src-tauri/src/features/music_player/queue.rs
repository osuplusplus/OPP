//! 队列导航：封装顺序、列表循环、单曲循环与洗牌袋随机策略，
//! 并维护前进历史以支持“上一首”。
use super::models::PlayMode;
use rand_core::{OsRng, RngCore};

/// 队列导航状态：`history` 为前进历史（供“上一首”回退），`bag` 为随机模式洗牌袋。
#[derive(Default)]
pub(crate) struct Navigation {
    pub history: Vec<String>,
    // 随机模式下尚未播出的曲目 ID，耗尽后重新洗牌填充。
    bag: Vec<String>,
}
impl Navigation {
    /// 清空历史与洗牌袋；换队列或换播放模式时调用，避免旧状态串台。
    pub fn reset(&mut self) {
        self.history.clear();
        self.bag.clear();
    }
    /// 计算下一首要播放的曲目 ID。
    ///
    /// `ended` 表示当前曲目自然播完：单曲循环此时重播当前曲，手动切歌则照常前进；
    /// `Sequential` 播到队尾返回 `None`（停止），`RepeatAll` 回绕到队首；
    /// 随机模式从洗牌袋取下一曲，袋空则排除当前曲后重新洗牌。
    pub fn next(
        &mut self,
        ids: &[String],
        current: Option<&str>,
        mode: PlayMode,
        ended: bool,
    ) -> Option<String> {
        if ids.is_empty() {
            return None;
        }
        if ended && mode == PlayMode::RepeatOne {
            return current.map(str::to_owned).or_else(|| ids.first().cloned());
        }
        let next = if mode == PlayMode::Shuffle {
            self.bag
                .retain(|id| ids.contains(id) && Some(id.as_str()) != current);
            if self.bag.is_empty() {
                // 袋子耗尽：排除当前曲后重新填充并做 Fisher–Yates 洗牌（熵来自系统 RNG）。
                self.bag = ids
                    .iter()
                    .filter(|id| Some(id.as_str()) != current)
                    .cloned()
                    .collect();
                for i in (1..self.bag.len()).rev() {
                    self.bag
                        .swap(i, (OsRng.next_u64() % (i as u64 + 1)) as usize);
                }
            }
            self.bag.pop().or_else(|| ids.first().cloned())
        } else {
            match current.and_then(|id| ids.iter().position(|v| v == id)) {
                Some(i) if i + 1 < ids.len() => Some(ids[i + 1].clone()),
                Some(_) if mode == PlayMode::Sequential => None,
                _ => ids.first().cloned(),
            }
        };
        if let Some(current) = current
            && next.as_deref() != Some(current)
        {
            // 真正前进时才记入历史；上限 1000 条防止无限增长。
            self.history.push(current.to_owned());
            if self.history.len() > 1000 {
                self.history.remove(0);
            }
        }
        next
    }
    /// 回退到上一个仍存在于队列的曲目；历史耗尽时退到当前曲在队列中的前一首。
    pub fn previous(&mut self, ids: &[String], current: Option<&str>) -> Option<String> {
        while let Some(id) = self.history.pop() {
            if ids.contains(&id) {
                return Some(id);
            }
        }
        current
            .and_then(|id| ids.iter().position(|v| v == id))
            .map(|i| ids[i.saturating_sub(1)].clone())
            .or_else(|| ids.first().cloned())
    }
}

#[cfg(test)]
mod tests {
    // 覆盖导航状态机：顺序停止、循环回绕、单曲循环、随机不重复与历史回退。
    use super::*;
    fn ids() -> Vec<String> {
        vec!["a".into(), "b".into(), "c".into()]
    }
    // 顺序模式到队尾停止；列表循环回绕；单曲循环自然播完重播、手动切歌前进。
    #[test]
    fn sequential_stops_and_repeat_wraps() {
        let mut n = Navigation::default();
        assert_eq!(n.next(&ids(), Some("c"), PlayMode::Sequential, true), None);
        assert_eq!(
            n.next(&ids(), Some("c"), PlayMode::RepeatAll, true)
                .as_deref(),
            Some("a")
        );
        assert_eq!(
            n.next(&ids(), Some("b"), PlayMode::RepeatOne, true)
                .as_deref(),
            Some("b")
        );
        assert_eq!(
            n.next(&ids(), Some("b"), PlayMode::RepeatOne, false)
                .as_deref(),
            Some("c")
        );
    }
    // 随机模式不重复访问剩余曲目，“上一首”按历史回退。
    #[test]
    fn shuffle_visits_every_song_and_previous_uses_history() {
        let mut n = Navigation::default();
        let second = n.next(&ids(), Some("a"), PlayMode::Shuffle, true).unwrap();
        let third = n
            .next(&ids(), Some(&second), PlayMode::Shuffle, true)
            .unwrap();
        assert_ne!(second, third);
        assert_ne!(third, "a");
        assert_eq!(n.previous(&ids(), Some(&third)), Some(second));
    }
}
