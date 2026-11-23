import type { AppModelCatalogPriceScheduleWindow } from './app-model-catalog-price-schedule-window';

/** App model catalog price schedule schema exposed by Cloud Router. */
export interface AppModelCatalogPriceSchedule {
  /** Exclude dates field on app model catalog price schedule. */
  excludeDates: string[];
  /** Holiday dates field on app model catalog price schedule. */
  holidayDates: string[];
  /** Include dates field on app model catalog price schedule. */
  includeDates: string[];
  /** Time zone field on app model catalog price schedule. */
  timeZone: string;
  /** Weekly windows field on app model catalog price schedule. */
  weeklyWindows: AppModelCatalogPriceScheduleWindow[];
}
