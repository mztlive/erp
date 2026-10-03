/**
 * W06 客户验收 — 登记 / 冲正（mutationFn）。
 * 从 api/acceptance.ts 拆出；api/acceptance.ts 保持原导出名 re-export。
 */

import { uploadEvidenceFileAsset } from "@/features/file-assets/api"
import { apiGetBlob, apiPost } from "@/lib/api"
import { getErrorMessage, type ApiError } from "@/lib/api/errors"
import type {
    PostAcceptanceInput,
    PostAcceptanceResult,
    ReverseAcceptanceInput,
    ReverseAcceptanceResult,
} from "@/features/sales-orders/lib/acceptance-types"
import { FACT_ONLY_NOTICE } from "@/features/sales-orders/lib/acceptance-types"
import { compactFixed, compareDecimal, sumFixed } from "@/lib/fixed-decimal"
import {
    mapOverallResult,
    mapOverallResultToBackend,
    mapFactTypeToBackend,
    type BackendAcceptanceDetail,
    type BackendAcceptanceHeader,
    type BackendEligibilityView,
} from "@/features/sales-orders/lib/acceptance-mappers"

export async function postCustomerAcceptanceWorkspace(
    input: PostAcceptanceInput,
): Promise<PostAcceptanceResult> {
    try {
        if (!input.evidenceFile) {
            return {
                status: "failed",
                message: "请上传签收单凭证（图片或 PDF）",
            }
        }
        const evidence = await uploadEvidenceFileAsset(input.evidenceFile)
        const hasServerDraft =
            Boolean(input.acceptanceDraftId) &&
            !input.acceptanceDraftId.startsWith("draft_")
        const posted = await apiPost<{
            acceptance: BackendAcceptanceHeader
            remaining_eligibility: BackendEligibilityView
        }>("/admin/customer-acceptances/commit", {
            evidence_attachment_id: evidence.id,
            work_item_id: input.workItemId ?? null,
            expected_task_version: input.expectedTaskVersion ?? null,
            acceptance_id: hasServerDraft ? input.acceptanceDraftId : null,
            expected_acceptance_version: hasServerDraft
                ? input.expectedDraftVersion
                : null,
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
            .filter(
                (fact) => compareDecimal(fact.eligible_quantity, "0", 6) > 0,
            )
        const quantitiesByUnit = new Map<string, string[]>()
        for (const fact of remainingFacts) {
            const quantities = quantitiesByUnit.get(fact.unitCode) ?? []
            quantities.push(fact.eligible_quantity)
            quantitiesByUnit.set(fact.unitCode, quantities)
        }

        return {
            status: "succeeded",
            acceptanceNo: header.acceptance_no,
            acceptanceId: header.id,
            remainingEligibleCount: remainingFacts.length,
            remainingEligibleQuantityLabel: Array.from(quantitiesByUnit)
                .map(
                    ([unit, quantities]) =>
                        `${compactFixed(
                            sumFixed(quantities, {
                                maxScale: 6,
                                outputScale: 6,
                            }),
                        )}${unit}`,
                )
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
        const reversed = await apiPost<
            BackendAcceptanceDetail | BackendAcceptanceHeader
        >(`/admin/customer-acceptances/${input.acceptanceId}/reverse`, {
            expected_version: input.expectedAcceptanceVersion,
            reason_text: input.reasonText,
            idempotency_key: input.idempotencyKey,
        })
        const header = "acceptance" in reversed ? reversed.acceptance : reversed
        return {
            status: "succeeded",
            reverseAcceptanceNo: header.acceptance_no,
            reverseAcceptanceId: header.id,
            originalAcceptanceNo: input.originalAcceptanceNo,
        }
    } catch (err) {
        return {
            status: "failed",
            message: getErrorMessage(err, "冲正失败，请稍后重试。"),
        }
    }
}

/** 沿签收单和销售单范围重新校验后下载签收凭证。 */
export async function downloadAcceptanceEvidence(
    acceptanceId: string,
    acceptanceNo: string,
): Promise<void> {
    const blob = await apiGetBlob(
        `/admin/customer-acceptances/${encodeURIComponent(acceptanceId)}/evidence`,
        { timeoutMs: 30_000, cache: "no-store" },
    )
    const url = URL.createObjectURL(blob)
    const link = document.createElement("a")
    link.href = url
    const extension =
        blob.type === "application/pdf"
            ? ".pdf"
            : blob.type === "image/jpeg"
              ? ".jpg"
              : blob.type === "image/webp"
                ? ".webp"
                : ".png"
    link.download = `${acceptanceNo || "签收单"}${extension}`
    document.body.append(link)
    link.click()
    link.remove()
    URL.revokeObjectURL(url)
}
