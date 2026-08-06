/**
 * W03 客户中心 — 真实 HTTP 适配层。
 * 保持 queries.ts 消费的导出签名与返回类型稳定；后端 Page/DTO 仅在此映射。
 *
 * 后端路由：
 * - GET/POST /admin/customers
 * - GET/PUT/DELETE /admin/customers/{id}
 * - GET/POST /admin/customers/{id}/assignments
 * - /admin/parties*（主体与联系人/地址/银行账户）
 */

import { apiGet, apiPost, apiPut } from "@/lib/api"
import type { ApiError, Page } from "@/lib/api"
import type {
  CreateCustomerAddressInput,
  CreateCustomerBankAccountInput,
  CreateCustomerContactInput,
  CreateCustomerInput,
  CustomerAddressView,
  CustomerAssignmentView,
  CustomerBankAccountView,
  CustomerCenterView,
  CustomerContactView,
  CustomerDirectoryItem,
  CustomerDirectoryQuery,
  CustomerDirectoryResult,
  CustomerMutationResult,
  CustomerStatus,
  RelatedObjectSummary,
  SaveCustomerDetailsInput,
  SaveCustomerRevisionInput,
} from "@/features/customers/types"
import {
  CONTRACT_STATUS_LABEL,
  CONTRACT_STATUS_TONE,
  type ContractStatus,
} from "@/features/contracts/types"

// ─── Backend DTO shapes (wire) ───────────────────────────────────────────────

type BackendCustomerStatus = "active" | "disabled"

type BackendCustomerView = {
  id: string
  party_id: string
  customer_no: string
  default_payment_term_id?: string | null
  status: BackendCustomerStatus
  version: number
  created_at: number
}

type BackendCustomerDetail = BackendCustomerView & {
  party_no?: string | null
  legal_name?: string | null
  owner_user_id?: string | null
}

type BackendAssignment = {
  id: string
  customer_id: string
  user_id: string
  assignment_role: "OWNER" | "COLLABORATOR"
  valid_from: string
  valid_to?: string | null
  change_reason: string
  version: number
  created_at: number
}

type BackendPartyView = {
  id: string
  party_no: string
  party_kind: string
  unified_credit_code?: string | null
  status: string
  current_revision_id?: string | null
  version: number
  created_at: number
}

type BackendPartyRevision = {
  id: string
  revision_no: number
  legal_name: string
  short_name?: string | null
  effective_from: string
  effective_to?: string | null
  change_reason: string
  version: number
  created_at: number
}

type BackendContact = {
  id: string
  party_id: string
  contact_name: string
  title?: string | null
  telephone?: string | null
  email?: string | null
  valid_from: string
  valid_to?: string | null
  is_default: boolean
  status: string
  version: number
  created_at: number
}

type BackendAddress = {
  id: string
  party_id: string
  address_type: "registered" | "operating" | "fulfillment" | string
  contact_name?: string | null
  valid_from: string
  valid_to?: string | null
  is_default: boolean
  status: string
  version: number
  created_at: number
}

type BackendBankAccount = {
  id: string
  bank_account_no: string
  party_id: string
  account_name: string
  bank_name: string
  bank_branch_name?: string | null
  valid_from: string
  valid_to?: string | null
  is_default: boolean
  status: string
  version: number
  created_at: number
}

