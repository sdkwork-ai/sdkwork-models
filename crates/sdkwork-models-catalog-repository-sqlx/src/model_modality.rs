pub const MODALITY_TEXT: i64 = 1;
pub const MODALITY_IMAGE: i64 = 2;
pub const MODALITY_AUDIO: i64 = 3;
pub const MODALITY_MUSIC: i64 = 4;
pub const MODALITY_VIDEO: i64 = 5;
pub const MODALITY_EMBEDDING: i64 = 6;
pub const MODALITY_RERANK: i64 = 7;
pub const MODALITY_DECISION: i64 = 9;

pub fn code_from_text(value: Option<&str>) -> Option<i64> {
    match value.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "llm" | "text" | "chat" => Some(MODALITY_TEXT),
        "image" | "images" => Some(MODALITY_IMAGE),
        "audio" | "speech" | "voice" | "transcription" | "tts" | "stt" => Some(MODALITY_AUDIO),
        "music" | "sfx" | "soundeffect" | "sound_effect" | "sound_effects" => Some(MODALITY_MUSIC),
        "video" | "videos" => Some(MODALITY_VIDEO),
        "embedding" | "embeddings" => Some(MODALITY_EMBEDDING),
        "rerank" | "reranker" | "ranking" => Some(MODALITY_RERANK),
        "decision" => Some(MODALITY_DECISION),
        _ => None,
    }
}

pub fn label(value: Option<i64>) -> &'static str {
    match value {
        Some(MODALITY_IMAGE) => "image",
        Some(MODALITY_AUDIO) => "audio",
        Some(MODALITY_MUSIC) => "music",
        Some(MODALITY_VIDEO) => "video",
        Some(MODALITY_EMBEDDING) => "embedding",
        Some(MODALITY_RERANK) => "rerank",
        Some(MODALITY_DECISION) => "decision",
        Some(MODALITY_TEXT) => "text",
        _ => "unknown",
    }
}

pub fn ranking_label(value: Option<i64>) -> &'static str {
    match value {
        Some(MODALITY_IMAGE) => "Image",
        Some(MODALITY_AUDIO) => "Audio",
        Some(MODALITY_MUSIC) => "Music",
        Some(MODALITY_VIDEO) => "Video",
        Some(MODALITY_EMBEDDING) => "Embedding",
        Some(MODALITY_RERANK) => "Rerank",
        Some(MODALITY_DECISION) => "Decision",
        Some(MODALITY_TEXT) => "LLM",
        _ => "LLM",
    }
}

pub fn model_type_capability_code(model_type: &str) -> i32 {
    match model_type {
        "Image" => MODALITY_IMAGE as i32,
        "Audio" => MODALITY_AUDIO as i32,
        "Music" => MODALITY_MUSIC as i32,
        "SoundEffect" => MODALITY_MUSIC as i32,
        "Video" => MODALITY_VIDEO as i32,
        "Embedding" => MODALITY_EMBEDDING as i32,
        "Decision" => MODALITY_DECISION as i32,
        _ => MODALITY_TEXT as i32,
    }
}
