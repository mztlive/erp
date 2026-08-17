"use client"

import type { Ref } from "react"
import Link from "next/link"
import { CircleCheckIcon, FileSearchIcon } from "lucide-react"

import { ValidationSummary, surfacePanelClassName } from "@/components/business"
import type { ValidationIssue } from "@/components/business/feedback"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
import { canOpenProcurementConfirmPlan } from "@/features/procurement-confirmation/lib/actions"
import type {
    ConfirmationLineDraft,
    CoverageByLine,
    ProcurementConfirmationTask,
    ProcurementRecommendation,
} from "@/features/procurement-confirmation/types"
import { FULFILLMENT_MODE_LABEL } from "@/features/procurement-confirmation/types"
import { cn } from "@/lib/utils"

const money = new Intl.NumberFormat("zh-CN", {
    style: "currency",
    currency: "CNY",
    minimumFractionDigits: 2,
})

type ProcurementConfirmationSidebarProps = {
    task: ProcurementConfirmationTask
    headingRef: Ref<HTMLHeadingElement>
    formalPending: boolean
    onReject: () => Promise<void>
    onConfirm: () => Promise<void>
    onStartProcessing: () => Promise<void>
    onReleaseToTeam: () => Promise<void>
    coverage: readonly CoverageByLine[]
    estimatedPurchase: string | undefined
    lineDrafts: readonly ConfirmationLineDraft[]
    recommendation: ProcurementRecommendation | undefined
    clientBlocking: readonly ValidationIssue[]
    salesOrderHref: string
}

