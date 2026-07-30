use crate::agent::{new_id, AgentError};
use image::{ImageBuffer, Rgba, RgbaImage};
use rawpsd::LayerInfo;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::{Path, PathBuf};

pub const MAX_PSD_BYTES: u64 = 100 * 1024 * 1024;
pub const MAX_PSD_DOCUMENTS_PER_CONVERSATION: usize = 8;
const MAX_PSD_LAYER_CACHE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_PSD_LAYER_CACHE_FILES: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatPsdDocument {
    pub id: String,
    pub name: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub color_mode: String,
    pub layer_count: usize,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PsdAttachmentSummary {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub color_mode: String,
    pub layer_count: usize,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PsdAttachmentManifest {
    pub count: usize,
    pub documents: Vec<PsdAttachmentSummary>,
}

pub(crate) fn attachment_manifest(documents: &[ChatPsdDocument]) -> PsdAttachmentManifest {
    let documents = documents
        .iter()
        .map(|document| PsdAttachmentSummary {
            id: document.id.clone(),
            name: document.name.clone(),
            width: document.width,
            height: document.height,
            color_mode: document.color_mode.clone(),
            layer_count: document.layer_count,
            available: document.available,
        })
        .collect::<Vec<_>>();
    PsdAttachmentManifest {
        count: documents.len(),
        documents,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PsdBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PsdMaskInfo {
    pub present: bool,
    pub disabled: bool,
    pub invert: bool,
    pub default_color: u8,
    pub bounds: Option<PsdBounds>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PsdLayerNode {
    pub id: String,
    pub name: String,
    pub kind: PsdLayerKind,
    pub visible: bool,
    pub opacity: f32,
    pub blend_mode: String,
    pub is_clipped: bool,
    pub has_mask: bool,
    pub mask: PsdMaskInfo,
    pub bounds: PsdBounds,
    pub children: Vec<PsdLayerNode>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PsdLayerKind {
    Group,
    Layer,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PsdStructure {
    pub width: u32,
    pub height: u32,
    pub color_mode: String,
    pub depth: u16,
    pub channel_count: u16,
    pub layer_count: usize,
    pub layers: Vec<PsdLayerNode>,
}

pub struct PsdService {
    data_dir: Option<PathBuf>,
}

impl PsdService {
    pub fn new(data_dir: Option<PathBuf>) -> Self {
        Self { data_dir }
    }

    fn data_dir(&self) -> Result<&Path, AgentError> {
        self.data_dir
            .as_deref()
            .ok_or_else(|| AgentError::new("psd_store_unavailable", "PSD 存储尚未初始化。"))
    }

    fn conversation_dir(&self, conversation_id: &str) -> Result<PathBuf, AgentError> {
        validate_storage_id(conversation_id)?;
        Ok(self.data_dir()?.join("chat-psd").join(conversation_id))
    }

    fn cache_dir(&self) -> Result<PathBuf, AgentError> {
        Ok(self.data_dir()?.join("cache").join("psd-layers"))
    }

    fn cache_document_dir(
        &self,
        conversation_id: &str,
        psd_id: &str,
    ) -> Result<PathBuf, AgentError> {
        validate_storage_id(conversation_id)?;
        validate_storage_id(psd_id)?;
        Ok(self.cache_dir()?.join(conversation_id).join(psd_id))
    }

    fn ensure_managed_dir(&self, relative: &[&str]) -> Result<PathBuf, AgentError> {
        let mut path = self.data_dir()?.to_path_buf();
        for segment in relative {
            validate_storage_id(segment)?;
            path.push(segment);
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                    return Err(AgentError::new(
                        "psd_unsafe_path",
                        "PSD 存储路径不安全，已拒绝访问。",
                    ));
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    std::fs::create_dir(&path).map_err(psd_io_error)?;
                }
                Err(error) => return Err(psd_io_error(error)),
            }
        }
        Ok(path)
    }

    pub fn load(
        &self,
        conversation_id: &str,
        source_path: &str,
    ) -> Result<(ChatPsdDocument, PsdStructure), AgentError> {
        let source = PathBuf::from(source_path);
        let metadata = std::fs::metadata(&source)
            .map_err(|_| AgentError::new("psd_unreadable", "无法读取该 PSD 文件。"))?;
        if !metadata.is_file() {
            return Err(AgentError::new("psd_unreadable", "所选项目不是文件。"));
        }
        if metadata.len() > MAX_PSD_BYTES {
            return Err(AgentError::new(
                "psd_too_large",
                "PSD 文件不能超过 100 MiB。",
            ));
        }
        let bytes = std::fs::read(&source)
            .map_err(|_| AgentError::new("psd_unreadable", "无法读取该 PSD 文件。"))?;
        let structure = parse_psd(&bytes)?;
        let psd_id = new_id();
        let directory = self.ensure_managed_dir(&["chat-psd", conversation_id])?;
        let stored_path = directory.join(format!("{psd_id}.psd"));
        std::fs::write(&stored_path, &bytes).map_err(psd_io_error)?;
        let name = source
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("document.psd")
            .to_string();
        let document = ChatPsdDocument {
            id: psd_id,
            name,
            path: stored_path.to_string_lossy().into_owned(),
            width: structure.width,
            height: structure.height,
            color_mode: structure.color_mode.clone(),
            layer_count: structure.layer_count,
            available: true,
        };
        Ok((document, structure))
    }

    pub fn read_structure(&self, psd_id: &str, conversation_id: &str) -> Result<PsdStructure, AgentError> {
        let path = self.resolve_path(psd_id, conversation_id)?;
        let bytes = std::fs::read(&path).map_err(|_| {
            AgentError::new("psd_unavailable", "PSD 文件已失效，请重新添加。")
        })?;
        parse_psd(&bytes)
    }

    pub fn extract_layer_image(
        &self,
        psd_id: &str,
        conversation_id: &str,
        layer_id: &str,
    ) -> Result<String, AgentError> {
        let path = self.resolve_path(psd_id, conversation_id)?;
        let index: usize = layer_id
            .parse()
            .map_err(|_| AgentError::new("invalid_arguments", "layerId 无效。"))?;
        let out = self
            .cache_document_dir(conversation_id, psd_id)?
            .join(format!("{index}.png"));
        if managed_regular_file_exists(&out)? {
            return Ok(out.to_string_lossy().into_owned());
        }
        let bytes = std::fs::read(&path).map_err(|_| {
            AgentError::new("psd_unavailable", "PSD 文件已失效，请重新添加。")
        })?;
        let layers = rawpsd::parse_layer_records(&bytes)
            .map_err(|_| AgentError::new("psd_invalid_data", "无法解析该 PSD 文件。"))?;
        let layer = layers
            .get(index)
            .ok_or_else(|| AgentError::new("invalid_arguments", "未找到该图层。"))?;
        if layer.group_opener || layer.group_closer {
            return Err(AgentError::new(
                "psd_layer_no_pixels",
                "该条目是图层组边界，没有独立画面。",
            ));
        }
        let (rgba, width, height) = match layer_pixels(layer) {
            Some(value) => value,
            None => match layer_pixels_via_psd_crate(&bytes, index) {
                Some(value) => value,
                None => {
                    return Err(AgentError::new(
                        "psd_layer_no_pixels",
                        "该图层没有可用的像素画面。",
                    ));
                }
            },
        };
        let png = encode_rgba_png(&rgba, width, height)?;
        self.store_layer_cache(conversation_id, psd_id, index, &png)
    }

    pub fn discard(&self, psd_id: &str, conversation_id: &str) -> Result<(), AgentError> {
        let path = self.stored_path(psd_id, conversation_id)?;
        let cache = self.cache_document_dir(conversation_id, psd_id)?;
        remove_managed_path(&cache)?;
        remove_legacy_layer_cache(&self.cache_dir()?, &[psd_id.to_string()])?;
        remove_managed_path(&path)?;
        if let Some(parent) = path.parent() {
            remove_empty_dir(parent)?;
        }
        Ok(())
    }

    pub fn delete_conversation_psds(&self, conversation_id: &str) -> Result<(), AgentError> {
        if self.data_dir.is_none() {
            return Ok(());
        }
        let dir = self.conversation_dir(conversation_id)?;
        let psd_ids = stored_psd_ids(&dir)?;
        let cache_root = self.cache_dir()?;
        let mut first_error = None;
        remember_cleanup_error(
            &mut first_error,
            remove_managed_path(&cache_root.join(conversation_id)),
        );
        remember_cleanup_error(
            &mut first_error,
            remove_legacy_layer_cache(&cache_root, &psd_ids),
        );
        remember_cleanup_error(&mut first_error, remove_managed_path(&dir));
        first_error.map_or(Ok(()), Err)
    }

    fn store_layer_cache(
        &self,
        conversation_id: &str,
        psd_id: &str,
        layer_index: usize,
        png: &[u8],
    ) -> Result<String, AgentError> {
        let directory =
            self.ensure_managed_dir(&["cache", "psd-layers", conversation_id, psd_id])?;
        let out = directory.join(format!("{layer_index}.png"));
        if managed_regular_file_exists(&out)? {
            return Ok(out.to_string_lossy().into_owned());
        }
        let cache_root = self.cache_dir()?;
        let (bytes, files) = managed_usage(&cache_root)?;
        let next_bytes = bytes.saturating_add(png.len() as u64);
        let next_files = files.saturating_add(1);
        if next_bytes > MAX_PSD_LAYER_CACHE_BYTES || next_files > MAX_PSD_LAYER_CACHE_FILES {
            return Err(AgentError::new(
                "psd_cache_limit",
                "PSD 图层缓存空间已满，请移除不再使用的 PSD。",
            ));
        }
        let temporary = directory.join(format!(".{layer_index}-{}.tmp", new_id()));
        std::fs::write(&temporary, png).map_err(psd_io_error)?;
        match managed_regular_file_exists(&out) {
            Ok(true) => {
                let _ = std::fs::remove_file(&temporary);
            }
            Ok(false) => {
                if let Err(error) = std::fs::rename(&temporary, &out) {
                    let _ = std::fs::remove_file(&temporary);
                    if !managed_regular_file_exists(&out)? {
                        return Err(psd_io_error(error));
                    }
                }
            }
            Err(error) => {
                let _ = std::fs::remove_file(&temporary);
                return Err(error);
            }
        }
        Ok(out.to_string_lossy().into_owned())
    }

    fn stored_path(&self, psd_id: &str, conversation_id: &str) -> Result<PathBuf, AgentError> {
        validate_storage_id(psd_id)?;
        Ok(self
            .conversation_dir(conversation_id)?
            .join(format!("{psd_id}.psd")))
    }

    fn resolve_path(&self, psd_id: &str, conversation_id: &str) -> Result<PathBuf, AgentError> {
        let path = self.stored_path(psd_id, conversation_id)?;
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(path),
            Ok(_) => Err(AgentError::new(
                "psd_unsafe_path",
                "PSD 存储路径不安全，已拒绝访问。",
            )),
            Err(_) => Err(AgentError::new(
                "psd_unavailable",
                "PSD 文件已失效，请重新添加。",
            )),
        }
    }
}

fn validate_storage_id(value: &str) -> Result<(), AgentError> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(AgentError::new(
            "invalid_arguments",
            "PSD 存储标识无效。",
        ));
    }
    Ok(())
}

fn managed_regular_file_exists(path: &Path) -> Result<bool, AgentError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(true),
        Ok(_) => Err(AgentError::new(
            "psd_unsafe_path",
            "PSD 图层缓存路径不安全，已拒绝访问。",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(psd_io_error(error)),
    }
}

fn remove_managed_path(path: &Path) -> Result<(), AgentError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            std::fs::remove_dir_all(path).map_err(psd_io_error)
        }
        Ok(_) => std::fs::remove_file(path).map_err(psd_io_error),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(psd_io_error(error)),
    }
}

