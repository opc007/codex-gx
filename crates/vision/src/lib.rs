//! v1.9.4：Vision 多模态 (图像/视频理解) 协议层
//!
//! 设计参考：docs/开发文档.md §5.31
//!
//! ## 简化的 v1.9.4 实现
//! - 图像格式检测（PNG/JPEG/GIF/WebP/BMP）
//! - 图像元数据提取（宽高、mode、aspect）
//! - 模拟 OCR（基于文件名 + size 的标签启发式）
//! - 模拟 image captioning（生成结构化 description template）
//! - 多模态 prompt 模板（注入到 system prompt）
//! - 视频帧提取提示（真实解码留 v1.9.5+）
//!
//! ## DoD
//! - 5 种图像格式识别
//! - 元数据解析
//! - 多模态 system prompt
//! - screenshot annotation 协议

use std::path::Path;

use serde::{Deserialize, Serialize};

/// 图像格式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    WebP,
    Bmp,
    Tiff,
    Heic,
    Unknown,
}

impl ImageFormat {
    pub fn from_ext(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "png" => Self::Png,
            "jpg" | "jpeg" => Self::Jpeg,
            "gif" => Self::Gif,
            "webp" => Self::WebP,
            "bmp" => Self::Bmp,
            "tif" | "tiff" => Self::Tiff,
            "heic" | "heif" => Self::Heic,
            _ => Self::Unknown,
        }
    }

    pub fn from_bytes(magic: &[u8]) -> Self {
        // PNG: 89 50 4E 47 0D 0A 1A 0A
        if magic.len() >= 8 && &magic[0..8] == b"\x89PNG\r\n\x1a\n" {
            return Self::Png;
        }
        // JPEG: FF D8 FF
        if magic.len() >= 3 && magic[0] == 0xFF && magic[1] == 0xD8 && magic[2] == 0xFF {
            return Self::Jpeg;
        }
        // GIF: GIF87a or GIF89a
        if magic.len() >= 6 && (&magic[0..6] == b"GIF87a" || &magic[0..6] == b"GIF89a") {
            return Self::Gif;
        }
        // WebP: RIFF....WEBP
        if magic.len() >= 12 && &magic[0..4] == b"RIFF" && &magic[8..12] == b"WEBP" {
            return Self::WebP;
        }
        // BMP: BM
        if magic.len() >= 2 && &magic[0..2] == b"BM" {
            return Self::Bmp;
        }
        // TIFF: II*\0 or MM\0*
        if magic.len() >= 4 {
            if (&magic[0..4] == b"II*\0") || (&magic[0..4] == b"MM\0*") {
                return Self::Tiff;
            }
        }
        Self::Unknown
    }

    pub fn mime(&self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::WebP => "image/webp",
            Self::Bmp => "image/bmp",
            Self::Tiff => "image/tiff",
            Self::Heic => "image/heic",
            Self::Unknown => "application/octet-stream",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Gif => "GIF",
            Self::WebP => "WebP",
            Self::Bmp => "BMP",
            Self::Tiff => "TIFF",
            Self::Heic => "HEIC",
            Self::Unknown => "Unknown",
        }
    }
}

/// 图像元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageMeta {
    pub format: ImageFormat,
    pub mime: String,
    pub size_bytes: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub aspect_ratio: Option<f32>,
    pub mode: Option<String>, // RGB / RGBA / L / P
    pub source: String, // path / url / base64
}

/// 图像标注区域（screenshot annotation 协议）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationBox {
    pub id: String,
    pub label: String,
    pub x: f32, // 0.0 - 1.0 relative
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub confidence: f32,
    pub description: Option<String>,
}

/// OCR 结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrResult {
    pub text: String,
    pub lines: Vec<OcrLine>,
    pub confidence: f32,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrLine {
    pub text: String,
    pub bbox: (f32, f32, f32, f32), // x, y, w, h (relative 0-1)
    pub confidence: f32,
}

/// 图像描述（caption）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageCaption {
    pub short: String,        // 一句话
    pub detailed: String,     // 详细描述
    pub tags: Vec<String>,    // 物体标签
    pub colors: Vec<String>,  // 主色调
    pub mood: Option<String>, // 情绪/氛围
}

/// 检测图像元数据
pub fn detect_image(path: &Path) -> Result<ImageMeta, VisionError> {
    let bytes = std::fs::read(path).map_err(VisionError::Io)?;
    let format = ImageFormat::from_bytes(&bytes);
    let (w, h, mode) = extract_dims(&format, &bytes);

    Ok(ImageMeta {
        format,
        mime: format.mime().to_string(),
        size_bytes: bytes.len() as u64,
        width: w,
        height: h,
        aspect_ratio: match (w, h) {
            (Some(w), Some(h)) if h > 0 => Some(w as f32 / h as f32),
            _ => None,
        },
        mode,
        source: path.display().to_string(),
    })
}

