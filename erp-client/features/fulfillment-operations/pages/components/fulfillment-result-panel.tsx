"use client"

import Link from "next/link"
import { ArrowRightIcon, LoaderCircleIcon } from "lucide-react"

import { FormalActionResult } from "@/components/business"
import type { ResultState as SharedResultState } from "@/components/business/feedback"
import { Button } from "@/components/ui/button"
import { buildPostedFacts } from "@/features/fulfillment-operations/lib/validation"
import type { FulfillmentFormalOutcome } from "@/features/fulfillment-operations/types"
import { NOT_ACCEPTANCE_NOTICE } from "@/features/fulfillment-operations/types"
import { acceptanceHref } from "@/features/fulfillment-operations/pages/lib/gate-copy"

type ResultState = SharedResultState<FulfillmentFormalOutcome>

export type FulfillmentResultPanelProps = {
    lastResult: ResultState
    currentUrl: string
    resolvePending: boolean
    onResolveUnknown: () => void
    onNext: () => void
    /** 入库完成后切到同一销售单的待仓发。 */
    onContinueWarehouseShip?: (salesOrderId: string) => void
    /** 嵌入销售单详情时原地展开客户验收。 */
    onOpenAcceptance?: () => void
    showNext?: boolean
}

/**
 * 确认/查询之后的结果面板：结果、记录要点与后续动作。
 * PurchaseReceipt 为 NO_APPROVAL，入库创建结果不展示绑定卡、决定或审批历史。
 * Delivery 为 NO_APPROVAL，仓发/直发创建结果不展示绑定卡、待办或审批入口。
 * ElectronicDelivery 为 NO_APPROVAL，电子交付创建结果不展示绑定卡、决定或审批历史。
 * ServiceFulfillment 为 NO_APPROVAL，服务履约创建结果不展示绑定卡、决定或审批历史。
 * CustomerAcceptance 为 NO_APPROVAL，履约结果交接客户验收时不展示绑定卡、决定或审批历史。
 */
export function FulfillmentResultPanel({
    lastResult,
    currentUrl,
    resolvePending,
    onResolveUnknown,
    onNext,
    onContinueWarehouseShip,
    onOpenAcceptance,
    showNext = true,
}: FulfillmentResultPanelProps) {
    if (!lastResult) return null

    return (
        <FormalActionResult
            status={
                lastResult.status === "failed" ? "blocked" : lastResult.status
            }
            title={lastResult.title}
            description={
                lastResult.outcome?.kind === "POSTED" &&
                lastResult.outcome.acceptanceRequired ? (
                    <span className="block space-y-1">
                        <span className="block">{lastResult.description}</span>
                        <span className="block text-muted-foreground">
                            {NOT_ACCEPTANCE_NOTICE}
                        </span>
                    </span>
                ) : (
                    lastResult.description
                )
            }
            reference={lastResult.reference}
            facts={
                lastResult.outcome
                    ? buildPostedFacts(lastResult.outcome)
                    : undefined
            }
            actions={
                <div className="flex flex-wrap gap-2">
                    {lastResult.status === "unknown" ? (
                        <Button
                            id="fulfillment-operations-result-resolve-unknown"
                            type="button"
                            size="sm"
                            disabled={resolvePending}
                            onClick={() => void onResolveUnknown()}
                        >
                            {resolvePending ? (
                                <LoaderCircleIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                    className="animate-spin"
                                />
                            ) : null}
                            {resolvePending ? "查询中…" : "查询最终结果"}
                        </Button>
                    ) : null}
                    {lastResult.outcome?.kind === "POSTED" &&
                    lastResult.outcome.operationType === "RECEIPT" &&
                    lastResult.outcome.salesOrderId &&
                    onContinueWarehouseShip ? (
                        <Button
                            id="fulfillment-operations-result-continue-warehouse-ship"
                            type="button"
                            size="sm"
                            onClick={() =>
                                onContinueWarehouseShip(
                                    lastResult.outcome!.salesOrderId,
                                )
                            }
                        >
                            本单待仓发
                            <ArrowRightIcon data-icon="inline-end" />
                        </Button>
                    ) : null}
                    {lastResult.outcome?.kind === "POSTED" &&
                    lastResult.outcome.acceptanceRequired ? (
                        onOpenAcceptance ? (
                            <Button
                                id="fulfillment-operations-result-open-acceptance"
                                type="button"
                                size="sm"
                                variant="outline"
                                onClick={onOpenAcceptance}
                            >
                                去登记客户验收
                                <ArrowRightIcon data-icon="inline-end" />
                            </Button>
                        ) : (
                            <Button
                                id="fulfillment-operations-result-go-acceptance"
                                type="button"
                                size="sm"
                                variant="outline"
                                render={
                                    <Link
                                        href={acceptanceHref(
                                            lastResult.outcome.salesOrderId,
                                            currentUrl,
                                        )}
                                    />
                                }
                            >
                                去登记客户验收
                                <ArrowRightIcon data-icon="inline-end" />
                            </Button>
                        )
                    ) : null}
                    {!showNext ||
                    lastResult.stayOnItem === false ||
                    lastResult.status === "blocked" ? null : (
                        <Button
                            id="fulfillment-operations-result-next"
                            type="button"
                            size="sm"
                            onClick={onNext}
                        >
                            下一条
                        </Button>
                    )}
                </div>
            }
        />
    )
}
