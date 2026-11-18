"use client"

import Link from "next/link"
import {
    CheckIcon,
    CircleAlertIcon,
    FileCheck2Icon,
    Link2Icon,
    LoaderCircleIcon,
    RefreshCwIcon,
    SendIcon,
    SparklesIcon,
} from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Progress,
    ProgressLabel,
    ProgressValue,
} from "@/components/ui/progress"
import { useBookOperations } from "@/features/sales-selection/hooks/queries"
import {
    bookIdentity,
    prepareFailure,
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
import { createIdempotencyKey } from "@/features/sales-selection/lib/validation"

const STAGE_LABELS: Record<string, string> = {
    QUEUED: "排队中",
    SNAPSHOT: "锁定候选商品",
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
    canPrepare,
    canCopyLink,
    canOpenProposal,
    operations,
    onEdit,
}: {
    detail: SelectionBookDetail
    pending: boolean
    canPrepare: boolean
    canCopyLink: boolean
    canOpenProposal: boolean
    operations: Operations
    onEdit?: () => void
}) {
    const failure = prepareFailure(detail)
    const activeStep = failure ? 2 : resolveActiveStepIndex(detail.status)
    const isTerminal = detail.status === "CLOSED" || detail.status === "VOIDED"
    const bookId = bookIdentity(detail)
    const publicHref = canCopyLink
        ? publicSelectionHref(
              operations.copyLink.data?.public_url ?? detail.public_url,
          )
        : undefined
    const tierTotal = detail.tiers.length
    const completedTiers = detail.completed_tier_count ?? 0
    const preparePercent =
        detail.status === "PREPARING" && tierTotal > 0
            ? Math.min(100, Math.round((completedTiers / tierTotal) * 100))
            : undefined

    return (
        <div className="flex flex-col gap-3">
            {/* 失败告警 */}
            {failure ? (
                <Alert
                    variant="destructive"
                    aria-labelledby="sales-selection-prepare-failure-title"
                >
                    <CircleAlertIcon aria-hidden="true" />
                    <AlertTitle id="sales-selection-prepare-failure-title">
                        {failure.title}
                        {failure.preservesDisplay
                            ? "，原陈列已保留"
                            : "，尚未生成可预览内容"}
                    </AlertTitle>
                    <AlertDescription className="space-y-3">
                        <div className="space-y-1">
                            <div>
                                {failure.preservesDisplay
                                    ? "原商品来源、档位、售价和陈列已保留，可继续核对并发布。再次准备前，请先确认配置。"
                                    : "本次准备已结束，选品册配置已保留，无需重新创建。"}
                            </div>
                            <div>
                                {failure.needsSourceChange
                                    ? "当前商品来源没有符合条件的商品，请调整来源后重新准备。"
                                    : failure.preservesDisplay
                                      ? "重新准备成功后会替换当前陈列并重置客户售价，需要重新核对。"
                                      : "请按当前配置重新准备；如果仍未完成，可调整来源与档位后再试。"}
                            </div>
                        </div>
                        <div className="flex flex-wrap gap-2">
                            {failure.preservesDisplay ||
                            failure.needsSourceChange ? (
                                onEdit ? (
                                    <Button
                                        id="sales-selection-failure-edit"
                                        type="button"
                                        size="sm"
                                        disabled={pending}
                                        onClick={onEdit}
                                    >
                                        {failure.needsSourceChange
                                            ? "调整商品来源"
                                            : "检查配置并重新准备"}
                                    </Button>
                                ) : null
                            ) : (
                                <>
                                    {canPrepare && (
                                        <LoadingButton
                                            id="sales-selection-failure-retry"
                                            type="button"
                                            size="sm"
                                            loading={
                                                operations.prepare.isPending
                                            }
                                            disabled={pending}
                                            onClick={() =>
                                                operations.prepare.mutate({
                                                    bookId,
                                                    kind: "FIRST_PREPARE",
                                                    expected_version:
                                                        detail.version,
                                                    idempotency_key:
                                                        createIdempotencyKey(),
                                                })
                                            }
                                        >
                                            <RefreshCwIcon
                                                data-icon="inline-start"
                                                aria-hidden="true"
                                            />
                                            {operations.prepare.isPending
                                                ? "正在启动准备"
                                                : "重新准备"}
                                        </LoadingButton>
                                    )}
                                    {onEdit ? (
                                        <Button
                                            id="sales-selection-failure-adjust"
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            disabled={pending}
                                            onClick={onEdit}
                                        >
                                            调整来源与档位
                                        </Button>
                                    ) : null}
                                </>
                            )}
                        </div>
                        <details className="text-xs">
                            <summary
                                id="sales-selection-failure-details"
                                className="w-fit cursor-pointer underline underline-offset-4"
                            >
                                查看失败原因
                            </summary>
                            <div className="mt-1 break-words">
                                {failure.reason}
                            </div>
                        </details>
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
                            const isFailed = isCurrent && Boolean(failure)
                            const isUpcoming = activeStep < step.stepIndex

                            return (
                                <li
                                    key={step.status}
                                    aria-current={
                                        isCurrent ? "step" : undefined
                                    }
                                    className="flex flex-1 items-center last:flex-none"
                                >
                                    <div className="flex items-center gap-2.5">
                                        <span
                                            className={cn(
                                                "inline-flex size-6 shrink-0 items-center justify-center rounded-full text-xs font-semibold transition-colors",
                                                isDone &&
                                                    "bg-primary text-primary-foreground",
                                                isCurrent &&
                                                    !isFailed &&
                                                    "bg-primary text-primary-foreground ring-4 ring-primary/20",
                                                isFailed &&
                                                    "bg-destructive-soft text-destructive-soft-foreground ring-4 ring-destructive/20",
                                                isUpcoming &&
                                                    "border border-border bg-muted text-muted-foreground",
                                            )}
                                        >
                                            {isDone ? (
                                                <CheckIcon
                                                    className="size-3.5 stroke-[2.5]"
                                                    aria-hidden="true"
                                                />
                                            ) : isFailed ? (
                                                <CircleAlertIcon
                                                    className="size-3.5"
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
                                                isFailed &&
                                                    "text-destructive-soft-foreground",
                                                isDone &&
                                                    "font-medium text-foreground",
                                                isUpcoming &&
                                                    "text-muted-foreground",
                                            )}
                                        >
                                            {isFailed ? "准备失败" : step.label}
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
            {detail.status === "DRAFT" && !failure ? (
                <Alert
                    variant="info"
                    className="flex flex-wrap items-center justify-between gap-4"
                >
                    <div className="min-w-0 space-y-0.5">
                        <div className="flex items-center gap-2 text-sm font-semibold">
                            <SparklesIcon
                                className="size-4"
                                aria-hidden="true"
                            />
                            <span>选品册已创建，请开始准备陈列</span>
                        </div>
                        <p className="text-sm">
                            选品形态：
                            {SELECTION_FORM_LABEL[detail.selection_form]} ·
                            商品来源：{POOL_SOURCE_LABEL[detail.source_kind]}
                            。点击右上角「开始准备」，系统将根据规则自动生成商品或套餐清单。
                        </p>
                    </div>
                    {onEdit ? (
                        <Button
                            id="sales-selection-draft-banner-edit"
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={pending}
                            onClick={onEdit}
                        >
                            调整来源与档位
                        </Button>
                    ) : null}
                </Alert>
            ) : null}

            {detail.status === "PREPARING" ? (
                <Alert variant="info">
                    <LoaderCircleIcon
                        className="animate-spin"
                        aria-hidden="true"
                    />
                    <AlertTitle>正在准备陈列商品</AlertTitle>
                    <AlertDescription className="space-y-2.5">
                        <p>
                            当前处理环节：
                            <span className="font-medium">
                                {STAGE_LABELS[
                                    detail.prepare_stage ?? "QUEUED"
                                ] ?? "处理商品"}
                            </span>
                            。系统正在生成商品与套餐清单，完成后即可核对价格并发布。
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

            {detail.status === "PENDING_PUBLISH" && !failure ? (
                <Alert
                    variant="info"
                    className="flex flex-wrap items-center justify-between gap-4"
                >
                    <div className="min-w-0 space-y-0.5">
                        <div className="flex items-center gap-2 text-sm font-semibold">
                            <FileCheck2Icon
                                className="size-4"
                                aria-hidden="true"
                            />
                            <span>陈列已就绪，请核对商品与售价</span>
                        </div>
                        <p className="text-sm">
                            请核对下方商品、规格与客户售价（共{" "}
                            <span className="num font-medium">
                                {detail.display_count}
                            </span>{" "}
                            项），确认无误后点击右上角「发布」。
                        </p>
                    </div>
                    {detail.selection_form === "PACKAGE" && onEdit ? (
                        <Button
                            id="sales-selection-pending-banner-edit"
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={pending}
                            onClick={onEdit}
                        >
                            调整来源与档位
                        </Button>
                    ) : null}
                </Alert>
            ) : null}

            {detail.status === "PUBLISHED" ? (
                <Alert
                    variant="success"
                    className="flex flex-wrap items-center justify-between gap-4"
                >
                    <div className="min-w-0 space-y-0.5">
                        <div className="flex items-center gap-2 text-sm font-semibold">
                            <SendIcon className="size-4" aria-hidden="true" />
                            <span>选品册已发布，客户专属链接生效中</span>
                        </div>
                        <p className="text-sm">
                            专属安全链接已生成（30天有效）。可复制右侧链接发送给客户，客户在手机端打开即可选品。
                        </p>
                    </div>
                    {publicHref ? (
                        <LoadingButton
                            id="sales-selection-detail-copy-link-banner"
                            loading={operations.copyLink.isPending}
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
                        </LoadingButton>
                    ) : null}
                </Alert>
            ) : null}

            {detail.status === "SUBMITTED" ? (
                <Alert
                    variant="success"
                    className="flex flex-wrap items-center justify-between gap-4"
                >
                    <div className="min-w-0 space-y-0.5">
                        <div className="flex items-center gap-2 text-sm font-semibold">
                            <CheckIcon className="size-4" aria-hidden="true" />
                            <span>客户已完成选品并提交方案</span>
                        </div>
                        <p className="text-sm">
                            客户已提交选品，商品清单已锁定。
                            {detail.proposal_no ? (
                                <>
                                    销售方案编号：
                                    <span className="font-semibold">
                                        {detail.proposal_no}
                                    </span>
                                    ，
                                </>
                            ) : null}
                            可进入销售方案查看客户确认明细并继续开单。
                        </p>
                    </div>
                    {canOpenProposal && detail.proposal_id ? (
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
                </Alert>
            ) : null}

            {isTerminal ? (
                <div className="rounded-xl border border-border bg-muted/40 p-4">
                    <p className="text-sm font-semibold text-foreground">
                        该选品册已{detail.status === "CLOSED" ? "关闭" : "作废"}
                    </p>
                    <p className="mt-0.5 text-xs text-muted-foreground">
                        该选品册已结束，仅支持查看历史商品清单，不可再修改或重新发布。
                    </p>
                </div>
            ) : null}
        </div>
    )
}
