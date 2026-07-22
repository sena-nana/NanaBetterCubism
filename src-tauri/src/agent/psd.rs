use crate::agent::{new_id, AgentError};
use image::{ImageBuffer, Rgba, RgbaImage};
use rawpsd::LayerInfo;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::{Path, PathBuf};

pub const MAX_PSD_BYTES: u64 = 100 * 1024 * 1024;
pub const MAX_PSD_DOCUMENTS_PER_CONVERSATION: usize = 8;

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
    pub kind: String,
    pub visible: bool,
    pub opacity: f32,
    pub blend_mode: String,
    pub is_clipped: bool,
    pub has_mask: bool,
    pub mask: PsdMaskInfo,
    pub bounds: PsdBounds,
    pub children: Vec<PsdLayerNode>,
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
        Ok(self.data_dir()?.join("chat-psd").join(conversation_id))
    }

    fn cache_dir(&self) -> Result<PathBuf, AgentError> {
        Ok(self.data_dir()?.join("cache").join("psd-layers"))
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
        let directory = self.conversation_dir(conversation_id)?;
        std::fs::create_dir_all(&directory).map_err(psd_io_error)?;
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
        let bytes = std::fs::read(&path).map_err(|_| {
            AgentError::new("psd_unavailable", "PSD 文件已失效，请重新添加。")
        })?;
        let layers = rawpsd::parse_layer_records(&bytes)
            .map_err(|_| AgentError::new("psd_invalid_data", "无法解析该 PSD 文件。"))?;
        let index: usize = layer_id
            .parse()
            .map_err(|_| AgentError::new("invalid_arguments", "layerId 无效。"))?;
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
        let cache = self.cache_dir()?;
        std::fs::create_dir_all(&cache).map_err(psd_io_error)?;
        let out = cache.join(format!("{}-{}-{}.png", psd_id, layer_id, new_id()));
        std::fs::write(&out, &png).map_err(psd_io_error)?;
        Ok(out.to_string_lossy().into_owned())
    }

    pub fn discard(&self, psd_id: &str, conversation_id: &str) -> Result<(), AgentError> {
        if let Some(path) = self.stored_path(psd_id, conversation_id) {
            if path.exists() {
                std::fs::remove_file(path).map_err(psd_io_error)?;
            }
        }
        Ok(())
    }

    pub fn delete_conversation_psds(&self, conversation_id: &str) -> Result<(), AgentError> {
        if self.data_dir.is_none() {
            return Ok(());
        }
        let dir = self.conversation_dir(conversation_id)?;
        if dir.exists() {
            std::fs::remove_dir_all(dir).map_err(psd_io_error)?;
        }
        Ok(())
    }

    fn stored_path(&self, psd_id: &str, conversation_id: &str) -> Option<PathBuf> {
        let dir = self.conversation_dir(conversation_id).ok()?;
        Some(dir.join(format!("{psd_id}.psd")))
    }

    fn resolve_path(&self, psd_id: &str, conversation_id: &str) -> Result<PathBuf, AgentError> {
        let path = self
            .stored_path(psd_id, conversation_id)
            .ok_or_else(|| AgentError::new("psd_store_unavailable", "PSD 存储尚未初始化。"))?;
        if !path.is_file() {
            return Err(AgentError::new("psd_unavailable", "PSD 文件已失效，请重新添加。"));
        }
        Ok(path)
    }
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
    // rawpsd returns layers bottom-to-top, with group boundaries marked by
    // group_opener (bottom of group) and group_closer (top of group).
    // Reverse to top-to-bottom so the closer becomes the group header.
    let order: Vec<(usize, &LayerInfo)> = layers.iter().enumerate().rev().collect();
    // Each frame holds the group header as its first element, followed by children.
    let mut frames: Vec<Vec<PsdLayerNode>> = vec![Vec::new()];
    for (idx, layer) in order {
        if layer.group_closer {
            // Start a new group: push its header node as the first element.
            frames.push(vec![layer_node(idx, layer)]);
        } else if layer.group_opener {
            // Close the current group frame into a group node.
            let frame = frames.pop().unwrap_or_default();
            let mut group_node = if let Some(header) = frame.first() {
                header.clone()
            } else {
                layer_node(idx, layer)
            };
            group_node.children = frame.into_iter().skip(1).collect();
            if let Some(parent) = frames.last_mut() {
                parent.push(group_node);
            }
        } else {
            if let Some(frame) = frames.last_mut() {
                frame.push(layer_node(idx, layer));
            }
        }
    }
    // Flatten any unclosed frames into root.
    let mut root: Vec<PsdLayerNode> = Vec::new();
    while let Some(frame) = frames.pop() {
        root.extend(frame);
    }
    root
}

fn layer_node(idx: usize, layer: &LayerInfo) -> PsdLayerNode {
    let kind = if layer.group_opener {
        "group_end".to_string()
    } else if layer.group_closer {
        "group".to_string()
    } else {
        "layer".to_string()
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
}

