/**
 * 客户资料 HTTP 适配层。
 *
 * 客户创建和修订只允许通过 customer-profiles 根命令提交，避免 Party、客户角色、
 * 归属和从属事实出现部分成功。Wire DTO 只在本文件存在，页面消费 camelCase 视图。
 */

import { apiGet, apiPost, apiPut } from "@/lib/api"
import type { ApiError, Page } from "@/lib/api"
import type {
    CreateCustomerAddressInput,
    CreateCustomerBankAccountInput,
    CreateCustomerContactInput,
    CreateCustomerInput,
    CustomerAssignmentChangeInput,
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
} from "@/features/customers/types"
import {
    CONTRACT_STATUS_LABEL,
    CONTRACT_STATUS_TONE,
    type ContractStatus,
} from "@/features/contracts/types"
import {
    fetchCustomerQuality,
    fetchCustomerQualityPeriodPolicy,
} from "@/features/customer-quality/api"
import {
    compareDecimal,
    formatScaled,
    normalizeFixed,
} from "@/lib/fixed-decimal"

type BackendCustomerStatus = "active" | "disabled"
type BackendCustomerScope =
    | "mine"
    | "collaborating"
    | "assigned"
    | "all_authorized"
type BackendSensitiveKind = "contact_mobile" | "address" | "bank_account_number"

type BackendCustomerView = {
    id: string
    party_id: string
    party_no?: string | null
    legal_name?: string | null
    short_name?: string | null
    customer_no: string
    default_payment_term_id?: string | null
    status: BackendCustomerStatus
    owner_user_id?: string | null
    owner_user_name?: string | null
    collaborator_count: number
    scope_tags: BackendCustomerScope[]
    version: number
    created_at: number
    updated_at: number
}

type BackendPartyRevision = {
    id: string
    revision_no: number
    legal_name: string
    short_name?: string | null
    change_reason: string
    version: number
    created_at: number
}

type BackendAssignment = {
    id: string
    customer_id: string
    user_id: string
    user_name: string
    assignment_role: "OWNER" | "COLLABORATOR"
    valid_from: string
    valid_to?: string | null
    change_reason: string
    version: number
    created_at: number
}

type BackendContact = {
    id: string
    contact_name: string
    title?: string | null
    telephone?: string | null
    mobile_masked: string
    email?: string | null
    valid_from: string
    valid_to?: string | null
    is_default: boolean
}

type BackendAddress = {
    id: string
    address_type: "registered" | "operating" | "fulfillment"
    contact_name?: string | null
    valid_from: string
    valid_to?: string | null
    is_default: boolean
}

type BackendBankAccount = {
    id: string
    bank_account_no: string
    account_name: string
    bank_name: string
    account_number_masked: string
    bank_branch_name?: string | null
    valid_from: string
    valid_to?: string | null
    is_default: boolean
}

type BackendSensitiveField = {
    kind: BackendSensitiveKind
    record_id: string
    masked_value: string
    reveal_token: string
    expires_at: number
}

type BackendCustomerProfile = BackendCustomerView & {
    party_status: string
    party_version: number
    unified_credit_code?: string | null
    current_revision: BackendPartyRevision
    revisions: BackendPartyRevision[]
    assignments: BackendAssignment[]
    contacts: BackendContact[]
    addresses: BackendAddress[]
    bank_accounts: BackendBankAccount[]
    sensitive_fields: BackendSensitiveField[]
    allowed_actions: string[]
    action_blockers: { action: string; code: string; message: string }[]
}

type BackendProfileMutation = {
    customer_id: string
    customer_no: string
    party_id: string
    revision_id: string
    revision_no: number
    customer_version: number
    party_version: number
    effective_from: string
    recorded_at: number
    change_reason: string
}

type BackendContractListRow = {
    id: string
    contract_no: string
    customer_id: string
    status: ContractStatus | string
}

type BackendSalesOrderListRow = {
    id: string
    order_no: string
    customer_id: string
    commercial_status: string
    close_status: string
    created_at: number
}

type BackendReceivableEntry = {
    direction: "increase" | "decrease"
    amount: string
    offset_total: string
    due_date: string
}

