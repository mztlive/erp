"use client"

import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"

import { DescriptionList } from "@/components/ui/description-list"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import {
    DetailWorkspaceSection,
    detailWorkspaceStyles,
    MoneyValue,
} from "@/components/business"
import { useAccountProfileQuery } from "@/features/auth/queries"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"
import { codeVersion } from "@/features/supplier-orders/types"
import { hasPermission } from "@/lib/permissions"
import { Item } from "@/features/supplier-orders/components/supplier-order-preview-center-section-parts"

export function OverviewSection({
    order,
    purchaseOrderId,
    purchaseOrderNo,
}: {
    order: SupplierOrderDetailView["order"]
    purchaseOrderId?: string
    purchaseOrderNo?: string
}) {
    const profile = useAccountProfileQuery()
    const canReadPurchaseOrder = hasPermission(
        profile.data?.permissions,
        "purchase_order:detail",
    )

    return (
        <DetailWorkspaceSection
            title="概览"
            width="form"
            subtitle="来源采购、供应商与下单记录版本"
        >
            <DescriptionList className="gap-y-3">
                <Item label="履约链" value="ERP 自动供应商履约" />
                <Item label="供应商" value={order.supplierName} />
                <Item
                    label="连接"
                    value={<span className="num">{order.connectionCode}</span>}
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
                    label="来源采购单"
                    value={
                        purchaseOrderId && purchaseOrderNo ? (
                            canReadPurchaseOrder ? (
                                <Link
                                    id="supplier-order-overview-open-purchase"
                                    className="num inline-flex items-center gap-1 text-primary underline-offset-4 hover:underline"
                                    href={`/procurement/orders/${encodeURIComponent(purchaseOrderId)}`}
                                >
                                    <span>{purchaseOrderNo}</span>
                                    <ArrowUpRightIcon
                                        className="size-3.5 shrink-0"
                                        aria-hidden="true"
                                    />
                                </Link>
                            ) : (
                                <span className="num">{purchaseOrderNo}</span>
                            )
                        ) : (
                            <span className="text-muted-foreground">
                                未关联采购来源
                            </span>
                        )
                    }
                />
            </DescriptionList>
            <p className="mt-3 text-xs text-muted-foreground">
                供给、商品与成本在下单时固定，不受后续基础资料变化影响。
            </p>
        </DetailWorkspaceSection>
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
    const distinctUnits = Array.from(
        new Set(items.map((item) => item.unit.trim()).filter(Boolean)),
    )
    const singleUnit = distinctUnits.length === 1 ? distinctUnits[0] : null

    return (
        <DetailWorkspaceSection
            title="商品明细"
            width="full"
            subtitle="一条商品明细只属于一个供应商子订单；下单记录不可改供应商"
        >
            <div
                data-slot="detail-inline-table"
                className={detailWorkspaceStyles.dataTableWrapper}
            >
                <Table>
                    <TableHeader>
                        <TableRow>
                            <TableHead>商品</TableHead>
                            <TableHead>数量</TableHead>
                            <TableHead>供应商商品 / 订货编码</TableHead>
                            <TableHead>供给版本</TableHead>
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
                                </TableCell>
                                <TableCell className="num">
                                    {item.quantity} {item.unit}
                                </TableCell>
                                <TableCell>
                                    <div className="num text-tiny text-muted-foreground">
                                        {item.supplierProductId}
                                    </div>
                                </TableCell>
                                <TableCell className="num text-xs">
                                    {codeVersion(item.supplyVersion)}
                                </TableCell>
                                <TableCell className="text-right">
                                    {item.unitCostGross != null ? (
                                        <MoneyValue
                                            display="exact"
                                            value={item.unitCostGross}
                                        />
                                    ) : (
                                        <span className="text-muted-foreground">
                                            •••
                                        </span>
                                    )}
                                </TableCell>
                            </TableRow>
                        ))}
                    </TableBody>
                    {items.length > 0 ? (
                        <TableFooter>
                            <TableRow className="font-medium">
                                <TableCell>合计</TableCell>
                                <TableCell className="num">
                                    {singleUnit
                                        ? `${totalQuantity} ${singleUnit}`
                                        : `共 ${items.length} 项（多单位）`}
                                </TableCell>
                                <TableCell />
                                <TableCell />
                                <TableCell className="text-right">
                                    {totalCostGross != null ? (
                                        <MoneyValue
                                            display="exact"
                                            value={totalCostGross}
                                        />
                                    ) : (
                                        <span className="text-muted-foreground">
                                            •••
                                        </span>
                                    )}
                                </TableCell>
                            </TableRow>
                        </TableFooter>
                    ) : null}
                </Table>
            </div>
        </DetailWorkspaceSection>
    )
}
