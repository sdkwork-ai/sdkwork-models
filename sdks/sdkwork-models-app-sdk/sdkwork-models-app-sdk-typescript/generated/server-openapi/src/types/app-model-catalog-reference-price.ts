import type { AppModelCatalogPriceSchedule } from './app-model-catalog-price-schedule';
import type { JsonNull } from './json-null';

/** App model catalog reference price schema exposed by Cloud Router. */
export interface AppModelCatalogReferencePrice {
  /** Billing meter field on app model catalog reference price. */
  billingMeter: string;
  /** Currency field on app model catalog reference price. */
  currency: string;
  /** Rate hash field on app model catalog reference price. */
  rateHash: string;
  /** Rate variant field on app model catalog reference price. */
  rateVariant: 'standard' | 'time_window' | 'batch';
  /** Region code field on app model catalog reference price. */
  regionCode: string;
  /** Schedule field on app model catalog reference price. */
  schedule: AppModelCatalogPriceSchedule | JsonNull;
  /** Unit price field on app model catalog reference price. */
  unitPrice: string;
}
