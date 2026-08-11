import { apiGet, apiPost } from "@/lib/api"
import type {
    BackendSalesOrderReview,
    BackendWorkItem,
    CardApprovalCompleteResult,
} from "./contracts"

export async function claimCardSalesApproval(input: {
    workItemId: string
}): Promise<{
    workItemId: string
    claimedByLabel: string
}> {
    // 审批记录 id 与 work_item 可能不同：优先 work-items claim，失败则仅返回占位领取态。
    try {
        const workItem = await apiGet<BackendWorkItem>(
            `/admin/work-items/${input.workItemId}`,
        )
        const claimed = await apiPost<BackendWorkItem>(
            `/admin/work-items/${input.workItemId}/claim`,
            { version: workItem.version },
        )
        return {
            workItemId: input.workItemId,
            claimedByLabel: claimed.owner_user_id ?? "当前用户",
        }
    } catch {
        // 卡券审批以 sales_order_review 为主路径：approve/reject 不强制 claim。
        return {
            workItemId: input.workItemId,
            claimedByLabel: "当前用户",
        }
    }
}

export async function completeCardSalesApproval(input: {
    workItemId: string
    workItemType:
        | "CARD_SALES_MANAGER_APPROVAL"
        | "CARD_SALES_OPERATION_APPROVAL"
    decision: "APPROVE" | "REJECT"
    reasonCode?: string
    comment?: string
}): Promise<CardApprovalCompleteResult> {
    const path =
        input.decision === "APPROVE"
            ? `/admin/sales-order-reviews/${input.workItemId}/approve`
            : `/admin/sales-order-reviews/${input.workItemId}/reject`

    const review = await apiPost<BackendSalesOrderReview>(path, {
        decision_reason:
            input.comment || input.reasonCode
                ? `${input.reasonCode ?? ""}${input.comment ? ` ${input.comment}` : ""}`.trim()
                : null,
    })

    if (input.decision === "REJECT") {
        return {
            outcome: "REJECTED_TO_SALES",
            reference: `CARD-REJ-${review.id.slice(-6).toUpperCase()}`,
            detail: "已驳回并退回销售处理；修改后须从领导审批重新开始。",
            primaryStatusLabel: "草稿",
        }
    }

    if (input.workItemType === "CARD_SALES_MANAGER_APPROVAL") {
        return {
            outcome: "MANAGER_APPROVED",
            reference: `CARD-MGR-${review.id.slice(-6).toUpperCase()}`,
            detail: "领导已通过；进入运营审批阶段。",
            primaryStatusLabel: "待运营审批",
        }
    }

    return {
        outcome: "OPERATIONS_APPROVED_AND_EFFECTIVE",
        reference: `CARD-OPS-${review.id.slice(-6).toUpperCase()}`,
        detail: "运营已通过；销售单应已生效。",
        primaryStatusLabel: "已生效",
    }
}
