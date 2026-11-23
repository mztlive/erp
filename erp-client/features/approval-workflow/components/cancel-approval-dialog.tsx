"use client"

import * as React from "react"

import { documentIsEditableDraft } from "../api/document-cancel"
import { displayReadableName } from "../display"
import { createApprovalIdempotencyKey } from "../idempotency"
import { useCancelApprovalMutation, useCancelBlockedMutation } from "../queries"
import type { ApprovalCommandView } from "../types"
import { ApprovalWithdrawDialog } from "./approval-withdraw-dialog"

/**
 * 撤回审批或取消受阻审批。
 *
 * 撤回走业务单据资源接口；受阻取消走专用端口。两者都不得调用通用任务关闭。
 * 底层统一复用 `ApprovalWithdrawDialog`（强制 destructive + BPM 512B UTF-8 校验 + 幂等恢复）。
 */
export function CancelApprovalDialog({
    open,
    onOpenChange,
    mode,
    instanceId,
    documentType,
    documentId,
    documentVersion,
    currentRoundNo = 1,
    reviseRejected = false,
    currentNodeName,
    afterStatusLabel,
    expectedInstanceVersion,
    expectedExecutionVersion,
    expectedTaskVersion,
    emergency = false,
    onApplied,
    id = "governance-approval-cancel-dialog",
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    mode: "withdraw" | "cancel-blocked"
    instanceId: string
    documentType?: string
    documentId?: string
    documentVersion?: number
    currentRoundNo?: number
    reviseRejected?: boolean
    currentNodeName?: string
    afterStatusLabel: string
    expectedInstanceVersion: string
    expectedExecutionVersion: string
    expectedTaskVersion?: string
    emergency?: boolean
    onApplied?: (view: ApprovalCommandView) => void
    id?: string
}) {
    const cancelApproval = useCancelApprovalMutation()
    const cancelBlocked = useCancelBlockedMutation(instanceId)
    const pending = cancelApproval.isPending || cancelBlocked.isPending
    const frozenWithdraw = React.useRef<
        Parameters<typeof cancelApproval.mutateAsync>[0] | null
    >(null)
    const frozenBlocked = React.useRef<
        Parameters<typeof cancelBlocked.mutateAsync>[0] | null
    >(null)

    React.useEffect(() => {
        if (open) return
        frozenWithdraw.current = null
        frozenBlocked.current = null
    }, [open])

    const isWithdraw = mode === "withdraw"
    const isRevise = isWithdraw && reviseRejected
    const nodeLabel = displayReadableName(currentNodeName) ?? "审批节点未标注"

    const title = isRevise
        ? "修改原单"
        : isWithdraw
          ? emergency
              ? "代申请人撤回"
              : "撤回审批"
          : "终止并退回此审批"

    const effects: string[] = []
    if (isWithdraw) {
        effects.push(`撤回后单据将回到${afterStatusLabel}。`)
        if (isRevise) {
            effects.push("原单编号与审批记录保留，修改后需重新提交审批。")
        }
        if (emergency) {
            effects.push(
                "您正在代原申请人撤回此审批，系统将记录本次代撤回操作。",
            )
        }
    }

    const irreversibleEffects =
        mode === "cancel-blocked"
            ? [
                  `终止后单据将回到${afterStatusLabel}，不会改派或继续推进。`,
                  "此操作不可恢复，如需继续审批须重新发起。",
              ]
            : []

    return (
        <ApprovalWithdrawDialog
            open={open}
            onOpenChange={onOpenChange}
            idPrefix={id}
            reasonId={`${id}-reason`}
            title={title}
            description={`当前节点：${nodeLabel}。`}
            actionLabel={isWithdraw ? "撤回审批" : "终止审批"}
            confirmLabel={
                isRevise
                    ? "撤回并修改原单"
                    : isWithdraw
                      ? "确认撤回"
                      : "确认终止"
            }
            unknownConfirmLabel="核对撤回结果"
            cancelLabel="取消"
            fromStatus={
                isWithdraw
                    ? {
                          label: isRevise ? "已驳回" : "审批中",
                          tone: isRevise ? "destructive" : "info",
                      }
                    : { label: "审批受阻", tone: "destructive" }
            }
            toStatus={{ label: afterStatusLabel, tone: "neutral" }}
            effects={effects}
            irreversibleEffects={irreversibleEffects}
            reasonLabel={isWithdraw ? "撤回原因" : "终止原因"}
            reasonPlaceholder={
                isWithdraw ? "请输入撤回原因" : "请说明终止本次受阻审批的原因"
            }
            reviseRejected={isRevise}
            pending={pending}
            createIdempotencyKey={() =>
                createApprovalIdempotencyKey(
                    isWithdraw ? "cancel" : "cancel-blocked",
                    instanceId,
                )
            }
            checkUnknownResolved={
                isWithdraw
                    ? async () => {
                          const targetType =
                              frozenWithdraw.current?.documentType ??
                              documentType ??
                              ""
                          const targetId =
                              frozenWithdraw.current?.documentId ??
                              documentId ??
                              ""
                          const resolved = await documentIsEditableDraft(
                              targetType,
                              targetId,
                          )
                          if (resolved) {
                              frozenWithdraw.current = null
                              onApplied?.({
                                  instanceId,
                                  currentRoundNo,
                                  instanceStatus: "CANCELLED",
                                  subjectStatus: "draft",
                                  outcome: "APPLIED",
                              })
                          }
                          return resolved
                      }
                    : undefined
            }
            onSubmitWithdraw={async ({ reason, idempotencyKey }) => {
                if (isWithdraw) {
                    const withdraw = frozenWithdraw.current ?? {
                        documentType: documentType ?? "",
                        documentId: documentId ?? "",
                        documentVersion: documentVersion ?? 0,
                        instanceId,
                        currentRoundNo,
                        request: {
                            reason,
                            expected_instance_version: expectedInstanceVersion,
                            expected_execution_version:
                                expectedExecutionVersion,
                            expected_task_version: expectedTaskVersion ?? null,
                            idempotency_key: idempotencyKey,
                        },
                    }
                    frozenWithdraw.current = withdraw
                    const view = await cancelApproval.mutateAsync(withdraw)
                    frozenWithdraw.current = null
                    onApplied?.(view)
                    return
                }

                const blocked = frozenBlocked.current ?? {
                    reason,
                    expected_instance_version: expectedInstanceVersion,
                    expected_execution_version: expectedExecutionVersion,
                    expected_task_version: expectedTaskVersion ?? null,
                    idempotency_key: idempotencyKey,
                }
                frozenBlocked.current = blocked
                const view = await cancelBlocked.mutateAsync(blocked)
                frozenBlocked.current = null
                onApplied?.(view)
            }}
            onSubmitError={(_error, isUnknown) => {
                if (!isUnknown) {
                    frozenWithdraw.current = null
                    frozenBlocked.current = null
                }
            }}
        />
    )
}
