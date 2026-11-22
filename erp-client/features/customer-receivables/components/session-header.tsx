"use client"

import Link from "next/link"
import { ArrowLeftIcon, LockKeyholeIcon } from "lucide-react"

import { DetailWorkspaceChrome } from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { formatDateTime } from "@/lib/datetime"
import type { AllocationSessionView } from "@/features/customer-receivables/types"

/** 核销会话页头：统一 DetailWorkspaceChrome 风格，支持独立页与嵌入式任务面板。 */
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
    const title = isReceipt
        ? existing
            ? "编辑回款草稿"
            : "登记回款"
        : existing
          ? "继续分配销项发票"
          : "登记销项发票"
    const status = {
        label: submitted ? "已提交" : "草稿",
        tone: submitted ? ("warning" as const) : ("neutral" as const),
    }

    const meta = (
        <span className="inline-flex flex-wrap items-center gap-x-2.5 gap-y-1">
            <span className="inline-flex items-center gap-1.5 text-foreground">
                <span className="text-muted-foreground">结算主体</span>
                <LockKeyholeIcon
                    className="size-3.5 shrink-0 text-muted-foreground"
                    aria-hidden="true"
                />
                <span className="font-medium">
                    {session.counterpartyPartyName}
                </span>
            </span>
            {session.existingFactNo ? (
                <span className="num text-muted-foreground">
                    · {session.existingFactNo}
                </span>
            ) : null}
            {temporarySaved && !submitted ? (
                <span
                    role="status"
                    className="text-muted-foreground"
                    title="已暂存本页，刷新可恢复填写内容；新选凭证需保存草稿后才会上传。"
                >
                    · 本页已暂存
                </span>
            ) : null}
            {draftSavedAt ? (
                <span className="text-muted-foreground">
                    · 已保存 {formatDateTime(draftSavedAt, "monthDayIntl")}
                </span>
            ) : null}
        </span>
    )

    return (
        <>
            {showClose ? (
                <DetailWorkspaceChrome
                    title={title}
                    primaryStatus={status}
                    meta={meta}
                    secondaryActions={
                        <Button
                            id="customer-receivables-session-close"
                            type="button"
                            variant="ghost"
                            size="sm"
                            disabled={closeDisabled}
                            onClick={onRequestClose}
                        >
                            <ArrowLeftIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            {session.returnContext?.returnTo
                                ? "取消并返回"
                                : "返回列表"}
                        </Button>
                    }
                />
            ) : (
                <div className="space-y-2 border-b border-border/60 pb-4">
                    <div className="flex flex-wrap items-center gap-2">
                        <h2 className="text-section-title font-semibold tracking-tight">
                            {title}
                        </h2>
                        <StatusBadge {...status} />
                    </div>
                    <div className="text-xs text-muted-foreground">{meta}</div>
                </div>
            )}

            {!isReceipt &&
            session.returnContext?.from === "W05" &&
            session.returnContext.returnTo ? (
                <div className={showClose ? "px-(--scaffold-px) pt-4" : "pt-3"}>
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
                                    <Link
                                        href={session.returnContext.returnTo}
                                    />
                                }
                            >
                                返回销售单
                            </Button>
                        </AlertDescription>
                    </Alert>
                </div>
            ) : null}
        </>
    )
}
