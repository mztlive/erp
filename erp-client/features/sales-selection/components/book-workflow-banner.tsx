"use client"

import * as React from "react"
import Link from "next/link"
import {
    CheckIcon,
    CircleAlertIcon,
    FileCheck2Icon,
    Link2Icon,
    LoaderCircleIcon,
    SendIcon,
    SparklesIcon,
} from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Progress,
    ProgressLabel,
    ProgressValue,
} from "@/components/ui/progress"
import { useBookOperations } from "@/features/sales-selection/hooks/queries"
import {
    bookIdentity,
    publicSelectionHref,
} from "@/features/sales-selection/lib/presentation"
import type {
    BookletStatus,
    SelectionBookDetail,
} from "@/features/sales-selection/types"
import {
    POOL_SOURCE_LABEL,
    SELECTION_FORM_LABEL,
} from "@/features/sales-selection/types"
import { cn } from "@/lib/utils"

const STAGE_LABELS: Record<string, string> = {
    QUEUED: "排队中",
    SNAPSHOT: "冻结商品池",
    SEARCH: "智能生成套餐",
    IMAGES: "准备陈列图片",
    WRITE: "保存陈列结果",
}

const WORKFLOW_STEPS: Array<{
    status: BookletStatus
    label: string
    stepIndex: number
}> = [
    { status: "DRAFT", label: "草稿配置", stepIndex: 1 },
    { status: "PREPARING", label: "准备生成", stepIndex: 2 },
    { status: "PENDING_PUBLISH", label: "核对调整", stepIndex: 3 },
    { status: "PUBLISHED", label: "发布对客", stepIndex: 4 },
    { status: "SUBMITTED", label: "方案生成", stepIndex: 5 },
]

function resolveActiveStepIndex(status: BookletStatus): number {
    switch (status) {
        case "DRAFT":
            return 1
        case "PREPARING":
            return 2
        case "PENDING_PUBLISH":
            return 3
        case "PUBLISHED":
            return 4
        case "SUBMITTED":
        case "CLOSED":
        case "VOIDED":
            return 5
        default:
            return 1
    }
}

type Operations = ReturnType<typeof useBookOperations>

