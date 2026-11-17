-- sdkwork:seed common
-- Initialization data for the models front-end configuration
-- (`ai_model_frontend_channel`, `ai_model_frontend_channel_model`,
-- `ai_model_frontend_field`, `ai_model_frontend_revision`).
--
-- Why this exists
-- ---------------
-- `/admin/model/frontend` was serving the service fallback document because
-- no database rows existed, so the surface opened with no exposed models and
-- no default model for any modality. The list below is the persisted form of
-- the same initialization document the backend serves for a tenant with no
-- rows yet (`default_frontend_channels` in
-- `crates/sdkwork-models-catalog-service/src/api/frontend_config.rs`). Keeping
-- it in `seeds/` follows DATABASE_FRAMEWORK_SPEC section 4.3: initialization data
-- belongs to the seed lifecycle, not to application code.
--
-- Scope: tenant 100001 / organization 0, the tenant that owns the admin
-- console (the same scope the storage-center defaults seed uses).
--
-- Idempotency: every insert is `ON CONFLICT DO NOTHING`, and the child rows
-- are gated on two independent conditions:
--   1. no live model/field exists for the channel, so a surface an operator
--      already authored is never augmented; and
--   2. none of the deterministic seed UUIDs exist, so a surface the operator
--      later emptied is never re-populated and the seed never resurrects a
--      row it already wrote (soft-deleted rows keep their UUID).
-- The model and field rows attach to whichever live channel carries the same
-- `channel_code`; a channel row an operator already created keeps its own
-- id, revision, and presentation edits.
-- Deterministic ids: fixed small ids, far below the Snowflake range produced by
-- `next_cloud_runtime_id`, so runtime inserts can never collide with seed rows.

INSERT INTO ai_model_frontend_channel
    (id, uuid, tenant_id, organization_id, data_scope, status, version,
     channel_code, display_name, display_name_i18n, description, enabled, sort_order,
     default_catalog_key, capability_codes, config, schema_version, revision,
     last_published_at)
