import { apiGet } from "@/lib/api"
import { fetchSalesOrders } from "@/features/sales-orders/api/sales-orders-list"
import { fetchPurchaseOrders } from "@/features/purchase-orders/api/purchase-order-queries-api"
import { fetchInquiries } from "@/features/sales-inquiries/api"
import { INQUIRY_STATUS_LABEL } from "@/features/sales-inquiries/types"
import { fetchReceivableScopeList } from "@/features/customer-receivables/api/scoped-view"
import { fetchSupplierScopeList } from "@/features/supplier-payables/api/scoped-view"
import {
    fetchPeriodBasisConfig,
    fetchProfitLossView,
} from "@/features/actual-profit-loss/api"
import { decodeFulfillmentQueuePage } from "@/features/fulfillment-operations/api/work-queue"
import type { OverviewData, OverviewSource } from "../lib/overview"

/** 复用正式列表与汇总；金额不从分页条目求和，受限字段保持空值。 */
export async function fetchOverviewData(
    source: OverviewSource,
    page = 1,
    scopeVersion?: string,
    pageSize = 20,
): Promise<OverviewData> {
    if (source.kind === "sales") {
        const result = await fetchSalesOrders({
            ...source.query,
            page,
            pageSize,
            scopeVersion,
        })
        return {
            count: result.total,
            scopeVersion: result.scopeVersion,
            scopeSummary: result.scopeSummary,
            asOf: result.asOf,
            noScope: result.emptyReason === "no_scope",
            rows: result.items.map((row) => ({
                id: row.id,
                title: row.documentNumber,
                description: row.customerName,
                status: row.primaryStatus.label,
                amount: row.amountGross,
                href: `/sales/orders/${encodeURIComponent(row.id)}`,
            })),
        }
    }
    if (source.kind === "purchase") {
        const result = await fetchPurchaseOrders({
            ...source.query,
            page,
            pageSize,
            scopeVersion,
        })
        return {
            count: result.total,
            scopeVersion: result.scopeVersion,
            scopeSummary: result.scopeSummary,
            asOf: result.asOf,
            noScope: result.emptyReason === "no_scope",
            rows: result.rows.map((row) => ({
                id: row.purchaseOrderId,
                title: row.purchaseNo || row.draftLabel || "采购单",
                description: row.supplierName,
                status: row.statusLabel,
                amount: row.costMasked ? null : row.grossAmount,
                href: `/procurement/orders/${encodeURIComponent(row.purchaseOrderId)}`,
            })),
        }
    }
    if (source.kind === "inquiry") {
        const result = await fetchInquiries(
            {
                page,
                page_size: pageSize,
                scope_version: scopeVersion,
                status: source.status,
                task_status: source.taskStatus,
            },
            source.procurement,
        )
        return {
            count: result.total,
            scopeVersion: result.scope_version,
            rows: result.items.map((row) => ({
                id: row.id,
                title: row.inquiry_no,
                description: row.customer_name,
                status: INQUIRY_STATUS_LABEL[row.status],
                href: `/${source.procurement ? "procurement" : "sales"}/inquiries/${encodeURIComponent(row.id)}`,
            })),
        }
    }
    if (source.kind === "receivable" || source.kind === "receipt") {
        const result = await fetchReceivableScopeList({
            view: source.kind,
            page,
            pageSize,
            scopeVersion,
        })
        return {
            count: result.total,
            balance: result.summary?.balance_total,
            overdue: result.summary?.overdue_total,
            limited: result.summary?.permission_limited,
            noScope: !result.hasScope,
            scopeVersion: result.scopeVersion,
            scopeSummary: result.scopeSummary,
            asOf: result.asOf,
            rows:
                source.kind === "receivable"
                    ? result.receivables.map((row) => ({
                          id: row.id,
                          title:
                              row.sales_order_no ||
                              `应收子账 ${row.account_seq}`,
                          description: "应收未收余额",
                          amount: row.open_total,
                          href: `/finance/customer-accounts?view=receivable&previewKind=receivable&previewId=${encodeURIComponent(row.id)}`,
                      }))
                    : result.receipts.map((row) => ({
                          id: row.id,
                          title: row.receipt_no,
                          description: "回款未分配余额",
                          amount: row.unallocated_amount,
                          href: `/finance/customer-accounts?view=receipt&previewKind=receipt&previewId=${encodeURIComponent(row.id)}`,
                      })),
        }
    }
    if (source.kind === "payable" || source.kind === "payment") {
        const result = await fetchSupplierScopeList({
            view: source.kind,
            page,
            pageSize,
            scopeVersion,
        })
        return {
            count: result.total,
            balance: result.summary?.balance_total,
            limited: result.summary?.permission_limited,
            noScope: !result.hasScope,
            scopeVersion: result.scopeVersion,
            scopeSummary: result.scopeSummary,
            asOf: result.asOf,
            rows:
                source.kind === "payable"
                    ? result.payables.map((row) => ({
                          id: row.id,
                          title: row.source_document_no || "应付账款",
                          description: "应付未付余额",
                          amount: row.open_total,
                          href: `/finance/supplier-accounts?view=payable&previewKind=payable&detailId=${encodeURIComponent(row.id)}`,
                      }))
                    : result.payments.map((row) => ({
                          id: row.id,
                          title: row.payment_no,
                          description: "付款未分配余额",
                          amount: row.unallocated_amount,
                          href: `/finance/supplier-accounts?view=payment&previewKind=payment&detailId=${encodeURIComponent(row.id)}`,
                      })),
        }
    }
    if (source.kind === "fulfillment") {
        const result = decodeFulfillmentQueuePage(
            await apiGet<unknown>("/admin/work-items/fulfillment-queue", {
                operation_types: source.operationType,
                due: source.due,
                page,
                page_size: pageSize,
                scope_version: scopeVersion,
                timezone: "Asia/Shanghai",
            }),
        )
        return {
            count: result.total,
            scopeVersion: result.scope_version,
            scopeSummary: "我的履约责任",
            asOf: new Date(result.as_of * 1000).toISOString(),
            rows: result.items.map((row) => ({
                id: row.work_item_id,
                title:
                    row.sales_order_no || row.purchase_order_no || "履约事项",
                description: row.summary,
                taskId: row.work_item_id,
            })),
        }
    }
    if (source.kind !== "profit") throw new Error("业务指标来源不受支持")
    const config = await fetchPeriodBasisConfig()
    if (!config.configuredPeriodBasis)
        return {
            count: 0,
            rows: [],
            note: "尚未配置经营分析期间口径",
            noScope: false,
        }
    const result = await fetchProfitLossView({
        from: source.from,
        to: source.to,
        periodBasis: config.configuredPeriodBasis,
        scopeId: "authorized",
        coverage: "all",
        dimension: "sales_order",
        sort: "actualProfitLossNet:asc",
        page,
        pageSize,
        scopeVersion,
    })
    const unavailable = result.coverage.reliability === "unavailable"
    return {
        count: result.rows.total,
        scopeVersion: result.scopeVersion,
        scopeSummary: result.scope.label,
        asOf: result.freshness.projectedAt,
        noScope: result.emptyReason === "no_scope",
        limited: result.coverage.reliability !== "reliable",
        revenue: result.fieldPermissions.canViewRevenue
            ? result.totals.netSalesRevenue
            : null,
        cost: result.fieldPermissions.canViewCost
            ? result.totals.actualProcurementCostNet
            : null,
        fulfillmentCost: result.fieldPermissions.canViewCost
            ? result.totals.actualFulfillmentCostNet
            : null,
        profit:
            result.fieldPermissions.canViewProfit && !unavailable
                ? result.totals.actualProfitLossNet
                : null,
        note: [
            result.period.basisLabel,
            result.businessTypeLabel,
            result.amountBasisLabel,
            result.coverage.reliability === "partial"
                ? "成本部分覆盖，盈亏仅代表可靠子集"
                : result.totals.marginUnavailableReason,
            result.freshness.state !== "fresh" ? "分析数据待更新" : undefined,
        ]
            .filter(Boolean)
            .join(" · "),
        trend: result.fieldPermissions.canViewProfit
            ? result.trend.map((point) => ({
                  label: point.period,
                  value:
                      point.reliability === "unavailable"
                          ? null
                          : (point.actualProfitLossNet ?? null),
              }))
            : [],
        rows: result.rows.items.map((row) => ({
            id: row.rowId,
            title: row.identityLabel,
            description: row.customerLabel || "",
            amounts: {
                revenue: result.fieldPermissions.canViewRevenue
                    ? row.netSalesRevenue
                    : null,
                cost: result.fieldPermissions.canViewCost
                    ? row.actualProcurementCostNet
                    : null,
                fulfillmentCost: result.fieldPermissions.canViewCost
                    ? row.actualFulfillmentCostNet
                    : null,
                profit:
                    result.fieldPermissions.canViewProfit &&
                    row.coverageState === "COVERED"
                        ? row.actualProfitLossNet
                        : null,
            },
            amount:
                result.fieldPermissions.canViewProfit &&
                row.coverageState === "COVERED"
                    ? row.actualProfitLossNet
                    : null,
            href:
                row.objectId && row.allowedDrilldowns.includes("W05")
                    ? `/sales/orders/${encodeURIComponent(row.objectId)}`
                    : undefined,
        })),
    }
}

