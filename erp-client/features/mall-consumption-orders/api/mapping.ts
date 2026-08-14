/**
 * W25 商城消费订单 · 后端 snake_case → 前端 camelCase 映射（列表与共享部分）。
 * 详情映射见 detail-mapping.ts。适配仅发生在本目录 api 模块内。
 */

import type {
    AttributionStatus,
    CostBasis,
    DataSource,
    FactType,
    FulfillmentChain,
    MallConsumptionOrderListQuery,
    MallConsumptionOrderMetric,
    MallConsumptionOrderRow,
    ProcessingStatus,
} from "@/features/mall-consumption-orders/types"
import {
    ATTRIBUTION_STATUS_LABEL,
    COST_BASIS_LABEL,
    DATA_SOURCE_LABEL,
    FACT_TYPE_LABEL,
    FULFILLMENT_CHAIN_LABEL,
    SUPPLIER_STATUS_LABEL,
} from "@/features/mall-consumption-orders/types"
import type { BackendListRow } from "./wire-types"

export function tsToIso(secs: number | null | undefined): string {
    if (secs == null || !Number.isFinite(secs) || secs <= 0) return ""
    return new Date(secs * 1000).toISOString()
}

export function dateToUnixStart(value?: string): number | undefined {
    if (!value) return undefined
    if (/^\d{4}-\d{2}-\d{2}$/.test(value)) {
        return Math.floor(new Date(`${value}T00:00:00+08:00`).getTime() / 1000)
    }
    const t = Math.floor(new Date(value).getTime() / 1000)
    return Number.isFinite(t) ? t : undefined
}

export function dateToUnixEnd(value?: string): number | undefined {
    if (!value) return undefined
    if (/^\d{4}-\d{2}-\d{2}$/.test(value)) {
        return Math.floor(new Date(`${value}T23:59:59+08:00`).getTime() / 1000)
    }
    const t = Math.floor(new Date(value).getTime() / 1000)
    return Number.isFinite(t) ? t : undefined
}

export function mapAttribution(raw: string): AttributionStatus {
    switch (raw) {
        case "attributed":
        case "ATTRIBUTED":
            return "ATTRIBUTED"
        case "difference":
        case "DIFFERENCE":
            return "DIFFERENCE"
        case "pending_attribution":
        case "PENDING":
        case "PENDING_ATTRIBUTION":
        default:
            return "PENDING"
    }
}

export function attributionToBackend(status: AttributionStatus): string {
    switch (status) {
        case "ATTRIBUTED":
            return "attributed"
        case "DIFFERENCE":
            return "difference"
        case "PENDING":
        default:
            return "pending_attribution"
    }
}

export function mapFulfillmentChain(raw: string): FulfillmentChain {
    if (raw === "ERP_AUTOMATED" || raw === "erp_automated")
        return "ERP_AUTOMATED"
    return "LEGACY_MANUAL"
}

export function mapDataSource(raw: string): DataSource {
    if (
        raw === "history_backfill" ||
        raw === "BACKFILL" ||
        raw === "HISTORY_BACKFILL"
    )
        return "BACKFILL"
    if (raw === "mixed" || raw === "MIXED") return "MIXED"
    return "REALTIME"
}

export function mapDataSourceWire(raw: string): "REALTIME" | "BACKFILL" {
    return mapDataSource(raw) === "BACKFILL" ? "BACKFILL" : "REALTIME"
}

export function mapFactType(raw: string): FactType {
    const u = raw.toUpperCase()
    if (u === "ORDER_CANCELED" || u === "ORDER_CANCELLED")
        return "ORDER_CANCELED"
    if (u === "REFUND_SUCCEEDED") return "REFUND_SUCCEEDED"
    if (u === "ORDER_COMPLETED") return "ORDER_COMPLETED"
    if (u === "CARD_BALANCE_RESTORED") return "CARD_BALANCE_RESTORED"
    return "PAYMENT_SUCCEEDED"
}

export function mapProcessingStatus(raw: string): ProcessingStatus {
    switch (raw) {
        case "saved":
            return "SAVED"
        case "pending_attribution":
            return "PENDING_ATTRIBUTION"
        case "attributed":
            return "ATTRIBUTED"
        case "difference":
            return "DIFFERENCE"
        case "rejected":
            return "REJECTED"
        default:
            return "SAVED"
    }
}

export function mapCostBasis(raw: string): CostBasis {
    const u = raw.toUpperCase()
    if (u === "ACTUAL") return "ACTUAL"
    if (u === "STANDARD") return "STANDARD"
    return "NONE"
}

