//! Credential-free process protocol. No Matrix SDK or GPUI dependency.
pub mod supervisor;
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

pub const ORIGIN: &str = "twayblade-call://localhost";
pub const MAX_FRAME: usize = 256 * 1024;
pub const QUEUE_CAPACITY: usize = 32;
pub const ELEMENT_CALL_VERSION: &str = "0.17.0";
pub const ELEMENT_CALL_SHA256: &str =
    "ba0f3f3b04b62577587faed603c3768465b1f9db33b15583473265531ca00121";

// Deliberately no Debug: frames can carry OpenID assertions and call keys.
#[derive(Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Frame {
    Configure {
        url: String,
        jwt_service: String,
    },
    Widget(serde_json::Value),
    Ping(u64),
    Pong(u64),
    Ready,
    ServiceHealth(bool),
    Environment {
        secure_context: bool,
        media_devices: bool,
        script_transform: bool,
    },
    Shutdown,
    Failed(Failure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Failure {
    PermissionDenied,
    UnsupportedRuntime,
    InvalidConfiguration,
    BridgeRejected,
    AssetsMissing,
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "call_frame_rejected")
}
pub fn encode(frame: &Frame) -> io::Result<Vec<u8>> {
    let bytes = serde_json::to_vec(frame).map_err(|_| invalid())?;
    if bytes.len() > MAX_FRAME {
        return Err(invalid());
    }
    let mut output = Vec::with_capacity(4 + bytes.len());
    output.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    output.extend_from_slice(&bytes);
    Ok(output)
}
pub fn write_frame(writer: &mut impl Write, frame: &Frame) -> io::Result<()> {
    writer.write_all(&encode(frame)?)?;
    writer.flush()
}
pub fn read_frame(reader: &mut impl Read) -> io::Result<Frame> {
    let mut header = [0; 4];
    reader.read_exact(&mut header)?;
    let length = u32::from_be_bytes(header) as usize;
    if length == 0 || length > MAX_FRAME {
        return Err(invalid());
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes).map_err(|_| invalid())
}
pub async fn read_async(reader: &mut (impl tokio::io::AsyncRead + Unpin)) -> io::Result<Frame> {
    use tokio::io::AsyncReadExt;
    let length = reader.read_u32().await? as usize;
    if length == 0 || length > MAX_FRAME {
        return Err(invalid());
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).await?;
    serde_json::from_slice(&bytes).map_err(|_| invalid())
}
pub async fn write_async(
    writer: &mut (impl tokio::io::AsyncWrite + Unpin),
    frame: &Frame,
) -> io::Result<()> {
    use tokio::io::AsyncWriteExt;
    writer.write_all(&encode(frame)?).await?;
    writer.flush().await
}
pub fn call_origin(input: &str) -> bool {
    url::Url::parse(input).is_ok_and(|u| {
        u.scheme() == "twayblade-call"
            && u.host_str() == Some("localhost")
            && u.port().is_none()
            && u.username().is_empty()
            && u.password().is_none()
    })
}
pub fn jwt_service(input: &str) -> Option<url::Url> {
    let u = url::Url::parse(input).ok()?;
    (u.scheme() == "https"
        && u.host_str().is_some()
        && u.username().is_empty()
        && u.password().is_none()
        && u.query().is_none()
        && u.fragment().is_none())
    .then_some(u)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_handles_fragmentation_truncation_and_caps_before_allocating() {
        let encoded = encode(&Frame::Ping(42)).unwrap();
        assert!(matches!(
            read_frame(&mut encoded.as_slice()).unwrap(),
            Frame::Ping(42)
        ));
        for n in 0..encoded.len() {
            assert!(read_frame(&mut &encoded[..n]).is_err());
        }
        for n in [0, MAX_FRAME as u32 + 1, u32::MAX] {
            assert!(read_frame(&mut n.to_be_bytes().as_slice()).is_err());
        }
        assert!(
            encode(&Frame::Widget(serde_json::Value::String(
                "x".repeat(MAX_FRAME)
            )))
            .is_err()
        );
    }
    #[test]
    fn origin_and_service_urls_are_exact() {
        assert!(call_origin(
            "twayblade-call://localhost/index.html#?roomId=x"
        ));
        for bad in [
            "https://localhost",
            "twayblade-call://localhost.evil",
            "twayblade-call://u@localhost",
            "twayblade-call://localhost:3",
        ] {
            assert!(!call_origin(bad));
        }
        for bad in [
            "http://sfu.test",
            "https://u:p@sfu.test",
            "https://sfu.test?token=x",
            "https://sfu.test/#token",
        ] {
            assert!(jwt_service(bad).is_none());
        }
    }
}
