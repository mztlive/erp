"use client"

import { DocumentSection, DocumentSummary, MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import type { MallConsumptionOrderView } from "@/features/mall-consumption-orders/types"
import { ATTRIBUTION_STATUS_LABEL } from "@/features/mall-consumption-orders/types"

export function ItemsSection({ view }: { view: MallConsumptionOrderView }) {
    return (
        <DocumentSection title="商品明细（下单时）">
            <div className="space-y-3">
                {view.items.map((item) => (
                    <Card
                        key={item.mallOrderItemId}
                        className="rounded-lg border-0 bg-muted/40 shadow-none ring-0"
                    >
                        <CardHeader className="border-b border-grid pb-2">
                            <CardTitle className="text-base">
                                {item.nameSnapshot}
                            </CardTitle>
                            <CardDescription>
                                {item.specSnapshot}
                                <span className="mx-1">·</span>
                                <span className="num">
                                    {item.externalItemId}
                                </span>
                                {item.skuId ? (
                                    <>
                                        <span className="mx-1">·</span>
                                        <span className="num">
                                            SKU {item.skuId}
                                        </span>
                                    </>
                                ) : (
                                    <Badge
                                        variant="warning"
                                        className="ml-2"
                                    >
                                        待映射
                                    </Badge>
                                )}
                            </CardDescription>
                        </CardHeader>
                        <CardContent>
                            <DocumentSummary
                                columns="four"
                                items={[
                                    {
                                        id: "f-93130",
                                        label: "数量",
                                        value: (
                                            <span className="num">
                                                {item.quantity}
                                            </span>
                                        ),
                                    },
                                    {
                                        id: "f-49274",
                                        label: "含税单价",
                                        value: (
                                            <MoneyValue
                                                value={item.unitPriceGross}
                                                taxBasis="gross"
                                            />
                                        ),
                                    },
                                    {
                                        id: "f-72923",
                                        label: "明细原价",
                                        value: (
                                            <MoneyValue
                                                value={item.lineGrossAmount}
                                                taxBasis="gross"
                                            />
                                        ),
                                    },
                                    {
                                        id: "f-28117",
                                        label: "明细实付",
                                        value: (
                                            <MoneyValue
                                                value={item.paidAmount}
                                                taxBasis="gross"
                                            />
                                        ),
                                    },
                                    {
                                        id: "f-49028",
                                        label: "分摊优惠",
                                        value: (
                                            <MoneyValue
                                                value={
                                                    item.allocatedDiscountAmount
                                                }
                                            />
                                        ),
                                    },
                                    {
                                        id: "f-58253",
                                        label: "分摊运费",
                                        value: (
                                            <MoneyValue
                                                value={
                                                    item.allocatedFreightAmount
                                                }
                                            />
                                        ),
                                    },
                                    {
                                        id: "f-47772",
                                        label: "归集",
                                        value: ATTRIBUTION_STATUS_LABEL[
                                            item.attributionStatus
                                        ],
                                    },
                                    {
                                        id: "f-25032",
                                        label: "下单时商城成本",
                                        value:
                                            view.fieldPermissions.cost ===
                                            "masked" ? (
                                                "****"
                                            ) : item.costSnapshotTotal ? (
                                                <MoneyValue
                                                    value={
                                                        item.costSnapshotTotal
                                                    }
                                                />
                                            ) : (
                                                "—"
                                            ),
                                    },
                                ]}
                            />
                        </CardContent>
                    </Card>
                ))}
            </div>
        </DocumentSection>
    )
}
