//! Attachment UI owns file dialogs and wording; the core owns SDK media operations.
use crate::app::MatrixApp;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::{component::*, *};
use std::{path::PathBuf, sync::Arc};

pub(crate) enum AttachmentSource {
    File(PathBuf),
    Bytes(String, Vec<u8>),
}

#[derive(Default)]
struct Thumbnails(std::collections::HashMap<usize, (std::sync::Weak<Vec<u8>>, Arc<RenderImage>)>);
impl Global for Thumbnails {}
/// Core supplies bounded PNGs. Decode once into render data, keyed by the
/// shared buffer's address so scrolling never copies, hashes or re-decodes.
pub(crate) fn render_png(bytes: &Arc<Vec<u8>>, cx: &mut App) -> Option<Arc<RenderImage>> {
    let key = Arc::as_ptr(bytes) as usize;
    // The weak handle guards against a freed buffer's address being reused.
    if let Some((_, image)) = cx
        .try_global::<Thumbnails>()
        .and_then(|images| images.0.get(&key))
        .filter(|(source, _)| {
            source
                .upgrade()
                .is_some_and(|live| Arc::ptr_eq(&live, bytes))
        })
    {
        return Some(image.clone());
    }
    let image = Image::from_bytes(ImageFormat::Png, bytes.as_ref().clone())
        .to_image_data(cx.svg_renderer())
        .ok()?;
    if cx.try_global::<Thumbnails>().is_none() {
        cx.set_global(Thumbnails::default());
    }
    let cache = cx.global_mut::<Thumbnails>();
    // Thumbnails are at most 512px; a few hundred stay well under 200 MB.
    if cache.0.len() >= 256 {
        cache.0.clear();
    }
    cache.0.insert(key, (Arc::downgrade(bytes), image.clone()));
    Some(image)
}

#[derive(Default)]
struct SmallImages(std::collections::HashMap<u64, Arc<RenderImage>>);
impl Global for SmallImages {}
/// For a handful of tiny images (sign-in provider icons) without a shared buffer.
pub(crate) fn render_small_png(bytes: &[u8], cx: &mut App) -> Option<Arc<RenderImage>> {
    let image = Image::from_bytes(ImageFormat::Png, bytes.to_vec());
    let id = image.id();
    if let Some(image) = cx
        .try_global::<SmallImages>()
        .and_then(|cache| cache.0.get(&id))
    {
        return Some(image.clone());
    }
    let image = image.to_image_data(cx.svg_renderer()).ok()?;
    if cx.try_global::<SmallImages>().is_none() {
        cx.set_global(SmallImages::default());
    }
    cx.global_mut::<SmallImages>().0.insert(id, image.clone());
    Some(image)
}

/// One stacked attachment; `id` keeps its identity for removal and focus.
pub(crate) struct Staged {
    pub id: u64,
    pub draft: twayblade_core::conversation::AttachmentDraft,
}

/// Bounds for one message's stack. Drafts hold their bytes in memory until sent.
pub(crate) const MAX_ATTACHMENTS: usize = 10;
const MAX_STACK_BYTES: u64 = 256 * 1024 * 1024;

/// The attachments stacked in one composer (the main one or a thread's),
/// sent with that composer's next message.
#[derive(Default)]
pub(crate) struct AttachmentStack {
    /// In the order they were added.
    pub drafts: Vec<Staged>,
    /// Still being read or checked.
    pub pending: usize,
    /// Bumped when the stack is cleared, so late results are dropped.
    pub request: u64,
    next_id: u64,
}

impl AttachmentStack {
    pub(crate) fn clear(&mut self) {
        self.request = self.request.wrapping_add(1);
        self.pending = 0;
        self.drafts.clear();
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.drafts.is_empty() && self.pending == 0
    }
    pub(crate) fn full(&self) -> bool {
        self.drafts.len() + self.pending >= MAX_ATTACHMENTS
    }
}

