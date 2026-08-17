"use client"

import type { Ref } from "react"
import Link from "next/link"
import { ArrowRightIcon, CircleCheckIcon } from "lucide-react"

import { type ResultState } from "@/components/business/feedback"
import {
    BusinessStatusBadge,
    surfaceInsetClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import {
    NEXT_SALES_RESOLUTION_COPY,
    REJECT_REASON_LABEL,
    type FormalOutcome,
} from "@/features/procurement-confirmation/types"

function salesOrderHref(
    salesOrderId: string,
    returnTo: string,
    section?: string,
) {
    const params = new URLSearchParams({
        from: "W07",
        returnTo,
    })
    if (section) params.set("section", section)
    return `/sales/orders/${salesOrderId}?${params.toString()}`
}

function resultBadge(
    status: NonNullable<ResultState<FormalOutcome>>["status"],
) {
    if (status === "rejected") {
        return { label: "未通过", tone: "destructive" as const }
    }
    if (status === "failed" || status === "unknown") {
        return { label: "待核实", tone: "warning" as const }
    }
    return { label: "已完成", tone: "success" as const }
}

type ProcurementOutcomeFeedbackProps = {
    finishedResult: ResultState<FormalOutcome>
    lastResult: ResultState<FormalOutcome>
    fallbackSalesOrderId?: string
    context?: { position: number; total: number }
    submissionNo?: number
    returnTo: string
    resultRef: Ref<HTMLDivElement>
    onDismissFinished: () => void
    onDismissLastResult: () => void
    onNext: () => void
}

export function ProcurementOutcomeFeedback({
    finishedResult,
    lastResult,
    fallbackSalesOrderId,
    context,
    submissionNo,
    returnTo,
    resultRef,
    onDismissFinished,
    onDismissLastResult,
    onNext,
}: ProcurementOutcomeFeedbackProps) {
    const salesOrderId =
        lastResult?.outcome && "salesOrderId" in lastResult.outcome
            ? lastResult.outcome.salesOrderId
            : fallbackSalesOrderId

    return (
        <>
            {finishedResult && !lastResult ? (
                <div
                    role="status"
                    className={`${surfaceInsetClassName} flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-2 text-sm`}
                >
                    <CircleCheckIcon
                        className="size-4 shrink-0 text-success-soft-foreground"
                        aria-hidden="true"
                    />
                    <span className="font-medium">
                        上一项已
                        {finishedResult.status === "rejected" ? "驳回" : "通过"}
                    </span>
                    {finishedResult.reference ? (
                        <span className="num text-muted-foreground">
                            {finishedResult.reference}
                        </span>
                    ) : null}
                    <span className="text-muted-foreground">
                        {finishedResult.status === "rejected"
                            ? "销售可在销售单选择三条固定出路"
                            : "已形成采购创建依据，后续建单将另行执行"}
                    </span>
                    <div className="ml-auto flex flex-wrap items-center gap-2">
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            render={
                                <Link
                                    href={salesOrderHref(
                                        finishedResult.outcome &&
                                            "salesOrderId" in
                                                finishedResult.outcome
                                            ? finishedResult.outcome
                                                  .salesOrderId
                                            : (fallbackSalesOrderId ?? "#"),
                                        returnTo,
                                    )}
                                />
                            }
                        >
                            打开销售单
                        </Button>
                        <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            onClick={onDismissFinished}
                        >
                            关闭
                        </Button>
                    </div>
                </div>
            ) : null}

            <Dialog
                open={Boolean(lastResult)}
                onOpenChange={(open) => {
                    if (!open) onDismissLastResult()
                }}
            >
                {lastResult ? (
                    <DialogContent ref={resultRef} className="sm:max-w-lg">
                        <DialogHeader>
                            <DialogTitle className="flex flex-wrap items-center gap-2 pr-8">
                                <span>{lastResult.title}</span>
                                <BusinessStatusBadge
                                    context="list"
                                    {...resultBadge(lastResult.status)}
                                />
                            </DialogTitle>
                            <DialogDescription>
                                {lastResult.description}
                            </DialogDescription>
                        </DialogHeader>
                        <div className="flex flex-col gap-4">
                            {lastResult.reference ? (
                                <p className="text-xs text-muted-foreground">
                                    结果编号：
                                    <span className="num font-mono">
                                        {lastResult.reference}
                                    </span>
                                </p>
                            ) : null}
                            <dl className="grid gap-3 sm:grid-cols-2">
                                {buildProcurementResultFacts(
                                    lastResult.outcome,
                                    context,
                                    submissionNo,
                                ).map((fact) => (
                                    <div key={fact.label}>
                                        <dt className="text-xs text-muted-foreground">
                                            {fact.label}
                                        </dt>
                                        <dd className="mt-1 text-sm">
                                            {fact.value}
                                        </dd>
                                    </div>
                                ))}
                            </dl>
                            {lastResult.outcome?.kind ===
                            "REJECTED_TO_SALES" ? (
                                <ProcurementRejectionNextSteps
                                    salesOrderId={
                                        lastResult.outcome.salesOrderId
                                    }
                                    returnTo={returnTo}
                                />
                            ) : null}
                        </div>
                        <DialogFooter>
                            <DialogClose
                                render={
                                    <Button type="button" variant="outline" />
                                }
                            >
                                关闭
                            </DialogClose>
                            <ProcurementResultActions
                                lastResult={lastResult}
                                taskSalesOrderId={salesOrderId}
                                returnTo={returnTo}
                                onNext={onNext}
                            />
                        </DialogFooter>
                    </DialogContent>
                ) : null}
            </Dialog>
        </>
    )
}

export function buildProcurementResultFacts(
    outcome: FormalOutcome | undefined,
    context:
        | {
              position: number
              total: number
          }
        | undefined,
    submissionNo?: number,
) {
    if (!outcome) {
        return [
            {
                label: "队列位置",
                value: context
                    ? `第 ${context.position}/${context.total}`
                    : "—",
            },
        ]
    }
    if (outcome.kind === "APPROVED_AND_SALES_EFFECTIVE") {
        return [
            { label: "销售单", value: outcome.salesOrderNo },
            {
                label: "提交记录",
                value: submissionNo ? `第 ${submissionNo} 次提交` : "本次提交",
            },
            { label: "处理结果", value: "销售单已生效" },
            {
                label: "采购创建依据",
                value: outcome.procurementCreationBasisId,
            },
            { label: "下一环节", value: "按创建依据进入采购建单" },
        ]
    }
    if (outcome.kind === "REJECTED_TO_SALES") {
        return [
            { label: "销售单", value: outcome.salesOrderNo },
            {
                label: "处理结果",
                value: "本次提交已驳回，未创建采购单或后继任务",
            },
            {
                label: "驳回原因",
                value: `${REJECT_REASON_LABEL[outcome.rejectReasonCode]} · ${outcome.comment}`,
            },
            {
                label: "销售下一步",
                value: "改品/改价后重提、申请低毛利上级确认，或作废",
            },
        ]
    }
    return [
        { label: "任务状态", value: "开放" },
        { label: "责任状态", value: "已退回团队" },
    ]
}

export function ProcurementResultActions({
    lastResult,
    taskSalesOrderId,
    returnTo,
    onNext,
}: {
    lastResult: NonNullable<ResultState<FormalOutcome>>
    taskSalesOrderId?: string
    returnTo: string
    onNext: () => void
}) {
    return (
        <>
            {taskSalesOrderId ? (
                <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    render={
                        <Link
                            href={salesOrderHref(taskSalesOrderId, returnTo)}
                        />
                    }
                >
                    查看详情
                </Button>
            ) : null}
            {lastResult.status !== "unknown" &&
            (lastResult.stayOnItem !== false ||
                lastResult.status === "blocked") ? (
                <Button type="button" size="sm" onClick={onNext}>
                    打开下一条
                    <ArrowRightIcon data-icon="inline-end" aria-hidden="true" />
                </Button>
            ) : null}
        </>
    )
}

export function ProcurementRejectionNextSteps({
    salesOrderId,
    returnTo,
}: {
    salesOrderId: string
    returnTo: string
}) {
    return (
        <div className="flex flex-col gap-3 rounded-lg border border-border bg-muted/30 p-3">
            <div>
                <p className="text-sm font-medium">销售三条固定出路</p>
                <p className="mt-1 text-xs text-muted-foreground">
                    上一驳回提交已作废；这里只读展示出路，不代销售选择。
                </p>
            </div>
            <ol className="flex list-decimal flex-col gap-2 pl-5 text-sm">
                {NEXT_SALES_RESOLUTION_COPY.map((item) => (
                    <li key={item.code}>
                        <p className="font-medium">{item.title}</p>
                        <p className="text-muted-foreground">
                            {item.description}
                        </p>
                    </li>
                ))}
            </ol>
            <Button
                variant="outline"
                size="sm"
                render={
                    <Link
                        href={salesOrderHref(
                            salesOrderId,
                            returnTo,
                            "procurement-rejection",
                        )}
                    />
                }
            >
                打开销售单驳回处理
                <ArrowRightIcon data-icon="inline-end" aria-hidden="true" />
            </Button>
        </div>
    )
}
