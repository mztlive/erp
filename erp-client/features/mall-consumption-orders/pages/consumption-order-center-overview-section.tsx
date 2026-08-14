"use client"

import { DocumentSection, DocumentSummary, MoneyValue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import type { MallConsumptionOrderView } from "@/features/mall-consumption-orders/types"
import { FULFILLMENT_CHAIN_LABEL } from "@/features/mall-consumption-orders/types"
import { formatDateTime } from "@/lib/datetime"

export function OverviewSection({ view }: { view: MallConsumptionOrderView }) {
    return (
        <div className="space-y-4">
            <DocumentSection title="金额与身份">
                <DocumentSummary
                    columns="three"
                    items={[
                        {
                            id: "f-57558",
                            label: "商城订单",
                            value: (
                                <span className="num">
                                    {view.identity.externalOrderNo}
                                </span>
                            ),
                        },
                        {
                            id: "f-17653",
                            label: "ERP 订单编号",
                            value: (
                                <span className="num">
                                    {view.identity.mallOrderId}
                                </span>
                            ),
                        },
                        {
                            id: "f-51562",
                            label: "来源商城",
                            value: view.identity.mallName,
                        },
                        {
                            id: "f-63424",
                            label: "客户",
                            value:
                                view.fieldPermissions.customer === "masked"
                                    ? "****（打码）"
                                    : view.customer.customerLabel,
                        },
                        {
                            id: "f-28981",
                            label: "下单时间",
                            value: (
                                <span className="num">
                                    {formatDateTime(view.orderedAt, "default")}
                                </span>
                            ),
                        },
                        {
                            id: "f-38567",
                            label: "支付时间（决定履约链）",
                            value: (
                                <span className="num">
                                    {formatDateTime(view.paidAt, "default")}
                                </span>
                            ),
                        },
                        {
                            id: "f-15545",
                            label: "商品原价",
                            value: (
                                <MoneyValue
                                    value={view.amounts.gross}
                                    taxBasis="gross"
                                />
                            ),
                        },
                        {
                            id: "f-82950",
                            label: "优惠",
                            value: (
                                <MoneyValue value={view.amounts.discount} />
                            ),
                        },
                        {
                            id: "f-38831",
                            label: "运费",
                            value: <MoneyValue value={view.amounts.freight} />,
                        },
                        {
                            id: "f-21324",
                            label: "实付",
                            value: (
                                <span className="text-lg font-semibold">
                                    <MoneyValue
                                        value={view.amounts.paid}
                                        taxBasis="gross"
                                    />
                                </span>
                            ),
                        },
                        {
                            id: "f-95351",
                            label: "守恒",
                            value:
                                view.amounts.conservationStatus === "VALID" ? (
                                    "有效"
                                ) : (
                                    <span className="text-destructive">
                                        差异
                                    </span>
                                ),
                        },
                        {
                            id: "f-8625",
                            label: "履约判定",
                            value: (
                                <span className="text-sm">
                                    {
                                        FULFILLMENT_CHAIN_LABEL[
                                            view.fulfillment.chain
                                        ]
                                    }
                                    <span className="mx-1 text-muted-foreground">
                                        ·
                                    </span>
                                    支付成功时间{" "}
                                    {formatDateTime(
                                        view.fulfillment.decidedByOccurredAt,
                                        "default",
                                    )}
                                    {view.fulfillment.chain === "LEGACY_MANUAL"
                                        ? "，早于切换时点"
                                        : "，不早于切换时点"}
                                </span>
                            ),
                        },
                    ]}
                />
                {view.fulfillment.chain === "LEGACY_MANUAL" ? (
                    <Alert variant="default" className="mt-3">
                        <AlertTitle>原人工履约链</AlertTitle>
                        <AlertDescription>
                            该支付发生在履约主责切换之前，仅作历史记录，不创建供应商子订单。
                        </AlertDescription>
                    </Alert>
                ) : null}
                {view.fulfillment.autoFulfillmentBlocker ? (
                    <Alert variant="warning" className="mt-3">
                        <AlertTitle>自动履约条件不足</AlertTitle>
                        <AlertDescription>
                            {view.fulfillment.autoFulfillmentBlocker}
                        </AlertDescription>
                    </Alert>
                ) : null}
            </DocumentSection>

            <DocumentSection title="敏感字段（按权限打码）">
                <DocumentSummary
                    columns="three"
                    items={[
                        {
                            id: "f-52328",
                            label: "收货地址",
                            value: view.address.maskedSummary,
                        },
                        {
                            id: "f-33695",
                            label: "手机号",
                            value: view.phoneMasked,
                        },
                        {
                            id: "f-91754",
                            label: "支付引用",
                            value: view.paymentRefMasked,
                        },
                    ]}
                />
                <p className="mt-2 text-xs text-muted-foreground">
                    地址、手机号与支付引用按权限打码展示，完整值不在此页显示；卡号与卡密永不展示。
                </p>
            </DocumentSection>
        </div>
    )
}
