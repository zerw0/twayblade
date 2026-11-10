//! Avatar images. Each `mxc://` URL is fetched once on a background worker
//! (bounded and decoded in core) and cached as render data; initials show until
//! it arrives, and permanently if it fails.
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use gpui_kit::*;

use crate::theme::scaled_px as px;
use crate::{app::MatrixApp, theme};

/// Fetched at one size and scaled down: crisp on Retina up to 48pt avatars.
const FETCH_PIXELS: u32 = 96;
/// Plenty for every member and sender on screen; cleared wholesale if exceeded.
const MAX_ENTRIES: usize = 4096;

pub(super) enum AvatarState {
    Loading,
    Ready(Arc<RenderImage>),
    Failed,
}

#[derive(Clone, Default)]
pub(super) struct AvatarCache(Rc<RefCell<HashMap<String, AvatarState>>>);

impl MatrixApp {
    /// An avatar for `mxc`, falling back to coloured initials of `name`, seeded by
    /// `seed` (a Matrix ID) so the fallback colour is stable.
    pub(super) fn avatar_image(
        &self,
        mxc: Option<&str>,
        seed: &str,
        name: &str,
        size: f32,
        radius: f32,
        cx: &Context<Self>,
    ) -> Div {
        let fallback = || {
            crate::workspace::avatar(
                &theme::initials(name),
                theme::avatar_color(seed),
                size,
                radius,
                &theme::palette(cx),
            )
        };
        let Some(mxc) = mxc.filter(|mxc| mxc.starts_with("mxc://")) else {
            return fallback();
        };
        if self.preview {
            return fallback();
        }
        let mut cache = self.avatars.0.borrow_mut();
        match cache.get(mxc) {
            Some(AvatarState::Ready(image)) => {
                return div()
                    .flex_none()
                    .size(px(size))
                    .rounded(px(radius))
                    .overflow_hidden()
                    .child(img(image.clone()).size_full());
            }
            Some(AvatarState::Loading | AvatarState::Failed) => return fallback(),
            None => {}
        }
        let Some(backend) = self.backend.clone() else {
            return fallback();
        };
        if cache.len() >= MAX_ENTRIES {
            cache.clear();
        }
        cache.insert(mxc.to_owned(), AvatarState::Loading);
        drop(cache);
        let key = mxc.to_owned();
        let avatars = self.avatars.clone();
        cx.spawn(async move |this, cx| {
            let fetch_key = key.clone();
            let result = cx
                .background_spawn(async move { backend.avatar(fetch_key, FETCH_PIXELS) })
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
        fallback()
    }

    /// Avatar URL for a user from the loaded room members, if any.
    pub(super) fn member_avatar(&self, user_id: &str) -> Option<&str> {
        if user_id == self.snapshot.user_id
            && let Some(own) = self.own_avatar.as_deref()
        {
            return Some(own);
        }
        self.snapshot
            .members
            .iter()
            .find(|member| member.user_id == user_id)
            .and_then(|member| member.avatar.as_deref())
    }
}