fn remove_empty_dir(path: &Path) -> Result<(), AgentError> {
    match std::fs::read_dir(path) {
        Ok(mut entries) => {
            if entries.next().is_none() {
                std::fs::remove_dir(path).map_err(psd_io_error)
            } else {
                Ok(())
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(psd_io_error(error)),
    }
}

fn stored_psd_ids(directory: &Path) -> Result<Vec<String>, AgentError> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(psd_io_error(error)),
    };
    let mut ids = Vec::new();
    for entry in entries {
        let entry = entry.map_err(psd_io_error)?;
        let metadata = std::fs::symlink_metadata(entry.path()).map_err(psd_io_error)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            continue;
        }
        let path = entry.path();
        let Some(id) = path
            .file_stem()
            .and_then(|value| value.to_str())
            .filter(|id| validate_storage_id(id).is_ok())
        else {
            continue;
        };
        if path.extension().and_then(|value| value.to_str()) == Some("psd") {
            ids.push(id.to_string());
        }
    }
    Ok(ids)
}

fn remove_legacy_layer_cache(cache_root: &Path, psd_ids: &[String]) -> Result<(), AgentError> {
    if psd_ids.is_empty() {
        return Ok(());
    }
    let entries = match std::fs::read_dir(cache_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(psd_io_error(error)),
    };
    for entry in entries {
        let entry = entry.map_err(psd_io_error)?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path).map_err(psd_io_error)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name.ends_with(".png")
            && psd_ids
                .iter()
                .any(|psd_id| name.starts_with(&format!("{psd_id}-")))
        {
            std::fs::remove_file(path).map_err(psd_io_error)?;
        }
    }
    Ok(())
}

