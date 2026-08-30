import { apiGet } from "@/lib/api"
import {
    fetchCustomerQuality,
    fetchCustomerQualityPeriodPolicy,
} from "@/features/customer-quality/api"
import type { CustomerCenterView } from "@/features/customers/types"
import { compareDecimal } from "@/lib/fixed-decimal"
import {
    decodeCustomerCenterReceivable,
    decodeCustomerCenterRelated,
} from "./center-read-model"
import { isApiError } from "./errors"
import {
    mapAddress,
    mapAssignment,
    mapBank,
    mapContact,
    mapContractSummary,
    mapCustomerStatus,
    mapSalesOrderSummary,
    sensitiveIndex,
    tsToIso,
} from "./mappers"
import type { BackendCustomerProfile } from "./wire-types"

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

    const [relatedResult, receivableResult, qualityResult] =
        await Promise.allSettled([
            apiGet<unknown>(
                `/admin/customer-profiles/${encodeURIComponent(customerId)}/related-summary`,
            ).then(decodeCustomerCenterRelated),
            apiGet<unknown>(
                `/admin/customer-profiles/${encodeURIComponent(customerId)}/receivable-summary`,
            ).then(decodeCustomerCenterReceivable),
            loadCustomerQualitySummary(customerId),
        ])

    const fields = sensitiveIndex(profile.sensitive_fields)
    const contractRows =
        relatedResult.status === "fulfilled"
            ? relatedResult.value.contracts
            : []
    const salesOrderRows =
        relatedResult.status === "fulfilled"
            ? relatedResult.value.sales_orders
            : []
    const contracts = contractRows.map(mapContractSummary)
    const salesOrders = salesOrderRows.map(mapSalesOrderSummary)
    const receivableSummary =
        receivableResult.status === "fulfilled"
            ? {
                  receivableBalance: receivableResult.value.receivable_balance,
                  overdueAmount: receivableResult.value.overdue_amount,
                  earliestOverdueDate:
                      receivableResult.value.earliest_overdue_date ?? undefined,
                  collectionProgressLabel:
                      compareDecimal(
                          receivableResult.value.receivable_balance,
                          "0",
                          2,
                      ) === 0
                          ? "已结清"
                          : "存在未结清余额",
                  invoicingProgressLabel:
                      compareDecimal(
                          receivableResult.value.open_invoiceable_total,
                          "0",
                          2,
                      ) === 0
                          ? "已完成"
                          : "存在可开票余额",
              }
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
                relatedResult.status === "fulfilled"
                    ? relatedResult.value.active_contract_count
                    : null,
            inProgressSalesOrderCount:
                relatedResult.status === "fulfilled"
                    ? relatedResult.value.in_progress_sales_order_count
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
            related: relatedResult.status === "fulfilled" ? "ok" : "error",
            settlement:
                receivableResult.status === "fulfilled" ? "ok" : "error",
            quality: qualityResult.status === "fulfilled" ? "ok" : "error",
            audit: "ok",
        },
    }
}