/** 六个月各自请求服务端总数，不以样本推算趋势。 */
export async function fetchSalesCountTrend(to: string) {
    const [year, month] = to.split("-").map(Number)
    const points = []
    // 两个并发一批，避免同时发出六个重型列表查询。
    for (let offset = 5; offset >= 0; offset -= 2) {
        const batch = await Promise.all(
            [offset, offset - 1].map(async (index) => {
                const start = new Date(
                    Date.UTC(year!, month! - 1 - index, 1) - 8 * 3600_000,
                )
                const next = new Date(
                    Date.UTC(year!, month! - index, 1) - 8 * 3600_000,
                )
                const end = Math.min(
                    next.getTime() - 1000,
                    Date.parse(`${to}T23:59:59+08:00`),
                )
                const result = await apiGet<{
                    total: number
                    empty_reason?: string
                }>("/admin/sales-orders", {
                    page: 1,
                    page_size: 1,
                    created_from: start.getTime() / 1000,
                    created_to: end / 1000,
                })
                return {
                    label: new Intl.DateTimeFormat("zh-CN", {
                        month: "short",
                        timeZone: "Asia/Shanghai",
                    }).format(start),
                    value:
                        result.empty_reason === "no_scope"
                            ? null
                            : result.total,
                }
            }),
        )
        points.push(...batch)
    }
    return { points }
}
