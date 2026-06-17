//! v1.9.4：Vision 多模态 Tauri 命令

use vision::{annotate_screenshot, caption_stub, detect_image, ocr_stub, AnnotationBox, ImageCaption, ImageMeta, OcrResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize)]
pub struct VisionStatus {
    pub version: String,
    pub capabilities: Vec<String>,
    pub prompt_excerpt: String,
}

#[tauri::command]
pub fn vision_status() -> VisionStatus {
    VisionStatus {
        version: "v1.9.4".into(),
        capabilities: vec![
            "image_meta".into(),
            "image_caption".into(),
            "ocr".into(),
            "screenshot_annotate".into(),
            "format_detect".into(),
        ],
        prompt_excerpt: vision::VISION_PROMPT.lines().take(3).collect::<Vec<_>>().join("\n"),
    }
}

#[derive(Deserialize)]
pub struct VisionMetaArgs {
    pub path: String,
}

#[tauri::command]
pub fn vision_meta(args: VisionMetaArgs) -> Result<ImageMeta, String> {
    let p = PathBuf::from(args.path);
    detect_image(&p).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
pub struct VisionCaptionArgs {
    pub path: String,
}

#[tauri::command]
pub fn vision_caption(args: VisionCaptionArgs) -> Result<ImageCaption, String> {
    let p = PathBuf::from(args.path);
    let meta = detect_image(&p).map_err(|e| e.to_string())?;
    Ok(caption_stub(&meta))
}

#[tauri::command]
pub fn vision_ocr(args: VisionMetaArgs) -> Result<OcrResult, String> {
    let p = PathBuf::from(args.path);
    let meta = detect_image(&p).map_err(|e| e.to_string())?;
    Ok(ocr_stub(&meta))
}

#[derive(Deserialize)]
pub struct VisionAnnotateArgs {
    pub path: String,
    pub boxes: Vec<AnnotationBox>,
}

#[tauri::command]
pub fn vision_annotate(args: VisionAnnotateArgs) -> Result<String, String> {
    let p = PathBuf::from(args.path);
    let meta = detect_image(&p).map_err(|e| e.to_string())?;
    Ok(annotate_screenshot(&meta, &args.boxes))
}

#[derive(Serialize)]
pub struct VisionFormatInfo {
    pub name: String,
    pub label: String,
    pub mime: String,
}

#[tauri::command]
pub fn vision_formats() -> Vec<VisionFormatInfo> {
    let fmts = [
        vision::ImageFormat::Png,
        vision::ImageFormat::Jpeg,
        vision::ImageFormat::Gif,
        vision::ImageFormat::WebP,
        vision::ImageFormat::Bmp,
        vision::ImageFormat::Tiff,
        vision::ImageFormat::Heic,
    ];
    fmts.iter()
        .map(|f| VisionFormatInfo {
            name: format!("{:?}", f).to_lowercase(),
            label: f.label().to_string(),
            mime: f.mime().to_string(),
        })
        .collect()
}

#[tauri::command]
pub fn vision_protocol_prompt() -> String {
    vision::VISION_PROMPT.to_string()
}