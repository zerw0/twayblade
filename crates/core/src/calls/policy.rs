use matrix_sdk::widget::{Capabilities, CapabilitiesProvider};
use serde_json::Value;

/// SDK enforces room scope; writes additionally restrict membership state keys
/// to this exact device/user. No timeline messages, reactions, media downloads,
/// ringing, rageshakes, redactions or legacy call signalling are granted.
pub struct CallCapabilities {
    user: String,
    device: String,
}
const MEMBER: &str = "org.matrix.msc3401.call.member";
const KEYS: &str = "io.element.call.encryption_keys";
impl CallCapabilities {
    pub fn new(user: &str, device: &str) -> Self {
        Self {
            user: user.into(),
            device: device.into(),
        }
    }
    pub fn grant(&self, requested: &str) -> bool {
        if [
            "io.element.requires_client",
            "org.matrix.msc4157.send.delayed_event",
            "org.matrix.msc4157.update_delayed_event",
        ]
        .contains(&requested)
        {
            return true;
        }
        for prefix in [
            "org.matrix.msc3819.send.to_device:",
            "org.matrix.msc3819.receive.to_device:",
            "org.matrix.msc2762.send.event:",
            "org.matrix.msc2762.receive.event:",
        ] {
            if requested.strip_prefix(prefix) == Some(KEYS) {
                return true;
            }
        }
        if requested == format!("org.matrix.msc2762.receive.state_event:{MEMBER}") {
            return true;
        }
        if ["m.room.create", "m.room.name", "m.room.encryption"]
            .iter()
            .any(|event| requested == format!("org.matrix.msc2762.receive.state_event:{event}#"))
        {
            return true;
        }
        if requested
            == format!(
                "org.matrix.msc2762.receive.state_event:m.room.member#{}",
                self.user
            )
        {
            return true;
        }
        [
            self.user.clone(),
            format!("_{}_{}_m.call", self.user, self.device),
            format!("{}_{}_m.call", self.user, self.device),
        ]
        .iter()
        .any(|key| requested == format!("org.matrix.msc2762.send.state_event:{MEMBER}#{key}"))
    }
}
impl CapabilitiesProvider for CallCapabilities {
    async fn acquire_capabilities(&self, capabilities: Capabilities) -> Capabilities {
        let values = serde_json::to_value(capabilities).unwrap_or(Value::Null);
        let granted: Vec<_> = values
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| {
                let requested = v.as_str()?;
                if self.grant(requested) {
                    return Some(v.clone());
                }
                // The widget asks for all room metadata; grant only the empty
                // state key and our own profile instead of the broad request.
                if ["m.room.create", "m.room.name", "m.room.encryption"]
                    .iter()
                    .any(|event| {
                        requested == format!("org.matrix.msc2762.receive.state_event:{event}")
                    })
                {
                    return Some(Value::String(format!("{requested}#")));
                }
                if requested == "org.matrix.msc2762.receive.state_event:m.room.member" {
                    return Some(Value::String(format!("{requested}#{}", self.user)));
                }
                None
            })
            .collect();
        serde_json::from_value(Value::Array(granted)).unwrap_or_default()
    }
}

