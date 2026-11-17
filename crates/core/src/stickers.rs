//! Sticker packs (#183): MSC2545 image packs from your account data
//! (`im.ponies.user_emotes`), the rooms you enabled packs from
//! (`im.ponies.emote_rooms`) and the open room's own packs
//! (`im.ponies.room_emotes` state). Only images usable as stickers are kept.
use crate::auth::Account;
use matrix_sdk::ruma::{
    OwnedMxcUri, RoomId, UInt,
    events::{
        AnyMessageLikeEventContent, GlobalAccountDataEventType, StateEventType, room::ImageInfo,
        sticker::StickerEventContent,
    },
};
use serde_json::Value;

const USER_PACK: &str = "im.ponies.user_emotes";
const PACK_ROOMS: &str = "im.ponies.emote_rooms";
const ROOM_PACK: &str = "im.ponies.room_emotes";
/// Bounds on what a pack may make the picker hold.
const MAX_PACKS: usize = 50;
const MAX_STICKERS: usize = 500;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sticker {
    pub shortcode: String,
    /// Description sent as the event body.
    pub body: String,
    pub url: String,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub mimetype: Option<String>,
    pub size: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StickerPack {
    /// Stable key: where the pack lives (`user`, or room ID and state key).
    pub id: String,
    pub name: String,
    pub stickers: Vec<Sticker>,
}

#[derive(Debug, thiserror::Error)]
pub enum StickerError {
    #[error("sticker_invalid")]
    Invalid,
    #[error("sticker_send_failed")]
    Send,
}

fn usage_has(value: Option<&Value>, kind: &str) -> Option<bool> {
    value
        .and_then(Value::as_array)
        .map(|usage| usage.iter().any(|entry| entry.as_str() == Some(kind)))
}

/// One pack's content, keeping images usable as stickers: an image's own
/// `usage`, else the pack's, else both (MSC2545 default).
pub(crate) fn parse_pack(id: String, fallback_name: &str, content: &Value) -> Option<StickerPack> {
    let pack = content.get("pack");
    let pack_usage = usage_has(pack.and_then(|p| p.get("usage")), "sticker");
    let name = pack
        .and_then(|p| p.get("display_name"))
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(fallback_name)
        .to_owned();
    let images = content.get("images")?.as_object()?;
    let mut stickers: Vec<Sticker> = images
        .iter()
        .filter_map(|(shortcode, image)| {
            let url = image.get("url")?.as_str()?;
            if !OwnedMxcUri::from(url).is_valid() {
                return None;
            }
            let usable = usage_has(image.get("usage"), "sticker")
                .or(pack_usage)
                .unwrap_or(true);
            if !usable {
                return None;
            }
            let info = image.get("info");
            let number = |key: &str| info.and_then(|i| i.get(key)).and_then(Value::as_u64);
            Some(Sticker {
                shortcode: shortcode.clone(),
                body: image
                    .get("body")
                    .and_then(Value::as_str)
                    .unwrap_or(shortcode)
                    .to_owned(),
                url: url.to_owned(),
                width: number("w"),
                height: number("h"),
                mimetype: info
                    .and_then(|i| i.get("mimetype"))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                size: number("size"),
            })
        })
        .collect();
    if stickers.is_empty() {
        return None;
    }
    stickers.sort_by(|a, b| a.shortcode.cmp(&b.shortcode));
    stickers.truncate(MAX_STICKERS);
    Some(StickerPack { id, name, stickers })
}

/// `(room ID, state key)` pairs from `im.ponies.emote_rooms`.
pub(crate) fn pack_rooms(content: &Value) -> Vec<(String, String)> {
    let Some(rooms) = content.get("rooms").and_then(Value::as_object) else {
        return Vec::new();
    };
    rooms
        .iter()
        .flat_map(|(room, keys)| {
            keys.as_object()
                .into_iter()
                .flat_map(move |keys| keys.keys().map(move |key| (room.clone(), key.clone())))
        })
        .collect()
}

fn raw_json(text: &str) -> Option<Value> {
    serde_json::from_str(text).ok()
}

impl Account {
    /// Your sticker packs, then packs of rooms you enabled them from, then the
    /// open room's packs. Missing or malformed packs are skipped.
    pub async fn sticker_packs(&self, room: Option<&str>) -> Vec<StickerPack> {
        let mut packs = Vec::new();
        let account = self.client.account();
        let user = account
            .fetch_account_data(GlobalAccountDataEventType::from(USER_PACK))
            .await
            .ok()
            .flatten()
            .and_then(|raw| raw_json(raw.json().get()));
        if let Some(content) = &user
            && let Some(pack) = parse_pack("user".into(), "Your stickers", content)
        {
            packs.push(pack);
        }
        let mut sources = account
            .fetch_account_data(GlobalAccountDataEventType::from(PACK_ROOMS))
            .await
            .ok()
            .flatten()
            .and_then(|raw| raw_json(raw.json().get()))
            .map(|content| pack_rooms(&content))
            .unwrap_or_default();
        if let Some(open) = room {
            sources.push((open.to_owned(), String::new()));
        }
        let mut seen = std::collections::HashSet::new();
        for (room_id, key) in sources {
            let Ok(id) = RoomId::parse(&room_id) else {
                continue;
            };
            let Some(joined) = self.client.get_room(&id) else {
                continue;
            };
            let room_name = joined
                .cached_display_name()
                .map(|name| name.to_string())
                .unwrap_or_else(|| room_id.clone());
            let events = joined
                .get_state_events(StateEventType::from(ROOM_PACK))
                .await
                .unwrap_or_default();
            for event in events {
                let Some(json) = (match &event {
                    matrix_sdk::deserialized_responses::RawAnySyncOrStrippedState::Sync(raw) => {
                        raw_json(raw.json().get())
                    }
                    matrix_sdk::deserialized_responses::RawAnySyncOrStrippedState::Stripped(
                        raw,
                    ) => raw_json(raw.json().get()),
                }) else {
                    continue;
                };
                let state_key = json
                    .get("state_key")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                // A listed room contributes only the listed pack; the open
                // room contributes all of its packs.
                if !key.is_empty() && state_key != key {
                    continue;
                }
                let pack_id = format!("{room_id}/{state_key}");
                if !seen.insert(pack_id.clone()) {
                    continue;
                }
                if let Some(content) = json.get("content")
                    && let Some(pack) = parse_pack(pack_id, &room_name, content)
                {
                    packs.push(pack);
                }
            }
            if packs.len() >= MAX_PACKS {
                break;
            }
        }
        packs.truncate(MAX_PACKS);
        packs
    }
}

impl crate::conversation::Conversation {
    /// Sends `sticker` as an `m.sticker` event.
    pub async fn send_sticker(&self, sticker: &Sticker) -> Result<(), StickerError> {
        self.ensure_text_channel()
            .map_err(|_| StickerError::Invalid)?;
        let url = OwnedMxcUri::from(sticker.url.as_str());
        if !url.is_valid() {
            return Err(StickerError::Invalid);
        }
        let mut info = ImageInfo::new();
        info.width = sticker.width.and_then(|w| UInt::try_from(w).ok());
        info.height = sticker.height.and_then(|h| UInt::try_from(h).ok());
        info.mimetype = sticker.mimetype.clone();
        info.size = sticker.size.and_then(|s| UInt::try_from(s).ok());
        self.send_content(AnyMessageLikeEventContent::Sticker(
            StickerEventContent::new(sticker.body.clone(), info, url),
        ))
        .await
        .map_err(|_| StickerError::Send)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn packs_keep_sticker_images_and_respect_usage() {
        let content = json!({
            "pack": {"display_name": "Cats"},
            "images": {
                "wave": {"url": "mxc://example.org/wave", "body": "Waving cat", "info": {"w": 256, "h": 256, "mimetype": "image/png"}},
                "smile": {"url": "mxc://example.org/smile", "usage": ["emoticon"]},
                "bad": {"url": "https://example.org/x.png"},
                "nap": {"url": "mxc://example.org/nap", "usage": ["sticker"]}
            }
        });
        let pack = parse_pack("user".into(), "Your stickers", &content).unwrap();
        assert_eq!(pack.name, "Cats");
        let codes: Vec<&str> = pack.stickers.iter().map(|s| s.shortcode.as_str()).collect();
        assert_eq!(codes, ["nap", "wave"]);
        assert_eq!(pack.stickers[1].body, "Waving cat");
        assert_eq!(pack.stickers[1].width, Some(256));
        // An emoticon-only pack has no stickers.
        let emotes =
            json!({"pack": {"usage": ["emoticon"]}, "images": {"a": {"url": "mxc://e.org/a"}}});
        assert!(parse_pack("x".into(), "Room", &emotes).is_none());
    }

    #[test]
    fn enabled_pack_rooms_list_room_and_state_key() {
        let content = json!({"rooms": {"!a:e.org": {"": {}, "cats": {}}, "!b:e.org": {}}});
        let mut rooms = pack_rooms(&content);
        rooms.sort();
        assert_eq!(
            rooms,
            [
                ("!a:e.org".to_owned(), String::new()),
                ("!a:e.org".to_owned(), "cats".to_owned())
            ]
        );
    }
}
