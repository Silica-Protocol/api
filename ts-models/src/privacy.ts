import type { HexString } from './common.js';

export interface StealthAddressRequestPayload {
  readonly seed_hex: HexString | null;
  readonly include_secrets: boolean;
}

export interface StealthAddressResponsePayload {
  readonly address: string;
  readonly view_key: string;
  readonly spend_public_key: string;
  readonly view_secret: string | null;
  readonly spend_secret: string | null;
}

export interface StealthKeyComponentPayload {
  readonly public: string;
  readonly secret: string;
}

export interface StealthKeyBundlePayload {
  readonly view_keypair: StealthKeyComponentPayload;
  readonly spend_keypair: StealthKeyComponentPayload;
}

export interface StealthScanRequestPayload {
  readonly stealth_keys: StealthKeyBundlePayload;
  readonly from_block: number | null;
  readonly to_block: number | null;
  readonly limit: number | null;
}

export interface StealthScanRangeSummary {
  readonly from_block: number;
  readonly to_block: number;
  readonly span: number;
}

export interface StealthAddressObservation {
  readonly public_key: string;
  readonly tx_public_key: string;
}

export interface OwnedStealthTransactionView {
  readonly transaction_id: string;
  readonly sender: string;
  readonly fee: number;
  readonly amount: number;
  /** The API currently returns a JSON value (historical compatibility). */
  readonly timestamp: unknown;
  readonly stealth_address: StealthAddressObservation;
  readonly memo: unknown | null;
}

export interface StealthScanResponsePayload {
  readonly range: StealthScanRangeSummary;
  readonly latest_block: number;
  readonly total_scanned: number;
  readonly total_balance: number;
  readonly transactions_returned: number;
  readonly has_more: boolean;
  readonly transactions: readonly OwnedStealthTransactionView[];
}

export type StealthPrivacyLevel = 'stealth' | 'encrypted';

export interface StealthTransferRequestPayload {
  readonly sender_keys: StealthKeyBundlePayload;
  readonly recipient_view_key: string;
  readonly recipient_spend_key: string;
  readonly amount: number;
  readonly fee: number;
  readonly nonce: number;
  readonly memo: string | null;
  readonly privacy_level: StealthPrivacyLevel;
}

export interface StealthTransferResponsePayload {
  readonly tx_hash: string;
  readonly status: string;
}