/// 从字节提取宽高（PNG/JPEG/GIF 简化）
fn extract_dims(format: &ImageFormat, bytes: &[u8]) -> (Option<u32>, Option<u32>, Option<String>) {
    match format {
        ImageFormat::Png => {
            if bytes.len() >= 24 {
                let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
                let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
                let mode = match bytes[25] {
                    0 => Some("GRAY".into()),
                    2 => Some("RGB".into()),
                    3 => Some("PALETTE".into()),
                    4 => Some("GRAY_ALPHA".into()),
                    6 => Some("RGBA".into()),
                    _ => Some(format!("TYPE_{}", bytes[25]),
                    ),
                };
                (Some(w), Some(h), mode)
            } else {
                (None, None, None)
            }
        }
        ImageFormat::Gif => {
            if bytes.len() >= 10 {
                let w = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
                let h = u16::from_le_bytes([bytes[8], bytes[9]]) as u32;
                (Some(w), Some(h), Some("PALETTE".into()))
            } else {
                (None, None, None)
            }
        }
        ImageFormat::Bmp => {
            if bytes.len() >= 26 {
                let w = u32::from_le_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]);
                let h = u32::from_le_bytes([bytes[22], bytes[23], bytes[24], bytes[25]]);
                (Some(w), Some(h), Some("BGR".into()))
            } else {
                (None, None, None)
            }
        }
        _ => (None, None, None),
    }
}

/// OCR（模拟 — 基于启发式生成 placeholder）
pub fn ocr_stub(meta: &ImageMeta) -> OcrResult {
    // 真实 OCR 引擎集成留 v2.0
    OcrResult {
        text: format!("[OCR stub: {} {}×{}]", meta.format.label(),
            meta.width.unwrap_or(0), meta.height.unwrap_or(0)),
        lines: vec![OcrLine {
            text: "(demo line 1: text detection requires real OCR engine)".into(),
            bbox: (0.05, 0.05, 0.9, 0.1),
            confidence: 0.85,
        }],
        confidence: 0.0,
        language: "auto".into(),
    }
}

/// 图像 caption（模拟 — 模板化生成）
pub fn caption_stub(meta: &ImageMeta) -> ImageCaption {
    let short = format!(
        "A {} image ({}×{})",
        meta.format.label(),
        meta.width.unwrap_or(0),
        meta.height.unwrap_or(0)
    );
    let aspect = meta
        .aspect_ratio
        .map(|a| {
            if a > 1.5 {
                "landscape"
            } else if a < 0.7 {
                "portrait"
            } else {
                "square-ish"
            }
        })
        .unwrap_or("unknown-aspect");
    let detailed = format!(
        "This is a {} image in {} layout, total size {:.1} KB. The image appears to be a typical screenshot or photo. Real caption generation requires a vision-language model API call (e.g., GPT-4o, Claude 3.5 Sonnet, or local LLaVA).",
        meta.format.label(),
        aspect,
        meta.size_bytes as f32 / 1024.0
    );
    ImageCaption {
        short,
        detailed,
        tags: vec!["image".into(), meta.format.label().to_lowercase()],
        colors: vec!["#ffffff".into(), "#000000".into()],
        mood: Some("neutral".into()),
    }
}

/// Screenshot 标注 — 根据 annotations 生成结构化描述
pub fn annotate_screenshot(meta: &ImageMeta, boxes: &[AnnotationBox]) -> String {
    let mut out = format!(
        "Screenshot annotation ({} boxes):\n\n",
        boxes.len()
    );
    out.push_str(&format!(
        "  image: {} {}×{}\n\n",
        meta.format.label(),
        meta.width.unwrap_or(0),
        meta.height.unwrap_or(0)
    ));
    for b in boxes {
        let x = (b.x * 100.0).round() as u32;
        let y = (b.y * 100.0).round() as u32;
        let w = (b.w * 100.0).round() as u32;
        let h = (b.h * 100.0).round() as u32;
        let desc = b.description.as_deref().unwrap_or("(no description)");
        out.push_str(&format!(
            "  - [{}] {}\n    bbox: ({}, {}, {}, {})  conf: {:.2}\n    {}\n",
            b.id, b.label, x, y, w, h, b.confidence, desc
        ));
    }
    out
}

/// 多模态 system prompt 注入
pub const VISION_PROMPT: &str = r#"# Vision 多模态协议（v1.9.4）

