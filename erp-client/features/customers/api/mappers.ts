/**
 * Wire DTO → 页面视图的映射与展示语义转换。
 * 仅限 api/ 内部使用；不对外导出（公共请求函数见 index.ts）。
 */

import type {
    CreateCustomerAddressInput,
    CreateCustomerBankAccountInput,
    CreateCustomerContactInput,
    CustomerAddressView,
    CustomerAssignmentView,
    CustomerBankAccountView,
    CustomerContactView,
    CustomerDirectoryItem,
    CustomerMutationResult,
    CustomerStatus,
    RelatedObjectSummary,
} from "@/features/customers/types"
import {
    CONTRACT_STATUS_LABEL,
    CONTRACT_STATUS_TONE,
    type ContractStatus,
} from "@/features/contracts/types"
import {
    compareDecimal,
    formatScaled,
    normalizeFixed,
} from "@/lib/fixed-decimal"
import type {
    BackendAddress,
    BackendAssignment,
    BackendBankAccount,
    BackendContact,
    BackendContractListRow,
    BackendCustomerStatus,
    BackendCustomerView,
    BackendProfileMutation,
    BackendReceivableAccount,
    BackendSalesOrderListRow,
    BackendSensitiveField,
} from "./wire-types"

/** 返回本地业务日期。 */
export function todayBusinessDate(): string {
    const date = new Date()
    const pad = (value: number) => String(value).padStart(2, "0")
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

/** 秒级时间戳转 ISO 时间。 */
export function tsToIso(seconds: number | undefined | null): string {
    return seconds == null || !Number.isFinite(seconds)
        ? new Date().toISOString()
        : new Date(seconds * 1000).toISOString()
}

/** 映射客户状态及其展示语义。 */
export function mapCustomerStatus(status: BackendCustomerStatus | string): {
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
export function mapAddressTypeFromBackend(
    code: BackendAddress["address_type"],
): string {
    if (code === "registered") return "注册地址"
    if (code === "fulfillment") return "履约地址"
    return "经营地址"
}

/** 前端地址展示值转后端稳定代码。 */
export function mapAddressTypeToBackend(
    label: string,
): BackendAddress["address_type"] {
    if (label === "注册地址" || label === "registered") return "registered"
    if (label === "经营地址" || label === "operating") return "operating"
    return "fulfillment"
}

/** 返回后端尚未提供聚合投影时的显式不可用值。 */
export function emptyMetrics() {
    return {
        activeContractCount: null,
        inProgressSalesOrderCount: null,
        receivableBalance: null,
        overdueAmount: null,
    }
}

/** 映射目录行；目录接口已批量补齐主体与归属，不再逐行请求详情。 */
export function mapDirectoryItem(
    row: BackendCustomerView,
): CustomerDirectoryItem {
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

/** 以字段类型和事实 ID 索引敏感字段令牌。 */
export function sensitiveIndex(fields: BackendSensitiveField[]) {
    return new Map(
        fields.map((field) => [`${field.kind}:${field.record_id}`, field]),
    )
}

/** 映射联系人当前事实。 */
export function mapContact(
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
export function mapAddress(
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
export function mapBank(
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
export function mapAssignment(
    assignment: BackendAssignment,
): CustomerAssignmentView {
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

/** 映射合同摘要。 */
export function mapContractSummary(
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
export function mapSalesOrderSummary(
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
export function moneyCents(value: string): bigint {
    return BigInt(
        normalizeFixed(value, {
            maxScale: 2,
            outputScale: 2,
            allowNegative: true,
        }).replace(".", ""),
    )
}

/** 汇总客户应收当前余额与已逾期未核销分录。 */
export function receivableProjection(accounts: BackendReceivableAccount[]) {
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

/** 映射联系人根命令输入。 */
export function mapContactInput(input: CreateCustomerContactInput) {
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
export function mapAddressInput(input: CreateCustomerAddressInput) {
    return {
        existing_id: input.existingId,
        address_type: mapAddressTypeToBackend(input.addressType),
        contact_name: input.contactName?.trim() || undefined,
        address: input.address?.trim() || undefined,
        is_default: input.isDefault,
    }
}

/** 映射银行账户根命令输入。 */
export function mapBankInput(input: CreateCustomerBankAccountInput) {
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
export function mapMutationResult(
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