export function ProcurementConfirmationSidebar({
    task,
    headingRef,
    formalPending,
    onReject,
    onConfirm,
    onStartProcessing,
    onReleaseToTeam,
    coverage,
    estimatedPurchase,
    lineDrafts,
    recommendation,
    clientBlocking,
    salesOrderHref,
}: ProcurementConfirmationSidebarProps) {
    return (
        <aside className="space-y-3 md:space-y-4 xl:sticky xl:top-16 xl:self-start">
            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="rounded-t-lg border-b border-border/30">
                    <CardTitle>
                        <h2
                            ref={headingRef}
                            tabIndex={-1}
                            className="outline-none"
                        >
                            本次确认
                        </h2>
                    </CardTitle>
                    <CardDescription>
                        当前责任和可执行动作以任务记录为准。
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3">
                    <dl className="space-y-2 text-sm">
                        <div className="flex justify-between gap-2">
                            <dt className="text-muted-foreground">销售单</dt>
                            <dd className="font-medium">
                                {task.salesSubmission.salesOrderNo}
                            </dd>
                        </div>
                        <div className="flex justify-between gap-2">
                            <dt className="text-muted-foreground">商品明细</dt>
                            <dd>{task.salesSubmission.lines.length} 项</dd>
                        </div>
                        <div className="flex justify-between gap-2">
                            <dt className="text-muted-foreground">
                                销售含税金额
                            </dt>
                            <dd className="num">
                                {money.format(
                                    Number(task.salesSubmission.grossAmount),
                                )}
                            </dd>
                        </div>
                    </dl>
                    <Alert variant="info">
                        <CircleCheckIcon aria-hidden="true" />
                        <AlertTitle>确认通过将同时生成采购草稿</AlertTitle>
                        <AlertDescription>
                            通过后销售单生效，并按供应商/履约方式拆出采购单草稿，可继续改草稿后提交财务审核。
                        </AlertDescription>
                    </Alert>
                    <div
                        className="grid grid-cols-2 gap-2"
                        role="group"
                        aria-label="本次确认操作"
                    >
                        {task.allowedActions.includes("START_PROCESSING") ? (
                            <Button
                                type="button"
                                disabled={formalPending}
                                onClick={() => void onStartProcessing()}
                            >
                                开始处理
                            </Button>
                        ) : null}
                        {task.allowedActions.includes("REJECT") ? (
                            <Button
                                type="button"
                                variant="destructive"
                                disabled={formalPending}
                                onClick={() => void onReject()}
                            >
                                驳回
                            </Button>
                        ) : null}
                        {canOpenProcurementConfirmPlan(task.allowedActions) ? (
                            <Button
                                type="button"
                                disabled={formalPending}
                                onClick={() => void onConfirm()}
                            >
                                确认通过
                            </Button>
                        ) : null}
                        {task.allowedActions.includes("RELEASE_TO_TEAM") ? (
                            <Button
                                type="button"
                                variant="outline"
                                disabled={formalPending}
                                onClick={() => void onReleaseToTeam()}
                            >
                                退回团队
                            </Button>
                        ) : null}
                    </div>
                    {canOpenProcurementConfirmPlan(task.allowedActions) &&
                    !task.allowedActions.includes("APPROVE") ? (
                        <p className="text-xs text-muted-foreground">
                            打开确认方案后，补齐供应商与数量再提交通过。
                        </p>
                    ) : null}
                    {task.allowedActions.every(
                        (action) =>
                            ![
                                "START_PROCESSING",
                                "SAVE",
                                "APPROVE",
                                "REJECT",
                                "RELEASE_TO_TEAM",
                            ].includes(action),
                    ) ? (
                        <Alert variant="destructive">
                            <AlertTitle>当前不可处理</AlertTitle>
                            <AlertDescription>
                                当前没有可执行的采购确认动作，请刷新任务或联系管理员。
                            </AlertDescription>
                        </Alert>
                    ) : null}
                </CardContent>
            </Card>

            <Card size="sm" className={cn(surfacePanelClassName, "hidden")}>
                <CardHeader className="rounded-t-lg border-b border-border/30">
                    <CardTitle>决策摘要</CardTitle>
                    <CardDescription>
                        数量覆盖按明细独立展示，不可跨行抵消。
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-4">
                    <ul className="space-y-2" aria-label="逐明细数量覆盖">
                        {coverage.map((c) => {
                            const unit = task.salesSubmission.lines.find(
                                (l) =>
                                    l.submissionLineId === c.submissionLineId,
                            )?.unit
                            return (
                                <li
                                    key={c.submissionLineId}
                                    className="flex items-start justify-between gap-2 text-sm"
                                >
                                    <span className="min-w-0 truncate">
                                        {c.itemName}
                                    </span>
                                    <span
                                        className={
                                            c.complete
                                                ? "num shrink-0 text-success-soft-foreground"
                                                : "num shrink-0 text-destructive"
                                        }
                                    >
                                        {c.confirmed}/{c.required} {unit ?? ""}
                                        {!c.complete
                                            ? ` 缺${c.gap} ${unit ?? ""}`
                                            : ""}
                                    </span>
                                </li>
                            )
                        })}
                    </ul>
                    <Separator />
                    <dl className="space-y-2 text-sm">
                        <div className="flex justify-between gap-2">
                            <dt className="text-muted-foreground">
                                推荐采购含税
                            </dt>
                            <dd className="num font-medium">
                                {estimatedPurchase
                                    ? money.format(Number(estimatedPurchase))
                                    : "—"}
                            </dd>
                        </div>
                        <div className="flex justify-between gap-2">
                            <dt className="text-muted-foreground">销售含税</dt>
                            <dd className="num">
                                {money.format(
                                    Number(task.salesSubmission.grossAmount),
                                )}
                            </dd>
                        </div>
                        <div className="flex justify-between gap-2">
                            <dt className="text-muted-foreground">供应商数</dt>
                            <dd className="num">
                                {
                                    new Set(lineDrafts.map((l) => l.supplierId))
                                        .size
                                }{" "}
                                家
                            </dd>
                        </div>
                    </dl>
                    {recommendation?.purchaseOrders.length ? (
                        <>
                            <Separator />
                            <div className="space-y-2">
                                <p className="text-xs font-medium text-muted-foreground">
                                    采购创建依据预览
                                </p>
                                <ul className="space-y-2">
                                    {recommendation.purchaseOrders.map(
                                        (order) => (
                                            <li
                                                key={`${order.supplierId}-${order.fulfillmentMode}`}
                                                className="rounded-md border border-border bg-muted/30 p-2 text-xs"
                                            >
                                                <div className="flex justify-between gap-2">
                                                    <span className="font-medium">
                                                        {order.supplierName}
                                                    </span>
                                                    <span className="num">
                                                        {money.format(
                                                            Number(
                                                                order.estimatedGross,
                                                            ),
                                                        )}
                                                    </span>
                                                </div>
                                                <p className="mt-1 text-muted-foreground">
                                                    {
                                                        FULFILLMENT_MODE_LABEL[
                                                            order
                                                                .fulfillmentMode
                                                        ]
                                                    }{" "}
                                                    · {order.lineCount} 条明细 ·
                                                    确认后生成采购单
                                                </p>
                                            </li>
                                        ),
                                    )}
                                </ul>
                            </div>
                        </>
                    ) : null}
                    {clientBlocking.length > 0 ? (
                        <ValidationSummary
                            title="通过前须补齐"
                            issues={clientBlocking}
                        />
                    ) : (
                        <p className="flex items-center gap-2 text-sm text-success-soft-foreground">
                            <CircleCheckIcon
                                className="size-4"
                                aria-hidden="true"
                            />
                            当前编辑态覆盖完整（最终以系统重验为准）
                        </p>
                    )}
                    {(
                        recommendation?.warnings ??
                        task.decisionSummary.warnings
                    ).map((w, index) => (
                        <p
                            key={`${w.code}-${w.lineId ?? "none"}-${index}`}
                            className="text-xs text-muted-foreground"
                        >
                            警告：{w.message}
                        </p>
                    ))}
                </CardContent>
            </Card>

            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="rounded-t-lg border-b border-border/30">
                    <CardTitle>销售单详情</CardTitle>
                    <CardDescription>
                        深挖后返回仍恢复队列位置、筛选与当前项。
                    </CardDescription>
                </CardHeader>
                <CardContent>
                    <Button
                        variant="outline"
                        className="w-full"
                        render={<Link href={salesOrderHref} />}
                    >
                        <FileSearchIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        打开销售单 · {task.salesSubmission.salesOrderNo}
                    </Button>
                </CardContent>
            </Card>
        </aside>
    )
}
