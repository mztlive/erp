import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api"
import {
    fetchCustomerQuality,
    fetchCustomerQualityPeriodPolicy,
} from "@/features/customer-quality/api"
import type { CustomerCenterView } from "@/features/customers/types"
import { isApiError } from "./errors"
import {
    mapAddress,
    mapAssignment,
    mapBank,
    mapContact,
    mapContractSummary,
    mapCustomerStatus,
    mapSalesOrderSummary,
    receivableProjection,
    sensitiveIndex,
    tsToIso,
} from "./mappers"
import type {
    BackendContractListRow,
    BackendCustomerProfile,
    BackendReceivableAccount,
    BackendSalesOrderListRow,
} from "./wire-types"

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