/// To-device capabilities alone have no room filter in SDK 0.19.1. Reject
/// key payloads without the selected room ID before the driver sees them.
pub fn room_scoped_message(message: &Value, room: &str, widget_id: &str) -> bool {
    if message.get("widgetId").and_then(Value::as_str) != Some(widget_id) {
        return false;
    }
    if message.get("api").and_then(Value::as_str) != Some("fromWidget")
        && message.get("api").and_then(Value::as_str) != Some("toWidget")
    {
        return false;
    }
    if message.get("response").is_some() {
        return true;
    }
    if message.get("action").and_then(Value::as_str) == Some("send_to_device") {
        let Some(data) = message.get("data") else {
            return false;
        };
        if data.get("type").and_then(Value::as_str) != Some(KEYS) {
            return false;
        }
        let Some(users) = data.get("messages").and_then(Value::as_object) else {
            return false;
        };
        return !users.is_empty()
            && users.values().all(|devices| {
                devices.as_object().is_some_and(|devices| {
                    !devices.is_empty()
                        && devices.values().all(|content| {
                            content.get("room_id").and_then(Value::as_str) == Some(room)
                        })
                })
            });
    }
    message
        .get("data")
        .and_then(|d| d.get("room_id"))
        .and_then(Value::as_str)
        .is_none_or(|id| id == room)
}
/// SDK 0.19.1's to-device event handler is account-wide. Filter decrypted
/// incoming keys before crossing the process boundary, not only outbound keys.
pub fn outgoing_room_scoped(message: &Value, room: &str) -> bool {
    if message.get("response").is_some() || message["action"] != "send_to_device" {
        return true;
    }
    message["data"]["type"] == KEYS
        && message["data"]["content"]["room_id"].as_str() == Some(room)
        && message["data"]["encrypted"] == true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capabilities_reject_wildcards_other_state_keys_and_unrelated_events() {
        let p = CallCapabilities::new("@a:test", "A");
        assert!(p.grant(
            "org.matrix.msc2762.send.state_event:org.matrix.msc3401.call.member#_@a:test_A_m.call"
        ));
        assert!(p.grant("org.matrix.msc3819.send.to_device:io.element.call.encryption_keys"));
        for c in [
            "org.matrix.msc2762.send.event:m.room.message",
            "org.matrix.msc2762.send.state_event:org.matrix.msc3401.call.member#@b:test",
            "org.matrix.msc3819.send.to_device:m.call.invite",
            "org.matrix.msc4039.download_file",
            "org.matrix.msc2762.send.state_event:org.matrix.msc3401.call.member",
            "org.matrix.msc2762.receive.state_event:m.room.member",
            "org.matrix.msc4515.rtc_transports",
        ] {
            assert!(!p.grant(c));
        }
    }
    #[test]
    fn keys_cannot_cross_room_boundaries() {
        let mut value = serde_json::json!({"api":"fromWidget","widgetId":"w","action":"send_to_device","data":{"type":KEYS,"messages":{"@b:test":{"B":{"room_id":"!r:test"}}}}});
        assert!(room_scoped_message(&value, "!r:test", "w"));
        assert!(!room_scoped_message(&value, "!other:test", "w"));
        value["data"]["messages"]["@b:test"]["B"] = serde_json::json!({});
        assert!(!room_scoped_message(&value, "!r:test", "w"));
    }
    #[test]
    fn other_room_or_clear_to_device_keys_never_cross_to_runtime() {
        let mut message = serde_json::json!({"action":"send_to_device","data":{"type":KEYS,"content":{"room_id":"!r:test"},"encrypted":true}});
        assert!(outgoing_room_scoped(&message, "!r:test"));
        assert!(!outgoing_room_scoped(&message, "!other:test"));
        message["data"]["encrypted"] = Value::Bool(false);
        assert!(!outgoing_room_scoped(&message, "!r:test"));
    }
    #[tokio::test]
    async fn sdk_capability_roundtrip_preserves_narrow_grants() {
        let provider = CallCapabilities::new("@a:test", "A");
        let requested: Capabilities = serde_json::from_value(serde_json::json!([
            "org.matrix.msc2762.receive.state_event:m.room.member",
            "org.matrix.msc2762.receive.state_event:m.room.create",
            "org.matrix.msc3819.send.to_device:io.element.call.encryption_keys",
            "org.matrix.msc2762.send.event:m.room.message"
        ]))
        .unwrap();
        let granted = serde_json::to_value(provider.acquire_capabilities(requested).await).unwrap();
        let granted: Vec<String> = serde_json::from_value(granted).unwrap();
        assert_eq!(granted.len(), 3);
        assert!(
            granted.contains(
                &"org.matrix.msc2762.receive.state_event:m.room.member#@a:test".to_owned()
            )
        );
        assert!(
            granted.contains(&"org.matrix.msc2762.receive.state_event:m.room.create#".to_owned())
        );
    }
}