export function mapListRow(row: BackendListRow): MallConsumptionOrderRow {
    const attributionStatus = mapAttribution(row.attribution_status)
    const chain = mapFulfillmentChain(row.fulfillment_chain)
    const costBasisBreakdown = (row.cost_basis_breakdown ?? []).map((b) => ({
        basis: mapCostBasis(b.basis),
        lineCount: b.line_count,
        costAmount: b.cost_amount ?? undefined,
    }))
    const normalized = row.normalized_cost_basis
        ? row.normalized_cost_basis === "MIXED"
            ? ("MIXED" as const)
            : mapCostBasis(row.normalized_cost_basis)
        : undefined

    return {
        mallOrderId: row.mall_order_id,
        mallId: row.mall_id,
        mallName: row.mall_name || row.mall_id,
        externalOrderNo: row.external_order_no,
        customerId: row.customer_id ?? undefined,
        customerLabel: row.customer_label ?? row.customer_id ?? "—",
        paidAt: tsToIso(row.paid_at),
        paidAmount: row.paid_amount,
        paymentComposition: {
            cardAmount: row.payment_composition?.card_amount ?? "0.00",
            wechatAmount: row.payment_composition?.wechat_amount ?? "0.00",
            sourceCount: row.payment_composition?.source_count ?? 0,
        },
        factSummary: (row.fact_summary ?? []).map((f) => ({
            factType: mapFactType(f.fact_type),
            latestOccurredAt: tsToIso(f.latest_occurred_at),
            count: f.count,
        })),
        fulfillmentChain: chain,
        supplierOrderSummary: {
            total: row.supplier_order_summary?.total ?? 0,
            statuses: row.supplier_order_summary?.statuses ?? [],
            hasException: row.supplier_order_summary?.has_exception ?? false,
        },
        attributionStatus,
        costBasisBreakdown,
        dataSource: mapDataSource(row.data_source),
        allowedActions: row.allowed_actions?.length
            ? row.allowed_actions
            : ["OPEN_CENTER", "EXPORT"],
        actionBlockers: (row.action_blockers ?? []).map((message) => ({
            action: "UNKNOWN",
            code: "BACKEND",
            message,
        })),
        costBasisPolicyState:
            row.cost_basis_policy_state === "UNCONFIGURED"
                ? "UNCONFIGURED"
                : "CONFIGURED",
        normalizedCostBasis: normalized,
    }
}

export function emptyMetrics(): MallConsumptionOrderMetric[] {
    return [
        { key: "paid", label: "支付成功", value: 0, detail: "有支付记录" },
        { key: "pending_attr", label: "待归集", value: 0 },
        { key: "fact_diff", label: "记录差异", value: 0 },
        { key: "auto_exception", label: "自动履约异常", value: 0 },
        { key: "cost_none", label: "成本未覆盖", value: 0 },
    ]
}

export function filterSummary(
    query: MallConsumptionOrderListQuery,
    total: number,
): string {
    const parts: string[] = []
    if (query.metric && query.metric !== "all") {
        const labels: Record<string, string> = {
            paid: "支付成功",
            pending_attr: "待归集",
            fact_diff: "记录差异",
            auto_exception: "自动履约异常",
            cost_none: "成本未覆盖",
        }
        parts.push(labels[query.metric] ?? query.metric)
    }
    if (query.mallIds?.length) parts.push(`商城 ${query.mallIds.join("/")}`)
    if (query.fulfillmentChains?.length) {
        parts.push(
            query.fulfillmentChains
                .map((c) => FULFILLMENT_CHAIN_LABEL[c])
                .join("/"),
        )
    }
    if (query.attributionStatuses?.length) {
        parts.push(
            query.attributionStatuses
                .map((s) => ATTRIBUTION_STATUS_LABEL[s])
                .join("/"),
        )
    }
    if (query.occurredFrom || query.occurredTo) {
        parts.push(
            `记录发生 ${query.occurredFrom ?? "…"} ~ ${query.occurredTo ?? "…"}`,
        )
    }
    if (query.factTypes?.length) {
        parts.push(query.factTypes.map((t) => FACT_TYPE_LABEL[t]).join("/"))
    }
    if (query.supplierStatuses?.length) {
        parts.push(
            query.supplierStatuses
                .map((s) => SUPPLIER_STATUS_LABEL[s] ?? s)
                .join("/"),
        )
    }
    if (query.dataSources?.length) {
        parts.push(query.dataSources.map((d) => DATA_SOURCE_LABEL[d]).join("/"))
    }
    if (query.costBases?.length) {
        parts.push(query.costBases.map((b) => COST_BASIS_LABEL[b]).join("/"))
    }
    if (query.q?.trim()) parts.push(`搜索「${query.q.trim()}」`)
    parts.push(`${total} 条`)
    return parts.join(" · ")
}
