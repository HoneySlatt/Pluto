//! A composer's attachments, after the desktop's `attachments.rs`
//! (`addComposerAttachments`, `attachmentUploadQueue.ts`): images the
//! providers take (PNG, JPEG, GIF, WebP, up to 10 MiB) go inline as data
//! URLs (`UploadChatImageAttachment`); other files (up to 50 MiB) are
//! uploaded when attached (`attachments.createUploadUrl`, then the bytes to
//! the signed URL) and sent as `file` attachments the agent reads by path.
//! A message the server refuses brings its attachments back with its text.

use std::sync::Arc;

use base64::Engine;
use serde_json::{Value, json};

use crate::session::Session;
use crate::{Core, Hub};

/// `PROVIDER_SEND_TURN_MAX_ATTACHMENTS`, `…_MAX_IMAGE_BYTES`,
/// `…_MAX_TOTAL_IMAGE_BYTES` and `…_MAX_FILE_BYTES`.
const MAX_ATTACHMENTS: usize = 100;
const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const MAX_TOTAL_IMAGE_BYTES: usize = 80 * 1024 * 1024;
const MAX_FILE_BYTES: usize = 50 * 1024 * 1024;
/// `PROVIDER_SEND_TURN_SUPPORTED_IMAGE_MIME_TYPES`.
const IMAGE_TYPES: [&str; 4] = ["image/gif", "image/jpeg", "image/png", "image/webp"];

#[derive(Clone, Debug)]
pub enum Kind {
    Image(Arc<Vec<u8>>),
    /// A file: its bytes until they are uploaded, then the server's id (or
    /// why the upload failed).
    File {
        bytes: Option<Arc<Vec<u8>>>,
        upload: Option<Result<String, String>>,
    },
}

#[derive(Clone, Debug)]
pub struct Attached {
    /// The composer's own id for it (the upload's key).
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub kind: Kind,
}

#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum UploadState {
    Uploading,
    Ready,
    Failed { error: String },
}

/// An attachment as the composer shows it.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct AttachmentView {
    pub id: String,
    pub name: String,
    pub image: bool,
    /// "2.4 MB".
    pub size: String,
    pub state: UploadState,
}

