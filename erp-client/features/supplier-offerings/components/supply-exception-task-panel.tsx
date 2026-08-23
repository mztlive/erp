"use client"

import Link from "next/link"
import { ClipboardCheckIcon, ShieldAlertIcon } from "lucide-react"

import { BusinessFailureState } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import type {
    SupplierOfferingView,
    SupplierSupplyExceptionWorkItem,
} from "@/features/supplier-offerings/types"
import type { WorkItemAllowedAction } from "@/features/work-items/types"

const ACTION_LABELS: Readonly<Partial<Record<WorkItemAllowedAction, string>>> =
    {
        VIEW: "可查看",
        PROCESS: "可核对",
        REASSIGN: "可转交",
    }

function responsibility(task: SupplierSupplyExceptionWorkItem): string {
    if (task.ownerUser) return task.ownerUser.displayName
    return "责任人信息不可用"
}

function sourceLabel(
    task: SupplierSupplyExceptionWorkItem,
    offering?: SupplierOfferingView,
): string {
    if (!offering) return task.businessObjectLabel
    return [
        offering.supplier_name ?? offering.supplier_no,
        offering.supplier_sku_code,
        offering.sku_no,
    ]
        .filter(Boolean)
        .join(" · ")
}

/** W22 安全暂停在 W21 的只读核对面；不提供业务终结或恢复动作。 */
export function SupplyExceptionTaskPanel({
    workItemId,
    queueContextId,
    task,
    offering,
    isPending,
    error,
    onRetry,
}: {
    workItemId: string
    queueContextId?: string
    task?: SupplierSupplyExceptionWorkItem
    offering?: SupplierOfferingView
    isPending: boolean
    error?: Error | null
    onRetry: () => void
}) {
    const returnParams = new URLSearchParams({
        currentWorkItemId: workItemId,
    })
    if (queueContextId) returnParams.set("queueContextId", queueContextId)
    const returnHref = `/workspace/tasks?${returnParams.toString()}`

    if (isPending) {
        return (
            <div
                className="h-44 animate-pulse rounded-xl border bg-muted/50"
                aria-label="正在核对供应停止任务"
            />
        )
    }

    if (error || !task) {
        return (
            <BusinessFailureState
                title="供应停止任务已阻止"
                description="当前责任、任务版本或供给对象未通过校验。本页不会提供供给写入动作。"
                error={error}
                onRetry={onRetry}
                action={
                    <div className="flex flex-wrap gap-2">
                        <Button
                            type="button"
                            variant="outline"
                            onClick={onRetry}
                        >
                            重试校验
                        </Button>
                        <Button
                            type="button"
                            variant="outline"
                            render={<Link href={returnHref} />}
                        >
                            返回待办队列
                        </Button>
                    </div>
                }
            />
        )
    }

    return (
        <Card className="border-destructive/40">
            <CardHeader>
                <div className="flex flex-wrap items-start justify-between gap-3">
                    <div>
                        <CardTitle className="flex items-center gap-2">
                            <ShieldAlertIcon
                                className="size-4 text-destructive"
                                aria-hidden="true"
                            />
                            供应停止核对
                            <Badge variant="destructive">保持待处理</Badge>
                        </CardTitle>
                        <CardDescription className="mt-1">
                            仅核对来源、已固定的暂停影响并准备候选证据；不选定替代供给，不发起恢复发布。
                        </CardDescription>
                    </div>
                    <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        render={<Link href={returnHref} />}
                    >
                        返回待办队列
                    </Button>
                </div>
            </CardHeader>
            <CardContent className="space-y-4">
                <Alert variant="destructive">
                    <ShieldAlertIcon aria-hidden="true" />
                    <AlertTitle>此任务保持待处理</AlertTitle>
                    <AlertDescription>
                        当前合同没有候选证据保存、任务完成或安全暂停恢复命令。本页不会恢复商品销售。
                    </AlertDescription>
                </Alert>

                <dl className="grid gap-px overflow-hidden rounded-lg border bg-border sm:grid-cols-2 lg:grid-cols-4">
                    <div className="bg-card p-3">
                        <dt className="text-xs text-muted-foreground">
                            来源供给
                        </dt>
                        <dd className="mt-1 text-sm font-medium">
                            {sourceLabel(task, offering)}
                        </dd>
                    </div>
                    <div className="bg-card p-3">
                        <dt className="text-xs text-muted-foreground">
                            当前责任
                        </dt>
                        <dd className="mt-1 text-sm font-medium">
                            {responsibility(task)}
                        </dd>
                        <div className="mt-1 text-xs text-muted-foreground">
                            {task.ownerRoleLabel} ·{" "}
                            {task.ownerOrganization.displayName}
                        </div>
                    </div>
                    <div className="bg-card p-3">
                        <dt className="text-xs text-muted-foreground">
                            任务版本
                        </dt>
                        <dd className="num mt-1 break-all text-sm">
                            {task.taskVersion}
                        </dd>
                    </div>
                    <div className="bg-card p-3">
                        <dt className="text-xs text-muted-foreground">
                            来源版本
                        </dt>
                        <dd className="num mt-1 break-all text-sm">
                            {task.subjectVersion}
                        </dd>
                    </div>
                </dl>

                <div className="grid gap-3 lg:grid-cols-2">
                    <div className="rounded-lg border p-3">
                        <div className="text-xs font-medium text-muted-foreground">
                            已固定影响
                        </div>
                        <p className="mt-1 text-sm">{task.impactSummary}</p>
                        <p className="mt-2 text-xs text-muted-foreground">
                            原因：{task.reasonLabel}
                            。影响只使用任务记录，不在页面重新计算。
                        </p>
                        <p className="mt-2 text-xs text-muted-foreground">
                            {offering
                                ? "当前列表行只用于识别供给对象，不覆盖任务冻结的来源版本。"
                                : "当前分页未加载完整供给行；不在页面推断来源记录。"}
                        </p>
                    </div>
                    <div className="rounded-lg border p-3">
                        <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
                            <ClipboardCheckIcon
                                className="size-4"
                                aria-hidden="true"
                            />
                            核对边界
                        </div>
                        <ul className="mt-2 list-disc space-y-1 pl-4 text-sm">
                            <li>核对停供来源和来源版本。</li>
                            <li>确认安全暂停影响已由系统固定。</li>
                            <li>准备候选证据，等待受控处理能力接入。</li>
                        </ul>
                    </div>
                </div>

                <div>
                    <div className="text-xs font-medium text-muted-foreground">
                        当前允许动作
                    </div>
                    <div className="mt-2 flex flex-wrap gap-2">
                        {task.allowedActions.length > 0 ? (
                            task.allowedActions.map((action) => (
                                <Badge key={action} variant="outline">
                                    {ACTION_LABELS[action] ?? action}
                                </Badge>
                            ))
                        ) : (
                            <Badge variant="secondary">当前只读</Badge>
                        )}
                    </div>
                    {task.processingBlocker ? (
                        <p className="mt-2 text-xs text-destructive">
                            {task.processingBlocker.message}
                        </p>
                    ) : null}
                    {task.actionBlockers.length > 0 ? (
                        <ul className="mt-2 list-disc space-y-1 pl-4 text-xs text-muted-foreground">
                            {task.actionBlockers.map((blocker) => (
                                <li key={blocker}>{blocker}</li>
                            ))}
                        </ul>
                    ) : null}
                    <p className="mt-2 text-xs text-muted-foreground">
                        转交属于统一待办责任动作；W21
                        不把它解释为恢复发布或完成任务。
                    </p>
                </div>
            </CardContent>
        </Card>
    )
}
