import type { AppModelCatalogPriceAvailability } from './app-model-catalog-price-availability';
import type { AppModelCatalogReferencePrice } from './app-model-catalog-reference-price';

/** App model catalog item schema exposed by Cloud Router. */
export interface AppModelCatalogItem {
  /** Api format field on app model catalog item. */
  apiFormat: string | null;
  /** Capabilities field on app model catalog item. */
  capabilities: string[];
  /** Capability intro field on app model catalog item. */
  capabilityIntro: string | null;
  /** Catalog key field on app model catalog item. */
  catalogKey: string;
  /** Categories field on app model catalog item. */
  categories: string[];
  /** Color token field on app model catalog item. */
  colorToken?: string | null;
  /** Context tokens field on app model catalog item. */
  contextTokens: number | null;
  /** Description field on app model catalog item. */
  description: string | null;
  /** Display name field on app model catalog item. */
  displayName: string;
  /** Docs url field on app model catalog item. */
  docsUrl?: string | null;
  /** Family code field on app model catalog item. */
  familyCode?: string | null;
  /** Groups field on app model catalog item. */
  groups: string[];
  /** Input modalities field on app model catalog item. */
  inputModalities: string[];
  /** Limitations field on app model catalog item. */
  limitations: string[];
  /** Max output tokens field on app model catalog item. */
  maxOutputTokens: number | null;
  /** Modalities field on app model catalog item. */
  modalities: string[];
  /** Model field on app model catalog item. */
  model: string;
  /** Official reference prices field on app model catalog item. */
  officialReferencePrices: AppModelCatalogReferencePrice[];
  /** Output modalities field on app model catalog item. */
  outputModalities: string[];
  /** Price availability field on app model catalog item. */
  priceAvailability: AppModelCatalogPriceAvailability;
  /** Provider codes field on app model catalog item. */
  providerCodes: string[];
  /** Release stage field on app model catalog item. */
  releaseStage: number | null;
  /** Replacement model field on app model catalog item. */
  replacementModel: string | null;
  /** Routing state field on app model catalog item. */
  routingState: number | null;
  /** Shelf state field on app model catalog item. */
  shelfState: number | null;
  /** Supported languages field on app model catalog item. */
  supportedLanguages: string[];
  /** Supports json schema field on app model catalog item. */
  supportsJsonSchema: boolean;
  /** Supports streaming field on app model catalog item. */
  supportsStreaming: boolean;
  /** Supports tools field on app model catalog item. */
  supportsTools: boolean;
  /** Training data cutoff field on app model catalog item. */
  trainingDataCutoff: string | null;
  /** Use cases field on app model catalog item. */
  useCases: string[];
  /** Vendor field on app model catalog item. */
  vendor: string;
  /** Vendor code field on app model catalog item. */
  vendorCode: string;
  /** Video profile count field on app model catalog item. */
  videoProfileCount?: number;
  /** Voice binding count field on app model catalog item. */
  voiceBindingCount?: number;
  /** Agent Config SPI provider ids that can consume this model. */
  supportedAgentProviderIds: string[];
  /** Product usage scopes where the model can be applied, e.g. coding (code IDE/agent surfaces). */
  usageScopes: ('coding' | 'chat' | 'agent')[];
  /** Whether the model is shown in code IDE surfaces. Defaults to true. */
  codingVisible: boolean;
}
