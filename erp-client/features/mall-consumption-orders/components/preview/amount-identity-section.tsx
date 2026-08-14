"use client"

import { DocumentSummary, MoneyValue } from "@/components/business"
import type { MallConsumptionOrderView } from "@/features/mall-consumption-orders/types"
import { FULFILLMENT_CHAIN_LABEL } from "@/features/mall-consumption-orders/types"
import { formatDateTime } from "@/lib/datetime"
import { SectionTitle } from "./section-title"

type Props = {
    view: MallConsumptionOrderView
}

export function AmountIdentitySection({ view }: Props) {
    return (
        <section className="space-y-2" aria-label="金额与身份">
            <SectionTitle>金额与身份</SectionTitle>
            <DocumentSummary
                columns="two"
                items={[
                    {
                        id: "co-pv-ext-no",
                        label: "商城订单",
                        value: (
                            <span className="num">
                                {view.identity.externalOrderNo}
                            </span>
                        ),
                    },
                    {
                        id: "co-pv-erp-id",
                        label: "ERP 订单编号",
                        value: (
                            <span className="num">
                                {view.identity.mallOrderId}
                            </span>
                        ),
                    },
                    {
                        id: "co-pv-mall",
                        label: "来源商城",
                        value: view.identity.mallName,
                    },
                    {
                        id: "co-pv-customer",
                        label: "客户",
                        value:
                            view.fieldPermissions.customer === "masked"
                                ? "****（打码）"
                                : view.customer.customerLabel,
                    },
                    {
                        id: "co-pv-ordered",
                        label: "下单时间",
                        value: (
                            <span className="num">
                                {formatDateTime(view.orderedAt, "default")}
                            </span>
                        ),
                    },
                    {
                        id: "co-pv-paid",
                        label: "支付时间（决定履约链）",
                        value: (
                            <span className="num">
                                {formatDateTime(view.paidAt, "default")}
                            </span>
                        ),
                    },
                    {
                        id: "co-pv-gross",
                        label: "商品原价",
                        value: (
                            <MoneyValue value={view.amounts.gross} taxBasis="gross" />
                        ),
                    },
                    {
                        id: "co-pv-discount",
                        label: "优惠",
                        value: <MoneyValue value={view.amounts.discount} />,
                    },
                    {
                        id: "co-pv-freight",
                        label: "运费",
                        value: <MoneyValue value={view.amounts.freight} />,
                    },
                    {
                        id: "co-pv-paid-amount",
                        label: "实付",
                        value: (
                            <MoneyValue value={view.amounts.paid} taxBasis="gross" />
                        ),
                    },
                    {
                        id: "co-pv-conservation",
                        label: "守恒",
                        value:
                            view.amounts.conservationStatus === "VALID"
                                ? "有效"
                                : "差异",
                    },
                    {
                        id: "co-pv-t-decision",
                        label: "履约判定",
                        value: (
                            <span className="text-sm">
                                {FULFILLMENT_CHAIN_LABEL[view.fulfillment.chain]}
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
            <dl className="grid gap-1 text-xs text-muted-foreground sm:grid-cols-3">
                <div className="flex flex-wrap gap-1">
                    <dt>收货地址</dt>
                    <dd>{view.address.maskedSummary}</dd>
                </div>
                <div className="flex flex-wrap gap-1">
                    <dt>手机号</dt>
                    <dd>{view.phoneMasked}</dd>
                </div>
                <div className="flex flex-wrap gap-1">
                    <dt>支付引用</dt>
                    <dd>{view.paymentRefMasked}</dd>
                </div>
            </dl>
        </section>
    )
}