fn remember_cleanup_error(first_error: &mut Option<AgentError>, result: Result<(), AgentError>) {
    if let Err(error) = result {
        first_error.get_or_insert(error);
    }
}

fn managed_usage(path: &Path) -> Result<(u64, usize), AgentError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((0, 0)),
        Err(error) => return Err(psd_io_error(error)),
    };
    if metadata.file_type().is_symlink() {
        return Ok((0, 0));
    }
    if metadata.is_file() {
        return Ok((metadata.len(), 1));
    }
    let mut usage = (0u64, 0usize);
    for entry in std::fs::read_dir(path).map_err(psd_io_error)? {
        let child = entry.map_err(psd_io_error)?.path();
        let child_usage = managed_usage(&child)?;
        usage.0 = usage.0.saturating_add(child_usage.0);
        usage.1 = usage.1.saturating_add(child_usage.1);
    }
    Ok(usage)
}

fn parse_psd(bytes: &[u8]) -> Result<PsdStructure, AgentError> {
    let meta = rawpsd::parse_psd_metadata(bytes)
        .map_err(|_| AgentError::new("psd_invalid_data", "无法解析该 PSD 文件头。"))?;
    let layers = rawpsd::parse_layer_records(bytes)
        .map_err(|_| AgentError::new("psd_invalid_data", "无法解析该 PSD 图层记录。"))?;
    let tree = build_layer_tree(&layers);
    Ok(PsdStructure {
        width: meta.width,
        height: meta.height,
        color_mode: color_mode_name(meta.color_mode),
        depth: meta.depth,
        channel_count: meta.channel_count,
        layer_count: tree.len(),
        layers: tree,
    })
}

