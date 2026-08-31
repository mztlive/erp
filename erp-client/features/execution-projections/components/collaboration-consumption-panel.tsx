"use client"

import Link from "next/link"
import { ExternalLinkIcon } from "lucide-react"

import { MoneyValue, surfaceInsetClassName } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { useSalesOrderConsumptionSummaryQuery } from "@/features/mall-consumption-orders/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { getErrorMessage } from "@/lib/api/errors"

/** 协同子区底部：商城侧消费情况（仅供查阅，不影响结案）。 */
export function CollaborationConsumptionPanel({
    salesOrderId,
    salesOrderNo,
}: {
    salesOrderId: string
    salesOrderNo: string
}) {
    const consumptionQuery = useSalesOrderConsumptionSummaryQuery(salesOrderId)

    return (
        <section
            className={`${surfaceInsetClassName} space-y-3 p-3 sm:col-span-2`}
        >
            <div>
                <h3 className="text-sm font-medium">商城侧消费情况</h3>
                <p className="mt-1 text-xs text-muted-foreground">
                    仅供查阅；持卡人消费多少都不决定本单是否结案。
                </p>
            </div>
            {consumptionQuery.isPending ? (
                <div className="h-12 animate-pulse rounded-lg bg-muted" />
            ) : consumptionQuery.isError ? (
                <Alert variant="destructive" role="alert" className="py-2">
                    <AlertTitle className="text-sm">
                        消费情况加载失败
                    </AlertTitle>
                    <AlertDescription className="text-xs">
                        {getErrorMessage(
                            consumptionQuery.error,
                            "无法读取商城消费订单汇总，请刷新后重试。",
                        )}
                    </AlertDescription>
                </Alert>
            ) : (
                <dl className="grid gap-3 text-sm sm:grid-cols-4">
                    <div>
                        <dt className="text-xs text-muted-foreground">
                            消费订单
                        </dt>
                        <dd className="num font-medium">
                            {consumptionQuery.data?.orderCount ?? 0} 单
                        </dd>
                    </div>
                    <div>
                        <dt className="text-xs text-muted-foreground">
                            支付成功
                        </dt>
                        <dd>
                            <MoneyValue
                                value={
                                    consumptionQuery.data?.paidAmount ?? "0.00"
                                }
                            />
                        </dd>
                    </div>
                    <div>
                        <dt className="text-xs text-muted-foreground">
                            商城退款
                        </dt>
                        <dd>
                            <MoneyValue
                                value={
                                    consumptionQuery.data?.refundedAmount ??
                                    "0.00"
                                }
                            />
                        </dd>
                    </div>
                    <div>
                        <dt className="text-xs text-muted-foreground">
                            余额恢复
                        </dt>
                        <dd>
                            <MoneyValue
                                value={
                                    consumptionQuery.data
                                        ?.restoredBalanceAmount ?? "0.00"
                                }
                            />
                        </dd>
                    </div>
                </dl>
            )}
            <div className="flex flex-wrap items-center justify-between gap-2">
                <p className="text-xs text-muted-foreground">
                    最近记录 {consumptionQuery.data?.latestFactAt ?? "暂无"}
                    ；本单结案仍看交付与回款是否完成。
                </p>
                <Button
                    id={`execution-collaboration-${toAutomationIdSegment(salesOrderId)}-consumption-orders`}
                    type="button"
                    size="sm"
                    variant="outline"
                    render={
                        <Link
                            id={`execution-collaboration-${toAutomationIdSegment(salesOrderId)}-consumption-orders`}
                            href={`/commerce/consumption-orders?from=W05&salesOrderId=${encodeURIComponent(salesOrderId)}&q=${encodeURIComponent(salesOrderNo)}`}
                        />
                    }
                >
                    <ExternalLinkIcon
                        data-icon="inline-start"
                        aria-hidden="true"
                    />
                    查看商城消费订单
                </Button>
            </div>
        </section>
    )
}