export function BookWorkflowBanner({
    detail,
    pending,
    operations,
    onEdit,
}: {
    detail: SelectionBookDetail
    pending: boolean
    operations: Operations
    onEdit?: () => void
}) {
    const activeStep = resolveActiveStepIndex(detail.status)
    const isTerminal = detail.status === "CLOSED" || detail.status === "VOIDED"
    const bookId = bookIdentity(detail)
    const publicHref = publicSelectionHref(
        operations.copyLink.data?.public_url ?? detail.public_url,
    )

    const tierTotal = detail.tiers.length
    const completedTiers = detail.completed_tier_count ?? 0
    const preparePercent =
        detail.status === "PREPARING" && tierTotal > 0
            ? Math.min(100, Math.round((completedTiers / tierTotal) * 100))
            : undefined

    return (
        <div className="flex flex-col gap-3">
            {/* 失败告警 */}
            {detail.last_prepare_failure ? (
                <Alert variant="destructive">
                    <CircleAlertIcon aria-hidden="true" />
                    <AlertTitle>最近一次准备未完成</AlertTitle>
                    <AlertDescription>
                        {detail.last_prepare_failure}
                    </AlertDescription>
                </Alert>
            ) : null}

            {/* 阶段向导步进条 */}
            <div className="hidden rounded-xl border border-border bg-card px-6 py-3.5 sm:block">
                <nav aria-label="选品流程进度">
                    <ol className="flex items-center justify-between gap-2">
                        {WORKFLOW_STEPS.map((step, idx) => {
                            const isDone =
                                activeStep > step.stepIndex ||
                                detail.status === "SUBMITTED"
                            const isCurrent =
                                activeStep === step.stepIndex && !isTerminal
                            const isUpcoming = activeStep < step.stepIndex

                            return (
                                <li
                                    key={step.status}
                                    className="flex flex-1 items-center last:flex-none"
                                >
                                    <div className="flex items-center gap-2.5">
                                        <span
                                            className={cn(
                                                "inline-flex size-6 shrink-0 items-center justify-center rounded-full text-xs font-semibold transition-colors",
                                                isDone &&
                                                    "bg-primary text-primary-foreground",
                                                isCurrent &&
                                                    "bg-primary text-primary-foreground ring-4 ring-primary/20",
                                                isUpcoming &&
                                                    "border border-border bg-muted text-muted-foreground",
                                            )}
                                        >
                                            {isDone ? (
                                                <CheckIcon
                                                    className="size-3.5 stroke-[2.5]"
                                                    aria-hidden="true"
                                                />
                                            ) : isCurrent &&
                                              detail.status === "PREPARING" ? (
                                                <LoaderCircleIcon
                                                    className="size-3.5 animate-spin"
                                                    aria-hidden="true"
                                                />
                                            ) : (
                                                step.stepIndex
                                            )}
                                        </span>
                                        <span
                                            className={cn(
                                                "text-xs transition-colors",
                                                isCurrent &&
                                                    "font-semibold text-foreground",
                                                isDone &&
                                                    "font-medium text-foreground",
                                                isUpcoming &&
                                                    "text-muted-foreground",
                                            )}
                                        >
                                            {step.label}
                                        </span>
                                    </div>
                                    {idx < WORKFLOW_STEPS.length - 1 ? (
                                        <div
                                            className={cn(
                                                "mx-3 h-0.5 flex-1 rounded-full transition-colors",
                                                isDone
                                                    ? "bg-primary"
                                                    : "bg-border",
                                            )}
                                            aria-hidden="true"
                                        />
                                    ) : null}
                                </li>
                            )
                        })}
                    </ol>
                </nav>
            </div>

            {/* 阶段引导横幅 */}
            {detail.status === "DRAFT" ? (
                <div className="flex flex-wrap items-center justify-between gap-4 rounded-xl border border-blue-500/20 bg-blue-50/50 p-4 dark:bg-blue-950/20">
                    <div className="min-w-0 space-y-0.5">
                        <div className="flex items-center gap-2 text-sm font-semibold text-blue-950 dark:text-blue-100">
                            <SparklesIcon
                                className="size-4 text-blue-600 dark:text-blue-400"
                                aria-hidden="true"
                            />
                            <span>选品册已创建，请开始准备陈列</span>
                        </div>
                        <p className="text-xs text-muted-foreground">
                            选品形态：
                            {SELECTION_FORM_LABEL[detail.selection_form]} ·
                            商品来源：{POOL_SOURCE_LABEL[detail.source_kind]}
                            。点击右上角「开始准备」抓取 SKU 并生成初始陈列。
                        </p>
                    </div>
                    {onEdit ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={pending}
                            onClick={onEdit}
                        >
                            调整来源与档位
                        </Button>
                    ) : null}
                </div>
            ) : null}

            {detail.status === "PREPARING" ? (
                <Alert
                    variant="info"
                    className="border-blue-500/20 bg-blue-50/50 dark:bg-blue-950/20"
                >
                    <LoaderCircleIcon
                        className="animate-spin text-blue-600"
                        aria-hidden="true"
                    />
                    <AlertTitle className="text-blue-950 dark:text-blue-100">
                        正在准备陈列商品
                    </AlertTitle>
                    <AlertDescription className="space-y-2.5">
                        <p className="text-xs text-muted-foreground">
                            当前处理环节：
                            <span className="font-medium text-foreground">
                                {STAGE_LABELS[
                                    detail.prepare_stage ?? "QUEUED"
                                ] ?? "处理商品"}
                            </span>
                            。后台正在冻结商品快照并生成陈列，期间不可修改与发布。
                        </p>
                        {preparePercent != null ? (
                            <Progress value={preparePercent}>
                                <ProgressLabel>档位生成进度</ProgressLabel>
                                <ProgressValue>
                                    {() => (
                                        <span className="num">
                                            {completedTiers}/{tierTotal} 档
                                        </span>
                                    )}
                                </ProgressValue>
                            </Progress>
                        ) : null}
                    </AlertDescription>
                </Alert>
            ) : null}

            {detail.status === "PENDING_PUBLISH" ? (
                <div className="flex flex-wrap items-center justify-between gap-4 rounded-xl border border-amber-500/30 bg-amber-50/60 p-4 dark:bg-amber-950/20">
                    <div className="min-w-0 space-y-0.5">
                        <div className="flex items-center gap-2 text-sm font-semibold text-amber-950 dark:text-amber-100">
                            <FileCheck2Icon
                                className="size-4 text-amber-600 dark:text-amber-400"
                                aria-hidden="true"
                            />
                            <span>陈列已就绪，请核对并删减商品</span>
                        </div>
                        <p className="text-xs text-muted-foreground">
                            下方已列出本次所有陈列项（共{" "}
                            <span className="num font-medium text-foreground">
                                {detail.display_count}
                            </span>{" "}
                            项）。请检查卡片，点击「删除该项」剔除不合适商品；确认无误后点击右上角「发布」。
                        </p>
                    </div>
                    {detail.selection_form === "PACKAGE" && onEdit ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={pending}
                            onClick={onEdit}
                        >
                            调整来源与档位
                        </Button>
                    ) : null}
                </div>
            ) : null}

            {detail.status === "PUBLISHED" ? (
                <div className="flex flex-wrap items-center justify-between gap-4 rounded-xl border border-emerald-500/20 bg-emerald-50/50 p-4 dark:bg-emerald-950/20">
                    <div className="min-w-0 space-y-0.5">
                        <div className="flex items-center gap-2 text-sm font-semibold text-emerald-950 dark:text-emerald-100">
                            <SendIcon
                                className="size-4 text-emerald-600 dark:text-emerald-400"
                                aria-hidden="true"
                            />
                            <span>选品册已发布，客户专属链接生效中</span>
                        </div>
                        <p className="text-xs text-muted-foreground">
                            专属安全链接已生成（30天有效）。可复制右侧链接发送给客户，客户在手机端打开即可选品。
                        </p>
                    </div>
                    {publicHref ? (
                        <Button
                            id="sales-selection-detail-copy-link-banner"
                            type="button"
                            size="sm"
                            disabled={operations.copyLink.isPending || pending}
                            onClick={() =>
                                void operations.copyLink.mutateAsync(bookId)
                            }
                        >
                            <Link2Icon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            复制对客选品链接
                        </Button>
                    ) : null}
                </div>
            ) : null}

            {detail.status === "SUBMITTED" ? (
                <div className="flex flex-wrap items-center justify-between gap-4 rounded-xl border border-emerald-500/30 bg-emerald-50/70 p-4 dark:bg-emerald-950/20">
                    <div className="min-w-0 space-y-0.5">
                        <div className="flex items-center gap-2 text-sm font-semibold text-emerald-950 dark:text-emerald-100">
                            <CheckIcon
                                className="size-4 text-emerald-600 dark:text-emerald-400"
                                aria-hidden="true"
                            />
                            <span>客户已完成选品并提交方案</span>
                        </div>
                        <p className="text-xs text-muted-foreground">
                            选品册陈列已冻结。
                            {detail.proposal_no ? (
                                <>
                                    销售方案编号：
                                    <span className="font-semibold text-foreground">
                                        {detail.proposal_no}
                                    </span>
                                    ，
                                </>
                            ) : null}
                            可进入销售方案查看客户确认明细并继续开单。
                        </p>
                    </div>
                    {detail.proposal_id ? (
                        <Button
                            id="sales-selection-detail-banner-open-proposal"
                            type="button"
                            size="sm"
                            render={
                                <Link
                                    href={`/sales/selection/proposals/${detail.proposal_id}`}
                                />
                            }
                        >
                            查看销售方案
                        </Button>
                    ) : null}
                </div>
            ) : null}

            {isTerminal ? (
                <div className="rounded-xl border border-border bg-muted/40 p-4">
                    <p className="text-sm font-semibold text-foreground">
                        该选品册已{detail.status === "CLOSED" ? "关闭" : "作废"}
                    </p>
                    <p className="mt-0.5 text-xs text-muted-foreground">
                        该选品册处于业务终态，仅供查阅历史快照，不可再编辑、重生成或发布。
                    </p>
                </div>
            ) : null}
        </div>
    )
}
