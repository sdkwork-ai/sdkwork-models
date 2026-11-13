export type ModelCapability =
  | "chat"
  | "embedding"
  | "image"
  | "audio"
  | "music"
  | "video"
  | "rerank"
  | "tool"
  | "decision";

export type ModelModality =
  | "text"
  | "image"
  | "audio"
  | "music"
  | "video"
  | "embedding"
  | "rerank"
  | "tool";

export type ModelUsageScope = "coding" | "chat" | "agent";

export interface SourceEvidence {
  sourceUrl: string;
  observedAt: string;
  sourceHash?: string;
}

export interface BillingMeter {
  meterCode: string;
  displayName: string;
  modality: string;
  defaultUnitSize: string;
}

export interface ProtocolStandard {
  protocolCode: string;
  vendorOrigin: string;
  displayName: string;
  family: string;
  docsUrl: string;
  maturity: string;
}

export type ClientApiSupportStatus = "supported" | "unsupported" | "partial";

export interface ClientApiCompatibility {
  clientApiCode: string;
  displayName: string;
  supportStatus: ClientApiSupportStatus;
  protocolCodes: string[];
  apiCodes: string[];
  resourceCodes: string[];
  notes: string;
  source: SourceEvidence;
}

export type ClientApiCompatibilityMap = Record<string, ClientApiCompatibility>;

export interface ModelVendor {
  vendorCode: string;
  regionCode: string;
  displayName: string;
  legalName?: string;
  vendorType: string;
  marketScope: string;
  billingCurrency: string;
  billingJurisdiction: string;
  operatingRegions: string[];
  capabilities: ModelCapability[];
  supportedProtocols: string[];
  clientApiCompatibility: ClientApiCompatibilityMap;
  openSource: boolean;
}

export interface ModelInfo {
  catalogKey: string;
  modelId: string;
  displayName: string;
  vendorCode: string;
  regionCode: string;
  familyCode: string;
  primaryCapability: ModelCapability;
  capabilities: ModelCapability[];
  inputModalities: ModelModality[];
  outputModalities: ModelModality[];
  apiFormat: string;
  lifecycle: string;
  releaseStage: string;
  shelfState: string;
  routingState: string;
  supportsStreaming?: boolean;
  supportsTools?: boolean;
  supportsJsonSchema?: boolean;
  usageScopes?: ModelUsageScope[];
  codingVisible?: boolean;
  source: SourceEvidence;
}

export interface ModelPrice {
  priceId: string;
  priceSide: string;
  pricingScope: string;
  meterCode: string;
  unitSize: string;
  unitPrice: string;
  minimumQuantity: string;
  currency: string;
  effectiveFrom: string;
  source: SourceEvidence;
}

export interface ModelPricing {
  catalogKey: string;
  vendorCode: string;
  regionCode: string;
  modelId: string;
  currency: string;
  prices: ModelPrice[];
}

export interface TtsVoice {
  schemaVersion: string;
  voiceId: string;
  displayName: string;
  vendorCode: string;
  regionCode: string;
  catalogKey: string;
  primaryLocale: string;
  supportedLocales?: string[];
  gender: string;
  voiceKind: string;
  provisioningMode: "static" | "vendor_api";
  wireParameter: string;
  vendorVoiceNamespace?: string;
  styles?: string[];
  roles?: string[];
  previewAudioUrl?: string;
  vendorListEndpoint?: string;
  description?: string;
  lifecycle: string;
  releaseStage: string;
  shelfState: string;
  routingState: string;
  source: SourceEvidence;
}

export interface ModelVoiceBinding {
  voiceKey: string;
  voiceId: string;
  isDefault?: boolean;
  compatibility: "full" | "preview" | "legacy";
  sortOrder?: number;
  notes?: string;
}

export interface ModelVoiceBindingsFile {
  schemaVersion: string;
  vendorCode: string;
  regionCode: string;
  modelId: string;
  catalogKey: string;
  bindings: ModelVoiceBinding[];
  source: SourceEvidence;
}

export type VideoGenerationMode =
  | "text_to_video"
  | "image_to_video"
  | "reference_to_video"
  | "start_end_frame"
  | "video_extension"
  | "video_edit"
  | "multi_shot";

export type VideoDurationPolicy = "fixed" | "discrete" | "range" | "continuous";

export interface VideoGenerationProfile {
  profileCode: string;
  catalogKey: string;
  displayName: string;
  generationMode: VideoGenerationMode;
  durationPolicy: VideoDurationPolicy;
  durationSeconds?: number;
  durationOptions?: number[];
  durationTierCode?: string;
  durationTierCodes?: string[];
  minDurationSeconds?: number;
  maxDurationSeconds?: number;
  durationStepSeconds?: number;
  resolution: string;
  resolutionTierCode?: string;
  aspectRatios?: string[];
  outputAudio?: boolean;
  isDefault?: boolean;
  sortOrder?: number;
  pricingTierCodes?: string[];
  wireParameters: Record<string, string>;
}

export interface ModelVideoProfilesFile {
  schemaVersion: string;
  vendorCode: string;
  regionCode: string;
  modelId: string;
  catalogKey: string;
  profiles: VideoGenerationProfile[];
  source: SourceEvidence;
}

export interface VendorCatalog {
  vendorCode: string;
  regionCode: string;
  vendor: ModelVendor;
  models: ModelInfo[];
  pricing: ModelPricing[];
  voices?: TtsVoice[];
  modelVoiceBindings?: ModelVoiceBindingsFile[];
  modelVideoProfiles?: ModelVideoProfilesFile[];
}

export interface VendorRegionRef {
  vendorCode: string;
  regionCode: string;
}

export interface ModelCatalog {
  catalogVersion: string;
  schemaVersion: string;
  meters: BillingMeter[];
  protocols: ProtocolStandard[];
  vendors: VendorCatalog[];
}

export interface ModelVendorIdentity {
  vendorCode: string;
  displayName: string;
  legalName?: string;
  vendorType: string;
  capabilities: ModelCapability[];
  supportedProtocols: string[];
  clientApiCompatibility: ClientApiCompatibilityMap;
  openSource: boolean;
}
