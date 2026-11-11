#![allow(
    dead_code,
    reason = "Preview viewport and Settings contracts for Claude's views"
)]
use super::*;
use std::collections::{HashMap, VecDeque};
pub(crate) use twayblade_core::conversation::PreviewSize;
use twayblade_core::conversation::TimelinePreview;

/// One selected room, main timeline plus thread, all cached sizes combined.
/// At most 16,777,216 pixels (64 MiB RGBA equivalent); PNGs are memory-only.
const CACHE_PIXELS: u64 = 16 * 1024 * 1024;
const MAX_VISIBLE: usize = 128;
// Tiny images must not bypass memory bounding through entry overhead.
const MAX_CACHE_ENTRIES: usize = 256;
type Key = (String, PreviewSize);
#[derive(Default)]
pub(super) struct Cache {
    entries: HashMap<Key, TimelinePreview>,
    order: VecDeque<Key>,
    pixels: u64,
}
impl Cache {
    fn get(&mut self, key: &Key) -> Option<&TimelinePreview> {
        if !self.entries.contains_key(key) {
            return None;
        }
        self.order.retain(|k| k != key);
        self.order.push_back(key.clone());
        self.entries.get(key)
    }
    fn insert(&mut self, key: Key, preview: TimelinePreview) {
        if preview.pixels() > CACHE_PIXELS {
            return;
        }
        if let Some(old) = self.entries.remove(&key) {
            self.pixels -= old.pixels();
        }
        self.order.retain(|k| k != &key);
        while self.pixels + preview.pixels() > CACHE_PIXELS
            || self.entries.len() >= MAX_CACHE_ENTRIES
        {
            let Some(old) = self.order.pop_front() else {
                break;
            };
            if let Some(preview) = self.entries.remove(&old) {
                self.pixels -= preview.pixels();
            }
        }
        self.pixels += preview.pixels();
        self.order.push_back(key.clone());
        self.entries.insert(key, preview);
    }
    pub(super) fn project(&self, view: &mut Snapshot, size: PreviewSize) {
        for message in view
            .messages
            .iter_mut()
            .chain(view.thread.iter_mut().flat_map(|t| &mut t.messages))
        {
            if let Some(attachment) = &mut message.attachment {
                // Clear evicted references too: snapshots must not retain an
                // unbounded second cache outside the LRU's pixel accounting.
                attachment.preview_png = self
                    .entries
                    .get(&(message.id.clone(), size))
                    .map(|p| p.png.clone());
            }
        }
        view.preview_size = size;
    }
}
#[derive(Default)]
pub(super) struct Previews {
    pub(super) size: PreviewSize,
    pub(super) cache: Cache,
    main: Vec<String>,
    thread: Vec<String>,
    plan: Vec<(Option<String>, String)>,
    request: u64,
    task: Option<JoinHandle<()>>,
    cancel: Option<watch::Sender<bool>>,
}
impl Previews {
    fn invalidate(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.send_replace(true);
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
        self.request = self.request.wrapping_add(1);
        self.plan.clear();
    }
    fn clear(&mut self) {
        self.invalidate();
        self.cache = Cache::default();
        self.main.clear();
        self.thread.clear();
    }
}
impl State {
    pub(super) fn clear_previews(&mut self) {
        self.previews.clear();
    }
    pub(super) fn clear_thread_previews(&mut self) {
        self.previews.thread.clear();
        self.previews.invalidate();
    }
}
fn save_size(path: &std::path::Path, size: PreviewSize) -> std::io::Result<()> {
    let mut preferences = crate::preferences::Preferences::load(path)?;
    preferences.chat.preview_size = size;
    preferences.save(path)
}
impl Backend {
    pub(crate) fn preview_size(&self) -> PreviewSize {
        self.state
            .lock()
            .map(|s| s.previews.size)
            .unwrap_or_default()
    }
    /// Settings persistence; keeps other preferences. The MatrixApp adapter also
    /// updates its preference model so subsequent Settings saves keep this value.
    pub(crate) fn set_preview_size(&self, size: PreviewSize) -> Result<(), ServiceError> {
        let _lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| failure("Settings are unavailable."))?;
        let path = crate::preferences::Preferences::path()
            .ok_or_else(|| failure("Settings are unavailable."))?;
        save_size(&path, size)
            .map_err(|_| failure("The image preview preference could not be saved."))?;
        self.apply_preview_size(size)
    }
    pub(super) fn apply_preview_size(&self, size: PreviewSize) -> Result<(), ServiceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        if state.previews.size != size {
            state.previews.invalidate();
            state.previews.size = size;
        }
        publish(&self.updates, |view| {
            state.previews.cache.project(view, size)
        });
        self.schedule_previews(&mut state);
        Ok(())
    }
    /// Selected room contract. This applies a supplied size without persisting;
    /// Settings uses set_preview_size. Viewport reporting follows selection.
    pub(crate) fn select_room_with_preview_size(
        &self,
        room: String,
        size: PreviewSize,
    ) -> Result<(), ServiceError> {
        self.select_room(room)?;
        self.apply_preview_size(size)
    }
    /// Stable Message.id values currently displayed, <=128 per surface. Report
    /// separately for the main timeline (None) and open thread (Some(root)). No
    /// hidden/history images are fetched. Empty lists release viewport demand.
    pub(crate) fn timeline_preview_viewport(
        &self,
        room: String,
        account: u128,
        root: Option<String>,
        ids: Vec<String>,
    ) -> Result<(), ServiceError> {
        if ids.len() > MAX_VISIBLE {
            return Err(failure("Too many visible images."));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        let view = self.updates.borrow();
        if state.account.as_ref().map(Account::instance_id) != Some(account)
            || view.selected_room != room
        {
            return Err(failure("Conversation changed."));
        }
        if let Some(root) = root {
            if !view.thread.as_ref().is_some_and(|t| t.root == root) {
                return Err(failure("Thread changed."));
            }
            state.previews.thread = ids;
        } else {
            state.previews.main = ids;
        }
        drop(view);
        self.schedule_previews(&mut state);
        Ok(())
    }
    /// Called under State's lock after main/thread projections and viewport or
    /// size changes. Only one bounded plan; concurrent fetches <=4, decoders <=1.
    pub(super) fn schedule_previews(&self, state: &mut State) {
        schedule(self.runtime.handle(), &self.state, &self.updates, state);
    }
}
pub(super) fn schedule(
    runtime: &tokio::runtime::Handle,
    shared: &Arc<Mutex<State>>,
    updates: &watch::Sender<Snapshot>,
    state: &mut State,
) {
    let view = updates.borrow().clone();
    let mut plan = Vec::new();
    let mut conversations = HashMap::new();
    for (root, visible, messages, conversation) in [
        (
            None,
            &state.previews.main,
            &view.messages,
            state.conversation.as_ref(),
        ),
        (
            view.thread.as_ref().map(|t| t.root.clone()),
            &state.previews.thread,
            view.thread
                .as_ref()
                .map(|t| &t.messages)
                .unwrap_or(&view.messages),
            state.thread_conversation.as_ref(),
        ),
    ] {
        let Some(conversation) = conversation else {
            continue;
        };
        for id in visible {
            if messages
                .iter()
                .any(|m| m.id == *id && m.kind == MessageKind::Image)
                && !conversations.contains_key(id)
            {
                plan.push((root.clone(), id.clone()));
                conversations.insert(id.clone(), conversation.clone());
            }
        }
    }
    if plan == state.previews.plan {
        return;
    }
    state.previews.invalidate();
    state.previews.plan = plan.clone();
    let size = state.previews.size;
    let pending: VecDeque<_> = plan
        .into_iter()
        .filter_map(|(_, id)| {
            if state.previews.cache.get(&(id.clone(), size)).is_some() {
                None
            } else {
                conversations
                    .remove(&id)
                    .map(|conversation| (id, conversation))
            }
        })
        .collect();
    publish(updates, |view| state.previews.cache.project(view, size));
    if pending.is_empty() {
        return;
    }
    let request = state.previews.request;
    let epoch = state.epoch;
    let selection = state.selection;
    let account = state.account.as_ref().map(Account::instance_id);
    let (cancel, receiver) = watch::channel(false);
    state.previews.cancel = Some(cancel);
    let shared = shared.clone();
    let updates = updates.clone();
    state.previews.task = Some(runtime.spawn(async move {
        let mut pending = pending;
        let mut work = tokio::task::JoinSet::new();
        loop {
            while work.len() < 4
                && let Some((id, conversation)) = pending.pop_front()
            {
                let cancel = receiver.clone();
                work.spawn(async move {
                    let result = conversation.attachment_preview(&id, size, cancel).await;
                    (id, result)
                });
            }
            let Some(completed) = work.join_next().await else {
                return;
            };
            let Ok((id, Ok(preview))) = completed else {
                continue;
            };
            let Ok(mut state) = shared.lock() else {
                return;
            };
            if state.epoch != epoch
                || state.selection != selection
                || state.previews.request != request
                || state.previews.size != size
                || state.account.as_ref().map(Account::instance_id) != account
            {
                return;
            }
            state.previews.cache.insert((id, size), preview);
            publish(&updates, |view| state.previews.cache.project(view, size));
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image(width: u32, height: u32) -> TimelinePreview {
        TimelinePreview {
            png: Arc::new(vec![0]),
            width,
            height,
        }
    }
    #[test]
    fn pixel_budget_evicts_least_recently_used_and_accounts_for_replacements() {
        let mut cache = Cache::default();
        for i in 0..16 {
            cache.insert((i.to_string(), PreviewSize::Default), image(1024, 1024));
        }
        assert_eq!(cache.pixels, CACHE_PIXELS);
        cache.get(&("0".into(), PreviewSize::Default));
        cache.insert(("new".into(), PreviewSize::Large), image(1024, 1024));
        assert!(
            cache
                .entries
                .contains_key(&("0".into(), PreviewSize::Default))
        );
        assert!(
            !cache
                .entries
                .contains_key(&("1".into(), PreviewSize::Default))
        );
        assert!(cache.pixels <= CACHE_PIXELS);
        cache.insert(("0".into(), PreviewSize::Default), image(512, 512));
        assert_eq!(cache.pixels, CACHE_PIXELS - 1024 * 1024 + 512 * 512);
        cache.insert(("oversize".into(), PreviewSize::Large), image(8192, 8192));
        assert!(
            !cache
                .entries
                .contains_key(&("oversize".into(), PreviewSize::Large))
        );
    }
    #[test]
    fn tiny_images_cannot_grow_cache_metadata_without_bound() {
        let mut cache = Cache::default();
        for i in 0..300 {
            cache.insert((i.to_string(), PreviewSize::Default), image(1, 1));
        }
        assert_eq!(cache.entries.len(), MAX_CACHE_ENTRIES);
        assert_eq!(cache.pixels, MAX_CACHE_ENTRIES as u64);
        assert!(
            !cache
                .entries
                .contains_key(&("0".into(), PreviewSize::Default))
        );
        assert!(
            cache
                .entries
                .contains_key(&("299".into(), PreviewSize::Default))
        );
    }
    #[test]
    fn changing_sizes_replaces_bytes_without_identity_or_order_churn() {
        let mut cache = Cache::default();
        let small = image(512, 256);
        let large = image(1280, 640);
        cache.insert(("stable".into(), PreviewSize::Default), small.clone());
        cache.insert(("stable".into(), PreviewSize::Large), large.clone());
        let row = Message {
            id: "stable".into(),
            event_id: Some("$event".into()),
            kind: MessageKind::Image,
            attachment: Some(Attachment::default()),
            ..Default::default()
        };
        let mut view = Snapshot {
            messages: vec![row.clone()],
            thread: Some(ThreadSnapshot {
                room: "room".into(),
                root: "root".into(),
                request: 0,
                messages: vec![row],
                status: TimelineStatus::Ready,
                ready: true,
                unread: 0,
            }),
            ..Default::default()
        };
        for (size, png) in [
            (PreviewSize::Default, small.png),
            (PreviewSize::Large, large.png),
        ] {
            cache.project(&mut view, size);
            assert_eq!(view.messages[0].id, "stable");
            assert_eq!(view.messages[0].event_id.as_deref(), Some("$event"));
            assert!(Arc::ptr_eq(
                view.messages[0]
                    .attachment
                    .as_ref()
                    .unwrap()
                    .preview_png
                    .as_ref()
                    .unwrap(),
                &png
            ));
            assert!(Arc::ptr_eq(
                view.thread.as_ref().unwrap().messages[0]
                    .attachment
                    .as_ref()
                    .unwrap()
                    .preview_png
                    .as_ref()
                    .unwrap(),
                &png
            ));
        }
    }
    #[test]
    fn eviction_releases_snapshot_references_and_cancellation_clears_demand() {
        let mut previews = Previews::default();
        let first = image(1024, 1024);
        let weak = Arc::downgrade(&first.png);
        previews
            .cache
            .insert(("first".into(), PreviewSize::Default), first);
        let mut view = Snapshot {
            messages: vec![Message {
                id: "first".into(),
                attachment: Some(Attachment::default()),
                ..Default::default()
            }],
            ..Default::default()
        };
        previews.cache.project(&mut view, PreviewSize::Default);
        for i in 0..16 {
            previews
                .cache
                .insert((i.to_string(), PreviewSize::Default), image(1024, 1024));
        }
        previews.cache.project(&mut view, PreviewSize::Default);
        assert!(
            view.messages[0]
                .attachment
                .as_ref()
                .unwrap()
                .preview_png
                .is_none()
        );
        assert!(weak.upgrade().is_none());
        previews.main.push("first".into());
        previews.thread.push("reply".into());
        let ticket = previews.request;
        let (cancel, receiver) = watch::channel(false);
        previews.cancel = Some(cancel);
        previews.clear();
        assert!(*receiver.borrow());
        assert_ne!(ticket, previews.request);
        assert!(previews.main.is_empty() && previews.thread.is_empty());
        assert_eq!(previews.cache.pixels, 0);
    }
    #[test]
    fn size_preference_round_trip_preserves_unrelated_settings() {
        let dir = std::env::temp_dir().join(format!("preview-settings-{}", std::process::id()));
        let path = dir.join("preferences.json");
        let mut preferences = crate::preferences::Preferences::default();
        preferences.chat.avatars = false;
        preferences.notifications.sound = false;
        preferences.save(&path).unwrap();
        for size in [PreviewSize::Large, PreviewSize::Default] {
            save_size(&path, size).unwrap();
            preferences.chat.preview_size = size;
            assert_eq!(
                crate::preferences::Preferences::load(&path).unwrap(),
                preferences
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn changing_size_keeps_selection_and_timeline_state_and_only_keeps_viewport_demand() {
        let backend = Backend::with_lock(None).unwrap();
        {
            let mut state = backend.state.lock().unwrap();
            state.selection = 7;
            state.previews.main = vec!["visible".into()];
            state
                .previews
                .cache
                .insert(("visible".into(), PreviewSize::Default), image(512, 256));
            publish(&backend.updates, |view| {
                view.selected_room = "selected".into();
                view.messages = vec![Message {
                    id: "visible".into(),
                    event_id: Some("$event".into()),
                    attachment: Some(Attachment::default()),
                    ..Default::default()
                }];
            });
        }
        backend.apply_preview_size(PreviewSize::Large).unwrap();
        let view = backend.snapshot();
        assert_eq!(view.selected_room, "selected");
        assert_eq!(view.messages[0].id, "visible");
        assert!(
            view.messages[0]
                .attachment
                .as_ref()
                .unwrap()
                .preview_png
                .is_none()
        );
        let state = backend.state.lock().unwrap();
        assert_eq!(state.selection, 7);
        assert_eq!(state.previews.main, ["visible"]);
        assert_eq!(state.previews.size, PreviewSize::Large);
    }
    #[test]
    fn viewport_and_room_commands_require_an_active_account() {
        let backend = Backend::with_lock(None).unwrap();
        assert!(
            backend
                .timeline_preview_viewport("room".into(), 0, None, vec!["id".into()])
                .is_err()
        );
        assert!(
            backend
                .select_room_with_preview_size("room".into(), PreviewSize::Large)
                .is_err()
        );
        assert_eq!(backend.preview_size(), PreviewSize::Default);
        assert_eq!(backend.snapshot().revision, 0);
    }
}
