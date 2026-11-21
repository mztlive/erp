"use client"

import Link from "next/link"
import { LockKeyholeIcon, XIcon } from "lucide-react"

import { PageHeader } from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { formatDateTime } from "@/lib/datetime"
import type { AllocationSessionView } from "@/features/customer-receivables/types"

/** 核销会话页头。工作台页内作业可隐藏返回按钮。 */
export function SessionHeader({
    session,
    isReceipt,
    existing,
    draftSavedAt,
    onRequestClose,
    showClose = true,
    submitted = false,
    closeDisabled = false,
    temporarySaved = false,
}: {
    session: AllocationSessionView
    isReceipt: boolean
    existing: boolean
    draftSavedAt: string | undefined
    onRequestClose: () => void
    showClose?: boolean
    submitted?: boolean
    closeDisabled?: boolean
    temporarySaved?: boolean
}) {
    if (isReceipt) {
        const title = existing ? "编辑回款草稿" : "登记回款"
        const status = {
            label: submitted ? "已提交" : "草稿",
            tone: submitted ? ("warning" as const) : ("neutral" as const),
        }
        return (
            <div className="space-y-3 border-b border-border pb-4">
                {showClose ? (
                    <PageHeader title={title} status={status} />
                ) : (
                    <div className="flex flex-wrap items-center gap-2">
                        <h2 className="text-section-title font-semibold tracking-tight">
                            {title}
                        </h2>
                        <StatusBadge {...status} />
                    </div>
                )}
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-body-compact">
                    <span className="text-muted-foreground">结算主体</span>
                    <LockKeyholeIcon
                        className="size-4 shrink-0 text-muted-foreground"
                        aria-hidden="true"
                    />
                    <span className="min-w-0 break-words font-medium">
                        {session.counterpartyPartyName}
                    </span>
                    {session.existingFactNo ? (
                        <span className="num text-xs text-muted-foreground">
                            {session.existingFactNo}
                        </span>
                    ) : null}
                    {temporarySaved && !submitted ? (
                        <span
                            role="status"
                            className="text-xs text-muted-foreground"
                            title="已暂存本页，刷新可恢复填写内容；新选凭证需保存草稿后才会上传。"
                        >
                            本页已暂存
                        </span>
                    ) : null}
                    {draftSavedAt ? (
                        <span className="text-xs text-muted-foreground">
                            已保存{" "}
                            {formatDateTime(draftSavedAt, "monthDayIntl")}
                        </span>
                    ) : null}
                </div>
            </div>
        )
    }
    return (
        <>
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                    <h2 className="font-heading text-section-title font-semibold tracking-tight">
                        核销 · {session.counterpartyPartyName}
                    </h2>
                    <p className="text-xs text-muted-foreground">
                        模式：{isReceipt ? "回款核销" : "发票核销"}
                        {existing
                            ? ` · 继续单号 ${session.existingFactNo}`
                            : null}
                        {draftSavedAt
                            ? ` · 草稿已保存 ${formatDateTime(draftSavedAt, "monthDayIntl")}`
                            : " · 未保存草稿"}
                    </p>
                    <p className="mt-1 text-xs text-muted-foreground">
                        {session.note}
                    </p>
                </div>
                {showClose ? (
                    <Button
                        id="customer-receivables-session-close"
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={closeDisabled}
                        onClick={onRequestClose}
                    >
                        <XIcon data-icon="inline-start" aria-hidden="true" />
                        {session.returnContext?.returnTo
                            ? "取消并返回"
                            : "返回列表"}
                    </Button>
                ) : null}
            </div>

            {session.returnContext?.from === "W05" &&
            session.returnContext.returnTo ? (
                <Alert variant="info">
                    <AlertTitle>来自销售单票款区</AlertTitle>
                    <AlertDescription>
                        正在处理该销售单的往来账款，办理完成后可直接返回该销售单；筛选与结算主体在本次核销内保留。
                        <Button
                            id="customer-receivables-session-return-source"
                            type="button"
                            size="sm"
                            variant="link"
                            className="ml-2 h-auto p-0"
                            render={
                                <Link href={session.returnContext.returnTo} />
                            }
                        >
                            返回销售单
                        </Button>
                    </AlertDescription>
                </Alert>
            ) : null}
        </>
    )
}
