"use client"

import * as React from "react"

import {
    DiscardConfirmDialog,
    FormalActionConfirmDialog,
} from "@/components/business"
import type {
    ApproveConclusion,
    CardFundsReviewItemView,
    ConfirmMode,
} from "@/features/card-funds-review/types"
import { REVIEW_TYPE_LABEL } from "@/features/card-funds-review/types"
import { shortHash } from "../lib/presentation"
import {
    RejectReviewDialog,
    type RejectReviewValue,
} from "./reject-review-dialog"

/** 复核页全部确认弹窗：通过/从 0 起强确认、退回团队、驳回、放弃未保存切换。 */
export function ReviewConfirmDialogs({
    confirmMode,
    setConfirmMode,
    task,
    completePending,
    releasePending,
    pendingNav,
    setPendingNav,
    neighborId,
    goToWorkItem,
    runApprove,
    submitReject,
    handleReleaseToTeam,
}: {
    confirmMode: ConfirmMode
    setConfirmMode: React.Dispatch<React.SetStateAction<ConfirmMode>>
    task: CardFundsReviewItemView | undefined
    completePending: boolean
    releasePending: boolean
    pendingNav: number | null
    setPendingNav: React.Dispatch<React.SetStateAction<number | null>>
    neighborId: (delta: number) => string | undefined
    goToWorkItem: (workItemId: string | undefined | null) => void
    runApprove: (
        conclusion: ApproveConclusion,
        advance: boolean,
    ) => Promise<void>
    submitReject: (value: RejectReviewValue) => Promise<void>
    handleReleaseToTeam: () => Promise<void>
}) {
    return (
        <>
            {/* 从 0 起 / 通过 强确认 */}
            <FormalActionConfirmDialog
                open={
                    confirmMode?.kind === "approve" ||
                    confirmMode?.kind === "zero"
                }
                onOpenChange={(open) => {
                    if (!open) setConfirmMode(null)
                }}
                title={
                    confirmMode?.kind === "zero"
                        ? "确认无历史票款，从 0 起"
                        : "确认复核通过"
                }
                description={
                    confirmMode?.kind === "zero"
                        ? `将提交「期初净额为零、无历史票款」结论：销售单 ${task?.salesOrder.orderNo ?? ""}、应收子账 #${task?.account.accountSeq ?? ""}（${task?.account.customerName ?? ""}）。不创建 0 元回款/发票。须证据完整；提交时将核对数据版本。`
                        : `将提交「复核通过并核对票款记录」。复核类型 ${task ? REVIEW_TYPE_LABEL[task.reviewType] : ""}，当前数据版本 ${task ? shortHash(task.workItem.subjectVersion) : ""}。`
                }
                actionLabel={
                    confirmMode?.kind === "zero" ? "从 0 起并完成" : "复核通过"
                }
                confirmLabel={
                    confirmMode?.kind === "zero"
                        ? "确认从 0 起并完成"
                        : "确认通过"
                }
                fromStatus={{ label: "待复核", tone: "warning" }}
                toStatus={
                    confirmMode?.kind === "zero"
                        ? { label: "从 0 起已通过", tone: "success" }
                        : { label: "复核已通过", tone: "success" }
                }
                lockedFields={
                    task
                        ? [
                              `销售单 ${task.salesOrder.orderNo}`,
                              `应收子账 #${task.account.accountSeq}（${task.account.customerName}）`,
                              "数据版本（短校验码）",
                              `复核类型 ${REVIEW_TYPE_LABEL[task.reviewType]}`,
                              "票款版本（仅显示，不可改）",
                          ]
                        : []
                }
                effects={
                    confirmMode?.kind === "zero"
                        ? [
                              "记录期初通过结论：无历史票款",
                              "不创建 0 元回款单或 0 元发票",
                              "记录复核结论并完成任务",
                          ]
                        : [
                              "记录本次复核并完成任务",
                              "提交时核对数据版本，不一致将阻断",
                              "同本次提交完成当前任务",
                          ]
                }
                pending={completePending}
                onConfirm={async () => {
                    if (confirmMode?.kind === "zero") {
                        await runApprove("NO_HISTORY_FROM_ZERO", confirmMode.advance)
                    } else if (confirmMode?.kind === "approve") {
                        await runApprove(
                            confirmMode.conclusion,
                            confirmMode.advance,
                        )
                    }
                }}
            />

            <FormalActionConfirmDialog
                open={confirmMode?.kind === "release"}
                onOpenChange={(open) => {
                    if (!open) setConfirmMode(null)
                }}
                title="退回团队继续安排"
                description="退回后原任务保持开放，只清除当前个人责任；不生成复核记录。"
                actionLabel="退回团队"
                confirmLabel="确认退回团队"
                fromStatus={{ label: "处理中", tone: "info" }}
                toStatus={{
                    label: "团队待处理",
                    tone: "warning",
                }}
                effects={[
                    "原任务保持开放",
                    "清除当前个人责任",
                    "不生成复核记录",
                ]}
                pending={releasePending}
                onConfirm={() => void handleReleaseToTeam()}
            />

            <RejectReviewDialog
                open={confirmMode?.kind === "reject"}
                onOpenChange={(open) => {
                    if (!open) setConfirmMode(null)
                }}
                pending={completePending}
                onSubmit={submitReject}
            />

            {/* 证据/备注未保存时切换任务确认 */}
            <DiscardConfirmDialog
                open={pendingNav != null}
                onOpenChange={(open) => {
                    if (!open) setPendingNav(null)
                }}
                title="放弃未保存的证据或备注？"
                description="当前凭证编号、证据说明或备注尚未保存，切换任务后将丢失。"
                confirmLabel="放弃并切换"
                cancelLabel="继续编辑"
                onConfirm={() => {
                    const delta = pendingNav
                    setPendingNav(null)
                    if (delta != null) {
                        const target = neighborId(delta)
                        if (target) goToWorkItem(target)
                    }
                }}
            />
        </>
    )
}
