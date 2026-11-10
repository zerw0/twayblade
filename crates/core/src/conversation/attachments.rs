//! Media preparation and SDK queue/download operations. Filenames are untrusted.
use super::*;
use matrix_sdk::{
    attachment::{AttachmentConfig, AttachmentInfo, BaseFileInfo, BaseImageInfo, Thumbnail},
    media::{MediaFormat, MediaRequestParameters, MediaThumbnailSettings},
    ruma::events::room::MediaSource,
};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
pub enum AttachmentError {
    #[error("attachment_file")]
    File,
    #[error("attachment_limit")]
    Limit { maximum: u64 },
    #[error("attachment_connection")]
    Connection,
    #[error("attachment_upload")]
    Upload,
    #[error("attachment_download")]
    Download,
    #[error("attachment_unavailable")]
    Unavailable,
}

pub struct AttachmentDraft {
    pub filename: String,
    pub size: u64,
    pub preview_png: Option<Arc<Vec<u8>>>,
    data: Vec<u8>,
    content_type: mime::Mime,
}

pub fn sanitize_filename(name: &str) -> String {
    let name = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches('.');
    let mut result = String::new();
    for ch in name.chars() {
        if result.len() + ch.len_utf8() > 180 {
            break;
        }
        result.push(if ch.is_control() || "<>:\"|?*".contains(ch) {
            '_'
        } else {
            ch
        });
    }
    let result = result.trim_end_matches([' ', '.']);
    if result.is_empty() {
        return "attachment".into();
    }
    let stem = result
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ]
    .contains(&stem.as_str())
    {
        format!("file_{result}")
    } else {
        result.to_owned()
    }
}

