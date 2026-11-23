/** App model catalog price schedule window schema exposed by Cloud Router. */
export interface AppModelCatalogPriceScheduleWindow {
  /** Days of week field on app model catalog price schedule window. */
  daysOfWeek: number[];
  /** End day offset field on app model catalog price schedule window. */
  endDayOffset: 0 | 1;
  /** End time field on app model catalog price schedule window. */
  endTime: string;
  /** Holiday only field on app model catalog price schedule window. */
  holidayOnly: boolean;
  /** Start time field on app model catalog price schedule window. */
  startTime: string;
  /** Window code field on app model catalog price schedule window. */
  windowCode: string;
}
