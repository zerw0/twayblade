//! Link previews under messages (Settings › Chat). The homeserver fetches the
//! page, so asking reveals the link to it: encrypted rooms only preview when
//! the person opted in separately.
use super::*;
use crate::backend::Message;
use std::sync::Arc;

/// Requests in flight at once; the rest wait for the next pass.
const IN_FLIGHT: usize = 3;
/// Previews kept in memory before the cache starts over.
const CACHE: usize = 300;
/// Newest messages per timeline considered for a preview.
const SCAN: usize = 80;

pub(crate) enum LinkPreviewState {
    Loading,
    Ready(Arc<LinkCard>),
    /// Failed or nothing to show; not asked again this session.
    Nothing,
}

pub(crate) struct LinkCard {
    pub url: String,
    pub site: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<Arc<Vec<u8>>>,
}

impl MatrixApp {
    /// Whether the open room may show previews under the current settings.
    pub(crate) fn link_previews_allowed(&self) -> bool {
        let chat = &self.preferences.chat;
        if !chat.link_previews {
            return false;
        }
        let encrypted = self
            .snapshot
            .rooms
            .iter()
            .find(|room| room.id == self.snapshot.selected_room)
            .is_none_or(|room| room.encrypted);
        !encrypted || chat.link_previews_encrypted
    }

    /// Asks for previews of links in the newest messages that have none yet.
    pub(crate) fn request_link_previews(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.preview || self.backend.is_none() || !self.link_previews_allowed() {
            return;
        }
        let in_flight = self
            .link_previews
            .values()
            .filter(|state| matches!(state, LinkPreviewState::Loading))
            .count();
        if in_flight >= IN_FLIGHT {
            return;
        }
        let mut wanted = Vec::new();
        for in_thread in [false, true] {
            for message in self.timeline_messages(in_thread).iter().rev().take(SCAN) {
                if let Some(url) = model::preview_link(message)
                    && !self.link_previews.contains_key(&url)
                    && !wanted.contains(&url)
                {
                    wanted.push(url);
                }
            }
        }
        if wanted.is_empty() {
            return;
        }
        if self.link_previews.len() + wanted.len() > CACHE {
            self.link_previews
                .retain(|_, state| matches!(state, LinkPreviewState::Loading));
        }
        for url in wanted.into_iter().take(IN_FLIGHT - in_flight) {
            self.link_previews
                .insert(url.clone(), LinkPreviewState::Loading);
            let account = self.snapshot.account_instance;
            let key = url.clone();
            self.job(
                window,
                cx,
                move |backend| backend.link_preview(url),
                move |this, result, window, cx| {
                    // A preview from another account must not land here.
                    if this.snapshot.account_instance != account {
                        this.link_previews.remove(&key);
                        return;
                    }
                    let state = match result {
                        Some(preview) => LinkPreviewState::Ready(Arc::new(LinkCard {
                            url: preview.url,
                            site: preview.site_name,
                            title: preview.title,
                            description: preview.description,
                            image: preview.image_png.map(Arc::new),
                        })),
                        None => LinkPreviewState::Nothing,
                    };
                    let shown = matches!(state, LinkPreviewState::Ready(_));
                    this.link_previews.insert(key.clone(), state);
                    if shown {
                        // The row grew: measure it again.
                        this.scroller.update(cx, |state, cx| state.remeasure(cx));
                        if let Some(thread) = &this.thread {
                            thread.scroller.update(cx, |state, cx| state.remeasure(cx));
                        }
                    }
                    this.request_link_previews(window, cx);
                },
            );
        }
    }

    /// The preview card for a message, once one has loaded.
    pub(crate) fn link_preview_card(
        &self,
        message: &Message,
        outgoing: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.link_previews_allowed() {
            return None;
        }
        let url = model::preview_link(message)?;
        let LinkPreviewState::Ready(card) = self.link_previews.get(&url)? else {
            return None;
        };
        let card = card.clone();
        let (text, muted, border) = if outgoing {
            let on = cx.theme().primary_foreground;
            (on, on.opacity(0.75), on.opacity(0.3))
        } else {
            (
                cx.theme().foreground,
                cx.theme().muted_foreground,
                cx.theme().border,
            )
        };
        let image = card
            .image
            .as_ref()
            .and_then(|bytes| crate::ui::attachments::render_png(bytes, cx));
        let site = card.site.clone().or_else(|| {
            url::Url::parse(&card.url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_owned))
        });
        let open = card.url.clone();
        let tooltip = SharedString::from(card.url.clone());
        Some(
            h_flex()
                .id(SharedString::from(format!("link-preview-{}", message.id)))
                .mt(px(2.))
                .max_w(px(420.))
                .min_w_0()
                .items_start()
                .gap(px(10.))
                .p(px(8.))
                .rounded(px(8.))
                .border_1()
                .border_color(border)
                .cursor_pointer()
                .hover(|card| card.bg(muted.opacity(0.08)))
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    cx.open_url(&open);
                })
                .when_some(image, |row, image| {
                    row.child(
                        div()
                            .flex_none()
                            .size(px(56.))
                            .rounded(px(6.))
                            .overflow_hidden()
                            .child(img(image).size_full().object_fit(ObjectFit::Cover)),
                    )
                })
                .child(
                    v_flex()
                        .min_w_0()
                        .flex_1()
                        .gap(px(2.))
                        .when_some(site, |col, site| {
                            col.child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(muted)
                                    .truncate()
                                    .child(site),
                            )
                        })
                        .when_some(card.title.clone(), |col, title| {
                            col.child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(text)
                                    .line_clamp(2)
                                    .child(title),
                            )
                        })
                        .when_some(card.description.clone(), |col, description| {
                            col.child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(muted)
                                    .line_clamp(2)
                                    .child(description),
                            )
                        }),
                )
                .test_support()
                .into_any_element(),
        )
    }
}