fn preview(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    if bytes.len() > 64 * 1024 * 1024 {
        return None;
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().ok()?;
    let (width, height) = (image.width(), image.height());
    let mut output = Cursor::new(Vec::new());
    image
        .thumbnail(512, 512)
        .write_to(&mut output, image::ImageFormat::Png)
        .ok()?;
    Some((output.into_inner(), width, height))
}

impl Conversation {
    async fn upload_limit(&self) -> Result<u64, AttachmentError> {
        self.timeline
            .room()
            .client()
            .load_or_fetch_max_upload_size()
            .await
            .map(u64::from)
            .map_err(|_| AttachmentError::Connection)
    }
    pub async fn prepare_file(&self, path: PathBuf) -> Result<AttachmentDraft, AttachmentError> {
        let metadata = tokio::fs::metadata(&path)
            .await
            .map_err(|_| AttachmentError::File)?;
        if !metadata.is_file() {
            return Err(AttachmentError::File);
        }
        let maximum = self.upload_limit().await?;
        if metadata.len() > maximum {
            return Err(AttachmentError::Limit { maximum });
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(AttachmentError::File)?;
        use tokio::io::AsyncReadExt;
        let file = tokio::fs::File::open(&path)
            .await
            .map_err(|_| AttachmentError::File)?;
        let mut bytes = Vec::new();
        file.take(maximum.saturating_add(1))
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| AttachmentError::File)?;
        self.prepare_bytes(name.to_owned(), bytes).await
    }
    pub async fn prepare_bytes(
        &self,
        name: String,
        data: Vec<u8>,
    ) -> Result<AttachmentDraft, AttachmentError> {
        let maximum = self.upload_limit().await?;
        if data.len() as u64 > maximum {
            return Err(AttachmentError::Limit { maximum });
        }
        let filename = sanitize_filename(&name);
        let content_type = image::guess_format(&data)
            .ok()
            .and_then(|format| format.to_mime_type().parse().ok())
            .unwrap_or_else(|| mime_guess::from_path(&filename).first_or_octet_stream());
        let preview_png = preview(&data).map(|(data, _, _)| Arc::new(data));
        Ok(AttachmentDraft {
            filename,
            size: data.len() as u64,
            preview_png,
            data,
            content_type,
        })
    }
    pub async fn send_attachment(&self, draft: AttachmentDraft) -> Result<(), AttachmentError> {
        let maximum = self.upload_limit().await?;
        if draft.size > maximum {
            return Err(AttachmentError::Limit { maximum });
        }
        let info = Some(if draft.content_type.type_() == mime::IMAGE {
            let dimensions = preview(&draft.data).map(|(_, w, h)| (w.into(), h.into()));
            AttachmentInfo::Image(BaseImageInfo {
                size: draft.size.try_into().ok(),
                width: dimensions.map(|(w, _)| w),
                height: dimensions.map(|(_, h)| h),
                ..Default::default()
            })
        } else {
            AttachmentInfo::File(BaseFileInfo {
                size: draft.size.try_into().ok(),
            })
        });
        let mut config = AttachmentConfig {
            info,
            ..Default::default()
        };
        if let Some(bytes) = draft.preview_png {
            let thumbnail = image::ImageReader::new(Cursor::new(bytes.as_slice()))
                .with_guessed_format()
                .map_err(|_| AttachmentError::File)?
                .into_dimensions()
                .map_err(|_| AttachmentError::File)?;
            config.thumbnail = Some(Thumbnail {
                data: bytes.as_ref().clone(),
                content_type: mime::IMAGE_PNG,
                width: thumbnail.0.into(),
                height: thumbnail.1.into(),
                size: (bytes.len() as u64)
                    .try_into()
                    .map_err(|_| AttachmentError::File)?,
            });
        }
        self.timeline
            .room()
            .client()
            .send_queue()
            .enable_upload_progress(true);
        self.timeline
            .room()
            .send_queue()
            .send_attachment(draft.filename, draft.content_type, draft.data, config)
            .await
            .map_err(|_| AttachmentError::Upload)?;
        Ok(())
    }
    pub(super) async fn media_source(&self, id: &str) -> Result<MediaSource, AttachmentError> {
        let items = self.timeline.items().await;
        let message = items
            .iter()
            .find(|item| item.unique_id().0 == id)
            .and_then(|item| item.as_event())
            .and_then(|event| event.content().as_message())
            .ok_or(AttachmentError::Unavailable)?;
        match message.msgtype() {
            MessageType::Image(content) => Ok(content.source.clone()),
            MessageType::File(content) => Ok(content.source.clone()),
            MessageType::Audio(content) => Ok(content.source.clone()),
            MessageType::Video(content) => Ok(content.source.clone()),
            _ => Err(AttachmentError::Unavailable),
        }
    }
    pub async fn attachment_thumbnail(&self, id: &str) -> Result<Arc<Vec<u8>>, AttachmentError> {
        let source = self.media_source(id).await?;
        let bytes = self
            .timeline
            .room()
            .client()
            .media()
            .get_media_content(
                &MediaRequestParameters {
                    source,
                    format: MediaFormat::Thumbnail(MediaThumbnailSettings::new(
                        512u32.into(),
                        512u32.into(),
                    )),
                },
                false,
            )
            .await
            .map_err(|_| AttachmentError::Download)?;
        preview(&bytes)
            .map(|(bytes, _, _)| Arc::new(bytes))
            .ok_or(AttachmentError::Download)
    }
    pub async fn download_attachment(&self, id: &str, path: &Path) -> Result<(), AttachmentError> {
        let source = self.media_source(id).await?;
        let bytes = self
            .timeline
            .room()
            .client()
            .media()
            .get_media_content(
                &MediaRequestParameters {
                    source,
                    format: MediaFormat::File,
                },
                false,
            )
            .await
            .map_err(|_| AttachmentError::Download)?;
        atomic_write(Arc::new(bytes), path.to_path_buf()).await
    }
    pub async fn cancel_attachment(&self, id: &str) -> Result<(), AttachmentError> {
        let handle = {
            let items = self.timeline.items().await;
            items
                .iter()
                .find(|item| item.unique_id().0 == id)
                .and_then(|item| item.as_event())
                .and_then(|event| event.local_echo_send_handle())
                .ok_or(AttachmentError::Unavailable)?
        };
        if handle.abort().await.map_err(|_| AttachmentError::Upload)? {
            Ok(())
        } else {
            Err(AttachmentError::Unavailable)
        }
    }
}

pub(super) async fn atomic_write(
    bytes: Arc<Vec<u8>>,
    path: PathBuf,
) -> Result<(), AttachmentError> {
    tokio::task::spawn_blocking(move || atomic_write_blocking(&bytes, &path))
        .await
        .map_err(|_| AttachmentError::Download)?
}
pub(super) fn atomic_write_blocking(bytes: &[u8], path: &Path) -> Result<(), AttachmentError> {
    use std::io::Write;
    let temporary = path.with_file_name(format!(".matrix-download-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
        return Err(AttachmentError::Download);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filenames_cannot_escape_a_user_chosen_directory_or_create_reserved_paths() {
        for name in [
            "../../report.pdf",
            "C:\\private\\report.pdf",
            "/etc/report.pdf",
        ] {
            assert_eq!(sanitize_filename(name), "report.pdf");
        }
        assert_eq!(sanitize_filename(".."), "attachment");
        assert_eq!(sanitize_filename("CON.txt"), "file_CON.txt");
        assert!(!sanitize_filename("evil\nname.txt").contains('\n'));
        assert!(sanitize_filename(&"🦀".repeat(100)).len() <= 180);
    }
    #[test]
    fn image_decode_limits_reject_oversized_dimensions() {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(9000, 1)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        assert!(preview(&bytes.into_inner()).is_none());
    }
}

#[cfg(test)]
mod integration {
    use super::*;
    use matrix_sdk::{
        Client,
        config::SyncSettings,
        reqwest::Certificate,
        ruma::{
            api::client::room::create_room,
            events::{InitialStateEvent, room::encryption::RoomEncryptionEventContent},
            serde::Raw,
        },
    };
    use serde::Deserialize;
    use std::time::Duration;
    fn checked<T, E>(value: Result<T, E>, stage: &str) -> T {
        value.unwrap_or_else(|_| panic!("Media fixture stage failed: {stage}"))
    }
    #[derive(Deserialize)]
    struct Fixture {
        homeserver: String,
        security_username: String,
        security_password: String,
    }
    async fn row(conversation: &Conversation, name: &str) -> String {
        checked(
            tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    if let Some(item) = conversation.timeline.items().await.iter().find(|item| {
                        item.as_event()
                            .and_then(|event| event.content().as_message())
                            .is_some_and(|message| message.body() == name)
                    }) {
                        return item.unique_id().0.clone();
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            })
            .await,
            "timeline media row",
        )
    }
    #[tokio::test]
    #[ignore = "requires isolated Synapse fixture with generated user"]
    async fn attachment_plain_encrypted_cancel_restart_round_trip() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/local/integration");
        let fixture: Fixture = checked(
            serde_json::from_slice(&checked(
                tokio::fs::read(root.join("fixture.json")).await,
                "fixture",
            )),
            "fixture JSON",
        );
        assert_eq!(fixture.homeserver, "https://127.0.0.1:18448");
        let certificate = checked(
            Certificate::from_pem(&checked(tokio::fs::read(root.join("ca.crt")).await, "CA")),
            "CA format",
        );
        for encrypted in [false, true] {
            let path = root.join(format!("media-{}", uuid::Uuid::new_v4()));
            let passphrase = uuid::Uuid::new_v4().to_string();
            let client = checked(
                Client::builder()
                    .homeserver_url(&fixture.homeserver)
                    .add_root_certificates(vec![certificate.clone()])
                    .sqlite_store(&path, Some(&passphrase))
                    .build()
                    .await,
                "client",
            );
            checked(
                client
                    .matrix_auth()
                    .login_username(&fixture.security_username, &fixture.security_password)
                    .await,
                "login",
            );
            let mut request = create_room::v3::Request::new();
            if encrypted {
                request.initial_state.push(
                    checked(
                        Raw::new(&InitialStateEvent::new(
                            Default::default(),
                            RoomEncryptionEventContent::with_recommended_defaults(),
                        )),
                        "encryption",
                    )
                    .cast(),
                );
            }
            let room = checked(client.create_room(request).await, "room");
            let room_id = room.room_id().to_owned();
            checked(client.sync_once(SyncSettings::default()).await, "sync");
            if encrypted {
                assert!(room.encryption_state().is_encrypted());
            }
            let conversation = Conversation {
                timeline: Arc::new(checked(room.timeline().await, "timeline")),
            };
            let data = b"isolated attachment round trip".to_vec();
            let draft = checked(
                conversation
                    .prepare_bytes("../../round-trip.txt".into(), data.clone())
                    .await,
                "prepare",
            );
            assert_eq!(draft.filename, "round-trip.txt");
            checked(conversation.send_attachment(draft).await, "send");
            let id = row(&conversation, "round-trip.txt").await;
            // Wait for the server event: a cached local echo alone cannot establish an upload.
            checked(
                tokio::time::timeout(Duration::from_secs(30), async {
                    loop {
                        if conversation.timeline.items().await.iter().any(|item| {
                            item.unique_id().0 == id
                                && item
                                    .as_event()
                                    .is_some_and(|event| event.event_id().is_some())
                        }) {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                })
                .await,
                "upload completion",
            );
            assert_eq!(
                matches!(
                    checked(conversation.media_source(&id).await, "media encryption"),
                    MediaSource::Encrypted(_)
                ),
                encrypted
            );
            let output = root.join(format!("download-{}", uuid::Uuid::new_v4()));
            checked(
                conversation.download_attachment(&id, &output).await,
                "download",
            );
            assert_eq!(
                checked(tokio::fs::read(&output).await, "download read"),
                data
            );
            checked(tokio::fs::remove_file(output).await, "download cleanup");
            let mut png = Cursor::new(Vec::new());
            checked(
                image::DynamicImage::new_rgb8(32, 16).write_to(&mut png, image::ImageFormat::Png),
                "sample image",
            );
            let draft = checked(
                conversation
                    .prepare_bytes("sample.png".into(), png.into_inner())
                    .await,
                "image prepare",
            );
            assert!(draft.preview_png.is_some());
            checked(conversation.send_attachment(draft).await, "image send");
            let image_id = row(&conversation, "sample.png").await;
            checked(
                tokio::time::timeout(Duration::from_secs(30), async {
                    loop {
                        if conversation.timeline.items().await.iter().any(|item| {
                            item.unique_id().0 == image_id
                                && item
                                    .as_event()
                                    .is_some_and(|event| event.event_id().is_some())
                        }) {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                })
                .await,
                "image upload",
            );
            let thumb = checked(
                conversation.attachment_thumbnail(&image_id).await,
                "server thumbnail",
            );
            assert!(
                image::ImageReader::new(Cursor::new(thumb.as_slice()))
                    .with_guessed_format()
                    .unwrap()
                    .into_dimensions()
                    .unwrap()
                    .0
                    <= 512
            );
            let maximum = checked(conversation.upload_limit().await, "limit");
            let oversize = root.join(format!("oversize-{}", uuid::Uuid::new_v4()));
            let file = checked(tokio::fs::File::create(&oversize).await, "sparse file");
            checked(file.set_len(maximum + 1).await, "sparse length");
            drop(file);
            assert!(matches!(
                conversation.prepare_file(oversize.clone()).await,
                Err(AttachmentError::Limit { .. })
            ));
            checked(tokio::fs::remove_file(oversize).await, "sparse cleanup");
            client.send_queue().set_enabled(false).await;
            let cancel = checked(
                conversation
                    .prepare_bytes("cancel.txt".into(), data.clone())
                    .await,
                "cancel prepare",
            );
            checked(conversation.send_attachment(cancel).await, "cancel enqueue");
            let cancel_id = row(&conversation, "cancel.txt").await;
            checked(
                conversation.cancel_attachment(&cancel_id).await,
                "cancel queue",
            );
            let pending = checked(
                conversation
                    .prepare_bytes("restart.txt".into(), data.clone())
                    .await,
                "restart prepare",
            );
            checked(
                conversation.send_attachment(pending).await,
                "restart enqueue",
            );
            let _ = row(&conversation, "restart.txt").await;
            let session = client.session().expect("session");
            drop(conversation);
            drop(room);
            drop(client);
            tokio::time::sleep(Duration::from_millis(300)).await;
            let restored = checked(
                Client::builder()
                    .homeserver_url(&fixture.homeserver)
                    .add_root_certificates(vec![certificate.clone()])
                    .sqlite_store(&path, Some(&passphrase))
                    .build()
                    .await,
                "reopen store",
            );
            checked(restored.restore_session(session).await, "restore session");
            checked(
                restored.sync_once(SyncSettings::default()).await,
                "restore sync",
            );
            restored.send_queue().set_enabled(true).await;
            let room = restored.get_room(&room_id).expect("restored room");
            let conversation = Conversation {
                timeline: Arc::new(checked(room.timeline().await, "restored timeline")),
            };
            let id = row(&conversation, "restart.txt").await;
            checked(
                tokio::time::timeout(Duration::from_secs(30), async {
                    loop {
                        if conversation.timeline.items().await.iter().any(|item| {
                            item.unique_id().0 == id
                                && item
                                    .as_event()
                                    .is_some_and(|event| event.event_id().is_some())
                        }) {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                })
                .await,
                "resumed upload",
            );
            let output = root.join(format!("resumed-{}", uuid::Uuid::new_v4()));
            checked(
                conversation.download_attachment(&id, &output).await,
                "resumed download",
            );
            assert_eq!(
                checked(tokio::fs::read(&output).await, "resumed read"),
                data
            );
            checked(tokio::fs::remove_file(output).await, "cleanup");
            checked(restored.logout().await, "logout");
        }
    }
    #[tokio::test]
    #[ignore = "requires isolated Synapse fixture with generated user"]
    async fn image_viewer_plain_encrypted_animation_limits_and_corrupt() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/local/integration");
        let fixture: Fixture = checked(
            serde_json::from_slice(&checked(
                tokio::fs::read(root.join("fixture.json")).await,
                "fixture",
            )),
            "fixture JSON",
        );
        assert_eq!(fixture.homeserver, "https://127.0.0.1:18448");
        let certificate = checked(
            Certificate::from_pem(&checked(tokio::fs::read(root.join("ca.crt")).await, "CA")),
            "CA format",
        );
        for encrypted in [false, true] {
            let path = root.join(format!("media-{}", uuid::Uuid::new_v4()));
            let passphrase = uuid::Uuid::new_v4().to_string();
            let client = checked(
                Client::builder()
                    .homeserver_url(&fixture.homeserver)
                    .add_root_certificates(vec![certificate.clone()])
                    .sqlite_store(&path, Some(&passphrase))
                    .build()
                    .await,
                "client",
            );
            checked(
                client
                    .matrix_auth()
                    .login_username(&fixture.security_username, &fixture.security_password)
                    .await,
                "login",
            );
            let mut request = create_room::v3::Request::new();
            if encrypted {
                request.initial_state.push(
                    checked(
                        Raw::new(&InitialStateEvent::new(
                            Default::default(),
                            RoomEncryptionEventContent::with_recommended_defaults(),
                        )),
                        "encryption",
                    )
                    .cast(),
                );
            }
            let room = checked(client.create_room(request).await, "room");
            checked(client.sync_once(SyncSettings::default()).await, "sync");
            if encrypted {
                assert!(room.encryption_state().is_encrypted());
            }
            let conversation = Conversation {
                timeline: Arc::new(checked(room.timeline().await, "timeline")),
            };

            let mut png = Cursor::new(Vec::new());
            image::DynamicImage::new_rgba8(2, 2)
                .write_to(&mut png, image::ImageFormat::Png)
                .unwrap();
            let mut large = Cursor::new(Vec::new());
            image::DynamicImage::new_rgba8(9000, 1)
                .write_to(&mut large, image::ImageFormat::Png)
                .unwrap();
            for (name, bytes, frames, error) in [
                ("full.png", png.into_inner(), 1, None),
                (
                    "animated.gif",
                    super::super::viewer::animated_gif_fixture(),
                    2,
                    None,
                ),
                (
                    "animated.webp",
                    super::super::viewer::animated_webp_fixture(),
                    2,
                    None,
                ),
                (
                    "large.png",
                    large.into_inner(),
                    0,
                    Some(super::super::ImageError::Limit),
                ),
                (
                    "corrupt.png",
                    vec![1, 2, 3],
                    0,
                    Some(super::super::ImageError::Corrupt),
                ),
            ] {
                let draft = checked(
                    conversation.prepare_bytes(name.into(), bytes.clone()).await,
                    "prepare viewer image",
                );
                checked(
                    conversation.send_attachment(draft).await,
                    "send viewer image",
                );
                let id = row(&conversation, name).await;
                checked(
                    tokio::time::timeout(Duration::from_secs(30), async {
                        loop {
                            if conversation
                                .timeline
                                .items()
                                .await
                                .iter()
                                .find(|i| i.unique_id().0 == id)
                                .and_then(|i| i.as_event())
                                .is_some_and(|e| e.event_id().is_some())
                            {
                                break;
                            }
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                    })
                    .await,
                    "remote viewer image",
                );
                let source = checked(conversation.media_source(&id).await, "viewer source");
                assert!(
                    matches!(&source, MediaSource::Encrypted(_)) == encrypted,
                    "viewer source encryption mismatch"
                );
                let request = MediaRequestParameters {
                    source,
                    format: MediaFormat::File,
                };
                checked(
                    client.media().remove_media_content(&request).await,
                    "clear upload cache before viewer",
                );
                let (_cancel, cancel) = tokio::sync::watch::channel(false);
                let result = conversation.viewer_image(&id, cancel).await;
                if let Some(error) = error {
                    assert!(
                        matches!(result,Err(actual) if actual == error),
                        "typed viewer failure"
                    );
                } else {
                    let image = checked(result, "full-resolution image fetch and decode");
                    assert!(
                        image.width == 2 && image.height == 2 && image.frames.len() == frames,
                        "viewer dimensions/animation frames"
                    );
                    let store = checked(client.media_store().lock().await, "media cache");
                    assert!(
                        checked(
                            store.get_media_content(&request).await,
                            "uncached viewer media"
                        )
                        .is_none(),
                        "viewer must not persist decrypted media"
                    );
                    drop(store);
                    let saved = path.join(format!("explicit-save-{name}"));
                    assert!(!saved.exists(), "viewer must not save automatically");
                    checked(image.save(&saved).await, "explicit viewer save");
                    assert!(
                        checked(tokio::fs::read(saved).await, "saved image") == bytes,
                        "save must preserve original image bytes"
                    );
                }
                let (_cancel, cancel) = tokio::sync::watch::channel(true);
                assert!(matches!(
                    conversation.viewer_image(&id, cancel).await,
                    Err(super::super::ImageError::Cancelled)
                ));
            }
            checked(client.matrix_auth().logout().await, "viewer fixture logout");
        }
        println!(
            "PASS: plain/encrypted full images, GIF/WebP animation, uncached decrypted media, explicit save, >8k and corrupt failures"
        );
    }
}