fn build_layer_tree(layers: &[LayerInfo]) -> Vec<PsdLayerNode> {
    // rawpsd returns bottom-to-top; reversing makes named openers precede their
    // children and hidden closers complete the current group.
    let mut root = Vec::new();
    let mut groups: Vec<PsdLayerNode> = Vec::new();
    for (idx, layer) in layers.iter().enumerate().rev() {
        if layer.group_opener {
            groups.push(layer_node(idx, layer));
            continue;
        }
        let node = if layer.group_closer {
            let Some(group) = groups.pop() else {
                continue;
            };
            group
        } else {
            layer_node(idx, layer)
        };
        if let Some(parent) = groups.last_mut() {
            parent.children.push(node);
        } else {
            root.push(node);
        }
    }
    while let Some(group) = groups.pop() {
        if let Some(parent) = groups.last_mut() {
            parent.children.push(group);
        } else {
            root.push(group);
        }
    }
    root
}

fn layer_node(idx: usize, layer: &LayerInfo) -> PsdLayerNode {
    let kind = if layer.group_opener {
        PsdLayerKind::Group
    } else {
        PsdLayerKind::Layer
    };
    let has_mask = layer.mask_channel_count > 0 && layer.image_data_mask.len() > 0;
    PsdLayerNode {
        id: idx.to_string(),
        name: layer.name.clone(),
        kind,
        visible: layer.is_visible,
        opacity: layer.opacity,
        blend_mode: layer.blend_mode.clone(),
        is_clipped: layer.is_clipped,
        has_mask,
        mask: PsdMaskInfo {
            present: has_mask,
            disabled: layer.mask_info.disabled,
            invert: layer.mask_info.invert,
            default_color: layer.mask_info.default_color,
            bounds: if has_mask && layer.mask_info.w > 0 {
                Some(PsdBounds {
                    x: layer.mask_info.x,
                    y: layer.mask_info.y,
                    width: layer.mask_info.w,
                    height: layer.mask_info.h,
                })
            } else {
                None
            },
        },
        bounds: PsdBounds {
            x: layer.x,
            y: layer.y,
            width: layer.w,
            height: layer.h,
        },
        children: Vec::new(),
    }
}

