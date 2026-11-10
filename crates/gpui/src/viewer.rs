//! In-memory image overlay. Saving is explicit, including before opening another app.
use crate::{
    app::MatrixApp,
    backend::{Message, MessageKind},
};
use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants},
        spinner::Spinner,
        *,
    },
    *,
};
use matrix_client_core::conversation::{ImageError, ViewerImage};
use std::sync::Arc;

gpui_kit::actions!(
    matrix_viewer,
    [
        CloseViewer,
        PreviousImage,
        NextImage,
        ZoomIn,
        ZoomOut,
        FitImage,
        SaveImage,
        CopyImage,
        OpenImage,
        RevealImage
    ]
);
pub(super) fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("escape", CloseViewer, Some("ImageViewer")),
        KeyBinding::new("left", PreviousImage, Some("ImageViewer")),
        KeyBinding::new("right", NextImage, Some("ImageViewer")),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-+"
            } else {
                "ctrl-+"
            },
            ZoomIn,
            Some("ImageViewer"),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-="
            } else {
                "ctrl-="
            },
            ZoomIn,
            Some("ImageViewer"),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-shift-="
            } else {
                "ctrl-shift-="
            },
            ZoomIn,
            Some("ImageViewer"),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd--"
            } else {
                "ctrl--"
            },
            ZoomOut,
            Some("ImageViewer"),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-0"
            } else {
                "ctrl-0"
            },
            FitImage,
            Some("ImageViewer"),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-s"
            } else {
                "ctrl-s"
            },
            SaveImage,
            Some("ImageViewer"),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-c"
            } else {
                "ctrl-c"
            },
            CopyImage,
            Some("ImageViewer"),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-o"
            } else {
                "ctrl-o"
            },
            OpenImage,
            Some("ImageViewer"),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-shift-l"
            } else {
                "ctrl-shift-l"
            },
            RevealImage,
            Some("ImageViewer"),
        ),
    ]
}
#[derive(Clone)]
pub(super) struct ImageEntry {
    pub message: Message,
    pub in_thread: bool,
}
pub(super) enum ImageLoad {
    Loading,
    Ready {
        image: Arc<ViewerImage>,
        render: Arc<RenderImage>,
    },
    Failed(ImageError),
}
pub(super) struct Viewer {
    pub room: String,
    pub entries: Vec<ImageEntry>,
    pub index: usize,
    pub request: u64,
    pub load: ImageLoad,
    pub zoom: f32,
    pub pan: Point<Pixels>,
    pub drag: Option<(Point<Pixels>, Point<Pixels>)>,
    pub focus: FocusHandle,
    pub cancel: tokio::sync::watch::Sender<bool>,
    pub status: String,
    pub previous_pending: bool,
}
fn failure(error: ImageError) -> &'static str {
    match error {
        ImageError::Limit => {
            "This image is too large to display safely (8192 px per side, 96 MB decoded or 240 frames). Save it to view in another app."
        }
        ImageError::Corrupt => "This image is corrupt or uses an unsupported format.",
        ImageError::Download => {
            "The image could not be downloaded. Check your connection and try again."
        }
        ImageError::Unavailable => "This image is no longer available in the conversation.",
        ImageError::Cancelled => "Image loading cancelled.",
        ImageError::Save => "The image could not be saved. Choose another location and try again.",
    }
}
impl Viewer {
    fn geometry(&self, window: &Window) -> (gpui_kit::Size<Pixels>, gpui_kit::Size<Pixels>) {
        let viewport = window.viewport_size();
        let available = size(
            (viewport.width - px(96.)).max(px(1.)),
            (viewport.height - px(136.)).max(px(1.)),
        );
        let rendered = if let ImageLoad::Ready { image, .. } = &self.load {
            let fit = (f32::from(available.width) / image.width as f32)
                .min(f32::from(available.height) / image.height as f32)
                // Small images are enlarged up to 2× so they don't sit tiny in
                // the window; large ones fit. ⌘0 still shows the fitted size.
                .min(2.);
            size(
                px(image.width as f32 * fit * self.zoom),
                px(image.height as f32 * fit * self.zoom),
            )
        } else {
            size(px(0.), px(0.))
        };
        (
            size(viewport.width, (viewport.height - px(104.)).max(px(1.))),
            rendered,
        )
    }
    pub(super) fn clamp_pan(&mut self, window: &Window) {
        let (viewport, image) = self.geometry(window);
        let max_x = ((image.width - viewport.width) / 2.).max(px(0.));
        let max_y = ((image.height - viewport.height) / 2.).max(px(0.));
        self.pan.x = self.pan.x.clamp(-max_x, max_x);
        self.pan.y = self.pan.y.clamp(-max_y, max_y);
    }
}
impl MatrixApp {
    pub fn preview_viewer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.preview {
            return;
        }
        let Some(sample) = self
            .snapshot
            .messages
            .iter()
            .find(|m| m.kind == MessageKind::Image)
            .cloned()
        else {
            return;
        };
        for ix in 1..3 {
            let mut message = sample.clone();
            message.id = format!("sample-image-{ix}");
            message.event_id = Some(format!("$sample-image-{ix}:preview.invalid"));
            message.timestamp_ms += ix * 60_000;
            self.snapshot.messages.push(message);
        }
        for message in &mut self.snapshot.messages {
            if message.kind == MessageKind::Image
                && let Some(attachment) = &mut message.attachment
            {
                attachment.preview_png = Some(Arc::new(
                    include_bytes!("../assets/attachment-sample.png").to_vec(),
                ));
            }
        }
        self.groups = crate::model::grouped(&self.snapshot.messages);
        self.scroller
            .update(cx, |s, cx| s.reset(self.groups.len(), cx));
        self.open_viewer(sample.id, false, window, cx);
    }
    pub fn image_focus(&self, id: &str, cx: &mut Context<Self>) -> FocusHandle {
        let mut handles = self.image_focus.borrow_mut();
        if handles.len() >= 512 && !handles.contains_key(id) {
            handles.clear();
        }
        handles
            .entry(id.to_owned())
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone()
    }
    fn image_entries(&self) -> Vec<ImageEntry> {
        let mut entries: Vec<_> = self
            .snapshot
            .messages
            .iter()
            .filter(|m| m.kind == MessageKind::Image && m.event_id.is_some())
            .cloned()
            .map(|message| ImageEntry {
                message,
                in_thread: false,
            })
            .collect();
        if let Some(thread) = &self.thread {
            let mut after = 0;
            for message in std::iter::once(&thread.root)
                .chain(&thread.messages)
                .filter(|m| m.kind == MessageKind::Image && m.event_id.is_some())
            {
                if let Some(ix) = entries
                    .iter()
                    .position(|e| e.message.event_id == message.event_id)
                {
                    after = ix + 1;
                    continue;
                }
                let ix = (after..entries.len())
                    .find(|&ix| entries[ix].message.timestamp_ms > message.timestamp_ms)
                    .unwrap_or(entries.len());
                entries.insert(
                    ix,
                    ImageEntry {
                        message: message.clone(),
                        in_thread: true,
                    },
                );
                after = ix + 1;
            }
        }
        let mut seen = std::collections::HashSet::new();
        entries.retain(|e| seen.insert(e.message.event_id.clone()));
        entries
    }
    pub fn open_viewer(
        &mut self,
        id: String,
        _in_thread: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entries = self.image_entries();
        let root_event = self
            .thread
            .as_ref()
            .filter(|t| t.root.id == id)
            .and_then(|t| t.root.event_id.clone());
        let Some(index) = entries.iter().position(|e| {
            e.message.id == id || (root_event.is_some() && e.message.event_id == root_event)
        }) else {
            return;
        };
        if let Some(old) = self.viewer.take() {
            old.cancel.send_replace(true);
        }
        self.viewer_request = self.viewer_request.wrapping_add(1);
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        self.viewer = Some(Viewer {
            room: self.snapshot.selected_room.clone(),
            entries,
            index,
            request: self.viewer_request,
            load: ImageLoad::Loading,
            zoom: 1.,
            pan: point(px(0.), px(0.)),
            drag: None,
            focus,
            cancel: tokio::sync::watch::channel(false).0,
            status: if self.preview {
                "Sample data · Image viewer".into()
            } else {
                String::new()
            },
            previous_pending: false,
        });
        self.load_viewer(window, cx);
    }
    pub fn load_viewer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(viewer) = &mut self.viewer else {
            return;
        };
        viewer.cancel.send_replace(true);
        self.viewer_request = self.viewer_request.wrapping_add(1);
        viewer.request = self.viewer_request;
        viewer.cancel = tokio::sync::watch::channel(false).0;
        let cancel = viewer.cancel.subscribe();
        viewer.load = ImageLoad::Loading;
        viewer.status = if self.preview {
            "Sample data · Image viewer".into()
        } else {
            String::new()
        };
        viewer.zoom = 1.;
        viewer.pan = point(px(0.), px(0.));
        viewer.drag = None;
        let request = viewer.request;
        let account = self.snapshot.account_instance;
        let room = viewer.room.clone();
        let id = viewer.entries[viewer.index].message.id.clone();
        if self.preview {
            let result = matrix_client_core::conversation::decode_viewer_image(
                include_bytes!("../assets/attachment-sample.png").to_vec(),
                "Sample landscape.png",
            )
            .map(|mut image| {
                let render = crate::backend::render_viewer_image(&mut image);
                (Arc::new(image), render)
            });
            self.finish_viewer(request, account, result, cx);
        } else {
            self.job(
                window,
                cx,
                move |b| b.viewer_image(room, account, id, cancel),
                move |this, result, _, cx| this.finish_viewer(request, account, result, cx),
            );
        }
        cx.notify();
    }
    pub fn finish_viewer(
        &mut self,
        request: u64,
        account: u128,
        result: Result<(Arc<ViewerImage>, Arc<RenderImage>), ImageError>,
        cx: &mut Context<Self>,
    ) {
        if self.snapshot.account_instance != account {
            return;
        }
        if let Some(v) = &mut self.viewer
            && v.request == request
        {
            v.load = match result {
                Ok((image, render)) => ImageLoad::Ready { image, render },
                Err(error) => ImageLoad::Failed(error),
            };
            cx.notify();
        }
    }
    pub fn close_viewer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(viewer) = self.viewer.take() else {
            return;
        };
        viewer.cancel.send_replace(true);
        let entry = &viewer.entries[viewer.index];
        if entry.in_thread {
            if let Some(t) = &self.thread
                && let Some(ix) = t.groups.iter().position(|g| {
                    t.messages[g.clone()]
                        .iter()
                        .any(|m| m.event_id == entry.message.event_id)
                })
            {
                t.scroller.update(cx, |s, cx| s.scroll_to_item(ix, cx));
            }
        } else if let Some(ix) = self.groups.iter().position(|g| {
            self.snapshot.messages[g.clone()]
                .iter()
                .any(|m| m.event_id == entry.message.event_id)
        }) {
            self.scroller.update(cx, |s, cx| s.scroll_to_item(ix, cx));
        }
        self.reveal_image = Some(entry.message.id.clone());
        self.image_focus(&entry.message.id, cx).focus(window, cx);
        cx.notify();
    }
    pub fn step_image(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(v) = &mut self.viewer else {
            return;
        };
        if delta < 0 && v.index == 0 && !self.preview && !self.pagination_exhausted {
            v.previous_pending = true;
            v.status = "Looking for earlier images…".into();
            self.load_earlier(window, cx);
            return;
        }
        let next = v
            .index
            .saturating_add_signed(delta)
            .min(v.entries.len().saturating_sub(1));
        if next == v.index {
            return;
        }
        v.index = next;
        v.previous_pending = false;
        self.load_viewer(window, cx);
    }
    pub fn refresh_viewer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.viewer.is_none() {
            return;
        }
        let entries = self.image_entries();
        let v = self.viewer.as_mut().expect("viewer");
        if v.room != self.snapshot.selected_room {
            v.cancel.send_replace(true);
            self.viewer = None;
            return;
        }
        let event = v.entries[v.index].message.event_id.clone();
        let Some(index) = entries.iter().position(|e| e.message.event_id == event) else {
            v.cancel.send_replace(true);
            self.viewer = None;
            return;
        };
        v.entries = entries;
        v.index = index;
        if v.previous_pending && index > 0 {
            v.previous_pending = false;
            self.step_image(-1, window, cx);
        }
    }
    pub fn zoom_viewer(&mut self, factor: f32, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(v) = &mut self.viewer
            && factor.is_finite()
            && factor > 0.
        {
            v.zoom = (v.zoom * factor).clamp(0.25, 16.);
            v.clamp_pan(window);
            cx.notify();
        }
    }
    pub fn fit_viewer(&mut self, cx: &mut Context<Self>) {
        if let Some(v) = &mut self.viewer {
            v.zoom = 1.;
            v.pan = point(px(0.), px(0.));
            cx.notify();
        }
    }
    pub fn copy_viewer(&mut self, cx: &mut Context<Self>) {
        if let Some(v) = &mut self.viewer
            && let ImageLoad::Ready { image, .. } = &v.load
        {
            cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(
                ImageFormat::Png,
                image.clipboard_png.as_ref().clone(),
            )));
            v.status = if self.preview {
                "Sample data · Image copied"
            } else {
                "Image copied"
            }
            .into();
            cx.notify();
        }
    }
    pub fn save_viewer(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(v) = &self.viewer else {
            return;
        };
        let ImageLoad::Ready { image, .. } = &v.load else {
            if !self.preview
                && matches!(
                    v.load,
                    ImageLoad::Failed(ImageError::Limit | ImageError::Corrupt)
                )
            {
                let entry = v.entries[v.index].message.clone();
                let filename = entry
                    .attachment
                    .as_ref()
                    .map(|a| a.filename.clone())
                    .unwrap_or_else(|| "image".into());
                self.save_attachment(entry.id, filename, open, window, cx);
            }
            return;
        };
        let image = image.clone();
        let request = v.request;
        let account = self.snapshot.account_instance;
        let selected = cx.prompt_for_new_path(std::path::Path::new("."), Some(&image.filename));
        cx.spawn_in(window, async move |view, cx| {
            if let Ok(Ok(Some(path))) = selected.await {
                let current = view
                    .update(cx, |this, _| {
                        this.snapshot.account_instance == account
                            && this.viewer.as_ref().is_some_and(|v| v.request == request)
                    })
                    .unwrap_or(false);
                if !current {
                    return;
                }
                let target = path.clone();
                let result = cx
                    .background_spawn(async move { image.save_blocking(&path) })
                    .await;
                let _ = view.update(cx, |this, cx| {
                    if this.snapshot.account_instance == account
                        && let Some(v) = &mut this.viewer
                        && v.request == request
                    {
                        v.status = match result {
                            Ok(()) => {
                                if open {
                                    cx.open_with_system(&target);
                                }
                                "Image saved.".into()
                            }
                            Err(e) => failure(e).into(),
                        };
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }
    pub fn reveal_viewer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_viewer(window, cx);
    }
    pub fn viewer_view(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let v = self.viewer.as_ref().expect("viewer");
        let entry = &v.entries[v.index].message;
        let (viewport, rendered) = v.geometry(window);
        let ready = matches!(v.load, ImageLoad::Ready { .. });
        let saveable = ready
            || (!self.preview
                && matches!(
                    v.load,
                    ImageLoad::Failed(ImageError::Limit | ImageError::Corrupt)
                ));
        let mut canvas = div()
            .id("viewer-canvas")
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.close_viewer(window, cx)),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if let Some(v) = &mut this.viewer
                    && let Some((start, pan)) = v.drag
                    && event.pressed_button == Some(MouseButton::Left)
                {
                    v.pan = pan + event.position - start;
                    v.clamp_pan(window);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    if let Some(v) = &mut this.viewer {
                        v.drag = None;
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    if let Some(v) = &mut this.viewer {
                        v.drag = None;
                    }
                }),
            )
            .on_pinch(cx.listener(|this, event: &PinchEvent, window, cx| {
                this.zoom_viewer(1. + event.delta, window, cx);
                cx.stop_propagation();
            }))
            .on_scroll(cx.listener(|this, event: &ScrollEvent, window, cx| {
                if let Some(v) = &mut this.viewer
                    && v.zoom > 1.
                {
                    v.pan += event.delta.pixel_delta(px(20.));
                    v.clamp_pan(window);
                    cx.notify();
                }
                cx.stop_propagation();
            }));
        canvas = match &v.load {
            ImageLoad::Ready { render, .. } => canvas.child(
                div()
                    .id("viewer-image")
                    .absolute()
                    .left((viewport.width - rendered.width) / 2. + v.pan.x)
                    .top((viewport.height - rendered.height) / 2. + v.pan.y)
                    .w(rendered.width)
                    .h(rendered.height)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, _, cx| {
                            if let Some(v) = &mut this.viewer {
                                v.drag = Some((event.position, v.pan));
                            }
                            cx.stop_propagation();
                        }),
                    )
                    .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                        if event.click_count() == 2 {
                            if this.viewer.as_ref().is_some_and(|v| v.zoom > 1.) {
                                this.fit_viewer(cx);
                            } else {
                                this.zoom_viewer(2., window, cx);
                            }
                        }
                        cx.stop_propagation();
                    }))
                    .child(
                        img(render.clone())
                            .id("viewer-pixels")
                            .size_full()
                            .object_fit(ObjectFit::Contain),
                    )
                    .test_support(),
            ),
            ImageLoad::Loading => canvas.child(
                v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .gap(px(8.))
                    .child(Spinner::new())
                    .child("Loading image…"),
            ),
            ImageLoad::Failed(error) => canvas.child(
                v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .gap(px(12.))
                    .child(div().max_w(px(420.)).child(failure(*error)))
                    .child(
                        Button::new("viewer-retry")
                            .label("Try Again")
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.load_viewer(window, cx)),
                            ),
                    ),
            ),
        };
        let button = |id, label, icon| {
            Button::new(id)
                .ghost()
                .size(px(24.))
                .w(px(28.))
                .h(px(28.))
                .icon(icon)
                .accessibility_label(label)
                .tooltip(label)
        };
        let time = chrono::DateTime::from_timestamp_millis(entry.timestamp_ms)
            .map(|d| {
                d.with_timezone(&chrono::Local)
                    .format("%b %-d · %H:%M")
                    .to_string()
            })
            .unwrap_or_default();
        v_flex()
            .id("image-viewer")
            .absolute()
            .inset_0()
            .size_full()
            .bg(cx.theme().background)
            .track_focus(&v.focus)
            .key_context("ImageViewer")
            .child(
                self.toolbar("viewer-toolbar", cx)
                    .pl(px(if cfg!(target_os = "macos") { 96. } else { 16. }))
                    .pr(px(12.))
                    .gap(px(10.))
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        button(
                            "viewer-previous",
                            "Previous image  ←",
                            assets::IconName::ChevronLeft,
                        )
                        .disabled(v.index == 0 && (self.preview || self.pagination_exhausted))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.step_image(-1, window, cx)),
                        ),
                    )
                    .child(div().child(format!("{} / {}", v.index + 1, v.entries.len())))
                    .child(
                        button(
                            "viewer-next",
                            "Next image  →",
                            assets::IconName::ChevronRight,
                        )
                        .disabled(v.index + 1 == v.entries.len())
                        .on_click(
                            cx.listener(|this, _, window, cx| this.step_image(1, window, cx)),
                        ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{} · {time}", entry.display_name)),
                    )
                    .child(
                        button("viewer-zoom-out", "Zoom out  ⌘−", assets::IconName::ZoomOut)
                            .disabled(!ready)
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.zoom_viewer(0.8, window, cx)
                                }),
                            ),
                    )
                    .child(
                        button(
                            "viewer-fit",
                            "Fit to Window  ⌘0",
                            assets::IconName::Maximize,
                        )
                        .disabled(!ready)
                        .on_click(cx.listener(|this, _, _, cx| this.fit_viewer(cx))),
                    )
                    .child(
                        button("viewer-zoom-in", "Zoom in  ⌘+", assets::IconName::ZoomIn)
                            .disabled(!ready)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.zoom_viewer(1.25, window, cx)
                            })),
                    )
                    .child(
                        button("viewer-close", "Close image  Escape", assets::IconName::X)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.close_viewer(window, cx)),
                            ),
                    ),
            )
            .child(canvas.test_support())
            .child(
                h_flex()
                    .h(px(52.))
                    .px(px(16.))
                    .gap(px(10.))
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("viewer-save")
                            .ghost()
                            .small()
                            .label("Save…")
                            .tooltip("Save Image…  ⌘S")
                            .disabled(!saveable)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save_viewer(false, window, cx)
                            })),
                    )
                    .child(
                        Button::new("viewer-copy")
                            .ghost()
                            .small()
                            .label("Copy Image")
                            .tooltip("Copy Image  ⌘C")
                            .disabled(!ready)
                            .on_click(cx.listener(|this, _, _, cx| this.copy_viewer(cx))),
                    )
                    .child(
                        Button::new("viewer-open")
                            .ghost()
                            .small()
                            .label("Open in Default App")
                            .tooltip("Save a copy, then open in the default app  ⌘O")
                            .disabled(!saveable)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save_viewer(true, window, cx)
                            })),
                    )
                    .child(
                        Button::new("viewer-reveal")
                            .ghost()
                            .small()
                            .label("Show in Timeline")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.reveal_viewer(window, cx)),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(cx.theme().muted_foreground)
                            .child(v.status.clone()),
                    ),
            )
            .on_action(
                cx.listener(|this, _: &CloseViewer, window, cx| this.close_viewer(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PreviousImage, window, cx| this.step_image(-1, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &NextImage, window, cx| this.step_image(1, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ZoomIn, window, cx| this.zoom_viewer(1.25, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ZoomOut, window, cx| this.zoom_viewer(0.8, window, cx)),
            )
            .on_action(cx.listener(|this, _: &FitImage, _, cx| this.fit_viewer(cx)))
            .on_action(
                cx.listener(|this, _: &SaveImage, window, cx| this.save_viewer(false, window, cx)),
            )
            .on_action(cx.listener(|this, _: &CopyImage, _, cx| this.copy_viewer(cx)))
            .on_action(
                cx.listener(|this, _: &OpenImage, window, cx| this.save_viewer(true, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &RevealImage, window, cx| this.reveal_viewer(window, cx)),
            )
            .focus_trap("viewer-focus", &v.focus)
            .into_any_element()
    }
}
