import type { StatusTone } from "@/components/ui/status-badge"

/** Directory scope (URL: scope=mine|collaborating|team). */
export type CustomerScope = "mine" | "collaborating" | "team"

export type CustomerStatus = "active" | "disabled"

export type CustomerSectionId =
  | "overview"
  | "contacts"
  | "related"
  | "settlement"
  | "quality"
  | "audit"

export type FieldVisibility = "full" | "masked" | "hidden"

export type CustomerAssignmentView = Readonly<{
  id: string
  role: "OWNER" | "COLLABORATOR"
  userId: string
  userName: string
  effectiveFrom: string
  effectiveTo?: string
  isCurrent: boolean
}>

export type CustomerContactView = Readonly<{
  id: string
  name: string
  title?: string
  purpose?: string
  phoneMasked: string
  phoneRevealToken?: string
  email?: string
  isDefault: boolean
  effectiveFrom: string
  effectiveTo?: string
  fieldVisibility: {
    phone: FieldVisibility
  }
}>

export type CustomerAddressView = Readonly<{
  id: string
  addressType: string
  addressMasked: string
  addressRevealToken?: string
  contactName?: string
  isDefault: boolean
  effectiveFrom: string
  effectiveTo?: string
  fieldVisibility: {
    address: FieldVisibility
  }
}>

export type CustomerBankAccountView = Readonly<{
  id: string
  internalNo: string
  accountName: string
  bankName: string
  accountMasked: string
  accountRevealToken?: string
  effectiveFrom: string
  effectiveTo?: string
  fieldVisibility: {
    accountNumber: FieldVisibility
  }
}>

export type RelatedObjectSummary = Readonly<{
  id: string
  number: string
  title: string
  status: { label: string; tone: StatusTone }
  href: string
  detail?: string
}>

export type CustomerRelationshipMetrics = Readonly<{
  /** Server-aggregated; do not sum from related lists. */
  activeContractCount: number
  inProgressSalesOrderCount: number
  receivableBalance: string
  overdueAmount: string
  expiringContractCount?: number
}>

export type ReceivableSummary = Readonly<{
  receivableBalance: string
  overdueAmount: string
  earliestOverdueDate?: string
  collectionProgressLabel?: string
  invoicingProgressLabel?: string
  reliabilityNote?: string
}>

export type CustomerQualitySummary = Readonly<{
  scaleLabel: string
  profitContributionLabel: string
  collectionRiskLabel: string
  lastBusinessAt?: string
  projectionAt: string
  isStale?: boolean
}>

export type CustomerDirectoryItem = Readonly<{
  id: string
  partyId: string
  customerNo: string
  legalName: string
  shortName?: string
  status: CustomerStatus
  statusLabel: { label: string; tone: StatusTone }
  ownerName: string
  collaboratorCount: number
  scopeTags: readonly CustomerScope[]
  metrics: CustomerRelationshipMetrics
  attentionTags?: readonly string[]
  updatedAt: string
  recentBusinessAt?: string
}>

export type CustomerCenterView = Readonly<{
  customerId: string
  partyId: string
  customerNo: string
  status: CustomerStatus
  statusLabel: { label: string; tone: StatusTone }
  lockVersion: number
  currentRevision: {
    revisionId: string
    revisionNo: number
    legalName: string
    shortName?: string
    unifiedCreditCode?: string
    defaultPaymentTerm?: string
    effectiveFrom: string
  }
  assignments: readonly CustomerAssignmentView[]
  contacts: readonly CustomerContactView[]
  addresses: readonly CustomerAddressView[]
  bankAccounts: readonly CustomerBankAccountView[]
  settlementNote?: string
  metrics: CustomerRelationshipMetrics
  contracts: readonly RelatedObjectSummary[]
  salesOrders: readonly RelatedObjectSummary[]
  receivableSummary?: ReceivableSummary
  qualitySummary?: CustomerQualitySummary
  freshness: { formalFactsAt: string; qualityProjectionAt?: string }
  allowedActions: readonly string[]
  actionBlockers: readonly { action: string; code: string; message: string }[]
  revisionTimeline: readonly {
    id: string
    revisionNo: number
    actor: string
    effectiveAt: string
    reason: string
    isCurrent: boolean
  }[]
  /** Partition load flags — failed partitions must not clear identity. */
  partitions: {
    identity: "ok" | "error"
    contacts: "ok" | "error"
    related: "ok" | "error"
    settlement: "ok" | "error"
    quality: "ok" | "error"
    audit: "ok" | "error"
  }
}>

export type CustomerDirectoryQuery = Readonly<{
  scope: CustomerScope
  status: "active" | "disabled" | "all"
  query?: string
  sort?: "recent_business" | "name" | "overdue_desc"
}>

export type CustomerDirectoryResult = Readonly<{
  /** False when role has no customer data scope at all. */
  hasCustomerScope: boolean
  items: readonly CustomerDirectoryItem[]
  totalInScope: number
  queriedAt: string
}>

export type SaveCustomerRevisionInput = Readonly<{
  customerId: string
  expectedLockVersion: number
  baseRevisionId: string
  legalName: string
  shortName?: string
  unifiedCreditCode?: string
  changeReason: string
  idempotencyKey: string
  /** Demo: force conflict / unknown outcome. */
  simulate?: "ok" | "conflict" | "unknown"
}>

export type CreateCustomerInput = Readonly<{
  legalName: string
  shortName?: string
  unifiedCreditCode?: string
  ownerUserId: string
  ownerName: string
  defaultPaymentTerm?: string
  idempotencyKey: string
  simulate?: "ok" | "conflict" | "unknown"
}>

export type CustomerMutationResult =
  | {
      outcome: "succeeded"
      customerId: string
      customerNo: string
      revisionNo: number
      lockVersion: number
      occurredAt: string
      reference: string
    }
  | {
      outcome: "conflict"
      message: string
      serverLockVersion: number
      serverRevisionNo: number
      serverLegalName: string
      serverShortName?: string
      serverUnifiedCreditCode?: string
      actor: string
      changedAt: string
    }
  | {
      outcome: "unknown"
      message: string
      idempotencyKey: string
    }