VALUES
    (9101, 'seed-fmc-channel-llm', 100001, 0, 1, 1, 0,
     'llm', 'LLM',
     '{"en-US":"LLM","zh-CN":"大语言模型"}'::jsonb, NULL, TRUE, 10,
     'deepseek/deepseek-flash', '["chat","responses","completion"]'::jsonb,
     '{"pageSize":100}'::jsonb, '1.0.0', 1, CURRENT_TIMESTAMP),
    (9102, 'seed-fmc-channel-video', 100001, 0, 1, 1, 0,
     'video', 'Video',
     '{"en-US":"Video","zh-CN":"视频生成"}'::jsonb, NULL, TRUE, 20,
     'luma_ai/ray-3.2', '["video_generation"]'::jsonb,
     '{"pageSize":100}'::jsonb, '1.0.0', 1, CURRENT_TIMESTAMP),
    (9103, 'seed-fmc-channel-image', 100001, 0, 1, 1, 0,
     'image', 'Image',
     '{"en-US":"Image","zh-CN":"图片生成"}'::jsonb, NULL, TRUE, 30,
     'google/gemini-3-pro-image', '["image_generation","image_editing"]'::jsonb,
     '{"pageSize":100}'::jsonb, '1.0.0', 1, CURRENT_TIMESTAMP),
    (9104, 'seed-fmc-channel-audio', 100001, 0, 1, 1, 0,
     'audio', 'Audio',
     '{"en-US":"Audio","zh-CN":"音频"}'::jsonb, NULL, TRUE, 40,
     'openai/gpt-realtime-2.1', '["audio_input","audio_output","speech_to_text","text_to_speech"]'::jsonb,
     '{"pageSize":100}'::jsonb, '1.0.0', 1, CURRENT_TIMESTAMP),
    (9105, 'seed-fmc-channel-music', 100001, 0, 1, 1, 0,
     'music', 'Music',
     '{"en-US":"Music","zh-CN":"音乐生成"}'::jsonb, NULL, TRUE, 50,
     'mureka/mureka-v9.5', '["music_generation"]'::jsonb,
     '{"pageSize":100}'::jsonb, '1.0.0', 1, CURRENT_TIMESTAMP),
    (9106, 'seed-fmc-channel-sound_effect', 100001, 0, 1, 1, 0,
     'sound_effect', 'Sound Effect',
     '{"en-US":"Sound Effect","zh-CN":"音效生成"}'::jsonb, NULL, TRUE, 60,
     'vidu/audio1.0-timing2audio', '["sound_effect_generation"]'::jsonb,
     '{"pageSize":100}'::jsonb, '1.0.0', 1, CURRENT_TIMESTAMP),
    (9107, 'seed-fmc-channel-embedding', 100001, 0, 1, 1, 0,
     'embedding', 'Embedding',
     '{"en-US":"Embedding","zh-CN":"向量嵌入"}'::jsonb, NULL, TRUE, 70,
     'openai/text-embedding-3-large', '["embedding","rerank"]'::jsonb,
     '{"pageSize":100}'::jsonb, '1.0.0', 1, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;

INSERT INTO ai_model_frontend_channel_model
    (id, uuid, tenant_id, organization_id, data_scope, status, version,
     channel_id, channel_code, catalog_key, visible, featured, sort_order, config)
SELECT
    seed.id, seed.uuid, 100001, 0, 1, 1, 0,
    frontend_channel.id, seed.channel_code, seed.catalog_key, TRUE, seed.featured,
    seed.sort_order, '{}'::jsonb
FROM (VALUES
    (9201, 'seed-fmc-model-llm-1', 'llm', 'deepseek/deepseek-flash', TRUE, 10),
    (9202, 'seed-fmc-model-llm-2', 'llm', 'deepseek/deepseek-v4-pro', FALSE, 20),
    (9203, 'seed-fmc-model-llm-3', 'llm', 'anthropic/claude-opus-5', FALSE, 30),
    (9204, 'seed-fmc-model-video-1', 'video', 'luma_ai/ray-3.2', TRUE, 10),
    (9205, 'seed-fmc-model-video-2', 'video', 'minimax/MiniMax-H3', FALSE, 20),
    (9206, 'seed-fmc-model-video-3', 'video', 'vidu/viduq3-pro', FALSE, 30),
    (9207, 'seed-fmc-model-image-1', 'image', 'google/gemini-3-pro-image', TRUE, 10),
    (9208, 'seed-fmc-model-image-2', 'image', 'openai/gpt-image-2', FALSE, 20),
    (9209, 'seed-fmc-model-image-3', 'image', 'kuaishou/kling-image-3.0', FALSE, 30),
    (9210, 'seed-fmc-model-audio-1', 'audio', 'openai/gpt-realtime-2.1', TRUE, 10),
    (9211, 'seed-fmc-model-audio-2', 'audio', 'google/gemini-2.5-pro-preview-tts', FALSE, 20),
    (9212, 'seed-fmc-model-audio-3', 'audio', 'openai/tts-1-hd', FALSE, 30),
    (9213, 'seed-fmc-model-music-1', 'music', 'mureka/mureka-v9.5', TRUE, 10),
    (9214, 'seed-fmc-model-music-2', 'music', 'mureka/mureka-o2', FALSE, 20),
    (9215, 'seed-fmc-model-music-3', 'music', 'elevenlabs/music_v2_5', FALSE, 30),
    (9216, 'seed-fmc-model-sound_effect-1', 'sound_effect', 'vidu/audio1.0-timing2audio', TRUE, 10),
    (9217, 'seed-fmc-model-sound_effect-2', 'sound_effect', 'kuaishou/kling-sound-t2a', FALSE, 20),
    (9218, 'seed-fmc-model-sound_effect-3', 'sound_effect', 'vidu/audio1.0-text2audio', FALSE, 30),
    (9219, 'seed-fmc-model-embedding-1', 'embedding', 'openai/text-embedding-3-large', TRUE, 10),
    (9220, 'seed-fmc-model-embedding-2', 'embedding', 'alibaba/text-embedding-v4', FALSE, 20),
    (9221, 'seed-fmc-model-embedding-3', 'embedding', 'zhipu/embedding-3', FALSE, 30)
) AS seed(id, uuid, channel_code, catalog_key, featured, sort_order)
JOIN ai_model_frontend_channel frontend_channel
    ON frontend_channel.channel_code = seed.channel_code
   AND frontend_channel.tenant_id = 100001
   AND frontend_channel.organization_id = 0
   AND frontend_channel.status = 1
   AND frontend_channel.deleted_at IS NULL
WHERE NOT EXISTS (
    SELECT 1
    FROM ai_model_frontend_channel_model existing_model
    WHERE existing_model.channel_id = frontend_channel.id
      AND existing_model.tenant_id = 100001
      AND existing_model.organization_id = 0
      AND existing_model.status = 1
      AND existing_model.deleted_at IS NULL
)
AND NOT EXISTS (
    SELECT 1
    FROM ai_model_frontend_channel_model prior_seed_model
    WHERE prior_seed_model.uuid = seed.uuid
)
ON CONFLICT DO NOTHING;

INSERT INTO ai_model_frontend_field
    (id, uuid, tenant_id, organization_id, data_scope, status, version,
     channel_id, channel_code, field_key, visible, sort_order, config)
SELECT
    seed.id, seed.uuid, 100001, 0, 1, 1, 0,
    frontend_channel.id, seed.channel_code, seed.field_key, TRUE, seed.sort_order,
    '{}'::jsonb
FROM (VALUES
    (9301, 'seed-fmc-field-llm-price', 'llm', 'price', 10),
    (9302, 'seed-fmc-field-llm-context', 'llm', 'context', 20),
    (9303, 'seed-fmc-field-llm-modalities', 'llm', 'modalities', 30),
    (9304, 'seed-fmc-field-llm-capabilities', 'llm', 'capabilities', 40),
    (9305, 'seed-fmc-field-video-price', 'video', 'price', 10),
    (9306, 'seed-fmc-field-video-context', 'video', 'context', 20),
    (9307, 'seed-fmc-field-video-modalities', 'video', 'modalities', 30),
    (9308, 'seed-fmc-field-video-capabilities', 'video', 'capabilities', 40),
    (9309, 'seed-fmc-field-image-price', 'image', 'price', 10),
    (9310, 'seed-fmc-field-image-context', 'image', 'context', 20),
    (9311, 'seed-fmc-field-image-modalities', 'image', 'modalities', 30),
    (9312, 'seed-fmc-field-image-capabilities', 'image', 'capabilities', 40),
    (9313, 'seed-fmc-field-audio-price', 'audio', 'price', 10),
    (9314, 'seed-fmc-field-audio-context', 'audio', 'context', 20),
    (9315, 'seed-fmc-field-audio-modalities', 'audio', 'modalities', 30),
    (9316, 'seed-fmc-field-audio-capabilities', 'audio', 'capabilities', 40),
    (9317, 'seed-fmc-field-music-price', 'music', 'price', 10),
    (9318, 'seed-fmc-field-music-context', 'music', 'context', 20),
    (9319, 'seed-fmc-field-music-modalities', 'music', 'modalities', 30),
    (9320, 'seed-fmc-field-music-capabilities', 'music', 'capabilities', 40),
    (9321, 'seed-fmc-field-sound_effect-price', 'sound_effect', 'price', 10),
    (9322, 'seed-fmc-field-sound_effect-context', 'sound_effect', 'context', 20),
    (9323, 'seed-fmc-field-sound_effect-modalities', 'sound_effect', 'modalities', 30),
    (9324, 'seed-fmc-field-sound_effect-capabilities', 'sound_effect', 'capabilities', 40),
    (9325, 'seed-fmc-field-embedding-price', 'embedding', 'price', 10),
    (9326, 'seed-fmc-field-embedding-context', 'embedding', 'context', 20),
    (9327, 'seed-fmc-field-embedding-modalities', 'embedding', 'modalities', 30),
    (9328, 'seed-fmc-field-embedding-capabilities', 'embedding', 'capabilities', 40)
) AS seed(id, uuid, channel_code, field_key, sort_order)
JOIN ai_model_frontend_channel frontend_channel
    ON frontend_channel.channel_code = seed.channel_code
   AND frontend_channel.tenant_id = 100001
   AND frontend_channel.organization_id = 0
   AND frontend_channel.status = 1
   AND frontend_channel.deleted_at IS NULL
WHERE NOT EXISTS (
    SELECT 1
    FROM ai_model_frontend_field existing_field
    WHERE existing_field.channel_id = frontend_channel.id
      AND existing_field.tenant_id = 100001
      AND existing_field.organization_id = 0
      AND existing_field.status = 1
      AND existing_field.deleted_at IS NULL
)
AND NOT EXISTS (
    SELECT 1
    FROM ai_model_frontend_field prior_seed_field
    WHERE prior_seed_field.uuid = seed.uuid
)
ON CONFLICT DO NOTHING;

-- The revision row records the digest of the seed document above using the
-- store's sorted-key JSON encoding, so the audit column describes these rows
-- instead of an arbitrary constant. It is only written while the tenant has no
-- published revision yet; an operator-authored history is never mixed with the
-- seed revision.
INSERT INTO ai_model_frontend_revision
    (id, uuid, tenant_id, organization_id, data_scope, status, version,
     revision, content_sha256, channel_count, published_at, source)
SELECT
    9401, 'seed-fmc-revision-1', 100001, 0, 1, 1, 0, 1, 'a47b5ad44b85f6f2850fbe6caa3be3cf542effc9620890fdb188ae83bb77ef1b', 7, CURRENT_TIMESTAMP, 'seed'
WHERE NOT EXISTS (
    SELECT 1
    FROM ai_model_frontend_revision
    WHERE tenant_id = 100001
      AND organization_id = 0
)
ON CONFLICT DO NOTHING;
