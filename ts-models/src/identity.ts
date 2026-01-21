import type { UnixSeconds } from './common.js';

export interface IdentityProfileView {
  readonly identity_id: string;
  readonly display_name: string | null;
  readonly avatar_hash: string | null;
  readonly bio: string | null;
  readonly stats_visibility: string;
  readonly wallet_count: number;
  readonly created_at: UnixSeconds;
  readonly updated_at: UnixSeconds;
  readonly last_synced_block: number;
  readonly profile_version: number;
}

export interface WalletLinkView {
  readonly wallet_address: string;
  readonly link_type: string;
  readonly proof_signature: string;
  readonly created_at: UnixSeconds;
  readonly verified_at: UnixSeconds | null;
  readonly last_synced_block: number;
}

export interface IdentitySearchResult {
  readonly identity_id: string;
  readonly display_name: string | null;
  readonly stats_visibility: string;
  readonly updated_at: UnixSeconds;
}

export interface IdentitySearchResponse {
  readonly query: string;
  readonly limit: number;
  readonly results: readonly IdentitySearchResult[];
}

export interface WalletVerificationRequest {
  readonly wallet_address: string;
  /** Optional proof signature (hex). */
  readonly signature: string | null;
}

export interface WalletVerificationResponse {
  readonly identity_id: string;
  readonly wallet_address: string;
  readonly linked: boolean;
  readonly verified: boolean;
  readonly proof_signature: string | null;
  readonly verified_at: UnixSeconds | null;
  readonly last_synced_block: number | null;
  readonly reason: string | null;
}
