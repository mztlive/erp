"use client"

import { Badge } from "@/components/ui/badge"
import { DescriptionList } from "@/components/ui/description-list"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { DocumentSection, MoneyValue } from "@/components/business"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"
import { codeVersion } from "@/features/supplier-orders/types"
import { Item } from "@/features/supplier-orders/components/supplier-order-preview-center-section-parts"

export function OverviewSection({
    order,
}: {
    order: SupplierOrderDetailView["order"]
}) {
    return (
        <DocumentSection
            title="概览"
            description="来源支付、供应商与下单记录版本"
        >
            <DescriptionList className="gap-y-3">
                <Item label="履约链" value="ERP 自动供应商履约" />
                <Item label="供应商" value={order.supplierName} />
                <Item
                    label="连接"
                    value={`${order.connectionCode} · ${order.connectionEnvironment}`}
                />
                <Item
                    label="供给数据版本"
                    value={
                        <span className="num">
                            {codeVersion(order.supplyVersion)}
                        </span>
                    }
                />
                <Item
                    label="发布数据版本"
                    value={
                        <span className="num">
                            {codeVersion(order.publicationVersion)}
                        </span>
                    }
                />
                <Item
                    label="支付凭证号"
                    value={<span className="num">{order.paymentFactKey}</span>}
                />
            </DescriptionList>
            <p className="mt-3 text-xs text-muted-foreground">
                发布版本、供给、商品与成本在下单时固定，不受后续基础资料变化影响。
            </p>
        </DocumentSection>
    )
}

export function ItemsSection({
    items,
    totalQuantity,
    totalCostGross,
}: {
    items: SupplierOrderDetailView["items"]
    totalQuantity: string
    totalCostGross: string | null
}) {
    return (
        <DocumentSection
            title="商品明细"
            description="一条商城明细只属于一个供应商子订单；下单记录不可改供应商"
        >
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>商品</TableHead>
                        <TableHead>数量</TableHead>
                        <TableHead>供应商订货编码</TableHead>
                        <TableHead>发布/供给版本</TableHead>
                        <TableHead className="text-right">
                            下单成本（含税）
                        </TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {items.map((item) => (
                        <TableRow key={item.itemId}>
                            <TableCell>
                                <div className="font-medium">
                                    {item.productName}
                                </div>
                                <div className="num text-xs text-muted-foreground">
                                    {item.skuCode}
                                </div>
                                <Badge
                                    variant="secondary"
                                    className="mt-1 text-2xs"
                                >
                                    下单记录不可变
                                </Badge>
                            </TableCell>
                            <TableCell className="num">
                                {item.quantity} {item.unit}
                            </TableCell>
                            <TableCell>
                                <div className="text-xs">
                                    {item.supplierProductName}
                                </div>
                                <div className="num text-tiny text-muted-foreground">
                                    {item.supplierProductId}
                                </div>
                            </TableCell>
                            <TableCell className="num text-xs">
                                {codeVersion(item.publicationVersion)} /{" "}
                                {codeVersion(item.supplyVersion)}
                            </TableCell>
                            <TableCell className="text-right">
                                {item.unitCostGross != null ? (
                                    <MoneyValue
                                        value={item.unitCostGross}
                                        taxBasis="gross"
                                    />
                                ) : (
                                    <span className="text-muted-foreground">
                                        •••
                                    </span>
                                )}
                            </TableCell>
                        </TableRow>
                    ))}
                    {items.length > 0 ? (
                        <TableRow className="border-t-2 border-border font-medium">
                            <TableCell>合计</TableCell>
                            <TableCell className="num">
                                {totalQuantity} {items[0].unit}
                            </TableCell>
                            <TableCell />
                            <TableCell />
                            <TableCell className="text-right">
                                {totalCostGross != null ? (
                                    <MoneyValue
                                        value={totalCostGross}
                                        taxBasis="gross"
                                    />
                                ) : (
                                    <span className="text-muted-foreground">
                                        •••
                                    </span>
                                )}
                            </TableCell>
                        </TableRow>
                    ) : null}
                </TableBody>
            </Table>
        </DocumentSection>
    )
}