impl MatrixApp {
    /// The main composer's stack, or the open thread's.
    pub(crate) fn attachments(&self, in_thread: bool) -> Option<&AttachmentStack> {
        if in_thread {
            self.thread.as_ref().map(|thread| &thread.attachments)
        } else {
            Some(&self.attachments)
        }
    }
    fn attachments_mut(&mut self, in_thread: bool) -> Option<&mut AttachmentStack> {
        if in_thread {
            self.thread.as_mut().map(|thread| &mut thread.attachments)
        } else {
            Some(&mut self.attachments)
        }
    }
    pub(crate) fn clear_attachment(&mut self) {
        self.attachments.clear();
        self.attachment_sample = false;
    }
    pub(crate) fn remove_attachment(&mut self, in_thread: bool, id: u64, cx: &mut Context<Self>) {
        if let Some(stack) = self.attachments_mut(in_thread) {
            stack.drafts.retain(|item| item.id != id);
        }
        cx.notify();
    }
    pub(crate) fn choose_attachment(
        &mut self,
        in_thread: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let room = self.snapshot.selected_room.clone();
        let account = self.snapshot.account_instance;
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach".into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            if let Ok(Ok(Some(paths))) = paths.await {
                let _ = view.update_in(cx, |this, window, cx| {
                    if this.snapshot.selected_room == room
                        && this.snapshot.account_instance == account
                    {
                        this.add_attachments(in_thread, paths, window, cx);
                    }
                });
            }
        })
        .detach();
    }
    pub(crate) fn add_attachments(
        &mut self,
        in_thread: bool,
        paths: impl IntoIterator<Item = PathBuf>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        for path in paths {
            self.prepare_attachment(in_thread, AttachmentSource::File(path), window, cx);
        }
    }
    /// A problem with an attachment, shown under the composer it belongs to.
    fn attachment_problem(&mut self, in_thread: bool, message: String) {
        match &mut self.thread {
            Some(thread) if in_thread => thread.status = message,
            _ => self.status = message,
        }
    }
    pub(crate) fn prepare_attachment(
        &mut self,
        in_thread: bool,
        source: AttachmentSource,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let ready = if in_thread {
            self.thread.as_ref().is_some_and(|thread| thread.ready)
        } else {
            self.snapshot.timeline_ready
        };
        if self.preview || !ready {
            return;
        }
        // The thread's own request tells a late result for a closed thread
        // from one for the thread now open.
        let thread_request = self.thread.as_ref().map(|thread| thread.request);
        let Some(stack) = self.attachments_mut(in_thread) else {
            return;
        };
        if stack.full() {
            self.attachment_problem(
                in_thread,
                format!("You can attach up to {MAX_ATTACHMENTS} files to one message."),
            );
            cx.notify();
            return;
        }
        stack.pending += 1;
        let request = stack.request;
        let room = self.snapshot.selected_room.clone();
        let account = self.snapshot.account_instance;
        self.job(
            window,
            cx,
            move |backend| backend.prepare_attachment(room, account, source),
            move |this, result, _, cx| {
                if this.snapshot.account_instance != account
                    || (in_thread && this.thread.as_ref().map(|t| t.request) != thread_request)
                {
                    return;
                }
                let Some(stack) = this.attachments_mut(in_thread) else {
                    return;
                };
                if stack.request != request {
                    return;
                }
                stack.pending = stack.pending.saturating_sub(1);
                let problem = match result {
                    Ok(draft) => {
                        let total: u64 = stack.drafts.iter().map(|i| i.draft.size).sum();
                        if total.saturating_add(draft.size) > MAX_STACK_BYTES {
                            Some(format!(
                                "{} wasn't added: one message can carry up to 256 MB of attachments.",
                                draft.filename
                            ))
                        } else {
                            stack.next_id += 1;
                            let id = stack.next_id;
                            stack.drafts.push(Staged { id, draft });
                            None
                        }
                    }
                    Err(error) => Some(error.to_string()),
                };
                if let Some(problem) = problem {
                    this.attachment_problem(in_thread, problem);
                }
                cx.notify();
            },
        );
        cx.notify();
    }
    pub(crate) fn save_attachment(
        &mut self,
        message: String,
        filename: String,
        open: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview {
            return;
        }
        let room = self.snapshot.selected_room.clone();
        let account = self.snapshot.account_instance;
        let filename = twayblade_core::conversation::sanitize_filename(&filename);
        let selected = cx.prompt_for_new_path(&PathBuf::from("."), Some(&filename));
        cx.spawn_in(window, async move |view, cx| {
            if let Ok(Ok(Some(path))) = selected.await {
                let _ = view.update_in(cx, |this, window, cx| {
                    let destination = path.clone();
                    this.job(
                        window,
                        cx,
                        move |backend| backend.media_action(room, account, message, Some(path)),
                        move |this, result, _, cx| {
                            if this.snapshot.account_instance != account {
                                return;
                            }
                            match result {
                                Ok(()) => {
                                    this.status = "Attachment saved.".into();
                                    if open {
                                        cx.open_with_system(&destination);
                                    }
                                }
                                Err(error) => this.status = error.to_string(),
                            }
                        },
                    );
                });
            }
        })
        .detach();
    }
    pub(crate) fn cancel_upload(
        &mut self,
        message: String,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let room = self.snapshot.selected_room.clone();
        let account = self.snapshot.account_instance;
        self.job(
            window,
            cx,
            move |backend| backend.media_action(room, account, message, None),
            move |this, result, _, _| {
                if this.snapshot.account_instance == account
                    && let Err(error) = result
                {
                    this.status = error.to_string();
                }
            },
        );
    }
    pub(crate) fn paste_attachment(
        &mut self,
        in_thread: bool,
        clipboard: &ClipboardItem,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        for entry in &clipboard.entries {
            match entry {
                ClipboardEntry::Image(image) => {
                    self.prepare_attachment(
                        in_thread,
                        AttachmentSource::Bytes("Pasted image.png".into(), image.bytes.to_vec()),
                        window,
                        cx,
                    );
                    return true;
                }
                ClipboardEntry::ExternalPaths(paths) if !paths.0.is_empty() => {
                    self.add_attachments(in_thread, paths.0.iter().cloned(), window, cx);
                    return true;
                }
                _ => {}
            }
        }
        false
    }
    /// Stacked attachments inside the composer box, above the message field.
    pub(crate) fn attachment_tray(&self, in_thread: bool, cx: &mut Context<Self>) -> AnyElement {
        let empty = AttachmentStack::default();
        let stack = self.attachments(in_thread).unwrap_or(&empty);
        struct Tile {
            id: Option<u64>,
            filename: String,
            size: u64,
            preview: Option<Arc<Vec<u8>>>,
        }
        let mut tiles: Vec<Tile> = stack
            .drafts
            .iter()
            .map(|item| Tile {
                id: Some(item.id),
                filename: item.draft.filename.clone(),
                size: item.draft.size,
                preview: item.draft.preview_png.clone(),
            })
            .collect();
        if self.preview && self.attachment_sample && !in_thread {
            tiles.push(Tile {
                id: None,
                filename: "Sample landscape.png".into(),
                size: 2480,
                preview: Some(Arc::new(
                    include_bytes!("../../assets/attachment-sample.png").to_vec(),
                )),
            });
            tiles.push(Tile {
                id: None,
                filename: "Travel notes.pdf".into(),
                size: 234_567,
                preview: None,
            });
        }
        let theme = cx.theme();
        let (border, muted, surface, background) = (
            theme.border,
            theme.muted_foreground,
            theme.muted,
            theme.background,
        );
        let prefix = if in_thread { "thread-" } else { "" };
        let mut row = h_flex()
            .id(if in_thread {
                "thread-attachment-tray"
            } else {
                "attachment-tray"
            })
            .flex_wrap()
            .gap(px(6.))
            .px(px(8.))
            .pt(px(8.));
        for (index, tile) in tiles.into_iter().enumerate() {
            let key = tile
                .id
                .map_or_else(|| format!("sample-{index}"), |id| id.to_string());
            let size = crate::model::size_label(Some(tile.size));
            let image = tile.preview.and_then(|bytes| render_png(&bytes, cx));
            let body = match image {
                Some(image) => div()
                    .size(px(56.))
                    .rounded(px(6.))
                    .overflow_hidden()
                    .bg(surface)
                    .child(img(image).size_full().object_fit(ObjectFit::Cover))
                    .into_any_element(),
                None => h_flex()
                    .h(px(56.))
                    .max_w(px(200.))
                    .gap(px(8.))
                    .px(px(10.))
                    .rounded(px(6.))
                    .bg(surface)
                    .child(Icon::new(IconName::File).size(px(18.)).text_color(muted))
                    .child(
                        v_flex()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .truncate()
                                    .child(tile.filename.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(muted)
                                    .child(size.clone()),
                            ),
                    )
                    .into_any_element(),
            };
            let remove_id = tile.id;
            row =
                row.child(
                    div()
                        .id(SharedString::from(format!("{prefix}attachment-{key}")))
                        .relative()
                        .border_1()
                        .border_color(border)
                        .rounded(px(7.))
                        .test_support()
                        .child(body)
                        .child(
                            div()
                                .absolute()
                                .top(px(-6.))
                                .right(px(-6.))
                                .rounded_full()
                                .border_1()
                                .border_color(border)
                                .bg(background)
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "{prefix}attachment-remove-{key}"
                                    )))
                                    .ghost()
                                    .size(px(18.))
                                    .rounded_full()
                                    .icon(gpui_kit::assets::IconName::X)
                                    .accessibility_label(format!("Remove {}", tile.filename))
                                    .on_click(cx.listener(move |this, _, _, cx| match remove_id {
                                        Some(id) => this.remove_attachment(in_thread, id, cx),
                                        None => {
                                            this.attachment_sample = false;
                                            cx.notify();
                                        }
                                    })),
                                ),
                        ),
                );
        }
        for index in 0..stack.pending {
            row = row.child(
                div()
                    .id(SharedString::from(format!(
                        "{prefix}attachment-pending-{index}"
                    )))
                    .size(px(56.))
                    .rounded(px(7.))
                    .border_1()
                    .border_color(border)
                    .bg(surface)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(11.))
                    .text_color(muted)
                    .test_support()
                    .child("Adding…"),
            );
        }
        row.test_support().into_any_element()
    }
}
