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

impl MatrixApp {
    pub(crate) fn clear_attachment(&mut self) {
        self.attachment_request = self.attachment_request.wrapping_add(1);
        self.attachment_pending = 0;
        self.attachment_sample = false;
        self.attachment_drafts.clear();
    }
    pub(crate) fn remove_attachment(&mut self, id: u64, cx: &mut Context<Self>) {
        self.attachment_drafts.retain(|item| item.id != id);
        cx.notify();
    }
    fn attachment_room(&self) -> usize {
        MAX_ATTACHMENTS.saturating_sub(self.attachment_drafts.len() + self.attachment_pending)
    }
    pub(crate) fn choose_attachment(&mut self, window: &Window, cx: &mut Context<Self>) {
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
                        this.add_attachments(paths, window, cx);
                    }
                });
            }
        })
        .detach();
    }
    pub(crate) fn add_attachments(
        &mut self,
        paths: impl IntoIterator<Item = PathBuf>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        for path in paths {
            self.prepare_attachment(AttachmentSource::File(path), window, cx);
        }
    }
    pub(crate) fn prepare_attachment(
        &mut self,
        source: AttachmentSource,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview || !self.snapshot.timeline_ready {
            return;
        }
        if self.attachment_room() == 0 {
            self.status = format!("You can attach up to {MAX_ATTACHMENTS} files to one message.");
            cx.notify();
            return;
        }
        self.attachment_pending += 1;
        let request = self.attachment_request;
        let room = self.snapshot.selected_room.clone();
        let account = self.snapshot.account_instance;
        self.job(
            window,
            cx,
            move |backend| backend.prepare_attachment(room, account, source),
            move |this, result, _, cx| {
                if this.attachment_request != request || this.snapshot.account_instance != account {
                    return;
                }
                this.attachment_pending = this.attachment_pending.saturating_sub(1);
                match result {
                    Ok(draft) => {
                        let total: u64 = this.attachment_drafts.iter().map(|i| i.draft.size).sum();
                        if total.saturating_add(draft.size) > MAX_STACK_BYTES {
                            this.status = format!(
                                "{} wasn't added: one message can carry up to 256 MB of attachments.",
                                draft.filename
                            );
                        } else {
                            this.attachment_next_id += 1;
                            this.attachment_drafts.push(Staged {
                                id: this.attachment_next_id,
                                draft,
                            });
                        }
                    }
                    Err(error) => this.status = error.to_string(),
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
        clipboard: &ClipboardItem,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        for entry in &clipboard.entries {
            match entry {
                ClipboardEntry::Image(image) => {
                    self.prepare_attachment(
                        AttachmentSource::Bytes("Pasted image.png".into(), image.bytes.to_vec()),
                        window,
                        cx,
                    );
                    return true;
                }
                ClipboardEntry::ExternalPaths(paths) if !paths.0.is_empty() => {
                    self.add_attachments(paths.0.iter().cloned(), window, cx);
                    return true;
                }
                _ => {}
            }
        }
        false
    }
    /// Stacked attachments inside the composer box, above the message field.
    pub(crate) fn attachment_tray(&self, cx: &mut Context<Self>) -> AnyElement {
        struct Tile {
            id: Option<u64>,
            filename: String,
            size: u64,
            preview: Option<Arc<Vec<u8>>>,
        }
        let mut tiles: Vec<Tile> = self
            .attachment_drafts
            .iter()
            .map(|item| Tile {
                id: Some(item.id),
                filename: item.draft.filename.clone(),
                size: item.draft.size,
                preview: item.draft.preview_png.clone(),
            })
            .collect();
        if self.preview && self.attachment_sample {
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
        let mut row = h_flex()
            .id("attachment-tray")
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
            row = row.child(
                div()
                    .id(SharedString::from(format!("attachment-{key}")))
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
                                Button::new(SharedString::from(format!("attachment-remove-{key}")))
                                    .ghost()
                                    .size(px(18.))
                                    .rounded_full()
                                    .icon(gpui_kit::assets::IconName::X)
                                    .accessibility_label(format!("Remove {}", tile.filename))
                                    .on_click(cx.listener(move |this, _, _, cx| match remove_id {
                                        Some(id) => this.remove_attachment(id, cx),
                                        None => {
                                            this.attachment_sample = false;
                                            cx.notify();
                                        }
                                    })),
                            ),
                    ),
            );
        }
        for index in 0..self.attachment_pending {
            row = row.child(
                div()
                    .id(SharedString::from(format!("attachment-pending-{index}")))
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
                    .child("Adding…"),
            );
        }
        row.test_support().into_any_element()
    }
}