/// "2.4 MB" from bytes (`format_size`).
fn format_size(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{:.0} KB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

impl Session {
    /// Adds a picked file to `composer`; why it was refused.
    pub fn attach(
        &mut self,
        composer: &str,
        name: String,
        mime: String,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        let list = self.attachments.entry(composer.to_owned()).or_default();
        if list.len() >= MAX_ATTACHMENTS {
            return Err(format!(
                "You can attach up to {MAX_ATTACHMENTS} files per message."
            ));
        }
        if bytes.is_empty() {
            return Err(format!("'{name}' is empty."));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let size = bytes.len() as u64;
        let image = IMAGE_TYPES.contains(&mime.to_ascii_lowercase().as_str())
            && bytes.len() <= MAX_IMAGE_BYTES;
        if image {
            let images: usize = list
                .iter()
                .filter_map(|a| match &a.kind {
                    Kind::Image(bytes) => Some(bytes.len()),
                    Kind::File { .. } => None,
                })
                .sum();
            if images + bytes.len() > MAX_TOTAL_IMAGE_BYTES {
                return Err(
                    "Images can total up to 80 MiB per message. Send fewer at once.".into(),
                );
            }
            list.push(Attached {
                id,
                name,
                mime,
                size,
                kind: Kind::Image(Arc::new(bytes)),
            });
            return Ok(());
        }
        if bytes.len() > MAX_FILE_BYTES {
            return Err(format!("'{name}' is larger than 50 MB."));
        }
        let mime = if mime.is_empty() {
            "application/octet-stream".to_owned()
        } else {
            mime
        };
        list.push(Attached {
            id: id.clone(),
            name: name.clone(),
            mime: mime.clone(),
            size,
            kind: Kind::File {
                bytes: Some(Arc::new(bytes)),
                upload: None,
            },
        });
        self.request(
            format!("file-upload:{id}"),
            "attachments.createUploadUrl",
            json!({"type": "file", "name": name, "mimeType": mime, "sizeBytes": size}),
        );
        // Refused at once (offline): the chip says so.
        if let Some(Err(error)) = self.replies.remove(&format!("file-upload:{id}")) {
            self.set_upload(&id, Err(error));
        }
        Ok(())
    }

    /// Removes an attachment; an uploaded file is deleted on the server.
    pub fn detach(&mut self, composer: &str, id: &str) {
        let Some(list) = self.attachments.get_mut(composer) else {
            return;
        };
        let Some(index) = list.iter().position(|a| a.id == id) else {
            return;
        };
        let removed = list.remove(index);
        if let Kind::File {
            upload: Some(Ok(attachment)),
            ..
        } = removed.kind
        {
            self.request(
                format!("file-delete:{attachment}"),
                "attachments.delete",
                json!({"attachmentId": attachment}),
            );
        }
    }

    fn file_mut(&mut self, id: &str) -> Option<&mut Attached> {
        self.attachments.values_mut().flatten().find(|a| a.id == id)
    }

    fn set_upload(&mut self, id: &str, result: Result<String, String>) {
        if let Some(Attached {
            kind: Kind::File { bytes, upload },
            ..
        }) = self.file_mut(id)
        {
            if result.is_ok() {
                *bytes = None;
            }
            *upload = Some(result);
        }
    }

    /// The upload URL arrived (or was refused): the bytes go to it on a
    /// background thread.
    pub fn upload_target(&mut self, hub: &Arc<Hub>, key: &str, reply: Result<Value, String>) {
        self.replies.remove(key);
        let Some(id) = key.strip_prefix("file-upload:") else {
            return;
        };
        let target = reply.and_then(|value| {
            let attachment = value["attachmentId"]
                .as_str()
                .ok_or("No upload was created.")?
                .to_owned();
            let url = value["relativeUrl"]
                .as_str()
                .and_then(|relative| {
                    url::Url::parse(self.server.trim())
                        .ok()?
                        .join(relative)
                        .ok()
                })
                .ok_or("The upload URL is invalid.")?;
            Ok((attachment, url))
        });
        let (attachment, url) = match target {
            Ok(target) => target,
            Err(error) => {
                self.set_upload(id, Err(error));
                return;
            }
        };
        let file = self.file_mut(id).and_then(|a| match &a.kind {
            Kind::File { bytes, .. } => bytes.clone().map(|b| (b, a.mime.clone())),
            Kind::Image(_) => None,
        });
        // Removed while the URL came: the server's upload goes too.
        let Some((bytes, mime)) = file else {
            self.request(
                format!("file-delete:{attachment}"),
                "attachments.delete",
                json!({"attachmentId": attachment}),
            );
            return;
        };
        let (id, hub) = (id.to_owned(), Arc::downgrade(hub));
        std::thread::spawn(move || {
            let result = pluto_core::http::post(&url, &mime, &bytes).map(|()| attachment.clone());
            let Some(hub) = hub.upgrade() else {
                return;
            };
            hub.update(|app, _| {
                let still = app.session.file_mut(&id).is_some();
                if still {
                    app.session.set_upload(&id, result);
                } else if result.is_ok() {
                    app.session.request(
                        format!("file-delete:{attachment}"),
                        "attachments.delete",
                        json!({"attachmentId": attachment}),
                    );
                }
            });
        });
    }

    /// `composer`'s attachments as the message carries them, or why it has
    /// to wait.
    pub fn wire_attachments(&self, composer: &str) -> Result<Vec<Value>, String> {
        let engine = base64::engine::general_purpose::STANDARD;
        self.attachments
            .get(composer)
            .into_iter()
            .flatten()
            .map(|a| match &a.kind {
                Kind::Image(bytes) => Ok(json!({
                    "type": "image",
                    "name": a.name,
                    "mimeType": a.mime,
                    "sizeBytes": a.size,
                    "dataUrl": format!("data:{};base64,{}", a.mime, engine.encode(bytes.as_slice())),
                })),
                Kind::File { upload: None, .. } => Err("Wait for the files to finish uploading.".to_owned()),
                Kind::File {
                    upload: Some(Err(error)),
                    ..
                } => Err(format!(
                    "'{}' could not be uploaded: {error} Remove it and attach it again.",
                    a.name
                )),
                Kind::File {
                    upload: Some(Ok(id)),
                    ..
                } => Ok(json!({"type": "file", "id": id, "name": a.name,
                    "mimeType": a.mime, "sizeBytes": a.size})),
            })
            .collect()
    }
}

#[uniffi::export]
impl Core {
    /// Adds a picked image or file to `composer`; why it was refused.
    pub fn attach(
        &self,
        composer: String,
        name: String,
        mime: String,
        bytes: Vec<u8>,
    ) -> Option<String> {
        self.update(|app, _| app.session.attach(&composer, name, mime, bytes).err())
    }

    pub fn detach(&self, composer: String, id: String) {
        self.update(|app, _| app.session.detach(&composer, &id));
    }

    pub fn attachments(&self, composer: String) -> Vec<AttachmentView> {
        self.read(|app| {
            app.session
                .attachments
                .get(&composer)
                .into_iter()
                .flatten()
                .map(|a| AttachmentView {
                    id: a.id.clone(),
                    name: a.name.clone(),
                    image: matches!(a.kind, Kind::Image(_)),
                    size: format_size(a.size),
                    state: match &a.kind {
                        Kind::Image(_)
                        | Kind::File {
                            upload: Some(Ok(_)),
                            ..
                        } => UploadState::Ready,
                        Kind::File { upload: None, .. } => UploadState::Uploading,
                        Kind::File {
                            upload: Some(Err(error)),
                            ..
                        } => UploadState::Failed {
                            error: error.clone(),
                        },
                    },
                })
                .collect()
        })
    }

    /// An image attachment's bytes, for its thumbnail.
    pub fn attachment_preview(&self, composer: String, id: String) -> Option<Vec<u8>> {
        self.read(|app| {
            app.session
                .attachments
                .get(&composer)?
                .iter()
                .find(|a| a.id == id)
                .and_then(|a| match &a.kind {
                    Kind::Image(bytes) => Some(bytes.to_vec()),
                    Kind::File { .. } => None,
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_go_inline_and_files_wait_for_their_upload() {
        let mut session = Session::new(crate::prefs::Prefs::load(None));
        session
            .attach("chat", "a.png".into(), "image/png".into(), vec![1, 2, 3])
            .unwrap();
        // Offline, the upload is refused at once and says so.
        session
            .attach(
                "chat",
                "notes.pdf".into(),
                "application/pdf".into(),
                vec![4; 10],
            )
            .unwrap();
        let wire = session.wire_attachments("chat");
        assert!(wire.unwrap_err().contains("could not be uploaded"));
        let id = session.attachments["chat"][1].id.clone();
        session.set_upload(&id, Ok("att-1".into()));
        let wire = session.wire_attachments("chat").unwrap();
        assert_eq!(wire[0]["dataUrl"], "data:image/png;base64,AQID");
        assert_eq!(
            wire[1],
            json!({"type": "file", "id": "att-1", "name": "notes.pdf",
            "mimeType": "application/pdf", "sizeBytes": 10})
        );
        assert!(
            session
                .attach("chat", "empty.txt".into(), "text/plain".into(), Vec::new())
                .is_err()
        );
        assert_eq!(format_size(2_516_582), "2.4 MB");
    }
}
