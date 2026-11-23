// Generated from docs/schema-registry/sdkwork-cloudrouter.tables.yaml.
// Do not edit by hand; update Schema Registry and regenerate.

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ModelVendor {
    OpenAi,
    Anthropic,
    Google,
    Alibaba,
    Baidu,
    BlackForestLabs,
    ByteDance,
    DeepSeek,
    ElevenLabs,
    Kuaishou,
    MiniMax,
    Moonshot,
    Mureka,
    Pixverse,
    Runway,
    LumaAi,
    Xai,
    StabilityAi,
    Suno,
    Tencent,
    Vidu,
    Zhipu,
    StepFun,
    Xiaomi,
    Meituan,
    Custom,
    Unknown,
}

impl ModelVendor {
    pub fn code(&self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
            Self::Google => "google",
            Self::Alibaba => "alibaba",
            Self::Baidu => "baidu",
            Self::BlackForestLabs => "black_forest_labs",
            Self::ByteDance => "bytedance",
            Self::DeepSeek => "deepseek",
            Self::ElevenLabs => "elevenlabs",
            Self::Kuaishou => "kuaishou",
            Self::MiniMax => "minimax",
            Self::Moonshot => "moonshot",
            Self::Mureka => "mureka",
            Self::Pixverse => "pixverse",
            Self::Runway => "runway",
            Self::LumaAi => "luma_ai",
            Self::Xai => "xai",
            Self::StabilityAi => "stability_ai",
            Self::Suno => "suno",
            Self::Tencent => "tencent",
            Self::Vidu => "vidu",
            Self::Zhipu => "zhipu",
            Self::StepFun => "stepfun",
            Self::Xiaomi => "xiaomi",
            Self::Meituan => "meituan",
            Self::Custom => "custom",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_code(code: &str) -> Self {
        match code {
            "openai" => Self::OpenAi,
            "anthropic" => Self::Anthropic,
            "google" => Self::Google,
            "alibaba" => Self::Alibaba,
            "baidu" => Self::Baidu,
            "black_forest_labs" => Self::BlackForestLabs,
            "bytedance" => Self::ByteDance,
            "deepseek" => Self::DeepSeek,
            "elevenlabs" => Self::ElevenLabs,
            "kuaishou" => Self::Kuaishou,
            "minimax" => Self::MiniMax,
            "moonshot" => Self::Moonshot,
            "mureka" => Self::Mureka,
            "pixverse" => Self::Pixverse,
            "runway" => Self::Runway,
            "luma_ai" => Self::LumaAi,
            "xai" => Self::Xai,
            "stability_ai" => Self::StabilityAi,
            "suno" => Self::Suno,
            "tencent" => Self::Tencent,
            "vidu" => Self::Vidu,
            "zhipu" => Self::Zhipu,
            "stepfun" => Self::StepFun,
            "xiaomi" => Self::Xiaomi,
            "meituan" => Self::Meituan,
            "custom" => Self::Custom,
            "unknown" => Self::Unknown,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IntegrationProviderType {
    Unknown,
    ModelVendorDirect,
    CloudPlatform,
    RelayAggregator,
    SelfHostedGateway,
    LocalRuntime,
    Custom,
}

impl IntegrationProviderType {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::ModelVendorDirect => "model_vendor_direct",
            Self::CloudPlatform => "cloud_platform",
            Self::RelayAggregator => "relay_aggregator",
            Self::SelfHostedGateway => "self_hosted_gateway",
            Self::LocalRuntime => "local_runtime",
            Self::Custom => "custom",
        }
    }

    pub fn from_code(code: &str) -> Self {
        match code {
            "unknown" => Self::Unknown,
            "model_vendor_direct" => Self::ModelVendorDirect,
            "cloud_platform" => Self::CloudPlatform,
            "relay_aggregator" => Self::RelayAggregator,
            "self_hosted_gateway" => Self::SelfHostedGateway,
            "local_runtime" => Self::LocalRuntime,
            "custom" => Self::Custom,
            _ => Self::Unknown,
        }
    }

    pub fn int_code(&self) -> i32 {
        match self {
            Self::Unknown => 0,
            Self::ModelVendorDirect => 1,
            Self::CloudPlatform => 2,
            Self::RelayAggregator => 3,
            Self::SelfHostedGateway => 4,
            Self::LocalRuntime => 5,
            Self::Custom => 6,
        }
    }

    pub fn try_from_int_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Unknown),
            1 => Some(Self::ModelVendorDirect),
            2 => Some(Self::CloudPlatform),
            3 => Some(Self::RelayAggregator),
            4 => Some(Self::SelfHostedGateway),
            5 => Some(Self::LocalRuntime),
            6 => Some(Self::Custom),
            _ => None,
        }
    }

    pub fn from_int_code(code: i32) -> Self {
        Self::try_from_int_code(code).unwrap_or(Self::Unknown)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BillingMeter {
    LlmInputToken,
    LlmOutputToken,
    LlmReasoningToken,
    LlmCacheWriteToken,
    LlmCacheReadToken,
    LlmCacheStorageTokenHour,
    EmbeddingInputToken,
    EmbeddingImage,
    ImageInputToken,
    ImageOutputToken,
    ImageResult,
    ImagePixel,
    ImageMegapixel,
    AudioInputToken,
    AudioOutputToken,
    AudioInputSecond,
    AudioOutputSecond,
    AudioInputMinute,
    AudioOutputMinute,
    TtsInputCharacter,
    SpeechCharacter,
    SttAudioMinute,
    VideoInputToken,
    VideoOutputToken,
    VideoInputSecond,
    VideoOutputSecond,
    VideoResult,
    MusicOutputSecond,
    SfxResult,
    RerankSearch,
    RerankDocument,
    ApiRequest,
    ApiResult,
    ApiItem,
    ToolCall,
    WebSearchCall,
    FileSearchCall,
    CodeInterpreterSession,
    ContainerSession,
    StorageGbDay,
    BandwidthGb,
    /// Document pages processed by file parsing or OCR APIs. Billed per page,
    /// so it is a count meter rather than a duration/storage meter.
    DocumentPage,
    Unknown,
}

impl BillingMeter {
    pub fn code(&self) -> &'static str {
        match self {
            Self::LlmInputToken => "llm_input_token",
            Self::LlmOutputToken => "llm_output_token",
            Self::LlmReasoningToken => "llm_reasoning_token",
            Self::LlmCacheWriteToken => "llm_cache_write_token",
            Self::LlmCacheReadToken => "llm_cache_read_token",
            Self::LlmCacheStorageTokenHour => "llm_cache_storage_token_hour",
            Self::EmbeddingInputToken => "embedding_input_token",
            Self::EmbeddingImage => "embedding_image",
            Self::ImageInputToken => "image_input_token",
            Self::ImageOutputToken => "image_output_token",
            Self::ImageResult => "image_result",
            Self::ImagePixel => "image_pixel",
            Self::ImageMegapixel => "image_megapixel",
            Self::AudioInputToken => "audio_input_token",
            Self::AudioOutputToken => "audio_output_token",
            Self::AudioInputSecond => "audio_input_second",
            Self::AudioOutputSecond => "audio_output_second",
            Self::AudioInputMinute => "audio_input_minute",
            Self::AudioOutputMinute => "audio_output_minute",
            Self::TtsInputCharacter => "tts_input_character",
            Self::SpeechCharacter => "speech_character",
            Self::SttAudioMinute => "stt_audio_minute",
            Self::VideoInputToken => "video_input_token",
            Self::VideoOutputToken => "video_output_token",
            Self::VideoInputSecond => "video_input_second",
            Self::VideoOutputSecond => "video_output_second",
            Self::VideoResult => "video_result",
            Self::MusicOutputSecond => "music_output_second",
            Self::SfxResult => "sfx_result",
            Self::RerankSearch => "rerank_search",
            Self::RerankDocument => "rerank_document",
            Self::ApiRequest => "api_request",
            Self::ApiResult => "api_result",
            Self::ApiItem => "api_item",
            Self::ToolCall => "tool_call",
            Self::WebSearchCall => "web_search_call",
            Self::FileSearchCall => "file_search_call",
            Self::CodeInterpreterSession => "code_interpreter_session",
            Self::ContainerSession => "container_session",
            Self::StorageGbDay => "storage_gb_day",
            Self::BandwidthGb => "bandwidth_gb",
            Self::DocumentPage => "document_page",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_code(code: &str) -> Self {
        match code {
            "llm_input_token" => Self::LlmInputToken,
            "llm_output_token" => Self::LlmOutputToken,
            "llm_reasoning_token" => Self::LlmReasoningToken,
            "llm_cache_write_token" => Self::LlmCacheWriteToken,
            "llm_cache_read_token" => Self::LlmCacheReadToken,
            "llm_cache_storage_token_hour" => Self::LlmCacheStorageTokenHour,
            "embedding_input_token" => Self::EmbeddingInputToken,
            "embedding_image" => Self::EmbeddingImage,
            "image_input_token" => Self::ImageInputToken,
            "image_output_token" => Self::ImageOutputToken,
            "image_result" => Self::ImageResult,
            "image_pixel" => Self::ImagePixel,
            "image_megapixel" => Self::ImageMegapixel,
            "audio_input_token" => Self::AudioInputToken,
            "audio_output_token" => Self::AudioOutputToken,
            "audio_input_second" => Self::AudioInputSecond,
            "audio_output_second" => Self::AudioOutputSecond,
            "audio_input_minute" => Self::AudioInputMinute,
            "audio_output_minute" => Self::AudioOutputMinute,
            "tts_input_character" => Self::TtsInputCharacter,
            "speech_character" => Self::SpeechCharacter,
            "stt_audio_minute" => Self::SttAudioMinute,
            "video_input_token" => Self::VideoInputToken,
            "video_output_token" => Self::VideoOutputToken,
            "video_input_second" => Self::VideoInputSecond,
            "video_output_second" => Self::VideoOutputSecond,
            "video_result" => Self::VideoResult,
            "music_output_second" => Self::MusicOutputSecond,
            "sfx_result" => Self::SfxResult,
            "rerank_search" => Self::RerankSearch,
            "rerank_document" => Self::RerankDocument,
            "api_request" => Self::ApiRequest,
            "api_result" => Self::ApiResult,
            "api_item" => Self::ApiItem,
            "tool_call" => Self::ToolCall,
            "web_search_call" => Self::WebSearchCall,
            "file_search_call" => Self::FileSearchCall,
            "code_interpreter_session" => Self::CodeInterpreterSession,
            "container_session" => Self::ContainerSession,
            "storage_gb_day" => Self::StorageGbDay,
            "bandwidth_gb" => Self::BandwidthGb,
            "document_page" => Self::DocumentPage,
            "unknown" => Self::Unknown,
            _ => Self::Unknown,
        }
    }
}
