"use client"

import * as React from "react"
import {
    CircleCheckIcon,
    Clock3Icon,
    FilterIcon,
    ListChecksIcon,
    ShieldAlertIcon,
    ShieldCheckIcon,
    TriangleAlertIcon,
} from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { StatusBadge } from "@/components/ui/status-badge"

export type BatchImpactPreviewProps = Omit<
    React.ComponentProps<typeof Card>,
    "children" | "title"
> & {
    title?: React.ReactNode
    description?: React.ReactNode
    filterSummary: React.ReactNode
    selectionScope: React.ReactNode
    estimated: number
    processable: number
    skipped: number
    background: boolean
    sensitiveFields?: readonly string[]
    skippedReason?: React.ReactNode
    /** 三个数字的列头文案；默认「预计处理 / 可处理 / 将跳过」。 */
    estimatedLabel?: React.ReactNode
    processableLabel?: React.ReactNode
    skippedLabel?: React.ReactNode
}

/** 批量动作执行前的范围、数量、后台任务和敏感字段预览。 */
function BatchImpactPreview({
    title = "批量操作影响预览",
    description,
    filterSummary,
    selectionScope,
    estimated,
    processable,
    skipped,
    background,
    sensitiveFields = [],
    skippedReason,
    estimatedLabel = "预计处理",
    processableLabel = "可处理",
    skippedLabel = "将跳过",
    className,
    ...props
}: BatchImpactPreviewProps) {
    return (
        <Card data-slot="batch-impact-preview" className={className} {...props}>
            <CardHeader>
                <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="min-w-0">
                        <CardTitle className="flex items-center gap-2">
                            <ListChecksIcon
                                aria-hidden="true"
                                className="size-4"
                            />
                            {title}
                        </CardTitle>
                        {description ? (
                            <CardDescription className="mt-1">
                                {description}
                            </CardDescription>
                        ) : null}
                    </div>
                    <StatusBadge
                        tone={background ? "info" : "neutral"}
                        icon={background ? Clock3Icon : CircleCheckIcon}
                        label={background ? "将创建后台任务" : "当前页面内执行"}
                    />
                </div>
            </CardHeader>

            <CardContent className="space-y-4">
                <dl className="grid gap-3 sm:grid-cols-2">
                    <div className="rounded-lg border border-border bg-muted p-4">
                        <dt className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
                            <FilterIcon
                                aria-hidden="true"
                                className="size-3.5"
                            />
                            当前筛选
                        </dt>
                        <dd className="mt-2 text-body-compact text-foreground">
                            {filterSummary}
                        </dd>
                    </div>
                    <div className="rounded-lg border border-border bg-muted p-4">
                        <dt className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
                            <ShieldCheckIcon
                                aria-hidden="true"
                                className="size-3.5"
                            />
                            选择范围
                        </dt>
                        <dd className="mt-2 text-body-compact text-foreground">
                            {selectionScope}
                        </dd>
                    </div>
                </dl>

                <dl className="grid gap-px overflow-hidden rounded-lg border border-border bg-border sm:grid-cols-3">
                    <div className="bg-card p-4">
                        <dt className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
                            <ListChecksIcon
                                aria-hidden="true"
                                className="size-3.5"
                            />
                            {estimatedLabel}
                        </dt>
                        <dd className="num mt-2 text-value-section font-semibold tracking-tight text-foreground">
                            {estimated.toLocaleString("zh-CN")}
                        </dd>
                    </div>
                    <div className="bg-card p-4">
                        <dt className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
                            <CircleCheckIcon
                                aria-hidden="true"
                                className="size-3.5"
                            />
                            {processableLabel}
                        </dt>
                        <dd className="num mt-2 text-value-section font-semibold tracking-tight text-foreground">
                            {processable.toLocaleString("zh-CN")}
                        </dd>
                    </div>
                    <div className="bg-card p-4">
                        <dt className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
                            <TriangleAlertIcon
                                aria-hidden="true"
                                className="size-3.5"
                            />
                            {skippedLabel}
                        </dt>
                        <dd className="num mt-2 text-value-section font-semibold tracking-tight text-foreground">
                            {skipped.toLocaleString("zh-CN")}
                        </dd>
                        {skippedReason != null ? (
                            <div className="mt-2 text-xs text-muted-foreground">
                                {skippedReason}
                            </div>
                        ) : null}
                    </div>
                </dl>

                {sensitiveFields.length > 0 ? (
                    <Alert>
                        <ShieldAlertIcon aria-hidden="true" />
                        <AlertTitle>包含敏感字段</AlertTitle>
                        <AlertDescription>
                            <p>结果将继续执行当前用户的字段权限和遮罩规则。</p>
                            <ul className="mt-2 flex flex-wrap gap-2">
                                {sensitiveFields.map((field) => (
                                    <li key={field}>
                                        <StatusBadge
                                            tone="warning"
                                            icon={ShieldAlertIcon}
                                            label={field}
                                        />
                                    </li>
                                ))}
                            </ul>
                        </AlertDescription>
                    </Alert>
                ) : null}
            </CardContent>
        </Card>
    )
}

export { BatchImpactPreview }

export {
    FormalActionConfirmDialog,
    type FormalActionConfirmDialogProps,
    type ResponsibilityStatus,
    type WorkflowStatus,
} from "@/components/business/workflow-actions"