type BackendContractListRow = {
  id: string
  contract_no: string
  customer_id: string
  settlement_party_id: string
  status: ContractStatus | string
  current_revision_id?: string | null
  created_at: number
  version: number
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function isApiError(error: unknown): error is ApiError {
  return (
    typeof error === "object" &&
    error !== null &&
    "kind" in error &&
    "message" in error
  )
}

function apiErrorMessage(error: ApiError): string {
  const data = error.responseData as { errorMessage?: string } | undefined
  if (data?.errorMessage && data.errorMessage !== "OK") {
    return data.errorMessage
  }
  return error.message
}

function todayBusinessDate(): string {
  const d = new Date()
  const pad = (n: number) => String(n).padStart(2, "0")
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}

function tsToIso(seconds: number | undefined | null): string {
  if (seconds == null || !Number.isFinite(seconds)) {
    return new Date().toISOString()
  }
  return new Date(seconds * 1000).toISOString()
}

function tsToDate(seconds: number | undefined | null): string {
  return tsToIso(seconds).slice(0, 10)
}

function mapCustomerStatus(status: BackendCustomerStatus | string): {
  status: CustomerStatus
  statusLabel: { label: string; tone: "success" | "neutral" }
} {
  if (status === "disabled") {
    return { status: "disabled", statusLabel: { label: "停用", tone: "neutral" } }
  }
  return { status: "active", statusLabel: { label: "启用", tone: "success" } }
}

function mapAddressTypeToBackend(label: string): string {
  const t = label.trim()
  if (t === "注册地址" || t === "registered") return "registered"
  if (t === "履约地址" || t === "fulfillment") return "fulfillment"
  if (t === "经营地址" || t === "operating") return "operating"
  // 开票地址 / 办公地址等前端历史选项 → 经营地址
  return "operating"
}

function mapAddressTypeFromBackend(code: string): string {
  switch (code) {
    case "registered":
      return "注册地址"
    case "fulfillment":
      return "履约地址"
    case "operating":
      return "经营地址"
    default:
      return code
  }
}

function emptyMetrics() {
  return {
    activeContractCount: 0,
    inProgressSalesOrderCount: 0,
    receivableBalance: "0.00",
    overdueAmount: "0.00",
  }
}

function genCode(prefix: string): string {
  const stamp = Date.now().toString(36).toUpperCase()
  const rand = Math.random().toString(36).slice(2, 6).toUpperCase()
  return `${prefix}-${stamp}${rand}`
}

async function safePage<T>(
  path: string,
  query?: Record<string, unknown>
): Promise<Page<T>> {
  try {
    return await apiGet<Page<T>>(path, query)
  } catch (error) {
    if (isApiError(error) && (error.status === 403 || error.status === 404)) {
      return { items: [], total: 0, page: 1, page_size: 20 }
    }
    throw error
  }
}

// ─── Directory ───────────────────────────────────────────────────────────────

function mapDirectoryItem(
  row: BackendCustomerView,
  detail: BackendCustomerDetail | null
): CustomerDirectoryItem {
  const { status, statusLabel } = mapCustomerStatus(row.status)
  const legalName =
    detail?.legal_name?.trim() ||
    detail?.party_no?.trim() ||
    row.customer_no
  return {
    id: row.id,
    partyId: row.party_id,
    customerNo: row.customer_no,
    legalName,
    shortName: undefined,
    status,
    statusLabel,
    ownerName: detail?.owner_user_id ?? "—",
    collaboratorCount: 0,
    // 后端列表无 mine/collaborating/team 范围；统一标记 team 供 UI 筛选不致空窗
    scopeTags: ["team", "mine", "collaborating"],
    metrics: emptyMetrics(),
    updatedAt: tsToDate(row.created_at),
    recentBusinessAt: tsToIso(row.created_at),
  }
}

/**
 * 客户目录：服务端 keyword + status + 分页；scope/sort 中后端不支持的部分见证据表。
 */
export async function fetchCustomerDirectory(
  query: CustomerDirectoryQuery
): Promise<CustomerDirectoryResult> {
  const statusParam =
    query.status === "all" ? undefined : query.status === "disabled" ? "disabled" : "active"

  // 后端排序白名单：created_at / customer_no / status
  const sort_by = query.sort === "name" ? "customer_no" : "created_at"
  const sort_dir: "asc" | "desc" = query.sortDir === "asc" ? "asc" : "desc"

  try {
    const page = await apiGet<Page<BackendCustomerView>>("/admin/customers", {
      keyword: query.query?.trim() || undefined,
      status: statusParam,
      page: 1,
      page_size: 100,
      sort_by,
      sort_dir,
    })

    const items = await Promise.all(
      page.items.map(async (row) => {
        try {
          const detail = await apiGet<BackendCustomerDetail>(
            `/admin/customers/${row.id}`
          )
          return mapDirectoryItem(row, detail)
        } catch {
          return mapDirectoryItem(row, null)
        }
      })
    )

    // scope 后端无实现：前端保留标签过滤语义（全部条目已打全 scopeTags）
    const scoped = items.filter((item) => item.scopeTags.includes(query.scope))

    return {
      hasCustomerScope: true,
      items: scoped,
      totalInScope: page.total,
      queriedAt: new Date().toISOString(),
    }
  } catch (error) {
    if (isApiError(error) && error.status === 403) {
      return {
        hasCustomerScope: false,
        items: [],
        totalInScope: 0,
        queriedAt: new Date().toISOString(),
      }
    }
    throw error
  }
}

// ─── Detail / center ─────────────────────────────────────────────────────────

function mapContact(c: BackendContact): CustomerContactView {
  return {
    id: c.id,
    name: c.contact_name,
    title: c.title ?? undefined,
    purpose: undefined,
    // 后端不返回手机号明文/掩码（仅指纹）；展示占位
    phoneMasked: "****",
    phoneRevealToken: undefined,
    email: c.email ?? undefined,
    isDefault: c.is_default,
    effectiveFrom: c.valid_from,
    effectiveTo: c.valid_to ?? undefined,
    fieldVisibility: { phone: "masked" },
  }
}

function mapAddress(a: BackendAddress): CustomerAddressView {
  return {
    id: a.id,
    addressType: mapAddressTypeFromBackend(a.address_type),
    addressMasked: "****",
    addressRevealToken: undefined,
    contactName: a.contact_name ?? undefined,
    isDefault: a.is_default,
    effectiveFrom: a.valid_from,
    effectiveTo: a.valid_to ?? undefined,
    fieldVisibility: { address: "masked" },
  }
}

function mapBank(b: BackendBankAccount): CustomerBankAccountView {
  return {
    id: b.id,
    internalNo: b.bank_account_no,
    accountName: b.account_name,
    bankName: b.bank_name,
    accountMasked: "****",
    accountRevealToken: undefined,
    isDefault: b.is_default,
    effectiveFrom: b.valid_from,
    effectiveTo: b.valid_to ?? undefined,
    fieldVisibility: { accountNumber: "masked" },
  }
}

function mapAssignment(a: BackendAssignment): CustomerAssignmentView {
  return {
    id: a.id,
    role: a.assignment_role,
    userId: a.user_id,
    userName: a.user_id,
    effectiveFrom: a.valid_from,
    effectiveTo: a.valid_to ?? undefined,
    isCurrent: !a.valid_to || a.valid_to >= todayBusinessDate(),
  }
}

function mapContractSummary(c: BackendContractListRow): RelatedObjectSummary {
  const status = (c.status as ContractStatus) || "EFFECTIVE"
  return {
    id: c.id,
    number: c.contract_no,
    title: c.contract_no,
    status: {
      label: CONTRACT_STATUS_LABEL[status] ?? String(c.status),
      tone: CONTRACT_STATUS_TONE[status] ?? "neutral",
    },
    href: `/sales/contracts/${c.id}`,
  }
}

/**
 * 客户对象中心：组合 customer detail + party 子资源 + 合同列表。
 * 应收/经营质量/销售单关联等投影字段后端本批未提供，填安全空值并在证据中登记。
 */
export async function fetchCustomerCenter(
  customerId: string
): Promise<CustomerCenterView | null> {
  if (!customerId) return null

  let detail: BackendCustomerDetail
  try {
    detail = await apiGet<BackendCustomerDetail>(`/admin/customers/${customerId}`)
  } catch (error) {
    if (isApiError(error) && (error.status === 404 || error.status === 403)) {
      return null
    }
    throw error
  }

  const partyId = detail.party_id
  const [
    party,
    revisionsPage,
    contactsPage,
    addressesPage,
    banksPage,
    assignmentsPage,
    contractsPage,
  ] = await Promise.all([
    apiGet<BackendPartyView>(`/admin/parties/${partyId}`).catch(() => null),
    safePage<BackendPartyRevision>(`/admin/parties/${partyId}/revisions`, {
      page: 1,
      page_size: 50,
      sort_by: "revision_no",
      sort_dir: "desc",
    }),
    safePage<BackendContact>(`/admin/parties/${partyId}/contacts`, {
      page: 1,
      page_size: 100,
    }),
    safePage<BackendAddress>(`/admin/parties/${partyId}/addresses`, {
      page: 1,
      page_size: 100,
    }),
    safePage<BackendBankAccount>(`/admin/parties/${partyId}/bank-accounts`, {
      page: 1,
      page_size: 100,
    }),
    safePage<BackendAssignment>(`/admin/customers/${customerId}/assignments`, {
      page: 1,
      page_size: 100,
    }),
    safePage<BackendContractListRow>("/admin/contracts", {
      customer_id: customerId,
      page: 1,
      page_size: 100,
    }),
  ])

  const currentRev =
    revisionsPage.items.find(
      (r) => party?.current_revision_id && r.id === party.current_revision_id
    ) ?? revisionsPage.items[0]

  const { status, statusLabel } = mapCustomerStatus(detail.status)
  const legalName =
    currentRev?.legal_name ?? detail.legal_name ?? detail.customer_no
  const nowIso = new Date().toISOString()
  const contracts = contractsPage.items.map(mapContractSummary)
  const activeContractCount = contractsPage.items.filter(
    (c) => c.status === "EFFECTIVE"
  ).length

  const assignments = assignmentsPage.items.map(mapAssignment)
  // 若归属列表为空但 detail 有 owner，补一条展示
  if (
    assignments.length === 0 &&
    detail.owner_user_id
  ) {
    assignments.push({
      id: `owner_${customerId}`,
      role: "OWNER",
      userId: detail.owner_user_id,
      userName: detail.owner_user_id,
      effectiveFrom: tsToDate(detail.created_at),
      isCurrent: true,
    })
  }

  return {
    customerId: detail.id,
    partyId,
    customerNo: detail.customer_no,
    status,
    statusLabel,
    lockVersion: detail.version,
    currentRevision: {
      revisionId: currentRev?.id ?? party?.current_revision_id ?? `rev_${customerId}`,
      revisionNo: currentRev?.revision_no ?? 1,
      legalName,
      shortName: currentRev?.short_name ?? undefined,
      unifiedCreditCode: party?.unified_credit_code ?? undefined,
      defaultPaymentTerm: detail.default_payment_term_id ?? undefined,
      effectiveFrom: currentRev?.effective_from
        ? `${currentRev.effective_from}T00:00:00.000Z`
        : tsToIso(detail.created_at),
    },
    assignments,
    contacts: contactsPage.items.map(mapContact),
    addresses: addressesPage.items.map(mapAddress),
    bankAccounts: banksPage.items.map(mapBank),
    metrics: {
      ...emptyMetrics(),
      activeContractCount,
    },
    contracts,
    salesOrders: [],
    receivableSummary: {
      receivableBalance: "0.00",
      overdueAmount: "0.00",
    },
    freshness: { formalFactsAt: nowIso },
    allowedActions: [
      "EDIT_CUSTOMER",
      "UPLOAD_CONTRACT_PDF",
      "CREATE_SALES_ORDER",
      "OPEN_RECEIVABLE",
    ],
    actionBlockers: [],
    revisionTimeline: revisionsPage.items.map((r) => ({
      id: r.id,
      revisionNo: r.revision_no,
      actor: "—",
      effectiveAt: r.effective_from
        ? `${r.effective_from}T00:00:00.000Z`
        : tsToIso(r.created_at),
      reason: r.change_reason,
      isCurrent: Boolean(
        party?.current_revision_id
          ? r.id === party.current_revision_id
          : r === revisionsPage.items[0]
      ),
    })),
    partitions: {
      identity: "ok",
      contacts: "ok",
      related: "ok",
      settlement: "ok",
      quality: "ok",
      audit: "ok",
    },
  }
}

// ─── Mutations ───────────────────────────────────────────────────────────────

async function createPartyContacts(
  partyId: string,
  contacts: readonly CreateCustomerContactInput[],
  validFrom: string
): Promise<void> {
  for (const c of contacts) {
    if (!c.name.trim()) continue
    if (!c.phone?.trim()) continue
    await apiPost(`/admin/parties/${partyId}/contacts`, {
      contact_name: c.name.trim(),
      title: c.title?.trim() || undefined,
      mobile: c.phone.trim(),
      email: c.email?.trim() || undefined,
      valid_from: validFrom,
      is_default: c.isDefault,
    })
  }
}

async function createPartyAddresses(
  partyId: string,
  addresses: readonly CreateCustomerAddressInput[],
  validFrom: string
): Promise<void> {
  for (const a of addresses) {
    if (!a.address.trim()) continue
    await apiPost(`/admin/parties/${partyId}/addresses`, {
      address_type: mapAddressTypeToBackend(a.addressType),
      address: a.address.trim(),
      valid_from: validFrom,
      is_default: a.isDefault,
    })
  }
}

async function createPartyBanks(
  partyId: string,
  banks: readonly CreateCustomerBankAccountInput[],
  validFrom: string
): Promise<void> {
  for (const [index, b] of banks.entries()) {
    if (!b.accountNumber.trim()) continue
    await apiPost(`/admin/parties/${partyId}/bank-accounts`, {
      bank_account_no: genCode(`BA${index + 1}`),
      account_name: b.accountName.trim(),
      bank_name: b.bankName.trim(),
      account_number: b.accountNumber.trim(),
      valid_from: validFrom,
      is_default: b.isDefault,
    })
  }
}

function conflictFromError(
  error: ApiError,
  fallback: {
    serverLockVersion: number
    serverRevisionNo: number
    serverLegalName: string
    serverShortName?: string
    serverUnifiedCreditCode?: string
  }
): CustomerMutationResult {
  return {
    outcome: "conflict",
    message:
      apiErrorMessage(error) ||
      "基础资料版本已变化，禁止静默覆盖。请查看系统最新版本后重做。",
    serverLockVersion: fallback.serverLockVersion,
    serverRevisionNo: fallback.serverRevisionNo,
    serverLegalName: fallback.serverLegalName,
    serverShortName: fallback.serverShortName,
    serverUnifiedCreditCode: fallback.serverUnifiedCreditCode,
    actor: "系统",
    changedAt: new Date().toISOString(),
  }
}

function unknownFromError(
  error: ApiError,
  idempotencyKey: string
): CustomerMutationResult | null {
  const msg = apiErrorMessage(error)
  if (
    error.status === 500 &&
    (msg.includes("无法确认") || msg.includes("未知") || msg.includes("OutcomeUnknown"))
  ) {
    return {
      outcome: "unknown",
      message:
        msg ||
        "提交结果不确定：未确认系统是否已生成新版本。请查询最终结果后再决定是否重试（同一任务号）。",
      idempotencyKey,
    }
  }
  return null
}

/**
 * 创建客户：先建 party，再建 customer_account（含 OWNER），再写联系人/地址/银行账户。
 */
export async function createCustomer(
  input: CreateCustomerInput
): Promise<CustomerMutationResult> {
  const validFrom = todayBusinessDate()
  const partyNo = genCode("P")
  const customerNo = genCode("KH")

  try {
    const party = await apiPost<BackendPartyView>("/admin/parties", {
      party_no: partyNo,
      legal_name: input.legalName.trim(),
      short_name: input.shortName?.trim() || undefined,
      unified_credit_code: input.unifiedCreditCode?.trim() || undefined,
      effective_from: validFrom,
      change_reason: "首版建档",
    })

    const customer = await apiPost<BackendCustomerView>("/admin/customers", {
      party_id: party.id,
      customer_no: customerNo,
      default_payment_term_id: input.defaultPaymentTerm || undefined,
      owner_user_id: input.ownerUserId,
      valid_from: validFrom,
      change_reason: "首版建档",
      status: "active",
    })

    await createPartyContacts(party.id, input.contacts ?? [], validFrom)
    await createPartyAddresses(party.id, input.addresses ?? [], validFrom)
    await createPartyBanks(party.id, input.bankAccounts ?? [], validFrom)

    return {
      outcome: "succeeded",
      customerId: customer.id,
      customerNo: customer.customer_no,
      revisionNo: 1,
      lockVersion: customer.version,
      occurredAt: tsToIso(customer.created_at),
      reference: `CUST-NEW-${customer.customer_no}`,
    }
  } catch (error) {
    if (isApiError(error)) {
      const unknown = unknownFromError(error, input.idempotencyKey)
      if (unknown) return unknown
      if (error.status === 409) {
        return {
          outcome: "conflict",
          message:
            apiErrorMessage(error) ||
            "存在相似主体候选或编号冲突，未自动合并。请选择既有客户或修改后重试。",
          serverLockVersion: 0,
          serverRevisionNo: 0,
          serverLegalName: input.legalName,
          actor: "系统",
          changedAt: new Date().toISOString(),
        }
      }
    }
    throw error
  }
}

/**
 * 仅修订主体身份（法定名称等）→ PUT /admin/parties/{id} 形成新修订。
 */
export async function saveCustomerRevision(
  input: SaveCustomerRevisionInput
): Promise<CustomerMutationResult> {
  try {
    const detail = await apiGet<BackendCustomerDetail>(
      `/admin/customers/${input.customerId}`
    )
    const party = await apiGet<BackendPartyView>(
      `/admin/parties/${detail.party_id}`
    )

    if (input.expectedLockVersion !== detail.version) {
      return {
        outcome: "conflict",
        message: "基础资料版本已变化，禁止静默覆盖。请查看系统最新版本后重做。",
        serverLockVersion: detail.version,
        serverRevisionNo: 0,
        serverLegalName: detail.legal_name ?? detail.customer_no,
        serverUnifiedCreditCode: party.unified_credit_code ?? undefined,
        actor: "系统",
        changedAt: new Date().toISOString(),
      }
    }

    const updated = await apiPut<BackendPartyView>(
      `/admin/parties/${detail.party_id}`,
      {
        version: party.version,
        legal_name: input.legalName.trim(),
        short_name: input.shortName?.trim() || undefined,
        unified_credit_code: input.unifiedCreditCode?.trim() ?? undefined,
        effective_from: todayBusinessDate(),
        change_reason: input.changeReason.trim(),
      }
    )

    // 客户乐观锁可能未变（仅改 party）；返回最新 customer version
    const refreshed = await apiGet<BackendCustomerDetail>(
      `/admin/customers/${input.customerId}`
    )
    const revs = await safePage<BackendPartyRevision>(
      `/admin/parties/${detail.party_id}/revisions`,
      { page: 1, page_size: 1, sort_by: "revision_no", sort_dir: "desc" }
    )
    const revNo = revs.items[0]?.revision_no ?? 1

    return {
      outcome: "succeeded",
      customerId: input.customerId,
      customerNo: refreshed.customer_no,
      revisionNo: revNo,
      lockVersion: refreshed.version,
      occurredAt: new Date().toISOString(),
      reference: `CUST-REV-${revNo}-${updated.version}`,
    }
  } catch (error) {
    if (isApiError(error)) {
      const unknown = unknownFromError(error, input.idempotencyKey)
      if (unknown) return unknown
      if (error.status === 409) {
        try {
          const detail = await apiGet<BackendCustomerDetail>(
            `/admin/customers/${input.customerId}`
          )
          return conflictFromError(error, {
            serverLockVersion: detail.version,
            serverRevisionNo: 0,
            serverLegalName: detail.legal_name ?? detail.customer_no,
          })
        } catch {
          return conflictFromError(error, {
            serverLockVersion: input.expectedLockVersion,
            serverRevisionNo: 0,
            serverLegalName: input.legalName,
          })
        }
      }
    }
    throw error
  }
}

/**
 * 保存身份 + 追加联系人/地址/银行账户（后端子资源为追加模型，非整表替换）。
 */
export async function saveCustomerDetails(
  input: SaveCustomerDetailsInput
): Promise<CustomerMutationResult> {
  try {
    const detail = await apiGet<BackendCustomerDetail>(
      `/admin/customers/${input.customerId}`
    )
    const party = await apiGet<BackendPartyView>(
      `/admin/parties/${detail.party_id}`
    )

    if (input.expectedLockVersion !== detail.version) {
      return {
        outcome: "conflict",
        message: "基础资料版本已变化，禁止静默覆盖。请查看系统最新版本后重做。",
        serverLockVersion: detail.version,
        serverRevisionNo: 0,
        serverLegalName: detail.legal_name ?? detail.customer_no,
        serverUnifiedCreditCode: party.unified_credit_code ?? undefined,
        actor: "系统",
        changedAt: new Date().toISOString(),
      }
    }

    await apiPut<BackendPartyView>(`/admin/parties/${detail.party_id}`, {
      version: party.version,
      legal_name: input.legalName.trim(),
      short_name: input.shortName?.trim() || undefined,
      unified_credit_code: input.unifiedCreditCode?.trim() ?? undefined,
      effective_from: todayBusinessDate(),
      change_reason: input.changeReason.trim(),
    })

    const validFrom = todayBusinessDate()
    await createPartyContacts(detail.party_id, input.contacts, validFrom)
    await createPartyAddresses(detail.party_id, input.addresses, validFrom)
    await createPartyBanks(detail.party_id, input.bankAccounts, validFrom)

    const refreshed = await apiGet<BackendCustomerDetail>(
      `/admin/customers/${input.customerId}`
    )
    const revs = await safePage<BackendPartyRevision>(
      `/admin/parties/${detail.party_id}/revisions`,
      { page: 1, page_size: 1, sort_by: "revision_no", sort_dir: "desc" }
    )
    const revNo = revs.items[0]?.revision_no ?? 1

    return {
      outcome: "succeeded",
      customerId: input.customerId,
      customerNo: refreshed.customer_no,
      revisionNo: revNo,
      lockVersion: refreshed.version,
      occurredAt: new Date().toISOString(),
      reference: `CUST-REV-${revNo}-${Date.now().toString(36).toUpperCase()}`,
    }
  } catch (error) {
    if (isApiError(error)) {
      const unknown = unknownFromError(error, input.idempotencyKey)
      if (unknown) return unknown
      if (error.status === 409) {
        try {
          const detail = await apiGet<BackendCustomerDetail>(
            `/admin/customers/${input.customerId}`
          )
          return conflictFromError(error, {
            serverLockVersion: detail.version,
            serverRevisionNo: 0,
            serverLegalName: detail.legal_name ?? detail.customer_no,
          })
        } catch {
          return conflictFromError(error, {
            serverLockVersion: input.expectedLockVersion,
            serverRevisionNo: 0,
            serverLegalName: input.legalName,
          })
        }
      }
    }
    throw error
  }
}

/**
 * 幂等查询：本批后端无客户域幂等查询接口，返回 null（前端可提示用户刷新）。
 */
export async function queryCustomerMutationByIdempotency(
  idempotencyKey: string
): Promise<CustomerMutationResult | null> {
  void idempotencyKey
  return null
}

/**
 * 敏感字段揭示：后端未提供 reveal token 接口（实体仅存指纹）。
 * 抛 ApiError 供 mutation 进入 error 态。
 */
export async function revealCustomerSensitiveField(
  revealToken: string
): Promise<string> {
  void revealToken
  const error: ApiError = {
    kind: "Http",
    status: 501,
    message: "敏感字段揭示接口尚未提供，无法查看明文",
  }
  throw error
}
