use std::collections::{HashMap, VecDeque};

const MAX_PENDING_PREVIEWS: usize = 32;

pub(super) struct PendingPreviews<T> {
    entries: HashMap<String, T>,
    order: VecDeque<String>,
}

impl<T> Default for PendingPreviews<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }
}

impl<T> PendingPreviews<T> {
    pub(super) fn insert(&mut self, preview_id: String, preview: T) {
        if self.entries.insert(preview_id.clone(), preview).is_some() {
            self.order.retain(|existing| existing != &preview_id);
        }
        self.order.push_back(preview_id);
        while self.entries.len() > MAX_PENDING_PREVIEWS {
            if let Some(expired) = self.order.pop_front() {
                self.entries.remove(&expired);
            }
        }
    }

    pub(super) fn remove(&mut self, preview_id: &str) -> Option<T> {
        let preview = self.entries.remove(preview_id)?;
        self.order.retain(|existing| existing != preview_id);
        Some(preview)
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    #[cfg(test)]
    pub(super) fn contains(&self, preview_id: &str) -> bool {
        self.entries.contains_key(preview_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_recent_previews_without_invalidating_existing_entries() {
        let mut previews = PendingPreviews::default();
        for index in 0..=MAX_PENDING_PREVIEWS {
            previews.insert(format!("preview-{index}"), index);
        }

        assert!(!previews.contains("preview-0"));
        assert!(previews.contains("preview-1"));
        assert_eq!(previews.remove("preview-1"), Some(1));
        assert!(!previews.contains("preview-1"));
    }
}
