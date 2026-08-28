/**
 * W06 客户验收 — 登记 / 冲正 / 草稿变更（mutationFn）。
 * 从 api/acceptance.ts 拆出；api/acceptance.ts 保持原导出名 re-export。
 */

import { apiPost } from "@/lib/api"
import { getErrorMessage, type ApiError } from "@/lib/api/errors"
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
    mapFactTypeToBackend,
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
                fulfillment_fact_type: mapFactTypeToBackend(
                    a.fulfillmentFactType,
                ),
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
        const hasServerDraft =
            Boolean(input.acceptanceDraftId) &&
            !input.acceptanceDraftId.startsWith("draft_")
        const posted = await apiPost<{
            acceptance: BackendAcceptanceHeader
            remaining_eligibility: BackendEligibilityView
        }>("/admin/customer-acceptances/commit", {
            work_item_id: input.workItemId ?? null,
            expected_task_version: input.expectedTaskVersion ?? null,
            acceptance_id: hasServerDraft ? input.acceptanceDraftId : null,
            expected_acceptance_version: hasServerDraft
                ? input.expectedDraftVersion
                : null,
            acceptance_no: hasServerDraft
                ? null
                : `YS-${input.idempotencyKey.replace(/[^A-Za-z0-9]/g, "").slice(0, 24)}`,
            sales_order_id: input.salesOrderId,
            expected_sales_order_version: input.expectedSalesOrderLockVersion,
            accepted_at: input.acceptedAt
                ? Math.floor(Date.parse(input.acceptedAt) / 1000) ||
                  Math.floor(Date.now() / 1000)
                : Math.floor(Date.now() / 1000),
            result: mapOverallResultToBackend(input.lines),
            lines: input.lines.map((line) => ({
                sales_order_line_id: line.salesOrderLineId,
                accepted_quantity: line.acceptedQuantity || "0",
                short_quantity: line.shortQuantity || "0",
                rejected_quantity: line.rejectedQuantity || "0",
                reason: line.reason || null,
                allocations: line.allocations.map((allocation) => ({
                    fulfillment_line_id: allocation.fulfillmentLineId,
                    fulfillment_fact_type: mapFactTypeToBackend(
                        allocation.fulfillmentFactType,
                    ),
                    allocated_quantity: allocation.allocatedQuantity || "0",
                })),
            })),
            idempotency_key: input.idempotencyKey,
        })

        const header = posted.acceptance
        const overall = mapOverallResult(header.result)
        const remainingFacts = posted.remaining_eligibility.sales_lines
            .flatMap((group) =>
                group.fulfillment_facts.map((fact) => ({
                    ...fact,
                    unitCode: group.unit_code ?? "",
                })),
            )
            .filter((fact) => Number(fact.eligible_quantity) > 0)
        const quantitiesByUnit = new Map<string, number>()
        for (const fact of remainingFacts) {
            quantitiesByUnit.set(
                fact.unitCode,
                (quantitiesByUnit.get(fact.unitCode) ?? 0) +
                    Number(fact.eligible_quantity),
            )
        }

        return {
            status: "succeeded",
            acceptanceNo: header.acceptance_no,
            acceptanceId: header.id,
            remainingEligibleCount: remainingFacts.length,
            remainingEligibleQuantityLabel: Array.from(quantitiesByUnit)
                .map(([unit, quantity]) => `${quantity}${unit}`)
                .join("、"),
            overallResult: overall,
            factOnlyNotice: FACT_ONLY_NOTICE,
        }
    } catch (err) {
        const apiErr = err as ApiError
        if (apiErr?.kind === "Network" || apiErr?.status === 500) {
            return {
                status: "unknown",
                message: getErrorMessage(
                    err,
                    "操作结果暂无法确认，请查询当前状态后再决定是否重试",
                ),
                idempotencyKey: input.idempotencyKey,
            }
        }
        return {
            status: "failed",
            message: getErrorMessage(err, "验收过账失败，请稍后重试。"),
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
        return {
            status: "failed",
            message: getErrorMessage(err, "冲正失败，请稍后重试。"),
        }
    }
}
