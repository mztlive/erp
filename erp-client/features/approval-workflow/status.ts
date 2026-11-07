/** 审批单据生命周期判断：未提交（草稿/已驳回）与已进入运行时的分界。 */

import type { DocumentApprovalView } from "./types"

const UNSUBMITTED_STATUS_CODES = new Set([
    "DRAFT",
    "draft",
    "REJECTED",
    "rejected",
])

/**
 * 单据是否仍处于未提交审批的生命周期。
 *
 * @param status 服务端状态码；缺省视为未提交。
 */
export const isUnsubmittedStatus = (status?: string): boolean =>
    !status || UNSUBMITTED_STATUS_CODES.has(status)

/**
 * 变更单审批区相位：未提交及已驳回进入 draft；已取消或已驳回实例继续作为
 * 审批历史展示。其它实例或已离开未提交阶段时进入 runtime。
 */
export function changeOrderApprovalPhase(
    approval: DocumentApprovalView | undefined,
    status: string | undefined,
): "draft" | "runtime" {
    return !isUnsubmittedStatus(status) ||
        (approval?.instance &&
            approval.instance.status !== "CANCELLED" &&
            approval.instance.status !== "REJECTED")
        ? "runtime"
        : "draft"
}
