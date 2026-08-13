import type { Ref } from "react"
import Link from "next/link"
import { ArrowRightIcon, CircleCheckIcon } from "lucide-react"

import { type ResultState } from "@/components/business/feedback"
import {
    FormalActionResult,
    surfaceInsetClassName,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
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

type ProcurementOutcomeFeedbackProps = {
    finishedResult: ResultState<FormalOutcome>
    lastResult: ResultState<FormalOutcome>
    fallbackSalesOrderId?: string
    context?: { position: number; total: number }
    submissionNo?: number
    returnTo: string
    resultRef: Ref<HTMLDivElement>
    onDismissFinished: () => void
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
    onNext,
}: ProcurementOutcomeFeedbackProps) {
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
                            : "已自动生成采购单草稿，可直接核对并提交"}
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

            {lastResult ? (
                <div ref={resultRef} tabIndex={-1} className="outline-none">
                    <FormalActionResult
                        status={
                            lastResult.status === "failed"
                                ? "blocked"
                                : lastResult.status
                        }
                        title={lastResult.title}
                        description={lastResult.description}
                        reference={lastResult.reference}
                        facts={buildProcurementResultFacts(
                            lastResult.outcome,
                            context,
                            submissionNo,
                        )}
                        actions={
                            <ProcurementResultActions
                                lastResult={lastResult}
                                taskSalesOrderId={
                                    lastResult.outcome &&
                                    "salesOrderId" in lastResult.outcome
                                        ? lastResult.outcome.salesOrderId
                                        : fallbackSalesOrderId
                                }
                                returnTo={returnTo}
                                onNext={onNext}
                            />
                        }
                    />
                    {lastResult.outcome?.kind === "REJECTED_TO_SALES" ? (
                        <ProcurementRejectionNextSteps
                            salesOrderId={lastResult.outcome.salesOrderId}
                            returnTo={returnTo}
                        />
                    ) : null}
                </div>
            ) : null}
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
                label: "采购单草稿",
                value:
                    outcome.purchaseOrders.length > 0
                        ? outcome.purchaseOrders
                              .map((order) => order.purchaseNo)
                              .join("、")
                        : "未生成，请联系管理员核查",
            },
            { label: "下一环节", value: "核对采购单草稿并提交" },
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
            { label: "销售下一步", value: "改品/改价后重提，或作废" },
        ]
    }
    return [
        {
            label: "任务状态",
            value:
                outcome.workItemStatus === "IN_PROGRESS" ? "处理中" : "待处理",
        },
        {
            label: "处理状态",
            value: outcome.leaseDisposition === "RELEASED" ? "已结束" : "保留",
        },
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
            {lastResult.outcome?.kind === "APPROVED_AND_SALES_EFFECTIVE"
                ? lastResult.outcome.purchaseOrders.map((order) => (
                      <Button
                          key={order.purchaseOrderId}
                          type="button"
                          size="sm"
                          variant="outline"
                          render={
                              <Link
                                  href={`/procurement/orders/${order.purchaseOrderId}`}
                              />
                          }
                      >
                          查看采购单 · {order.purchaseNo}
                      </Button>
                  ))
                : null}
            {lastResult.stayOnItem !== false ||
            lastResult.status === "blocked" ? (
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
        <Card size="sm" className={`mt-3 ${surfacePanelClassName}`}>
            <CardHeader className="rounded-t-lg border-b border-border/30">
                <CardTitle>销售三条固定出路</CardTitle>
                <CardDescription>
                    上一驳回提交已作废；本页只读展示出路，不代销售选择。
                </CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
                <ol className="list-decimal space-y-3 pl-5 text-sm">
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
            </CardContent>
        </Card>
    )
}
