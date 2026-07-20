use crate::agent::{new_id, AgentError};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use image::ImageFormat;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_IMAGES_PER_MESSAGE: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatImageAttachment {
    pub id: String,
    pub name: String,
    pub path: String,
    pub mime: String,
    pub size: u64,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatImageDraft {
    pub draft_id: String,
    pub id: String,
    pub name: String,
    pub path: String,
    pub mime: String,
    pub size: u64,
    pub available: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImagePrepareInput {
    Path {
        path: String,
    },
    Bytes {
        name: Option<String>,
        bytes_base64: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImagePrepareRejection {
    pub index: usize,
    pub name: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImagePrepareResult {
    pub accepted: Vec<ChatImageDraft>,
    pub rejected: Vec<ImagePrepareRejection>,
}

#[derive(Debug, Clone)]
struct DraftEntry {
    draft: ChatImageDraft,
    path: PathBuf,
}

pub struct ImageCommit {
    pub attachments: Vec<ChatImageAttachment>,
    drafts: Vec<DraftEntry>,
}

pub struct ImageService {
    data_dir: Option<PathBuf>,
    drafts: Mutex<HashMap<String, DraftEntry>>,
}

impl ImageService {
    pub fn new(data_dir: Option<PathBuf>) -> Self {
        Self {
            data_dir,
            drafts: Mutex::new(HashMap::new()),
        }
    }

    fn data_dir(&self) -> Result<&Path, AgentError> {
        self.data_dir
            .as_deref()
            .ok_or_else(|| AgentError::new("image_store_unavailable", "图片存储尚未初始化。"))
    }

    fn draft_dir(&self) -> Result<PathBuf, AgentError> {
        Ok(self.data_dir()?.join("cache").join("chat-image-drafts"))
    }

    fn conversation_dir(&self, conversation_id: &str) -> Result<PathBuf, AgentError> {
        Ok(self.data_dir()?.join("chat-images").join(conversation_id))
    }

    pub fn clear_stale_drafts(&self) -> Result<(), AgentError> {
        let dir = self.draft_dir()?;
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(image_io_error)?;
        }
        std::fs::create_dir_all(&dir).map_err(image_io_error)?;
        self.drafts.lock().unwrap().clear();
        Ok(())
    }

    pub fn prepare(
        &self,
        inputs: Vec<ImagePrepareInput>,
        remaining_slots: usize,
    ) -> ImagePrepareResult {
        let available_slots = remaining_slots.min(MAX_IMAGES_PER_MESSAGE);
        let mut accepted = Vec::new();
        let mut rejected = Vec::new();
        for (index, input) in inputs.into_iter().enumerate() {
            let display_name = input_name(&input);
            if accepted.len() >= available_slots {
                rejected.push(ImagePrepareRejection {
                    index,
                    name: display_name,
                    code: "image_limit".into(),
                    message: format!("每条消息最多添加 {MAX_IMAGES_PER_MESSAGE} 张图片。"),
                });
                continue;
            }
            match self.prepare_one(input) {
                Ok(draft) => accepted.push(draft),
                Err(error) => rejected.push(ImagePrepareRejection {
                    index,
                    name: display_name,
                    code: error.code,
                    message: error.message,
                }),
            }
        }
        ImagePrepareResult { accepted, rejected }
    }

    fn prepare_one(&self, input: ImagePrepareInput) -> Result<ChatImageDraft, AgentError> {
        let (name, bytes) = match input {
            ImagePrepareInput::Path { path } => {
                let path = PathBuf::from(path);
                let metadata = std::fs::metadata(&path)
                    .map_err(|_| AgentError::new("image_unreadable", "无法读取这张图片。"))?;
                if !metadata.is_file() {
                    return Err(AgentError::new("image_unreadable", "所选项目不是文件。"));
                }
                if metadata.len() > MAX_IMAGE_BYTES {
                    return Err(image_too_large());
                }
                let name = path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("image")
                    .to_string();
                let bytes = std::fs::read(path)
                    .map_err(|_| AgentError::new("image_unreadable", "无法读取这张图片。"))?;
                (name, bytes)
            }
            ImagePrepareInput::Bytes { name, bytes_base64 } => {
                if bytes_base64.len() as u64 > (MAX_IMAGE_BYTES * 4 / 3) + 16 {
                    return Err(image_too_large());
                }
                let bytes = STANDARD
                    .decode(bytes_base64)
                    .map_err(|_| AgentError::new("image_invalid_data", "剪贴板图片数据无效。"))?;
                (name.unwrap_or_else(|| "clipboard-image".into()), bytes)
            }
        };
        if bytes.len() as u64 > MAX_IMAGE_BYTES {
            return Err(image_too_large());
        }
        let (format, mime, extension) = validate_image(&bytes)?;
        image::load_from_memory_with_format(&bytes, format)
            .map_err(|_| AgentError::new("image_invalid_data", "文件不是有效的图片。"))?;

        let draft_id = new_id();
        let image_id = new_id();
        let path = self.draft_dir()?.join(format!("{draft_id}.{extension}"));
        std::fs::create_dir_all(path.parent().unwrap()).map_err(image_io_error)?;
        std::fs::write(&path, &bytes).map_err(image_io_error)?;
        let draft = ChatImageDraft {
            draft_id: draft_id.clone(),
            id: image_id,
            name: normalized_name(&name, extension),
            path: path.to_string_lossy().into_owned(),
            mime: mime.into(),
            size: bytes.len() as u64,
            available: true,
        };
        self.drafts.lock().unwrap().insert(
            draft_id,
            DraftEntry {
                draft: draft.clone(),
                path,
            },
        );
        Ok(draft)
    }

    pub fn discard(&self, draft_ids: &[String]) -> Result<(), AgentError> {
        let mut drafts = self.drafts.lock().unwrap();
        for draft_id in draft_ids {
            if let Some(entry) = drafts.remove(draft_id) {
                if entry.path.exists() {
                    std::fs::remove_file(entry.path).map_err(image_io_error)?;
                }
            }
        }
        Ok(())
    }

    pub fn commit(
        &self,
        conversation_id: &str,
        draft_ids: &[String],
    ) -> Result<ImageCommit, AgentError> {
        if draft_ids.is_empty() {
            return Ok(ImageCommit {
                attachments: Vec::new(),
                drafts: Vec::new(),
            });
        }
        if draft_ids.len() > MAX_IMAGES_PER_MESSAGE {
            return Err(AgentError::new(
                "image_limit",
                format!("每条消息最多添加 {MAX_IMAGES_PER_MESSAGE} 张图片。"),
            ));
        }
        let unique: HashSet<&String> = draft_ids.iter().collect();
        if unique.len() != draft_ids.len() {
            return Err(AgentError::new("image_draft_invalid", "图片草稿无效。"));
        }
        let drafts_guard = self.drafts.lock().unwrap();
        let selected = draft_ids
            .iter()
            .map(|id| {
                drafts_guard.get(id).cloned().ok_or_else(|| {
                    AgentError::new("image_draft_missing", "图片草稿已失效，请重新添加。")
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        drop(drafts_guard);

        for entry in &selected {
            if !entry.path.is_file() {
                return Err(AgentError::new(
                    "image_draft_missing",
                    "图片草稿已失效，请重新添加。",
                ));
            }
            let bytes = std::fs::read(&entry.path).map_err(image_io_error)?;
            if bytes.len() as u64 > MAX_IMAGE_BYTES {
                return Err(image_too_large());
            }
            let (format, mime, _) = validate_image(&bytes)?;
            if mime != entry.draft.mime
                || image::load_from_memory_with_format(&bytes, format).is_err()
            {
                return Err(AgentError::new(
                    "image_invalid_data",
                    "图片草稿已损坏，请重新添加。",
                ));
            }
        }

        let directory = self.conversation_dir(conversation_id)?;
        std::fs::create_dir_all(&directory).map_err(image_io_error)?;
        let mut moved: Vec<(DraftEntry, PathBuf)> = Vec::new();
        for entry in &selected {
            if !entry.path.is_file() {
                rollback_moves(&moved);
                return Err(AgentError::new(
                    "image_draft_missing",
                    "图片草稿已失效，请重新添加。",
                ));
            }
            let extension = mime_extension(&entry.draft.mime)
                .ok_or_else(|| AgentError::new("image_invalid_data", "图片格式无效。"))?;
            let destination = directory.join(format!("{}.{}", entry.draft.id, extension));
            if let Err(error) = move_file(&entry.path, &destination) {
                rollback_moves(&moved);
                return Err(error);
            }
            moved.push((entry.clone(), destination));
        }

        let attachments = moved
            .iter()
            .map(|(entry, path)| ChatImageAttachment {
                id: entry.draft.id.clone(),
                name: entry.draft.name.clone(),
                path: path.to_string_lossy().into_owned(),
                mime: entry.draft.mime.clone(),
                size: entry.draft.size,
                available: true,
            })
            .collect();
        let mut drafts = self.drafts.lock().unwrap();
        for id in draft_ids {
            drafts.remove(id);
        }
        Ok(ImageCommit {
            attachments,
            drafts: selected,
        })
    }

    pub fn rollback(&self, commit: ImageCommit) {
        let mut restored = HashMap::new();
        for (entry, attachment) in commit.drafts.into_iter().zip(commit.attachments) {
            let _ = move_file(Path::new(&attachment.path), &entry.path);
            restored.insert(entry.draft.draft_id.clone(), entry);
        }
        self.drafts.lock().unwrap().extend(restored);
    }

    pub fn delete_conversation_images(&self, conversation_id: &str) -> Result<(), AgentError> {
        if self.data_dir.is_none() {
            return Ok(());
        }
        let dir = self.conversation_dir(conversation_id)?;
        if dir.exists() {
            std::fs::remove_dir_all(dir).map_err(image_io_error)?;
        }
        Ok(())
    }
}

fn validate_image(bytes: &[u8]) -> Result<(ImageFormat, &'static str, &'static str), AgentError> {
    let format = image::guess_format(bytes).map_err(|_| {
        AgentError::new("image_unsupported", "仅支持 PNG、JPEG、WebP 和 GIF 图片。")
    })?;
    match format {
        ImageFormat::Png => Ok((format, "image/png", "png")),
        ImageFormat::Jpeg => Ok((format, "image/jpeg", "jpg")),
        ImageFormat::WebP => Ok((format, "image/webp", "webp")),
        ImageFormat::Gif => Ok((format, "image/gif", "gif")),
        _ => Err(AgentError::new(
            "image_unsupported",
            "仅支持 PNG、JPEG、WebP 和 GIF 图片。",
        )),
    }
}

fn input_name(input: &ImagePrepareInput) -> String {
    match input {
        ImagePrepareInput::Path { path } => Path::new(path)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("图片")
            .to_string(),
        ImagePrepareInput::Bytes { name, .. } => {
            name.clone().unwrap_or_else(|| "剪贴板图片".into())
        }
    }
}

fn normalized_name(name: &str, extension: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return format!("image.{extension}");
    }
    let safe = Path::new(trimmed)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("image");
    if Path::new(safe).extension().is_some() {
        safe.to_string()
    } else {
        format!("{safe}.{extension}")
    }
}

fn mime_extension(mime: &str) -> Option<&'static str> {
    match mime {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        _ => None,
    }
}

fn image_too_large() -> AgentError {
    AgentError::new("image_too_large", "单张图片不能超过 20 MiB。")
}

fn image_io_error(error: std::io::Error) -> AgentError {
    AgentError::new("image_io_error", error.to_string())
}

fn move_file(source: &Path, destination: &Path) -> Result<(), AgentError> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(image_io_error)?;
    }
    match std::fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(_) => {
            std::fs::copy(source, destination).map_err(image_io_error)?;
            if let Err(error) = std::fs::remove_file(source) {
                let _ = std::fs::remove_file(destination);
                return Err(image_io_error(error));
            }
            Ok(())
        }
    }
}

fn rollback_moves(moved: &[(DraftEntry, PathBuf)]) {
    for (entry, destination) in moved.iter().rev() {
        let _ = move_file(destination, &entry.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn test_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nbc-images-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn encoded_image(format: ImageFormat) -> Vec<u8> {
        let image = image::DynamicImage::new_rgba8(2, 2);
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, format).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn accepts_supported_content_and_rejects_spoofed_or_excess_inputs() {
        let dir = test_dir();
        let service = ImageService::new(Some(dir.clone()));
        service.clear_stale_drafts().unwrap();
        let supported = [
            (ImageFormat::Png, "image/png"),
            (ImageFormat::Jpeg, "image/jpeg"),
            (ImageFormat::WebP, "image/webp"),
            (ImageFormat::Gif, "image/gif"),
        ];
        let mut inputs = supported
            .iter()
            .map(|(format, _)| ImagePrepareInput::Bytes {
                name: Some("wrong.txt".into()),
                bytes_base64: STANDARD.encode(encoded_image(*format)),
            })
            .collect::<Vec<_>>();
        inputs.push(ImagePrepareInput::Bytes {
            name: Some("fake.png".into()),
            bytes_base64: STANDARD.encode(b"not an image"),
        });

        let prepared = service.prepare(inputs, MAX_IMAGES_PER_MESSAGE);
        assert_eq!(prepared.accepted.len(), 4);
        assert_eq!(
            prepared
                .accepted
                .iter()
                .map(|draft| draft.mime.as_str())
                .collect::<Vec<_>>(),
            supported.iter().map(|(_, mime)| *mime).collect::<Vec<_>>()
        );
        assert_eq!(prepared.rejected.len(), 1);
        assert_eq!(prepared.rejected[0].code, "image_unsupported");

        let png = STANDARD.encode(encoded_image(ImageFormat::Png));
        let limited = service.prepare(
            (0..9)
                .map(|index| ImagePrepareInput::Bytes {
                    name: Some(format!("{index}.png")),
                    bytes_base64: png.clone(),
                })
                .collect(),
            MAX_IMAGES_PER_MESSAGE,
        );
        assert_eq!(limited.accepted.len(), MAX_IMAGES_PER_MESSAGE);
        assert_eq!(limited.rejected[0].code, "image_limit");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn draft_commit_rollback_discard_and_startup_cleanup_keep_paths_managed() {
        let dir = test_dir();
        let service = ImageService::new(Some(dir.clone()));
        service.clear_stale_drafts().unwrap();
        let prepared = service.prepare(
            vec![ImagePrepareInput::Bytes {
                name: Some("one.png".into()),
                bytes_base64: STANDARD.encode(encoded_image(ImageFormat::Png)),
            }],
            MAX_IMAGES_PER_MESSAGE,
        );
        let draft = prepared.accepted[0].clone();
        let commit = service
            .commit("conversation", std::slice::from_ref(&draft.draft_id))
            .unwrap();
        assert!(!Path::new(&draft.path).exists());
        assert!(Path::new(&commit.attachments[0].path).is_file());

        service.rollback(commit);
        assert!(Path::new(&draft.path).is_file());
        service
            .discard(std::slice::from_ref(&draft.draft_id))
            .unwrap();
        assert!(!Path::new(&draft.path).exists());

        let stale = dir.join("cache/chat-image-drafts/stale.png");
        std::fs::write(&stale, encoded_image(ImageFormat::Png)).unwrap();
        service.clear_stale_drafts().unwrap();
        assert!(!stale.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_files_larger_than_twenty_mebibytes_before_decoding() {
        let dir = test_dir();
        let path = dir.join("large.png");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_IMAGE_BYTES + 1).unwrap();
        let service = ImageService::new(Some(dir.clone()));
        service.clear_stale_drafts().unwrap();
        let result = service.prepare(
            vec![ImagePrepareInput::Path {
                path: path.to_string_lossy().into_owned(),
            }],
            MAX_IMAGES_PER_MESSAGE,
        );
        assert_eq!(result.rejected[0].code, "image_too_large");
        let _ = std::fs::remove_dir_all(dir);
    }
}
