"use client"

import Link from "next/link"

import {
    DocumentSection,
    MoneyValue,
    RelatedDocumentList,
} from "@/components/business"
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
import type { MallConsumptionOrderView } from "@/features/mall-consumption-orders/types"
import { ATTRIBUTION_STATUS_LABEL } from "@/features/mall-consumption-orders/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { openWorkspaceLabel } from "@/lib/ui-text"

export function OriginSection({ view }: { view: MallConsumptionOrderView }) {
    return (
        <DocumentSection
            title="来源追溯"
            description="从卡券引用可追溯到客户、原销售单与对应卡券明细；不展示卡号与卡密。"
        >
            <div className="space-y-3">
                {view.paymentSources.map((s) => (
                    <Card
                        key={s.paymentSourceId}
                        className="rounded-lg border-0 bg-muted/40 shadow-none ring-0"
                    >
                        <CardHeader className="border-b border-grid pb-2">
                            <CardTitle className="text-base">
                                {s.sourceType === "CARD"
                                    ? "卡券来源"
                                    : "微信支付"}
                                <span className="num ml-2 text-sm font-normal">
                                    {s.sourceReference}
                                </span>
                                {s.sourceType === "CARD" ? (
                                    <Badge variant="outline" className="ml-2">
                                        非卡号
                                    </Badge>
                                ) : null}
                            </CardTitle>
                            <CardDescription>
                                金额 <MoneyValue value={s.amount} /> · 归集{" "}
                                {ATTRIBUTION_STATUS_LABEL[s.attributionStatus]}
                            </CardDescription>
                        </CardHeader>
                        <CardContent className="space-y-3">
                            {s.sourceType === "WECHAT" ? (
                                <Alert variant="info">
                                    <AlertTitle>
                                        微信支付不挂企业卡券收入归属
                                    </AlertTitle>
                                    <AlertDescription>
                                        微信支付仅显示支付摘要，不关联卡券明细。
                                    </AlertDescription>
                                </Alert>
                            ) : null}
                            {s.attributionIssue ? (
                                <Alert
                                    variant={
                                        s.attributionIssue.type ===
                                        "BASELINE_CONFLICT"
                                            ? "destructive"
                                            : "warning"
                                    }
                                >
                                    <AlertTitle>
                                        {s.attributionIssue.type ===
                                        "BASELINE_CONFLICT"
                                            ? "数据已更新，禁止覆盖"
                                            : s.attributionIssue.type ===
                                                "SOURCE_OBJECT_MISSING"
                                              ? "来源对象缺失 · 待归集"
                                              : "未归属 · 待归集"}
                                    </AlertTitle>
                                    <AlertDescription>
                                        责任角色：
                                        {s.attributionIssue.ownerRole ===
                                        "FINANCE"
                                            ? "财务"
                                            : "运营"}
                                        {s.attributionIssue.workItemId ? (
                                            <>
                                                {" · "}
                                                <Link
                                                    id={`mall-consumption-order-center-origin-${toAutomationIdSegment(s.paymentSourceId)}-workitem-link`}
                                                    className="underline"
                                                    href={`/governance/integration-errors?resolveWorkItemId=${s.attributionIssue.workItemId}&queueContextId=queue:W29:mine:all`}
                                                >
                                                    打开接口错误 / 复核任务
                                                </Link>
                                            </>
                                        ) : null}
                                    </AlertDescription>
                                </Alert>
                            ) : null}
                            {s.origin ? (
                                <RelatedDocumentList
                                    documents={[
                                        {
                                            id: s.origin.customerId,
                                            documentType: "客户",
                                            documentNumber:
                                                s.origin.customerLabel,
                                            status: {
                                                label: "已归属",
                                                tone: "success",
                                            },
                                            measure: {
                                                kind: "quantity",
                                                value: "—",
                                            },
                                            owner: "—",
                                            openAction: (
                                                <Button
                                                    id={`mall-consumption-order-center-origin-${toAutomationIdSegment(s.origin.customerId)}-customer`}
                                                    type="button"
                                                    size="xs"
                                                    variant="outline"
                                                    render={
                                                        <Link
                                                            id={`mall-consumption-order-center-origin-${toAutomationIdSegment(s.origin.customerId)}-customer-link`}
                                                            href={`/sales/customers/${s.origin.customerId}`}
                                                        />
                                                    }
                                                >
                                                    打开客户
                                                </Button>
                                            ),
                                        },
                                        {
                                            id: s.origin.salesOrderId,
                                            documentType: "原销售单",
                                            documentNumber:
                                                s.origin.salesOrderNo,
                                            status: {
                                                label: "可追溯",
                                                tone: "info",
                                            },
                                            measure: {
                                                kind: "quantity",
                                                value: s.origin
                                                    .salesOrderLineId,
                                                label: "卡券明细",
                                            },
                                            owner: "—",
                                            openAction: (
                                                <Button
                                                    id={`mall-consumption-order-center-origin-${toAutomationIdSegment(s.origin.salesOrderId)}-sales-order`}
                                                    type="button"
                                                    size="xs"
                                                    variant="outline"
                                                    render={
                                                        <Link
                                                            id={`mall-consumption-order-center-origin-${toAutomationIdSegment(s.origin.salesOrderId)}-sales-order-link`}
                                                            href={`/sales/orders/${s.origin.salesOrderId}`}
                                                        />
                                                    }
                                                >
                                                    {openWorkspaceLabel("W05")}
                                                </Button>
                                            ),
                                        },
                                    ]}
                                />
                            ) : s.sourceType === "CARD" ? (
                                <p className="text-sm text-muted-foreground">
                                    卡券或客户尚未归集，保留原始引用，不补充猜测值。
                                </p>
                            ) : null}
                        </CardContent>
                    </Card>
                ))}
            </div>
        </DocumentSection>
    )
}
