//! Stickers (#183): the picker opened from the composer's ＋ menu and the
//! sticker pictures it shows. Incoming stickers render in `timeline`.
use super::*;
use crate::backend::TimelineAction;
use crate::ui::avatars::AvatarState;
use crate::ui::spaces::ManageKind;
use twayblade_core::stickers::{Sticker, StickerPack};

/// Picker cells, in points; pictures are fetched at twice that for Retina.
const CELL: f32 = 72.;
const PICTURE: f32 = 60.;

/// Sample packs for the offline preview (pictures show as placeholders).
fn sample_packs() -> Vec<StickerPack> {
    let sticker = |code: &str| Sticker {
        shortcode: code.into(),
        body: code.into(),
        url: format!("mxc://preview.invalid/{code}"),
        ..Default::default()
    };
    vec![
        StickerPack {
            id: "user".into(),
            name: "Your stickers".into(),
            stickers: ["wave", "thumbs-up", "party", "coffee"]
                .map(sticker)
                .to_vec(),
        },
        StickerPack {
            id: "!design/".into(),
            name: "Design team".into(),
            stickers: ["ship-it", "lgtm", "rubber-duck"].map(sticker).to_vec(),
        },
    ]
}

impl MatrixApp {
    /// A sticker picture fitted into `size` points, or a quiet placeholder
    /// while it loads (or if it cannot).
    pub(crate) fn sticker_image(&self, mxc: &str, size: f32, cx: &Context<Self>) -> Div {
        let placeholder = || {
            div()
                .flex_none()
                .size(px(size))
                .rounded(px(8.))
                .bg(cx.theme().secondary)
        };
        if self.preview || !mxc.starts_with("mxc://") {
            return placeholder();
        }
        let key = format!("sticker:{mxc}");
        let mut cache = self.avatars.0.borrow_mut();
        match cache.get(&key) {
            Some(AvatarState::Ready(image)) => {
                return div().flex_none().size(px(size)).child(
                    img(image.clone())
                        .size_full()
                        .object_fit(ObjectFit::Contain),
                );
            }
            Some(AvatarState::Loading | AvatarState::Failed) => return placeholder(),
            None => {}
        }
        let Some(backend) = self.backend.clone() else {
            return placeholder();
        };
        cache.insert(key.clone(), AvatarState::Loading);
        drop(cache);
        let avatars = self.avatars.clone();
        let fetch = mxc.to_owned();
        let pixels = (size * 2.).round() as u32;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { backend.sticker_picture(fetch, pixels) })
                .await;
            let _ = this.update(cx, |_, cx| {
                let state = result
                    .ok()
                    .and_then(|png| {
                        Image::from_bytes(ImageFormat::Png, png)
                            .to_image_data(cx.svg_renderer())
                            .ok()
                    })
                    .map_or(AvatarState::Failed, AvatarState::Ready);
                avatars.0.borrow_mut().insert(key, state);
                cx.notify();
            });
        })
        .detach();
        placeholder()
    }

    /// Opens the sticker picker for the selected room and loads its packs.
    pub(crate) fn open_sticker_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(room) = self.navigation.selected_room.clone() else {
            return;
        };
        self.open_management(ManageKind::Stickers(room.clone()), window, cx);
        let Some(page) = &mut self.management else {
            return;
        };
        if self.preview {
            page.sticker_packs = Some(sample_packs());
            cx.notify();
            return;
        }
        let request = page.request;
        self.job(
            window,
            cx,
            move |backend| backend.sticker_packs(Some(room)),
            move |this, result, _, cx| {
                let Some(page) = this.management.as_mut().filter(|p| p.request == request) else {
                    return;
                };
                match result {
                    Ok(packs) => page.sticker_packs = Some(packs),
                    Err(error) => {
                        page.sticker_packs = Some(Vec::new());
                        page.status = error.to_string();
                    }
                }
                cx.notify();
            },
        );
        cx.notify();
    }

    fn send_sticker(&mut self, sticker: Sticker, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview {
            return;
        }
        // Only into the room the picker was opened for.
        let picked_for = match self.management.as_ref().map(|page| &page.kind) {
            Some(ManageKind::Stickers(room)) => Some(room.clone()),
            _ => None,
        };
        if picked_for.is_none() || picked_for != self.navigation.selected_room {
            self.close_management_page(window, cx);
            return;
        }
        let id = format!("sticker-{}", sticker.url);
        self.close_management_page(window, cx);
        self.timeline_action(id, TimelineAction::SendSticker(sticker), window, cx);
    }

    /// The picker's packs and grids.
    pub(crate) fn sticker_dialog(
        &self,
        page: &crate::ui::spaces::Management,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let Some(packs) = &page.sticker_packs else {
            return div()
                .text_color(muted)
                .child("Loading stickers…")
                .into_any_element();
        };
        if packs.is_empty() {
            return v_flex()
                .gap(px(6.))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("No sticker packs yet"),
                )
                .child(div().text_color(muted).child(
                    "Sticker packs you add in another Matrix app, or that this room \
                             offers, appear here.",
                ))
                .into_any_element();
        }
        let mut body = v_flex().w_full().gap(px(14.));
        for pack in packs {
            let mut grid = h_flex().w_full().flex_wrap().gap(px(4.));
            for sticker in &pack.stickers {
                let chosen = sticker.clone();
                grid = grid.child(
                    Button::new(SharedString::from(format!(
                        "sticker-{}-{}",
                        pack.id, sticker.shortcode
                    )))
                    .ghost()
                    .size(px(CELL))
                    .p_0()
                    .rounded(px(10.))
                    .accessibility_label(format!("Send sticker {}", sticker.body))
                    .tooltip(sticker.body.clone())
                    .disabled(self.preview)
                    .child(self.sticker_image(&sticker.url, PICTURE, cx))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.send_sticker(chosen.clone(), window, cx)
                    })),
                );
            }
            body = body.child(
                v_flex()
                    .w_full()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(muted)
                            .child(pack.name.clone()),
                    )
                    .child(grid),
            );
        }
        body.into_any_element()
    }
}
