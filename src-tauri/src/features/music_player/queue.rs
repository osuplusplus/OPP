use super::models::PlayMode;
use rand_core::{OsRng, RngCore};

#[derive(Default)]
pub(crate) struct Navigation {
    pub history: Vec<String>,
    bag: Vec<String>,
}
impl Navigation {
    pub fn reset(&mut self) {
        self.history.clear();
        self.bag.clear();
    }
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
            self.history.push(current.to_owned());
            if self.history.len() > 1000 {
                self.history.remove(0);
            }
        }
        next
    }
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
    use super::*;
    fn ids() -> Vec<String> {
        vec!["a".into(), "b".into(), "c".into()]
    }
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