fn layer_pixels(layer: &LayerInfo) -> Option<(Vec<u8>, u32, u32)> {
    let width = layer.w;
    let height = layer.h;
    if width == 0 || height == 0 {
        return None;
    }
    let expected = (width as usize) * (height as usize) * 4;
    if layer.image_data_rgba.len() == expected {
        Some((layer.image_data_rgba.clone(), width, height))
    } else {
        None
    }
}

fn layer_pixels_via_psd_crate(bytes: &[u8], index: usize) -> Option<(Vec<u8>, u32, u32)> {
    let psd = psd::Psd::from_bytes(bytes).ok()?;
    let layer = psd.layers().get(index)?;
    let rgba = layer.rgba();
    if rgba.is_empty() {
        return None;
    }
    Some((rgba, layer.width() as u32, layer.height() as u32))
}

fn encode_rgba_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, AgentError> {
    let img: RgbaImage = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(width, height, rgba.to_vec())
        .ok_or_else(|| AgentError::new("psd_invalid_data", "图层像素数据与尺寸不匹配。"))?;
    let mut bytes = Cursor::new(Vec::new());
    img.write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|_| AgentError::new("psd_encode_failed", "图层图像编码失败。"))?;
    Ok(bytes.into_inner())
}

fn color_mode_name(code: u16) -> String {
    match code {
        0 => "bitmap",
        1 => "indexed",
        2 => "rgb",
        3 => "grayscale",
        4 => "cmyk",
        7 => "multichannel",
        8 => "duotone",
        9 => "lab",
        _ => "unknown",
    }
    .to_string()
}

