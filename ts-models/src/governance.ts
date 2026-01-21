import type { Address, UnixSeconds } from './common.js';

export interface ProposalSummary {
  readonly proposal_id: number;
  readonly proposer: Address;
  readonly description: string;
  readonly vote_start: UnixSeconds;
  readonly vote_end: UnixSeconds;
  readonly votes_for: number;
  readonly votes_against: number;
  readonly votes_abstain: number;
  readonly state: string;
  readonly created_at: UnixSeconds;
}

export interface VoteView {
  readonly proposal_id: number;
  readonly voter: Address;
  /** 0=Against, 1=For, 2=Abstain */
  readonly support: number;
  readonly weight: number;
  readonly reason: string | null;
  readonly voted_at: UnixSeconds;
}

export interface ProposalView {
  readonly proposal_id: number;
  readonly proposer: Address;
  readonly targets: readonly string[];
  readonly values: readonly string[];
  readonly calldatas: readonly string[];
  readonly description: string;
  readonly vote_start: UnixSeconds;
  readonly vote_end: UnixSeconds;
  readonly votes_for: number;
  readonly votes_against: number;
  readonly votes_abstain: number;
  readonly state: string;
  readonly executed_at: UnixSeconds | null;
  readonly created_at: UnixSeconds;
  readonly updated_at: UnixSeconds;
  readonly has_voted: boolean | null;
  readonly user_vote: VoteView | null;
}

export interface VoteHistoryEntry {
  readonly proposal_id: number;
  readonly support: number;
  readonly weight: number;
  readonly reason: string | null;
  readonly voted_at: UnixSeconds;
}

export interface DelegationView {
  readonly delegator: Address;
  readonly delegatee: Address;
  readonly amount: number;
  readonly delegated_at: UnixSeconds;
}

export interface VotingPowerView {
  readonly address: Address;
  readonly voting_power: number;
  readonly delegated_power: number;
  readonly total_power: number;
}

export interface GovernanceStatsView {
  readonly address: Address;
  readonly proposals_submitted: number;
  readonly votes_cast: number;
  readonly participation_rate: number;
  readonly last_vote_at: UnixSeconds | null;
  readonly delegated_in: number;
  readonly delegated_out: number;
  readonly net_voting_power: number;
}

export interface ProposalCreateRequest {
  readonly proposer: Address;
  readonly title: string;
  readonly description: string;
  readonly justification: string | null;
  readonly targets: readonly string[];
  readonly values: readonly string[];
  readonly calldatas: readonly string[];
  readonly vote_duration_seconds: number | null;
}

export interface VoteSubmissionRequest {
  /** Can be numeric string identifier */
  readonly proposal_id: string;
  readonly voter: Address;
  /** 0=Against, 1=For, 2=Abstain */
  readonly support: number | null;
  /** Alternative: "yes", "no", "abstain", etc. */
  readonly option: string | null;
  readonly reason: string | null;
}

export interface VoteSubmissionResponse {
  readonly proposal_id: number;
  readonly status: string;
  readonly votes_for: number;
  readonly votes_against: number;
  readonly voter: Address;
  readonly vote_weight: number;
  readonly approve: boolean;
  readonly finalized: boolean;
}

export interface DelegateRequest {
  readonly delegator: Address;
  readonly validator: Address;
  readonly amount: number;
}

export interface DelegateResponse {
  readonly delegator: Address;
  readonly validator: Address;
  readonly amount: number;
  readonly delegation: DelegationView;
}
