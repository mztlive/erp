"use client"

import Link from "next/link"
import { ExternalLinkIcon } from "lucide-react"

import {
    DocumentSection,
    DocumentSummary,
    MoneyValue,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import type { MallConsumptionOrderView } from "@/features/mall-consumption-orders/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    SUPPLIER_CANCEL_LABEL,
    SUPPLIER_REFUND_LABEL,
    SUPPLIER_STATUS_LABEL,
} from "@/features/mall-consumption-orders/types"
import { openWorkspaceLabel } from "@/lib/ui-text"

export function SupplierSection({ view }: { view: MallConsumptionOrderView }) {
    return (
        <DocumentSection title="供应商履约">
            {view.fulfillment.chain === "LEGACY_MANUAL" ? (
                <Alert variant="default">
                    <AlertTitle>原人工履约链 · 无供应商子订单</AlertTitle>
                    <AlertDescription>
                        截止时点前支付只显示原人工履约，历史回填只记账。不创建供应商子订单，也不显示缺单错误。
                    </AlertDescription>
                </Alert>
            ) : view.supplierOrders.length === 0 ? (
                <Alert variant="warning">
                    <AlertTitle>未形成供应商子订单</AlertTitle>
                    <AlertDescription>
                        {view.fulfillment.autoFulfillmentBlocker ??
                            "自动履约条件不足或归集未完成；支付记录已保留，标记为差异。"}
                        {view.workItemIds[0] ? (
                            <div className="mt-2">
                                <Button
                                    id={`mall-consumption-order-center-supplier-workitem-${toAutomationIdSegment(view.workItemIds[0])}`}
                                    type="button"
                                    size="xs"
                                    variant="outline"
                                    render={
                                        <Link
                                            id={`mall-consumption-order-center-supplier-workitem-${toAutomationIdSegment(view.workItemIds[0])}-link`}
                                            href={`/governance/integration-errors?resolveWorkItemId=${view.workItemIds[0]}&queueContextId=queue:W29:mine:all`}
                                        />
                                    }
                                >
                                    {openWorkspaceLabel("W29")}
                                </Button>
                            </div>
                        ) : null}
                    </AlertDescription>
                </Alert>
            ) : (
                <div className="space-y-3">
                    {view.supplierOrders.map((so) => (
                        <Card
                            key={so.supplierFulfillmentOrderId}
                            className="rounded-lg border-0 bg-muted/40 shadow-none ring-0"
                        >
                            <CardHeader className="border-b border-grid pb-2">
                                <CardTitle className="text-base">
                                    <span className="num">
                                        {so.fulfillmentOrderNo}
                                    </span>
                                    <span className="mx-2 font-normal text-muted-foreground">
                                        {so.supplierLabel}
                                    </span>
                                </CardTitle>
                                <CardDescription>
                                    履约{" "}
                                    {
                                        SUPPLIER_STATUS_LABEL[
                                            so.fulfillmentStatus
                                        ]
                                    }{" "}
                                    · 取消{" "}
                                    {SUPPLIER_CANCEL_LABEL[so.cancelStatus] ??
                                        so.cancelStatus}{" "}
                                    · 退款{" "}
                                    {SUPPLIER_REFUND_LABEL[so.refundStatus] ??
                                        so.refundStatus}
                                </CardDescription>
                            </CardHeader>
                            <CardContent className="space-y-2">
                                {(so.fulfillmentStatus === "RESULT_UNKNOWN" ||
                                    so.fulfillmentStatus === "REJECTED" ||
                                    so.fulfillmentStatus === "EXCEPTION") && (
                                    <Alert variant="warning">
                                        <AlertTitle>
                                            商城支付已发生，正在处理履约异常
                                        </AlertTitle>
                                        <AlertDescription>
                                            本页不支持编辑或重试商城订单，也不直接操作供应商订单；请前往供应商订单按原任务号处理。
                                        </AlertDescription>
                                    </Alert>
                                )}
                                {so.supplierRefundSummary ? (
                                    <DocumentSummary
                                        columns="three"
                                        items={[
                                            {
                                                id: "f-40005",
                                                label: "供应商退款记录数",
                                                value: String(
                                                    so.supplierRefundSummary
                                                        .refundFactCount,
                                                ),
                                            },
                                            {
                                                id: "f-37855",
                                                label: "成本冲减",
                                                value: (
                                                    <MoneyValue
                                                        value={
                                                            so
                                                                .supplierRefundSummary
                                                                .costReductionGross
                                                        }
                                                    />
                                                ),
                                            },
                                            {
                                                id: "f-5004",
                                                label: "应付冲减",
                                                value: (
                                                    <MoneyValue
                                                        value={
                                                            so
                                                                .supplierRefundSummary
                                                                .payableReductionGross
                                                        }
                                                    />
                                                ),
                                            },
                                            {
                                                id: "f-69035",
                                                label: "现金退回",
                                                value: (
                                                    <MoneyValue
                                                        value={
                                                            so
                                                                .supplierRefundSummary
                                                                .cashRefundGross
                                                        }
                                                    />
                                                ),
                                            },
                                            {
                                                id: "f-22899",
                                                label: "付款分配冲正数",
                                                value: String(
                                                    so.supplierRefundSummary
                                                        .reversedPaymentAllocationCount,
                                                ),
                                            },
                                        ]}
                                    />
                                ) : null}
                                <Button
                                    id={`mall-consumption-order-center-supplier-${toAutomationIdSegment(so.supplierFulfillmentOrderId)}-open`}
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    render={
                                        <Link
                                            id={`mall-consumption-order-center-supplier-${toAutomationIdSegment(so.supplierFulfillmentOrderId)}-open-link`}
                                            href={`/supplier-api/orders?supplierOrderId=${so.supplierFulfillmentOrderId}&from=W25&mallOrderId=${view.identity.mallOrderId}`}
                                        />
                                    }
                                >
                                    打开供应商订单
                                    <ExternalLinkIcon data-icon="inline-end" />
                                </Button>
                            </CardContent>
                        </Card>
                    ))}
                </div>
            )}
        </DocumentSection>
    )
}
