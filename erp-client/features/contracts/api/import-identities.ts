import { apiGet, createApiError } from "@/lib/api"
import { fetchSelectorList } from "@/lib/selector-list"
import type { SelectorPage } from "@/lib/selector-list"
import type { Company } from "@/features/companies/api"
import type { CustomerComboboxItem } from "@/components/business/entity-comboboxes"

export type ImportCustomer = CustomerComboboxItem & { creditCode: string }
type CustomerIdentity = {
    id: string
    customer_no: string
    status: string
    party_status: string
    unified_credit_code?: string | null
    current_revision: { legal_name: string }
}
type CustomerRow = { id: string; legal_name?: string | null }
type Identity = { name: string; code: string }
export type ImportIdentityMatches = {
    customer?: ImportCustomer
    customerCandidates: ImportCustomer[]
    hasAvailableCustomers: boolean
    company?: Company
    customerMessage: string
    companyMessage: string
}

/** 只检查当前可选范围是否为空，不把关键词零命中当作客户不存在。 */
async function hasAvailableCustomers(): Promise<boolean> {
    const page = await apiGet<SelectorPage<CustomerRow>>("/admin/customers", {
        scope: "assigned",
        status: "active",
        page: 1,
        page_size: 1,
    })
    if (page.empty_reason === "no_scope")
        throw createApiError({
            kind: "Validation",
            message: "当前没有可查询的客户范围，请先配置客户归属后重新匹配。",
        })
    if (
        !Number.isSafeInteger(page.total) ||
        page.total < 0 ||
        (page.total === 0 && page.items.length > 0) ||
        (page.total > 0 && page.items.length === 0)
    )
        throw createApiError({
            kind: "Parse",
            message: "客户列表结果不完整，请重新匹配。",
        })
    return page.total > 0
}

export async function fetchImportCustomer(id: string): Promise<ImportCustomer> {
    const row = await apiGet<CustomerIdentity>(
        `/admin/customer-profiles/${encodeURIComponent(id)}`,
    )
    const active = row.status === "active" && row.party_status === "active"
    return {
        id: row.id,
        customerNo: row.customer_no,
        legalName: row.current_revision.legal_name,
        creditCode: row.unified_credit_code?.trim() ?? "",
        statusLabel: active ? "启用" : "停用",
        statusTone: active ? "success" : "neutral",
    }
}

function matching<T>(
    identity: Identity,
    rows: readonly T[],
    name: (row: T) => string,
    code: (row: T) => string,
) {
    const legalName = identity.name.trim()
    const creditCode = identity.code.trim().toUpperCase()
    if (!legalName && !creditCode) return []
    return rows.filter(
        (row) =>
            (!legalName || name(row).trim() === legalName) &&
            (!creditCode || code(row).trim().toUpperCase() === creditCode),
    )
}

export async function matchImportIdentities(input: {
    customer: Identity
    company: Identity
    expectedCustomerId?: string
}): Promise<ImportIdentityMatches> {
    const customerWords = [
        input.customer.name.trim(),
        input.customer.code.trim(),
    ].filter(Boolean)
    const companyWords = [
        input.company.name.trim(),
        input.company.code.trim(),
    ].filter(Boolean)
    const [customerPages, companyPages] = await Promise.all([
        input.expectedCustomerId
            ? Promise.resolve([])
            : Promise.all(
                  [...new Set(customerWords)].map((keyword) =>
                      fetchSelectorList<CustomerRow>("/admin/customers", {
                          scope: "assigned",
                          keyword,
                          status: "active",
                      }),
                  ),
              ),
        Promise.all(
            [...new Set(companyWords)].map((keyword) =>
                fetchSelectorList<Company>("/admin/companies", {
                    keyword,
                    status: "active",
                }),
            ),
        ),
    ])
    if (customerPages.some((page) => page.empty_reason === "no_scope"))
        throw createApiError({
            kind: "Validation",
            message: "当前没有可查询的客户范围，请先配置客户归属后重新匹配。",
        })
    const customerIds = input.expectedCustomerId
        ? [input.expectedCustomerId]
        : [
              ...new Set(
                  customerPages.flatMap((page) =>
                      page.items.map((row) => row.id),
                  ),
              ),
          ]
    const customers = (
        await Promise.all(customerIds.map(fetchImportCustomer))
    ).filter((row) => row.statusTone === "success")
    const companies = [
        ...new Map(
            companyPages
                .flatMap((page) => page.items)
                .map((row) => [row.id, row]),
        ).values(),
    ]
    const customerMatches = matching(
        input.customer,
        customers,
        (row) => row.legalName,
        (row) => row.creditCode,
    )
    const companyMatches = matching(
        input.company,
        companies,
        (row) => row.legal_name,
        (row) => row.unified_credit_code ?? "",
    )
    return {
        customer: customerMatches.length === 1 ? customerMatches[0] : undefined,
        customerCandidates:
            customerMatches.length === 1 || input.expectedCustomerId
                ? []
                : customers.slice(0, 5),
        hasAvailableCustomers:
            Boolean(input.expectedCustomerId) ||
            customers.length > 0 ||
            (await hasAvailableCustomers()),
        company: companyMatches.length === 1 ? companyMatches[0] : undefined,
        customerMessage:
            customerMatches.length === 1
                ? "已匹配系统客户，请核对签约名称和信用代码"
                : input.expectedCustomerId
                  ? "识别出的对方主体与当前客户不一致，请对照原文修正；不能更换客户"
                  : customerMatches.length > 1
                    ? "找到多个同名记录，请核对编号和信用代码后选择"
                    : "未找到完全匹配的客户，请手动选择或新建客户",
        companyMessage:
            companyMatches.length === 1
                ? "已匹配我方签约主体"
                : companyMatches.length > 1
                  ? "找到多个签约主体，请核对信用代码后选择"
                  : "未匹配到我方签约主体，请从系统公司中选择",
    }
}
