/**
 * W06 客户验收 — 登记 / 冲正 / 草稿变更（mutationFn）。
 * 从 api/acceptance.ts 拆出；api/acceptance.ts 保持原导出名 re-export。
 */

import { apiGet, apiPost } from "@/lib/api"
import type { ApiError } from "@/lib/api/errors"
import type {
    PostAcceptanceInput,
    PostAcceptanceResult,
    ReverseAcceptanceInput,
    ReverseAcceptanceResult,
    SaveAcceptanceDraftInput,
} from "@/features/sales-orders/lib/acceptance-types"
import { FACT_ONLY_NOTICE } from "@/features/sales-orders/lib/acceptance-types"
import {
    mapOverallResult,
    mapOverallResultToBackend,
    type BackendAcceptanceDetail,
    type BackendAcceptanceHeader,
    type BackendEligibilityView,
} from "@/features/sales-orders/lib/acceptance-mappers"

export async function saveCustomerAcceptanceDraft(
    input: SaveAcceptanceDraftInput,
) {
    // 后端无独立「保存草稿」接口：POST 创建 DRAFT 验收单。
    // 已有 draft id 时无法局部更新（缺口），重新创建一笔草稿。
    const acceptanceNo =
        input.acceptanceDraftId && input.acceptanceDraftId.startsWith("YS")
            ? input.acceptanceDraftId
            : `YS${Date.now().toString(36).toUpperCase()}`

    const acceptedAtSecs = input.acceptedAt
        ? Math.floor(Date.parse(input.acceptedAt) / 1000) ||
          Math.floor(Date.now() / 1000)
        : Math.floor(Date.now() / 1000)

    const created = await apiPost<
        BackendAcceptanceDetail | BackendAcceptanceHeader
    >("/admin/customer-acceptances", {
        acceptance_no: acceptanceNo,
        sales_order_id: input.salesOrderId,
        accepted_at: acceptedAtSecs,
        result: mapOverallResultToBackend(input.lines),
        lines: input.lines.map((line) => ({
            sales_order_line_id: line.salesOrderLineId,
            accepted_quantity: line.acceptedQuantity || "0",
            short_quantity: line.shortQuantity || "0",
            rejected_quantity: line.rejectedQuantity || "0",
            reason: line.reason || null,
            allocations: line.allocations.map((a) => ({
                fulfillment_line_id: a.fulfillmentLineId,
                fulfillment_fact_type: "DELIVERY",
                allocated_quantity: a.allocatedQuantity || "0",
            })),
        })),
    })

    const header =
        "acceptance" in created && created.acceptance
            ? created.acceptance
            : (created as BackendAcceptanceHeader)

    return {
        acceptanceDraftId: header.id,
        draftVersion: header.version,
        salesOrderId: input.salesOrderId,
        acceptedAt: input.acceptedAt,
        comment: input.comment,
        lines: input.lines,
        updatedAt: new Date().toISOString(),
    }
}

export async function postCustomerAcceptanceWorkspace(
    input: PostAcceptanceInput,
): Promise<PostAcceptanceResult> {
    try {
        let acceptanceId = input.acceptanceDraftId

        // 若无服务端草稿 id，先创建
        if (!acceptanceId || acceptanceId.startsWith("draft_")) {
            const saved = await saveCustomerAcceptanceDraft({
                salesOrderId: input.salesOrderId,
                acceptedAt: input.acceptedAt,
                comment: input.comment,
                lines: input.lines,
            })
            acceptanceId = saved.acceptanceDraftId
        }

        // 解析履约事实类型：优先用工作台已加载的类型映射
        const eligibility = await apiGet<BackendEligibilityView>(
            "/admin/customer-acceptances/eligible",
            { sales_order_id: input.salesOrderId },
        ).catch(() => null)

        const factTypeByLineId = new Map<string, string>()
        for (const group of eligibility?.sales_lines ?? []) {
            for (const fact of group.fulfillment_facts ?? []) {
                factTypeByLineId.set(
                    fact.fulfillment_line_id,
                    fact.fulfillment_fact_type,
                )
            }
        }

        const posted = await apiPost<BackendAcceptanceDetail>(
            `/admin/customer-acceptances/${acceptanceId}/post`,
            {
                lines: input.lines.map((line) => ({
                    sales_order_line_id: line.salesOrderLineId,
                    allocations: line.allocations.map((a) => ({
                        fulfillment_line_id: a.fulfillmentLineId,
                        fulfillment_fact_type:
                            factTypeByLineId.get(a.fulfillmentLineId) ??
                            "DELIVERY",
                        allocated_quantity: a.allocatedQuantity || "0",
                    })),
                })),
            },
        )

        const header =
            posted.acceptance ?? (posted as unknown as BackendAcceptanceHeader)
        const overall = mapOverallResult(header.result)

        // 估算剩余可验收
        let remainingEligibleCount = 0
        if (eligibility) {
            remainingEligibleCount = eligibility.sales_lines
                .flatMap((g) => g.fulfillment_facts)
                .filter((f) => Number(f.eligible_quantity) > 0).length
        }

        return {
            status: "succeeded",
            acceptanceNo: header.acceptance_no,
            acceptanceId: header.id,
            remainingEligibleCount,
            remainingEligibleQuantityLabel: "",
            overallResult: overall,
            factOnlyNotice: FACT_ONLY_NOTICE,
        }
    } catch (err) {
        const apiErr = err as ApiError
        if (apiErr?.kind === "Network" || apiErr?.status === 500) {
            return {
                status: "unknown",
                message:
                    apiErr.message ||
                    "操作结果暂无法确认，请查询当前状态后再决定是否重试",
                idempotencyKey: input.idempotencyKey,
            }
        }
        return {
            status: "failed",
            message: apiErr?.message ?? "验收过账失败",
        }
    }
}

export async function reverseCustomerAcceptanceWorkspace(
    input: ReverseAcceptanceInput,
): Promise<ReverseAcceptanceResult> {
    try {
        const reversed = await apiPost<BackendAcceptanceDetail>(
            `/admin/customer-acceptances/${input.acceptanceId}/reverse`,
            {
                expected_version: input.expectedAcceptanceVersion,
                reason_text: input.reasonText,
            },
        )
        const header =
            reversed.acceptance ??
            (reversed as unknown as BackendAcceptanceHeader)
        return {
            status: "succeeded",
            reverseAcceptanceNo: header.acceptance_no,
            reverseAcceptanceId: header.id,
            originalAcceptanceNo: input.acceptanceId,
        }
    } catch (err) {
        const apiErr = err as ApiError
        return {
            status: "failed",
            message: apiErr?.message ?? "冲正失败",
        }
    }
}
