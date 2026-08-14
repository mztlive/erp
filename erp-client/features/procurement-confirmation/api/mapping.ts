/** 后端线格式 → W07 页面契约的纯映射函数。 */

import type { QueueFilters } from "./filters"
import type {
    BackendConfirmationLine,
    BackendRecommendation,
} from "./backend-types"
import type {
    ConfirmationLineDraft,
    CoverageByLine,
    FulfillmentMode,
    ProcurementConfirmationTask,
    ProcurementRecommendation,
} from "@/features/procurement-confirmation/types"

export function secsToIso(secs?: number | null): string {
    if (secs == null || secs <= 0) return ""
    return new Date(secs * 1000).toISOString()
}

export function priorityToNumber(p?: string | number | null): number {
    if (typeof p === "number") return p
    switch (p) {
        case "urgent":
            return 100
        case "high":
            return 80
        case "normal":
            return 50
        case "low":
            return 20
        default:
            return 50
    }
}

export function mapFulfillmentMode(mode: string): FulfillmentMode {
    if (mode === "COMPANY_WAREHOUSE") return "WAREHOUSE"
    if (mode === "ELECTRONIC_DELIVERY") return "ELECTRONIC"
    if (mode === "OFFLINE_SERVICE") return "SERVICE"
    if (
        mode === "WAREHOUSE" ||
        mode === "SUPPLIER_DIRECT" ||
        mode === "ELECTRONIC" ||
        mode === "SERVICE"
    ) {
        return mode
    }
    return "WAREHOUSE"
}

/** 将后端最低成本方案转换为 W07 页面契约。 */
export function mapRecommendation(
    recommendation: BackendRecommendation,
): ProcurementRecommendation {
    const mapIssue = (
        issue: BackendRecommendation["blocking_issues"][number],
    ) => ({
        code: issue.code,
        message: issue.message,
        lineId: issue.sales_order_submission_line_id ?? undefined,
    })
    return {
        confirmationId: recommendation.confirmation_id,
        policyVersion: recommendation.policy_version,
        calculatedAt: secsToIso(recommendation.calculated_at),
        ready: recommendation.ready,
        lines: recommendation.lines.map((line) => ({
            lineKey: `recommended-${line.line_no}-${line.supplier_offering_revision_id}`,
            submissionLineId: line.sales_order_submission_line_id,
            supplierId: line.supplier_id,
            supplierName: line.supplier_name,
            offeringRevisionId: line.supplier_offering_revision_id,
            confirmedQuantity: line.confirmed_quantity,
            latestCostGross: line.latest_cost_gross,
            inputTaxRate: line.input_tax_rate,
            expectedDeliveryDate: line.expected_delivery_date,
            fulfillmentMode: mapFulfillmentMode(line.fulfillment_mode),
            capabilityRevisionId: line.supplier_capability_revision_id,
            capabilitySummary: "当前有效供应商能力",
            qualificationStatus: "VALID" as const,
            itemName: line.item_name,
            itemSku: line.sku_id,
            landedGross: line.landed_gross,
            freightAmount: line.freight_amount ?? undefined,
            serviceFeeAmount: line.service_fee_amount ?? undefined,
            recommendationReason: line.recommendation_reason,
        })),
        purchaseOrders: recommendation.purchase_orders.map((order) => ({
            supplierId: order.supplier_id,
            supplierName: order.supplier_name,
            fulfillmentMode: mapFulfillmentMode(order.fulfillment_mode),
            lineCount: order.line_count,
            estimatedGross: order.estimated_gross,
        })),
        estimatedPurchaseGross: recommendation.estimated_purchase_gross,
        salesGross: recommendation.sales_gross,
        estimatedGrossMargin: recommendation.estimated_gross_margin,
        blockingIssues: recommendation.blocking_issues.map(mapIssue),
        warnings: recommendation.warnings.map(mapIssue),
    }
}

export function filterSummary(filters: QueueFilters): string {
    const parts = [
        filters.scope === "mine" ? "仅我的" : "团队",
        filters.due === "overdue"
            ? "已超期"
            : filters.due === "today"
              ? "今日到期"
              : "有效全部",
        filters.sort === "priority"
            ? "优先级"
            : filters.sort === "submitted_at"
              ? "提交时间"
              : "截止优先",
    ]
    if (filters.orderNo) parts.push(`单号 ${filters.orderNo}`)
    return parts.join(" · ")
}

export function emptyCoverageFromLines(
    lines: readonly ConfirmationLineDraft[],
    submissionLineIds: readonly {
        id: string
        name: string
        required: string
    }[],
): {
    coverageByLine: CoverageByLine[]
    estimatedPurchaseGross: string
    blockingIssues: ProcurementConfirmationTask["decisionSummary"]["blockingIssues"]
    warnings: ProcurementConfirmationTask["decisionSummary"]["warnings"]
} {
    // 覆盖/毛利由服务端裁决；此处仅做展示占位，不重算金额（P4 §2.7）。
    // 缺口：后端详情未返回 decisionSummary / coverageByLine。
    const coverageByLine: CoverageByLine[] = submissionLineIds.map((s) => {
        const confirmedLines = lines.filter((l) => l.submissionLineId === s.id)
        const confirmed =
            confirmedLines.length > 0
                ? confirmedLines.map((l) => l.confirmedQuantity).join("+") ||
                  "0"
                : "0"
        return {
            submissionLineId: s.id,
            itemName: s.name,
            confirmed,
            required: s.required,
            complete: confirmedLines.length > 0,
            gap: confirmedLines.length > 0 ? "0" : s.required,
        }
    })
    return {
        coverageByLine,
        estimatedPurchaseGross: "—",
        blockingIssues: [],
        warnings: [],
    }
}

export function mapConfirmationLines(
    lines: BackendConfirmationLine[],
): ConfirmationLineDraft[] {
    return lines.map((line) => ({
        lineKey: line.id,
        submissionLineId: line.sales_order_submission_line_id,
        supplierId: line.supplier_id,
        supplierName: "供应商名称加载中",
        offeringRevisionId: line.supplier_offering_revision_id ?? "",
        confirmedQuantity: String(line.confirmed_quantity ?? "0"),
        latestCostGross: String(line.latest_cost_gross ?? "0"),
        inputTaxRate: String(line.input_tax_rate ?? "0"),
        expectedDeliveryDate: line.expected_delivery_date ?? "",
        fulfillmentMode: mapFulfillmentMode(String(line.fulfillment_mode)),
        capabilityRevisionId: line.supplier_capability_revision_id ?? "",
        capabilitySummary: "",
        qualificationStatus: line.supplier_capability_revision_id
            ? ("VALID" as const)
            : ("INVALID" as const),
    }))
}
