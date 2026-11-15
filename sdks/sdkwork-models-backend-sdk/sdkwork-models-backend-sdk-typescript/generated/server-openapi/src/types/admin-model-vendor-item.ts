import type { JsonValue } from './json-value';

/** Admin model vendor item schema exposed by Cloud Router. */
export interface AdminModelVendorItem {
  /** Client api compatibility field on admin model vendor item. */
  clientApiCompatibility: Record<string, JsonValue>;
  /** Color field on admin model vendor item. */
  color: string;
  /** Description field on admin model vendor item. */
  description: string;
  /** Id field on admin model vendor item. */
  id: string;
  /** Model count field on admin model vendor item. */
  modelCount: number;
  /** Name field on admin model vendor item. */
  name: string;
  /** Status field on admin model vendor item. */
  status: 'active' | 'inactive';
  /** Supported protocols field on admin model vendor item. */
  supportedProtocols: string[];
  /** Vendor code field on admin model vendor item. */
  vendorCode: string;
}