## 可用能力
- **图像理解**：`vision_describe <path>` 生成 caption
- **OCR**：`vision_ocr <path>` 提取文字（stub）
- **标注**：`vision_annotate` 输出结构化 screenshot annotation
- **元数据**：`vision_meta <path>` 宽高/格式/aspect

## 注意事项
- 真实多模态能力需调用 VLM API（OpenAI GPT-4o / Anthropic Claude 3.5 Sonnet / Google Gemini / 本地 LLaVA）
- 简化版仅做格式检测 + 元数据 + 模板化 caption
- 截图相对坐标协议（M3）见 §5.9.1（RelativeCoord 0.0-1.0）

## 调用示例
```
user> /vision meta ~/Desktop/screen.png
agent> 📷 PNG image 1920×1080 (16:9, 234 KB)

user> 这张图里有什么？
agent> 调用 vision_describe + vision_ocr，给出综合描述
```
"#;

#[derive(Debug, thiserror::Error)]
pub enum VisionError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_from_ext() {
        assert_eq!(ImageFormat::from_ext("png"), ImageFormat::Png);
        assert_eq!(ImageFormat::from_ext("JPG"), ImageFormat::Jpeg);
        assert_eq!(ImageFormat::from_ext("webp"), ImageFormat::WebP);
        assert_eq!(ImageFormat::from_ext("xyz"), ImageFormat::Unknown);
    }

    #[test]
    fn test_format_from_bytes_png() {
        let magic = b"\x89PNG\r\n\x1a\n";
        assert_eq!(ImageFormat::from_bytes(magic), ImageFormat::Png);
    }

    #[test]
    fn test_format_from_bytes_jpeg() {
        let magic = b"\xFF\xD8\xFF\xE0";
        assert_eq!(ImageFormat::from_bytes(magic), ImageFormat::Jpeg);
    }

    #[test]
    fn test_format_from_bytes_gif() {
        let magic = b"GIF89a...";
        assert_eq!(ImageFormat::from_bytes(magic), ImageFormat::Gif);
    }

    #[test]
    fn test_format_from_bytes_webp() {
        let magic = b"RIFF\x00\x00\x00\x00WEBP";
        assert_eq!(ImageFormat::from_bytes(magic), ImageFormat::WebP);
    }

    #[test]
    fn test_format_mime() {
        assert_eq!(ImageFormat::Png.mime(), "image/png");
        assert_eq!(ImageFormat::Jpeg.mime(), "image/jpeg");
        assert_eq!(ImageFormat::WebP.mime(), "image/webp");
    }

    #[test]
    fn test_annotate_screenshot() {
        let meta = ImageMeta {
            format: ImageFormat::Png,
            mime: "image/png".into(),
            size_bytes: 1024,
            width: Some(1920),
            height: Some(1080),
            aspect_ratio: Some(16.0 / 9.0),
            mode: Some("RGBA".into()),
            source: "screen.png".into(),
        };
        let boxes = vec![
            AnnotationBox {
                id: "btn-1".into(),
                label: "Submit button".into(),
                x: 0.5, y: 0.6, w: 0.1, h: 0.05,
                confidence: 0.95,
                description: Some("blue button".into()),
            },
        ];
        let txt = annotate_screenshot(&meta, &boxes);
        assert!(txt.contains("1 boxes"));
        assert!(txt.contains("Submit button"));
        assert!(txt.contains("btn-1"));
    }

    #[test]
    fn test_ocr_stub() {
        let meta = ImageMeta {
            format: ImageFormat::Png,
            mime: "image/png".into(),
            size_bytes: 1024,
            width: Some(800),
            height: Some(600),
            aspect_ratio: Some(4.0 / 3.0),
            mode: Some("RGBA".into()),
            source: "test.png".into(),
        };
        let r = ocr_stub(&meta);
        assert!(r.text.contains("OCR stub"));
        assert_eq!(r.lines.len(), 1);
    }

    #[test]
    fn test_caption_stub() {
        let meta = ImageMeta {
            format: ImageFormat::Jpeg,
            mime: "image/jpeg".into(),
            size_bytes: 50_000,
            width: Some(1920),
            height: Some(1080),
            aspect_ratio: Some(16.0 / 9.0),
            mode: Some("RGB".into()),
            source: "photo.jpg".into(),
        };
        let c = caption_stub(&meta);
        assert!(c.short.contains("JPEG"));
        assert!(c.detailed.contains("landscape"));
        assert!(c.tags.contains(&"jpeg".to_string()));
    }

    #[test]
    fn test_vision_prompt_has_protocol() {
        assert!(VISION_PROMPT.contains("v1.9.4"));
        assert!(VISION_PROMPT.contains("OCR"));
        assert!(VISION_PROMPT.contains("RelativeCoord"));
    }
}