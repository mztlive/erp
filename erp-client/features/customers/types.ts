import type { StatusTone } from "@/components/ui/status-badge"

/** Directory scope (URL and backend contract). */
export type CustomerScope =
  | "mine"
  | "collaborating"
  | "assigned"
  | "all_authorized"

export type CustomerStatus = "active" | "disabled"

export type CustomerSectionId =
  | "overview"
  | "contacts"
  | "related"
  | "settlement"
  | "quality"
  | "audit"

type FieldVisibility = "full" | "masked" | "hidden"

export type CustomerAssignmentView = Readonly<{
  id: string
  role: "OWNER" | "COLLABORATOR"
  userId: string
  userName: string
  effectiveFrom: string
  effectiveTo?: string
  changeReason: string
  version: number
  isCurrent: boolean
}>

export type CustomerAssignmentChangeInput = Readonly<{
  customerId: string
  action: "assign" | "end"
  userId?: string
  role?: "OWNER" | "COLLABORATOR"
  effectiveFrom?: string
  effectiveTo?: string
  assignmentId?: string
  version?: number
  changeReason: string
}>

export type CustomerContactView = Readonly<{
  id: string
  name: string
  title?: string
  purpose?: string
  telephone?: string
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
  branchName?: string
  accountMasked: string
  accountRevealToken?: string
  isDefault: boolean
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

type CustomerRelationshipMetrics = Readonly<{
  /** 来自正式关联接口的完整分页汇总；分区失败时必须返回 null。 */
  activeContractCount: number | null
  inProgressSalesOrderCount: number | null
  receivableBalance: string | null
  overdueAmount: string | null
  expiringContractCount?: number
}>

type ReceivableSummary = Readonly<{
  receivableBalance: string
  overdueAmount: string
  earliestOverdueDate?: string
  collectionProgressLabel?: string
  invoicingProgressLabel?: string
  reliabilityNote?: string
}>

type CustomerQualitySummary = Readonly<{
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
  partyLockVersion: number
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
  sort?: "updated_at"
  sortDir?: "asc" | "desc"
  page: number
  pageSize: number
}>

export type CustomerDirectoryResult = Readonly<{
  /** False when role has no customer data scope at all. */
  hasCustomerScope: boolean
  items: readonly CustomerDirectoryItem[]
  totalInScope: number
  page: number
  pageSize: number
  queriedAt: string
}>

export type SaveCustomerRevisionInput = Readonly<{
  customerId: string
  expectedLockVersion: number
  expectedPartyVersion: number
  baseRevisionId: string
  legalName: string
  shortName?: string
  unifiedCreditCode?: string
  defaultPaymentTerm?: string
  status: CustomerStatus
  changeReason: string
  idempotencyKey: string
}>

export type SaveCustomerDetailsInput = Readonly<{
  customerId: string
  expectedLockVersion: number
  expectedPartyVersion: number
  baseRevisionId: string
  legalName: string
  shortName?: string
  unifiedCreditCode?: string
  defaultPaymentTerm?: string
  status: CustomerStatus
  changeReason: string
  contacts?: readonly CreateCustomerContactInput[]
  addresses?: readonly CreateCustomerAddressInput[]
  bankAccounts?: readonly CreateCustomerBankAccountInput[]
  idempotencyKey: string
}>

export type CreateCustomerContactInput = Readonly<{
  existingId?: string
  name: string
  title?: string
  telephone?: string
  phone?: string
  email?: string
  isDefault: boolean
}>

export type CreateCustomerAddressInput = Readonly<{
  existingId?: string
  addressType: string
  contactName?: string
  address?: string
  isDefault: boolean
}>

export type CreateCustomerBankAccountInput = Readonly<{
  existingId?: string
  accountName: string
  bankName: string
  branchName?: string
  accountNumber?: string
  isDefault: boolean
}>

export type CreateCustomerInput = Readonly<{
  legalName: string
  shortName?: string
  unifiedCreditCode?: string
  ownerUserId: string
  ownerName: string
  defaultPaymentTerm?: string
  status?: CustomerStatus
  contacts?: readonly CreateCustomerContactInput[]
  addresses?: readonly CreateCustomerAddressInput[]
  bankAccounts?: readonly CreateCustomerBankAccountInput[]
  idempotencyKey: string
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
