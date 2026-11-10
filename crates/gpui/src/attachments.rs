//! Attachment UI owns file dialogs and wording; the core owns SDK media operations.
use crate::app::MatrixApp;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{component::*, *};
use std::{path::PathBuf, sync::Arc};

pub(super) enum AttachmentSource {
    File(PathBuf),
    Bytes(String, Vec<u8>),
}

#[derive(Default)]
struct Thumbnails(std::collections::HashMap<u64, Arc<RenderImage>>);
impl Global for Thumbnails {}
/// Core supplies bounded PNGs. Decode once into render data; no asynchronous layout race.
pub(super) fn render_png(bytes: &[u8], cx: &mut App) -> Option<Arc<RenderImage>> {
    let image = Image::from_bytes(ImageFormat::Png, bytes.to_vec());
    let id = image.id();
    if let Some(image) = cx
        .try_global::<Thumbnails>()
        .and_then(|images| images.0.get(&id))
    {
        return Some(image.clone());
    }
    let image = image.to_image_data(cx.svg_renderer()).ok()?;
    if cx.try_global::<Thumbnails>().is_none() {
        cx.set_global(Thumbnails::default());
    }
    let cache = cx.global_mut::<Thumbnails>();
    if cache.0.len() >= 32 {
        cache.0.clear();
    }
    cache.0.insert(id, image.clone());
    Some(image)
}

impl MatrixApp {
    pub(super) fn clear_attachment(&mut self) {
        self.attachment_request = self.attachment_request.wrapping_add(1);
        self.attachment_busy = false;
        self.attachment_sample = false;
        self.attachment_draft = None;
    }
    pub(super) fn choose_attachment(&mut self, window: &Window, cx: &mut Context<Self>) {
        let room = self.snapshot.selected_room.clone();
        let account = self.snapshot.account_instance;
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose attachment".into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = view.update_in(cx, |this, window, cx| {
                    if this.snapshot.selected_room == room
                        && this.snapshot.account_instance == account
                    {
                        this.prepare_attachment(AttachmentSource::File(path), window, cx);
                    }
                });
            }
        })
        .detach();
    }
    pub(super) fn prepare_attachment(
        &mut self,
        source: AttachmentSource,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview || self.attachment_busy || !self.snapshot.timeline_ready {
            return;
        }
        self.clear_attachment();
        self.attachment_busy = true;
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
                this.attachment_busy = false;
                match result {
                    Ok(draft) => this.attachment_draft = Some(draft),
                    Err(error) => this.status = error.to_string(),
                }
                cx.notify();
            },
        );
        cx.notify();
    }
    pub(super) fn send_file(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(draft) = self.attachment_draft.take() else {
            return;
        };
        let room = self.snapshot.selected_room.clone();
        let account = self.snapshot.account_instance;
        self.job(
            window,
            cx,
            move |backend| backend.send_attachment(room, account, draft),
            move |this, result, _, _| {
                if this.snapshot.account_instance == account
                    && let Err(error) = result
                {
                    this.status = error.to_string();
                }
            },
        );
        cx.notify();
    }
    pub(super) fn save_attachment(
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
        let filename = matrix_client_core::conversation::sanitize_filename(&filename);
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
    pub(super) fn cancel_upload(
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
    pub(super) fn paste_attachment(
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
                ClipboardEntry::ExternalPaths(paths) => {
                    if let Some(path) = paths.0.first() {
                        self.prepare_attachment(AttachmentSource::File(path.clone()), window, cx);
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }
    pub(super) fn attachment_confirmation(&self, cx: &mut Context<Self>) -> AnyElement {
        let (filename, size, preview_png) = if let Some(draft) = &self.attachment_draft {
            (
                draft.filename.clone(),
                draft.size,
                draft.preview_png.clone(),
            )
        } else if self.preview && self.attachment_sample {
            (
                "Sample landscape.png".into(),
                2480,
                Some(Arc::new(
                    include_bytes!("../assets/attachment-sample.png").to_vec(),
                )),
            )
        } else {
            return div().into_any_element();
        };
        let preview_image = preview_png.and_then(|bytes| render_png(&bytes, cx));
        v_flex()
            .id("attachment-confirmation")
            .gap(px(6.))
            .p(px(10.))
            .border_1()
            .border_color(cx.theme().border)
            .rounded(px(8.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Send attachment?"),
            )
            .when_some(preview_image, |view, image| {
                view.child(
                    img(image)
                        .w(px(240.))
                        .h(px(120.))
                        .object_fit(ObjectFit::Contain),
                )
            })
            .child(format!("{filename} · {size} bytes"))
            .child(
                h_flex()
                    .gap(px(8.))
                    .child(
                        Button::new("attachment-confirm")
                            .primary()
                            .label("Send attachment")
                            .disabled(self.preview)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.send_file(window, cx)),
                            ),
                    )
                    .child(Button::new("attachment-dismiss").label("Cancel").on_click(
                        cx.listener(|this, _, _, cx| {
                            this.clear_attachment();
                            cx.notify();
                        }),
                    )),
            )
            .into_any_element()
    }
}
