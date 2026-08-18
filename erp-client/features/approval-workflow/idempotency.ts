/**
 * 审批命令幂等键生命周期。
 *
 * 同一次用户意图重试保持同一键；用户修改决定或原因后必须换新键。
 * 键本身不得渲染到界面。
 */

export type ApprovalIdempotencyKind =
    | "decision"
    | "resume"
    | "reassign"
    | "cancel-blocked"
    | "cancel"
    | "upgrade"

/**
 * 生成一次新的审批命令幂等键。
 *
 * @param kind 命令类别
 * @param scopeId 实例或任务身份
 * @returns 新键
 */
export const createApprovalIdempotencyKey = (
    kind: ApprovalIdempotencyKind,
    scopeId: string,
): string => `approval:${kind}:${scopeId}:${crypto.randomUUID()}`

/**
 * 计算决定意图指纹。决定或原因变化即视为新意图。
 *
 * @param decision 通过或驳回
 * @param reason 原因
 * @returns 稳定指纹
 */
export const decisionIntentFingerprint = (
    decision: string,
    reason: string,
): string => `${decision}:${reason.trim()}`

export type IdempotencySlot = Readonly<{
    key: string
    fingerprint: string
}>

/**
 * 按意图指纹复用或轮换幂等键。
 *
 * @param current 当前槽位
 * @param kind 命令类别
 * @param scopeId 实例或任务身份
 * @param fingerprint 当前意图
 * @returns 应提交的槽位
 */
export const slotForIntent = (
    current: IdempotencySlot | null,
    kind: ApprovalIdempotencyKind,
    scopeId: string,
    fingerprint: string,
): IdempotencySlot => {
    if (current && current.fingerprint === fingerprint) return current
    return {
        key: createApprovalIdempotencyKey(kind, scopeId),
        fingerprint,
    }
}