fn psd_io_error(error: std::io::Error) -> AgentError {
    AgentError::new("psd_io_error", error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_u16(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn push_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn psd_layer_record(name: &str, section_kind: Option<u32>) -> Vec<u8> {
        let mut record = Vec::new();
        record.extend_from_slice(&[0; 16]);
        push_u16(&mut record, 0);
        record.extend_from_slice(b"8BIMnorm");
        record.extend_from_slice(&[255, 0, 0, 0]);

        let mut extra = Vec::new();
        push_u32(&mut extra, 18);
        extra.extend_from_slice(&[0; 16]);
        extra.extend_from_slice(&[255, 0]);
        push_u32(&mut extra, 0);

        let mut pascal_name = vec![name.len() as u8];
        pascal_name.extend_from_slice(name.as_bytes());
        while pascal_name.len() % 4 != 0 {
            pascal_name.push(0);
        }
        extra.extend(pascal_name);

        if let Some(kind) = section_kind {
            extra.extend_from_slice(b"8BIMlsct");
            push_u32(&mut extra, 4);
            push_u32(&mut extra, kind);
        }

        push_u32(&mut record, extra.len() as u32);
        record.extend(extra);
        record
    }

    fn nested_group_psd_fixture() -> Vec<u8> {
        let records = [
            psd_layer_record("</Layer group>", Some(3)),
            psd_layer_record("</Layer group>", Some(3)),
            psd_layer_record("First Layer", None),
            psd_layer_record("group inside", Some(1)),
            psd_layer_record("group outside", Some(1)),
        ]
        .concat();

        let mut layer_info = Vec::new();
        push_u16(&mut layer_info, 5);
        layer_info.extend(records);

        let mut layer_mask_info = Vec::new();
        push_u32(&mut layer_mask_info, layer_info.len() as u32);
        layer_mask_info.extend(layer_info);
        push_u32(&mut layer_mask_info, 0);

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"8BPS");
        push_u16(&mut bytes, 1);
        bytes.extend_from_slice(&[0; 6]);
        push_u16(&mut bytes, 3);
        push_u32(&mut bytes, 1);
        push_u32(&mut bytes, 1);
        push_u16(&mut bytes, 8);
        push_u16(&mut bytes, 3);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, layer_mask_info.len() as u32);
        bytes.extend(layer_mask_info);
        bytes
    }

    fn layer(name: &str) -> LayerInfo {
        let mut layer = LayerInfo::default();
        layer.name = name.into();
        layer
    }

    fn group_opener(name: &str) -> LayerInfo {
        let mut layer = layer(name);
        layer.group_opener = true;
        layer
    }

    fn group_closer() -> LayerInfo {
        let mut layer = layer("</Layer group>");
        layer.group_closer = true;
        layer
    }

    #[test]
    fn attachment_manifest_exposes_only_safe_conversation_metadata() {
        let manifest = attachment_manifest(&[ChatPsdDocument {
            id: "psd-1".into(),
            name: "character.psd".into(),
            path: "C:\\managed\\private\\psd-1.psd".into(),
            width: 2048,
            height: 4096,
            color_mode: "rgb".into(),
            layer_count: 42,
            available: true,
        }]);
        let value = serde_json::to_value(&manifest).unwrap();

        assert_eq!(value["count"], 1);
        assert_eq!(value["documents"][0]["id"], "psd-1");
        assert_eq!(value["documents"][0]["layerCount"], 42);
        assert!(value["documents"][0].get("path").is_none());
    }

    #[test]
    fn layer_tree_preserves_nested_groups_and_top_level_order() {
        let layers = vec![
            layer("Bottom"),
            group_closer(),
            layer("Outer bottom"),
            group_closer(),
            layer("Inner child"),
            group_opener("Inner"),
            layer("Outer top"),
            group_opener("Outer"),
            layer("Top"),
        ];

        let tree = build_layer_tree(&layers);

        assert_eq!(
            tree.iter()
                .map(|node| node.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Top", "Outer", "Bottom"]
        );
        assert_eq!(tree[1].id, "7");
        assert_eq!(
            tree[1]
                .children
                .iter()
                .map(|node| node.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Outer top", "Inner", "Outer bottom"]
        );
        assert_eq!(tree[1].children[1].id, "5");
        assert_eq!(tree[1].children[1].kind, PsdLayerKind::Group);
        assert_eq!(tree[1].children[1].children[0].name, "Inner child");
    }

    #[test]
    fn parsed_nested_group_structure_never_exposes_hidden_boundaries() {
        let bytes = nested_group_psd_fixture();
        let raw_layers = rawpsd::parse_layer_records(&bytes)
            .map_err(|(_, error)| error)
            .unwrap();
        assert_eq!(
            raw_layers.iter().filter(|layer| layer.group_closer).count(),
            2
        );

        let structure = parse_psd(&bytes).unwrap();

        assert_eq!((structure.width, structure.height), (1, 1));
        assert_eq!(structure.layers.len(), 1);
        let outer = &structure.layers[0];
        assert_eq!(
            (outer.name.as_str(), outer.kind),
            ("group outside", PsdLayerKind::Group)
        );
        assert_eq!(outer.children.len(), 1);
        let inner = &outer.children[0];
        assert_eq!(
            (inner.name.as_str(), inner.kind),
            ("group inside", PsdLayerKind::Group)
        );
        assert_eq!(inner.children.len(), 1);
        assert_eq!(
            (inner.children[0].name.as_str(), inner.children[0].kind),
            ("First Layer", PsdLayerKind::Layer)
        );

        let json = serde_json::to_string(&structure).unwrap();
        assert!(!json.contains("</Layer group>"));
        assert!(!json.contains("group_end"));
    }

    #[test]
    fn layer_cache_is_reused_and_removed_with_its_owner() {
        let data_dir = std::env::temp_dir().join(format!("nbc-psd-cache-{}", new_id()));
        std::fs::create_dir_all(&data_dir).unwrap();
        let source = data_dir.join("source.psd");
        std::fs::write(&source, nested_group_psd_fixture()).unwrap();
        let service = PsdService::new(Some(data_dir.clone()));

        let (chat_document, _) = service.load("chat", &source.to_string_lossy()).unwrap();
        let (mcp_document, _) = service.load("mcp", &source.to_string_lossy()).unwrap();
        let first = service
            .store_layer_cache("mcp", &mcp_document.id, 2, b"first")
            .unwrap();
        let repeated = service
            .store_layer_cache("mcp", &mcp_document.id, 2, b"replacement")
            .unwrap();
        let chat_cache = service
            .store_layer_cache("chat", &chat_document.id, 2, b"chat")
            .unwrap();
        let legacy_cache = service
            .cache_dir()
            .unwrap()
            .join(format!("{}-2-legacy.png", mcp_document.id));
        std::fs::write(&legacy_cache, b"legacy").unwrap();

        assert_eq!(first, repeated);
        assert_eq!(std::fs::read(&first).unwrap(), b"first");
        assert_eq!(managed_usage(&service.cache_dir().unwrap()).unwrap().1, 3);

        service.discard(&mcp_document.id, "mcp").unwrap();
        assert!(!Path::new(&mcp_document.path).exists());
        assert!(!Path::new(&first).exists());
        assert!(!legacy_cache.exists());
        assert!(Path::new(&chat_document.path).is_file());
        assert!(Path::new(&chat_cache).is_file());

        service.delete_conversation_psds("chat").unwrap();
        assert!(!Path::new(&chat_document.path).exists());
        assert!(!Path::new(&chat_cache).exists());
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn layer_cache_budget_rejects_unbounded_growth() {
        let data_dir = std::env::temp_dir().join(format!("nbc-psd-budget-{}", new_id()));
        let cache_root = data_dir.join("cache/psd-layers");
        std::fs::create_dir_all(&cache_root).unwrap();
        let oversized = std::fs::File::create(cache_root.join("existing.png")).unwrap();
        oversized.set_len(MAX_PSD_LAYER_CACHE_BYTES).unwrap();
        let service = PsdService::new(Some(data_dir.clone()));

        let error = service
            .store_layer_cache("chat", "document", 0, b"new")
            .unwrap_err();
        assert_eq!(error.code, "psd_cache_limit");

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn managed_psd_paths_reject_parent_and_symlink_escape() {
        let data_dir = std::env::temp_dir().join(format!("nbc-psd-paths-{}", new_id()));
        std::fs::create_dir_all(data_dir.join("chat-psd")).unwrap();
        let service = PsdService::new(Some(data_dir.clone()));

        assert_eq!(
            service
                .delete_conversation_psds("../outside")
                .unwrap_err()
                .code,
            "invalid_arguments"
        );

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(std::env::temp_dir(), data_dir.join("chat-psd").join("mcp"))
                .unwrap();
            let source = data_dir.join("source.psd");
            std::fs::write(&source, nested_group_psd_fixture()).unwrap();
            assert_eq!(
                service
                    .load("mcp", &source.to_string_lossy())
                    .unwrap_err()
                    .code,
                "psd_unsafe_path"
            );
        }

        let _ = std::fs::remove_dir_all(data_dir);
    }
}
