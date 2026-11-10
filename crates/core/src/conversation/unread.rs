//! Selection-scoped unread work. No SDK work runs in snapshot projection.
use super::{Conversation, MessageRow};
use std::collections::HashMap;
use tokio::task::JoinSet;

#[derive(Default)]
struct Entry {
    replies: u32,
    revision: u64,
    value: u64,
    ready: bool,
    pending: bool,
}

#[derive(Default)]
pub(super) struct UnreadCache {
    entries: HashMap<String, Entry>,
    revision: u64,
}

pub(super) type ResultItem = (String, u64, Option<u64>);

impl UnreadCache {
    pub fn invalidate(&mut self, root: Option<&str>) {
        self.revision += 1;
        for (id, entry) in &mut self.entries {
            if root.is_none_or(|root| root == id) {
                entry.revision = self.revision;
                entry.ready = false;
                entry.pending = false;
                entry.value = 0;
            }
        }
    }

    fn track(&mut self, id: &str, replies: u32) -> u64 {
        let entry = self.entries.entry(id.to_owned()).or_insert_with(|| {
            self.revision += 1;
            Entry {
                revision: self.revision,
                ..Default::default()
            }
        });
        if entry.replies != replies {
            self.revision += 1;
            entry.revision = self.revision;
            entry.replies = replies;
            entry.ready = false;
            entry.pending = false;
            entry.value = 0;
        }
        entry.value
    }

    pub fn project(&mut self, messages: &mut [MessageRow], root: Option<&str>) {
        let mut active = std::collections::HashSet::new();
        // A focused SDK timeline can omit the root's bundled summary. Its
        // loaded reply count still changes when replies arrive or paginate.
        let mut focused_replies = messages.len().saturating_sub(1).min(u32::MAX as usize) as u32;
        for message in messages.iter_mut() {
            if message.event_id.as_deref() == root && root.is_some() {
                focused_replies = focused_replies.max(message.thread_replies);
                continue;
            }
            if message.thread_replies == 0 {
                continue;
            }
            let Some(id) = &message.event_id else {
                continue;
            };
            active.insert(id.clone());
            message.thread_unread = self.track(id, message.thread_replies);
        }
        if let Some(root) = root {
            active.insert(root.to_owned());
            let value = self.track(root, focused_replies);
            for message in messages {
                if message.event_id.as_deref() == Some(root) {
                    message.thread_unread = value;
                }
            }
        }
        self.entries.retain(|id, _| active.contains(id));
    }

    pub fn value(&self, root: &str) -> u64 {
        self.entries.get(root).map_or(0, |entry| entry.value)
    }

    fn misses(&mut self, limit: usize) -> Vec<(String, u64)> {
        self.entries
            .iter_mut()
            .filter_map(|(id, entry)| {
                if entry.ready || entry.pending {
                    return None;
                }
                entry.pending = true;
                Some((id.clone(), entry.revision))
            })
            .take(limit)
            .collect()
    }

    pub fn schedule(&mut self, conversation: &Conversation, tasks: &mut JoinSet<ResultItem>) {
        self.schedule_with(tasks, |id| {
            let conversation = conversation.clone();
            async move { conversation.unread(&id).await.ok() }
        });
    }

    fn schedule_with<F, Fut>(&mut self, tasks: &mut JoinSet<ResultItem>, query: F)
    where
        F: Fn(String) -> Fut,
        Fut: std::future::Future<Output = Option<u64>> + Send + 'static,
    {
        for (id, revision) in self.misses(4usize.saturating_sub(tasks.len())) {
            let future = query(id.clone());
            tasks.spawn(async move {
                let value = future.await;
                (id, revision, value)
            });
        }
    }

    pub fn complete(&mut self, (id, revision, value): ResultItem) {
        if let Some(entry) = self.entries.get_mut(&id)
            && entry.revision == revision
        {
            entry.value = value.unwrap_or(0);
            // Avoid a tight retry loop after an SDK failure. A later receipt
            // or reply invalidates this entry and permits another attempt.
            entry.ready = true;
            entry.pending = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn counting_query_stub_runs_only_for_cache_misses() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let queries = Arc::new(AtomicUsize::new(0));
        let mut cache = UnreadCache::default();
        let mut tasks = JoinSet::new();
        let mut messages = vec![MessageRow {
            event_id: Some("$root:example.invalid".into()),
            thread_replies: 2,
            ..Default::default()
        }];
        for _ in 0..10 {
            cache.project(&mut messages, None);
            cache.schedule_with(&mut tasks, |_| {
                let queries = queries.clone();
                async move {
                    queries.fetch_add(1, Ordering::SeqCst);
                    Some(2)
                }
            });
            while let Some(result) = tasks.join_next().await {
                cache.complete(result.unwrap());
            }
        }
        assert_eq!(queries.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn unchanged_updates_do_not_query_unread_and_invalidations_reject_stale_results() {
        let mut cache = UnreadCache::default();
        let mut messages = vec![MessageRow {
            event_id: Some("$root:example.invalid".into()),
            thread_replies: 2,
            ..Default::default()
        }];
        let mut queries = 0;
        cache.project(&mut messages, None);
        for (id, revision) in cache.misses(4) {
            queries += 1;
            cache.complete((id, revision, Some(2)));
        }
        for _ in 0..10 {
            cache.project(&mut messages, None);
            queries += cache.misses(4).len();
        }
        assert_eq!(queries, 1);
        assert_eq!(messages[0].thread_unread, 2);
        messages[0].thread_replies += 1;
        cache.project(&mut messages, None);
        let (id, revision) = cache.misses(4).pop().unwrap();
        cache.invalidate(Some(&id));
        cache.complete((id.clone(), revision, Some(99)));
        assert_eq!(cache.value(&id), 0);
        let (id, revision) = cache.misses(4).pop().unwrap();
        cache.complete((id.clone(), revision, Some(0)));
        assert!(cache.misses(4).is_empty());
        cache.invalidate(None);
        assert_eq!(cache.misses(4).len(), 1);
    }

    #[test]
    fn queries_are_bounded_and_removed_roots_are_evicted() {
        let mut cache = UnreadCache::default();
        let mut messages: Vec<_> = (0..12)
            .map(|i| MessageRow {
                event_id: Some(format!("$root{i}:example.invalid")),
                thread_replies: 1,
                ..Default::default()
            })
            .collect();
        cache.project(&mut messages, None);
        assert_eq!(cache.misses(4).len(), 4);
        assert_eq!(cache.misses(0).len(), 0);
        cache.project(&mut [], None);
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn focused_thread_without_root_summary_invalidates_on_new_replies() {
        let mut cache = UnreadCache::default();
        let root = "$root:example.invalid";
        let mut messages = vec![MessageRow::default(); 2];
        cache.project(&mut messages, Some(root));
        let (id, revision) = cache.misses(4).pop().unwrap();
        cache.complete((id, revision, Some(1)));
        cache.project(&mut messages, Some(root));
        assert!(cache.misses(4).is_empty());
        messages.push(MessageRow::default());
        cache.project(&mut messages, Some(root));
        assert_eq!(cache.value(root), 0);
        assert_eq!(cache.misses(4).len(), 1);
    }
}
