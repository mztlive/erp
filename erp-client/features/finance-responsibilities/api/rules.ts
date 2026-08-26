import { apiGet, apiPost, apiPut } from "@/lib/api"
import type {
    FinanceResponsibilityOperation,
    FinanceResponsibilityOwnerOption,
    FinanceResponsibilityRule,
    FinanceResponsibilityScope,
    SaveFinanceResponsibilityRuleInput,
} from "@/features/finance-responsibilities/types"

const RULES_PATH = "/admin/finance-responsibility-rules"
const OWNER_OPTIONS_PATH = "/admin/finance-responsibility-owner-options"

type BackendRule = Readonly<{
    id: string
    operation: string
    scope: string
    counterparty_id?: string | null
    counterparty_no?: string | null
    owner_user_id: string
    owner_name?: string | null
    status: string
    version: number
}>

type BackendOwnerOption = Readonly<{
    user_id: string
    display_name: string
    account: string
    supplier_payment_eligible: boolean
    sales_invoice_eligible: boolean
}>

function operation(value: string): FinanceResponsibilityOperation {
    if (value === "SUPPLIER_PAYMENT" || value === "SALES_INVOICE") {
        return value
    }
    throw new Error(`服务端返回了未注册的财务责任操作：${value}`)
}

function scope(value: string): FinanceResponsibilityScope {
    if (value === "COUNTERPARTY" || value === "DEFAULT") return value
    throw new Error(`服务端返回了未注册的财务责任范围：${value}`)
}

function enabled(status: string): boolean {
    if (status === "active") return true
    if (status === "disabled") return false
    throw new Error(`服务端返回了未注册的财务责任状态：${status}`)
}

function mapRule(rule: BackendRule): FinanceResponsibilityRule {
    return {
        id: rule.id,
        operation: operation(rule.operation),
        scope: scope(rule.scope),
        counterpartyId: rule.counterparty_id ?? undefined,
        counterpartyNo: rule.counterparty_no ?? undefined,
        ownerUserId: rule.owner_user_id,
        ownerName: rule.owner_name?.trim() || "负责人账号不可用",
        enabled: enabled(rule.status),
        version: rule.version,
    }
}

function payload(input: SaveFinanceResponsibilityRuleInput) {
    return {
        operation: input.operation,
        scope: input.scope,
        counterparty_id:
            input.scope === "COUNTERPARTY" ? input.counterpartyId : undefined,
        owner_user_id: input.ownerUserId,
        status: input.enabled ? "active" : "disabled",
    }
}

export async function fetchFinanceResponsibilityRules(): Promise<
    readonly FinanceResponsibilityRule[]
> {
    const rows = await apiGet<readonly BackendRule[]>(RULES_PATH)
    return rows.map(mapRule)
}

export async function fetchFinanceResponsibilityOwnerOptions(): Promise<
    readonly FinanceResponsibilityOwnerOption[]
> {
    const rows = await apiGet<readonly BackendOwnerOption[]>(OWNER_OPTIONS_PATH)
    return rows.map((row) => ({
        userId: row.user_id,
        displayName: row.display_name,
        account: row.account,
        supplierPaymentEligible: row.supplier_payment_eligible,
        salesInvoiceEligible: row.sales_invoice_eligible,
    }))
}

export async function saveFinanceResponsibilityRule(
    input: SaveFinanceResponsibilityRuleInput,
): Promise<FinanceResponsibilityRule> {
    const body = input.id
        ? { ...payload(input), version: input.expectedVersion }
        : payload(input)
    const row = input.id
        ? await apiPut<BackendRule>(
              `${RULES_PATH}/${encodeURIComponent(input.id)}`,
              body,
          )
        : await apiPost<BackendRule>(RULES_PATH, body)
    return mapRule(row)
}