type BackendReceivableAccount = {
    open_total: string
    gross_total: string
    settled_total: string
    open_invoiceable_total: string
    entries: BackendReceivableEntry[]
}

type BackendProfileRequest = {
    idempotency_key: string
    expected_party_version?: number
    expected_customer_version?: number
    legal_name: string
    short_name?: string
    unified_credit_code?: string
    default_payment_term_id?: string
    status?: CustomerStatus
    owner_user_id?: string
    contacts?: ReturnType<typeof mapContactInput>[]
    addresses?: ReturnType<typeof mapAddressInput>[]
    bank_accounts?: ReturnType<typeof mapBankInput>[]
    effective_from: string
    change_reason: string
}

/** 判断未知异常是否为统一 API 错误。 */
function isApiError(error: unknown): error is ApiError {
    return (
        typeof error === "object" &&
        error !== null &&
        "kind" in error &&
        "message" in error
    )
}

/** 提取服务端稳定错误消息。 */
function apiErrorMessage(error: ApiError): string {
    const data = error.responseData as { errorMessage?: string } | undefined
    return data?.errorMessage && data.errorMessage !== "OK"
        ? data.errorMessage
        : error.message
}

/** 返回本地业务日期。 */
function todayBusinessDate(): string {
    const date = new Date()
    const pad = (value: number) => String(value).padStart(2, "0")
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

/** 秒级时间戳转 ISO 时间。 */
function tsToIso(seconds: number | undefined | null): string {
    return seconds == null || !Number.isFinite(seconds)
        ? new Date().toISOString()
        : new Date(seconds * 1000).toISOString()
}

/** 映射客户状态及其展示语义。 */
function mapCustomerStatus(status: BackendCustomerStatus | string): {
    status: CustomerStatus
    statusLabel: { label: string; tone: "success" | "neutral" }
} {
    return status === "disabled"
        ? {
              status: "disabled",
              statusLabel: { label: "停用", tone: "neutral" },
          }
        : { status: "active", statusLabel: { label: "启用", tone: "success" } }
}

/** 后端地址代码转中文展示值。 */
function mapAddressTypeFromBackend(
    code: BackendAddress["address_type"],
): string {
    if (code === "registered") return "注册地址"
    if (code === "fulfillment") return "履约地址"
    return "经营地址"
}

/** 前端地址展示值转后端稳定代码。 */
function mapAddressTypeToBackend(
    label: string,
): BackendAddress["address_type"] {
    if (label === "注册地址" || label === "registered") return "registered"
    if (label === "经营地址" || label === "operating") return "operating"
    return "fulfillment"
}

/** 返回后端尚未提供聚合投影时的显式不可用值。 */
function emptyMetrics() {
    return {
        activeContractCount: null,
        inProgressSalesOrderCount: null,
        receivableBalance: null,
        overdueAmount: null,
    }
}

/** 按服务端 total 读取完整小型关联集合，避免只汇总第一页。 */
async function loadAllPages<T>(
    path: string,
    query: Record<string, unknown>,
): Promise<Page<T>> {
    const first = await apiGet<Page<T>>(path, {
        ...query,
        page: 1,
        page_size: 100,
    })
    const pages = Math.ceil(first.total / 100)
    if (pages <= 1) return first
    const rest = await Promise.all(
        Array.from({ length: pages - 1 }, (_, index) =>
            apiGet<Page<T>>(path, {
                ...query,
                page: index + 2,
                page_size: 100,
            }),
        ),
    )
    return {
        ...first,
        items: [...first.items, ...rest.flatMap((page) => page.items)],
    }
}

/** 映射目录行；目录接口已批量补齐主体与归属，不再逐行请求详情。 */
function mapDirectoryItem(row: BackendCustomerView): CustomerDirectoryItem {
    const { status, statusLabel } = mapCustomerStatus(row.status)
    return {
        id: row.id,
        partyId: row.party_id,
        customerNo: row.customer_no,
        legalName:
            row.legal_name?.trim() || row.party_no?.trim() || row.customer_no,
        shortName: row.short_name ?? undefined,
        status,
        statusLabel,
        ownerName: row.owner_user_name ?? row.owner_user_id ?? "—",
        collaboratorCount: row.collaborator_count,
        scopeTags: row.scope_tags,
        metrics: emptyMetrics(),
        updatedAt: tsToIso(row.updated_at),
    }
}

/**
 * 查询客户目录。范围、过滤、排序和分页均由服务端执行。
 */
export async function fetchCustomerDirectory(
    query: CustomerDirectoryQuery,
): Promise<CustomerDirectoryResult> {
    const status = query.status === "all" ? undefined : query.status
    const path =
        query.scope === "all_authorized"
            ? "/admin/customers/all-authorized"
            : "/admin/customers"
    try {
        const page = await apiGet<Page<BackendCustomerView>>(path, {
            scope: query.scope,
            keyword: query.query?.trim() || undefined,
            status,
            page: query.page,
            page_size: query.pageSize,
            sort_by: "updated_at",
            sort_dir: query.sortDir === "asc" ? "asc" : "desc",
        })
        return {
            hasCustomerScope: true,
            items: page.items.map(mapDirectoryItem),
            totalInScope: page.total,
            page: page.page,
            pageSize: page.page_size,
            queriedAt: new Date().toISOString(),
        }
    } catch (error) {
        if (isApiError(error) && error.status === 403) {
            return {
                hasCustomerScope: false,
                items: [],
                totalInScope: 0,
                page: query.page,
                pageSize: query.pageSize,
                queriedAt: new Date().toISOString(),
            }
        }
        throw error
    }
}

/** 以字段类型和事实 ID 索引敏感字段令牌。 */
function sensitiveIndex(fields: BackendSensitiveField[]) {
    return new Map(
        fields.map((field) => [`${field.kind}:${field.record_id}`, field]),
    )
}

/** 映射联系人当前事实。 */
function mapContact(
    contact: BackendContact,
    fields: Map<string, BackendSensitiveField>,
): CustomerContactView {
    const sensitive = fields.get(`contact_mobile:${contact.id}`)
    return {
        id: contact.id,
        name: contact.contact_name,
        title: contact.title ?? undefined,
        telephone: contact.telephone ?? undefined,
        phoneMasked: sensitive?.masked_value ?? contact.mobile_masked,
        phoneRevealToken: sensitive?.reveal_token,
        email: contact.email ?? undefined,
        isDefault: contact.is_default,
        effectiveFrom: contact.valid_from,
        effectiveTo: contact.valid_to ?? undefined,
        fieldVisibility: { phone: "masked" },
    }
}

/** 映射地址当前事实。 */
function mapAddress(
    address: BackendAddress,
    fields: Map<string, BackendSensitiveField>,
): CustomerAddressView {
    const sensitive = fields.get(`address:${address.id}`)
    return {
        id: address.id,
        addressType: mapAddressTypeFromBackend(address.address_type),
        addressMasked: sensitive?.masked_value ?? "********",
        addressRevealToken: sensitive?.reveal_token,
        contactName: address.contact_name ?? undefined,
        isDefault: address.is_default,
        effectiveFrom: address.valid_from,
        effectiveTo: address.valid_to ?? undefined,
        fieldVisibility: { address: "masked" },
    }
}

/** 映射银行账户当前事实。 */
function mapBank(
    account: BackendBankAccount,
    fields: Map<string, BackendSensitiveField>,
): CustomerBankAccountView {
    const sensitive = fields.get(`bank_account_number:${account.id}`)
    return {
        id: account.id,
        internalNo: account.bank_account_no,
        accountName: account.account_name,
        bankName: account.bank_name,
        branchName: account.bank_branch_name ?? undefined,
        accountMasked: sensitive?.masked_value ?? account.account_number_masked,
        accountRevealToken: sensitive?.reveal_token,
        isDefault: account.is_default,
        effectiveFrom: account.valid_from,
        effectiveTo: account.valid_to ?? undefined,
        fieldVisibility: { accountNumber: "masked" },
    }
}

/** 映射客户归属历史。 */
function mapAssignment(assignment: BackendAssignment): CustomerAssignmentView {
    const today = todayBusinessDate()
    return {
        id: assignment.id,
        role: assignment.assignment_role,
        userId: assignment.user_id,
        userName: assignment.user_name || assignment.user_id,
        effectiveFrom: assignment.valid_from,
        effectiveTo: assignment.valid_to ?? undefined,
        changeReason: assignment.change_reason,
        version: assignment.version,
        isCurrent:
            assignment.valid_from <= today &&
            (!assignment.valid_to || assignment.valid_to > today),
    }
}

/** 建立、换任或结束客户责任归属。 */
export async function applyCustomerAssignment(
    input: CustomerAssignmentChangeInput,
): Promise<CustomerAssignmentView[]> {
    const rows = await apiPost<BackendAssignment[]>(
        `/admin/customers/${input.customerId}/assignments`,
        {
            action: input.action,
            user_id: input.userId,
            assignment_role: input.role,
            valid_from: input.effectiveFrom,
            valid_to: input.effectiveTo,
            assignment_id: input.assignmentId,
            change_reason: input.changeReason.trim(),
            version: input.version,
        },
    )
    return rows.map(mapAssignment)
}

/** 映射合同摘要。 */
function mapContractSummary(
    contract: BackendContractListRow,
): RelatedObjectSummary {
    const status = contract.status as ContractStatus
    return {
        id: contract.id,
        number: contract.contract_no,
        title: contract.contract_no,
        status: {
            label: CONTRACT_STATUS_LABEL[status] ?? String(contract.status),
            tone: CONTRACT_STATUS_TONE[status] ?? "neutral",
        },
        href: `/sales/contracts/${contract.id}`,
    }
}

/** 映射销售单摘要。 */
function mapSalesOrderSummary(
    order: BackendSalesOrderListRow,
): RelatedObjectSummary {
    const status = order.commercial_status
    const meta =
        status === "EFFECTIVE"
            ? { label: "已生效", tone: "success" as const }
            : status === "PENDING_REVIEW"
              ? { label: "审核中", tone: "warning" as const }
              : status === "VOIDED"
                ? { label: "已作废", tone: "neutral" as const }
                : { label: "草稿", tone: "neutral" as const }
    return {
        id: order.id,
        number: order.order_no,
        title: order.order_no,
        status: meta,
        href: `/sales/orders/${order.id}`,
    }
}

/** 把金额字符串转换为分，禁止经过 JavaScript number。 */
function moneyCents(value: string): bigint {
    return BigInt(
        normalizeFixed(value, {
            maxScale: 2,
            outputScale: 2,
            allowNegative: true,
        }).replace(".", ""),
    )
}

/** 汇总客户应收当前余额与已逾期未核销分录。 */
function receivableProjection(accounts: BackendReceivableAccount[]) {
    const today = todayBusinessDate()
    const receivableCents = accounts.reduce(
        (total, account) => total + moneyCents(account.open_total),
        BigInt(0),
    )
    let overdueCents = BigInt(0)
    const overdueDates: string[] = []
    for (const account of accounts) {
        for (const entry of account.entries ?? []) {
            if (
                entry.direction !== "increase" ||
                !entry.due_date ||
                entry.due_date >= today ||
                compareDecimal(entry.amount, entry.offset_total, 2) <= 0
            ) {
                continue
            }
            overdueCents +=
                moneyCents(entry.amount) - moneyCents(entry.offset_total)
            overdueDates.push(entry.due_date)
        }
    }
    const openInvoiceCents = accounts.reduce(
        (total, account) => total + moneyCents(account.open_invoiceable_total),
        BigInt(0),
    )
    return {
        receivableBalance: formatScaled(receivableCents, 2),
        overdueAmount: formatScaled(overdueCents, 2),
        earliestOverdueDate: overdueDates.sort()[0],
        collectionProgressLabel:
            receivableCents === BigInt(0) ? "已结清" : "存在未结清余额",
        invoicingProgressLabel:
            openInvoiceCents === BigInt(0) ? "已完成" : "存在可开票余额",
    }
}

/** 查询指定客户的当前经营质量投影摘要。 */
async function loadCustomerQualitySummary(customerId: string) {
    const policy = await fetchCustomerQualityPeriodPolicy()
    if (
        !policy.hasDefault ||
        !policy.from ||
        !policy.to ||
        !policy.periodBasis
    ) {
        return undefined
    }
    const view = await fetchCustomerQuality({
        from: policy.from,
        to: policy.to,
        periodBasis: policy.periodBasis,
        periodSelectionSource: policy.selectionSource ?? "SERVER_DEFAULT",
        customerQualityPeriodPolicyId: policy.customerQualityPeriodPolicyId,
        customerQualityPeriodPolicyVersion:
            policy.customerQualityPeriodPolicyVersion,
        scopeId: `customer:${customerId}`,
        fundsReview: "all",
        sort: "salesGrossAmount:desc",
        page: 1,
        pageSize: 1,
        customerId,
    })
    const row = view.customers.items[0]
    if (!row) return undefined
    return {
        scaleLabel:
            row.tags.find((tag) => tag.type === "scale")?.label ?? "未分层",
        profitContributionLabel:
            row.tags.find((tag) => tag.type === "profit")?.label ?? "未分层",
        collectionRiskLabel:
            row.tags.find((tag) => tag.type === "risk")?.label ?? "未分层",
        lastBusinessAt: row.latestBusinessAt,
        projectionAt: view.freshness.projectedAt,
        isStale: view.freshness.state !== "fresh",
    }
}

/** 查询客户对象中心；客户正式资料只来自统一 profile 读模型。 */
export async function fetchCustomerCenter(
    customerId: string,
): Promise<CustomerCenterView | null> {
    if (!customerId) return null
    let profile: BackendCustomerProfile
    try {
        profile = await apiGet<BackendCustomerProfile>(
            `/admin/customer-profiles/${customerId}`,
        )
    } catch (error) {
        if (
            isApiError(error) &&
            (error.status === 403 || error.status === 404)
        ) {
            return null
        }
        throw error
    }

    const [
        contractsResult,
        salesOrdersResult,
        receivablesResult,
        qualityResult,
    ] = await Promise.allSettled([
        loadAllPages<BackendContractListRow>("/admin/contracts", {
            customer_id: customerId,
        }),
        loadAllPages<BackendSalesOrderListRow>("/admin/sales-orders", {
            customer_id: customerId,
            sort_by: "created_at",
            sort_dir: "desc",
        }),
        loadAllPages<BackendReceivableAccount>("/admin/receivable-accounts", {
            customer_id: customerId,
            sort_by: "created_at",
            sort_dir: "desc",
        }),
        loadCustomerQualitySummary(customerId),
    ])

    const fields = sensitiveIndex(profile.sensitive_fields)
    const contractRows =
        contractsResult.status === "fulfilled"
            ? contractsResult.value.items
            : []
    const salesOrderRows =
        salesOrdersResult.status === "fulfilled"
            ? salesOrdersResult.value.items
            : []
    const contracts = contractRows.map(mapContractSummary)
    const salesOrders = salesOrderRows.map(mapSalesOrderSummary)
    const receivableSummary =
        receivablesResult.status === "fulfilled"
            ? receivableProjection(receivablesResult.value.items)
            : undefined
    const qualitySummary =
        qualityResult.status === "fulfilled" ? qualityResult.value : undefined
    const { status, statusLabel } = mapCustomerStatus(profile.status)
    return {
        customerId: profile.id,
        partyId: profile.party_id,
        customerNo: profile.customer_no,
        status,
        statusLabel,
        lockVersion: profile.version,
        partyLockVersion: profile.party_version,
        currentRevision: {
            revisionId: profile.current_revision.id,
            revisionNo: profile.current_revision.revision_no,
            legalName: profile.current_revision.legal_name,
            shortName: profile.current_revision.short_name ?? undefined,
            unifiedCreditCode: profile.unified_credit_code ?? undefined,
            defaultPaymentTerm: profile.default_payment_term_id ?? undefined,
            effectiveFrom: tsToIso(profile.current_revision.created_at),
        },
        assignments: profile.assignments.map(mapAssignment),
        contacts: profile.contacts.map((contact) =>
            mapContact(contact, fields),
        ),
        addresses: profile.addresses.map((address) =>
            mapAddress(address, fields),
        ),
        bankAccounts: profile.bank_accounts.map((account) =>
            mapBank(account, fields),
        ),
        metrics: {
            activeContractCount:
                contractsResult.status === "fulfilled"
                    ? contractRows.filter(
                          (contract) => contract.status === "EFFECTIVE",
                      ).length
                    : null,
            inProgressSalesOrderCount:
                salesOrdersResult.status === "fulfilled"
                    ? salesOrderRows.filter(
                          (order) =>
                              order.commercial_status !== "VOIDED" &&
                              order.close_status !== "CLOSED",
                      ).length
                    : null,
            receivableBalance: receivableSummary?.receivableBalance ?? null,
            overdueAmount: receivableSummary?.overdueAmount ?? null,
        },
        contracts,
        salesOrders,
        receivableSummary,
        qualitySummary,
        freshness: {
            formalFactsAt: tsToIso(profile.updated_at),
            qualityProjectionAt: qualitySummary?.projectionAt,
        },
        allowedActions: profile.allowed_actions,
        actionBlockers: profile.action_blockers,
        revisionTimeline: profile.revisions.map((revision) => ({
            id: revision.id,
            revisionNo: revision.revision_no,
            actor: "—",
            effectiveAt: tsToIso(revision.created_at),
            reason: revision.change_reason,
            isCurrent: revision.id === profile.current_revision.id,
        })),
        partitions: {
            identity: "ok",
            contacts: "ok",
            related:
                contractsResult.status === "fulfilled" &&
                salesOrdersResult.status === "fulfilled"
                    ? "ok"
                    : "error",
            settlement:
                receivablesResult.status === "fulfilled" ? "ok" : "error",
            quality: qualityResult.status === "fulfilled" ? "ok" : "error",
            audit: "ok",
        },
    }
}

/** 映射联系人根命令输入。 */
function mapContactInput(input: CreateCustomerContactInput) {
    return {
        existing_id: input.existingId,
        contact_name: input.name.trim(),
        title: input.title?.trim() || undefined,
        mobile: input.phone?.trim() || undefined,
        telephone: input.telephone?.trim() || undefined,
        email: input.email?.trim() || undefined,
        is_default: input.isDefault,
    }
}

/** 映射地址根命令输入。 */
function mapAddressInput(input: CreateCustomerAddressInput) {
    return {
        existing_id: input.existingId,
        address_type: mapAddressTypeToBackend(input.addressType),
        contact_name: input.contactName?.trim() || undefined,
        address: input.address?.trim() || undefined,
        is_default: input.isDefault,
    }
}

/** 映射银行账户根命令输入。 */
function mapBankInput(input: CreateCustomerBankAccountInput) {
    return {
        existing_id: input.existingId,
        account_name: input.accountName.trim(),
        bank_name: input.bankName.trim(),
        bank_branch_name: input.branchName?.trim() || undefined,
        account_number: input.accountNumber?.trim() || undefined,
        is_default: input.isDefault,
    }
}

/** 映射稳定成功结果。 */
function mapMutationResult(
    result: BackendProfileMutation,
): CustomerMutationResult {
    return {
        outcome: "succeeded",
        customerId: result.customer_id,
        customerNo: result.customer_no,
        revisionNo: result.revision_no,
        lockVersion: result.customer_version,
        occurredAt: tsToIso(result.recorded_at),
        reference: `${result.customer_no}-R${result.revision_no}`,
    }
}

/** 判断提交错误是否属于结果未知。 */
function unknownFromError(
    error: ApiError,
    idempotencyKey: string,
): CustomerMutationResult | null {
    const message = apiErrorMessage(error)
    if (
        error.kind === "Network" ||
        (error.status != null && error.status >= 500)
    ) {
        return { outcome: "unknown", message, idempotencyKey }
    }
    return null
}

/** 从服务端当前资料构造乐观锁冲突结果。 */
async function conflictResult(
    error: ApiError,
    customerId: string,
    fallback: Pick<
        SaveCustomerDetailsInput,
        "expectedLockVersion" | "legalName" | "shortName" | "unifiedCreditCode"
    >,
): Promise<CustomerMutationResult> {
    try {
        const current = await apiGet<BackendCustomerProfile>(
            `/admin/customer-profiles/${customerId}`,
        )
        return {
            outcome: "conflict",
            message: apiErrorMessage(error),
            serverLockVersion: current.version,
            serverRevisionNo: current.current_revision.revision_no,
            serverLegalName: current.current_revision.legal_name,
            serverShortName: current.current_revision.short_name ?? undefined,
            serverUnifiedCreditCode: current.unified_credit_code ?? undefined,
            actor: "系统",
            changedAt: tsToIso(current.updated_at),
        }
    } catch {
        return {
            outcome: "conflict",
            message: apiErrorMessage(error),
            serverLockVersion: fallback.expectedLockVersion,
            serverRevisionNo: 0,
            serverLegalName: fallback.legalName,
            serverShortName: fallback.shortName,
            serverUnifiedCreditCode: fallback.unifiedCreditCode,
            actor: "系统",
            changedAt: new Date().toISOString(),
        }
    }
}

/** 原子创建完整客户资料。 */
export async function createCustomer(
    input: CreateCustomerInput,
): Promise<CustomerMutationResult> {
    const request: BackendProfileRequest = {
        idempotency_key: input.idempotencyKey,
        legal_name: input.legalName.trim(),
        short_name: input.shortName?.trim() || undefined,
        unified_credit_code: input.unifiedCreditCode?.trim() || undefined,
        default_payment_term_id: input.defaultPaymentTerm?.trim() || undefined,
        status: input.status ?? "active",
        owner_user_id: input.ownerUserId,
        contacts: input.contacts?.map(mapContactInput),
        addresses: input.addresses?.map(mapAddressInput),
        bank_accounts: input.bankAccounts?.map(mapBankInput),
        effective_from: todayBusinessDate(),
        change_reason: "首版建档",
    }
    try {
        return mapMutationResult(
            await apiPost<BackendProfileMutation>(
                "/admin/customer-profiles",
                request,
            ),
        )
    } catch (error) {
        if (isApiError(error)) {
            const unknown = unknownFromError(error, input.idempotencyKey)
            if (unknown) return unknown
            if (error.status === 409) {
                return {
                    outcome: "conflict",
                    message: apiErrorMessage(error),
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

/** 原子保存客户身份、客户角色和显式提交的从属事实集合。 */
export async function saveCustomerDetails(
    input: SaveCustomerDetailsInput,
): Promise<CustomerMutationResult> {
    const request: BackendProfileRequest = {
        idempotency_key: input.idempotencyKey,
        expected_party_version: input.expectedPartyVersion,
        expected_customer_version: input.expectedLockVersion,
        legal_name: input.legalName.trim(),
        short_name: input.shortName,
        unified_credit_code: input.unifiedCreditCode,
        default_payment_term_id: input.defaultPaymentTerm,
        status: input.status,
        contacts: input.contacts?.map(mapContactInput),
        addresses: input.addresses?.map(mapAddressInput),
        bank_accounts: input.bankAccounts?.map(mapBankInput),
        effective_from: todayBusinessDate(),
        change_reason: input.changeReason.trim(),
    }
    try {
        return mapMutationResult(
            await apiPut<BackendProfileMutation>(
                `/admin/customer-profiles/${input.customerId}`,
                request,
            ),
        )
    } catch (error) {
        if (isApiError(error)) {
            const unknown = unknownFromError(error, input.idempotencyKey)
            if (unknown) return unknown
            if (error.status === 409) {
                return conflictResult(error, input.customerId, input)
            }
        }
        throw error
    }
}

/** 按原幂等键查询已经提交成功的最终结果。 */
export async function queryCustomerMutationByIdempotency(
    idempotencyKey: string,
): Promise<CustomerMutationResult | null> {
    const result = await apiGet<BackendProfileMutation | null>(
        `/admin/customer-profile-commands/${encodeURIComponent(idempotencyKey)}`,
    )
    return result ? mapMutationResult(result) : null
}

/** 使用短时字段令牌揭示单个敏感值。 */
export async function revealCustomerSensitiveField(
    revealToken: string,
): Promise<string> {
    const result = await apiPost<{ value: string }>(
        "/admin/customer-sensitive-fields/reveal",
        { reveal_token: revealToken },
    )
    return result.value
}
