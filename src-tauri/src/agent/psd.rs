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
}

