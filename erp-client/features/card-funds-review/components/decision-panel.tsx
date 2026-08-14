"use client"

import { CircleCheckIcon, XIcon } from "lucide-react"

import { surfacePanelClassName } from "@/components/business"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import type { CardFundsReviewItemView } from "@/features/card-funds-review/types"

/**
 * sticky 结论区：凭证/备注草稿 + 从 0 起 / 复核通过 / 驳回 / 退回团队动作。
 * 通过回调开放确认弹窗，不持有 ConfirmMode 状态。
 */
export function DecisionPanel({
    task,
    evidenceDocId,
    evidenceRef,
    comment,
    evidenceOk,
    keyHint,
    canConfirmZero,
    formalPending,
    autoNext,
    onEvidenceDocIdChange,
    onEvidenceRefChange,
    onCommentChange,
    onZero,
    onApprove,
    onReject,
    onRelease,
}: {
    task: CardFundsReviewItemView
    evidenceDocId: string
    evidenceRef: string
    comment: string
    evidenceOk: boolean
    keyHint: string | null
    canConfirmZero: boolean
    formalPending: boolean
    autoNext: boolean
    onEvidenceDocIdChange: (value: string) => void
    onEvidenceRefChange: (value: string) => void
    onCommentChange: (value: string) => void
    onZero: () => void
    onApprove: (advance: boolean) => void
    onReject: () => void
    onRelease: () => void
}) {
    return (
        <Card
            size="sm"
            className={cn(surfacePanelClassName, "sticky bottom-2 z-10")}
        >
            <CardHeader className="border-b border-border/30 py-3">
                <CardTitle className="text-base">结论区</CardTitle>
                <CardDescription>
                    提交时将核对账户、历史复核记录与数据版本。快捷键：j/k
                    切换任务 · ⌘↵ 复核通过
                </CardDescription>
            </CardHeader>
            <CardContent className="space-y-3 pt-4">
                <div className="grid gap-3 sm:grid-cols-2">
                    <div className="space-y-1.5">
                        <Label htmlFor="ev-doc">凭证编号</Label>
                        <Input
                            id="ev-doc"
                            value={evidenceDocId}
                            disabled={
                                task.workItem.workItemStatus !== "OPEN"
                            }
                            onChange={(e) => {
                                onEvidenceDocIdChange(e.target.value)
                            }}
                            placeholder="银行回单号 / 发票号"
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="ev-ref">证据说明</Label>
                        <Input
                            id="ev-ref"
                            value={evidenceRef}
                            disabled={
                                task.workItem.workItemStatus !== "OPEN"
                            }
                            onChange={(e) => {
                                onEvidenceRefChange(e.target.value)
                            }}
                            placeholder="如记账凭证、商城对账记录"
                        />
                    </div>
                </div>
                {!evidenceOk ? (
                    <p className="text-xs text-destructive" role="alert">
                        完成复核前须至少填写一项凭证编号或证据说明；提交时将与复核结论一并保存。
                    </p>
                ) : null}
                <div className="space-y-1.5">
                    <Label htmlFor="ev-comment">备注</Label>
                    <Textarea
                        id="ev-comment"
                        value={comment}
                        disabled={task.workItem.workItemStatus !== "OPEN"}
                        onChange={(e) => {
                            onCommentChange(e.target.value)
                        }}
                        rows={2}
                    />
                </div>
                <div className="flex flex-wrap items-center gap-2">
                    {keyHint ? (
                        <span
                            className="text-xs text-destructive"
                            role="alert"
                        >
                            {keyHint}
                        </span>
                    ) : null}
                    {canConfirmZero ? (
                        <Button
                            type="button"
                            variant="secondary"
                            disabled={formalPending || !evidenceOk}
                            title={
                                evidenceOk
                                    ? undefined
                                    : "须先填写凭证编号或证据说明"
                            }
                            onClick={onZero}
                        >
                            <CircleCheckIcon data-icon="inline-start" />
                            无历史票款，从 0 起
                        </Button>
                    ) : null}
                    <Button
                        type="button"
                        disabled={
                            formalPending ||
                            !evidenceOk ||
                            !task.workItem.allowedActions.includes("APPROVE")
                        }
                        title={
                            evidenceOk
                                ? undefined
                                : "须先填写凭证编号或证据说明"
                        }
                        onClick={() => onApprove(autoNext)}
                    >
                        复核通过
                    </Button>
                    <Button
                        type="button"
                        variant="destructive"
                        disabled={
                            formalPending ||
                            !evidenceOk ||
                            !task.workItem.allowedActions.includes("REJECT")
                        }
                        onClick={onReject}
                    >
                        <XIcon data-icon="inline-start" />
                        驳回
                    </Button>
                    <Button
                        type="button"
                        variant="outline"
                        disabled={
                            formalPending ||
                            !task.workItem.allowedActions.includes(
                                "RELEASE_TO_TEAM",
                            )
                        }
                        onClick={onRelease}
                    >
                        退回团队
                    </Button>
                </div>
            </CardContent>
        </Card>
    )
}
