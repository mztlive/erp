"use client"

import Link from "next/link"

import { toAutomationIdSegment } from "@/lib/automation-id"
import { MoneyValue, surfaceInsetClassName } from "@/components/business"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type { SettlementDetailView } from "@/features/supplier-settlements/types"
import { cn } from "@/lib/utils"

function SettlementCenterItems({
    items,
    statementId,
}: {
    items: SettlementDetailView["items"]
    statementId: string
}) {
    return (
        <Card
            size="sm"
            className={cn(surfaceInsetClassName, "shadow-none ring-0")}
        >
            <CardHeader className="rounded-t-lg border-b border-grid py-3">
                <CardTitle className="text-section-title font-semibold tracking-tight">
                    结算明细
                </CardTitle>
                <CardDescription>
                    按结算单锁定的业务明细汇总，金额仅供核对，不可直接修改
                </CardDescription>
            </CardHeader>
            <CardContent className="pt-0">
                <Table className="min-w-[48rem]" data-density="compact">
                    <TableHeader>
                        <TableRow>
                            <TableHead>供应商订单</TableHead>
                            <TableHead>采购单号</TableHead>
                            <TableHead>外部单号</TableHead>
                            <TableHead>商品</TableHead>
                            <TableHead data-align="end">数量</TableHead>
                            <TableHead>记录</TableHead>
                            <TableHead data-align="end">订单</TableHead>
                            <TableHead data-align="end">运费</TableHead>
                            <TableHead data-align="end">服务费</TableHead>
                            <TableHead data-align="end">退款</TableHead>
                            <TableHead data-align="end">ERP</TableHead>
                            <TableHead data-align="end">账单行</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {items.map((it) => (
                            <TableRow key={it.itemId}>
                                <TableCell>
                                    <Link
                                        id={`supplier-settlements-items-row-${toAutomationIdSegment(it.itemId)}-supplier-order`}
                                        href={`/supplier-api/orders?preview=${encodeURIComponent(it.supplierOrderId)}`}
                                        className="num font-medium text-primary underline-offset-2 hover:underline"
                                    >
                                        {it.supplierOrderNo}
                                    </Link>
                                </TableCell>
                                <TableCell>
                                    {it.purchaseNo ? (
                                        <Link
                                            id={`supplier-settlements-items-row-${toAutomationIdSegment(it.itemId)}-purchase`}
                                            href={
                                                it.purchaseOrderId
                                                    ? `/procurement/orders/${it.purchaseOrderId}?returnTo=${encodeURIComponent(`/supplier-api/settlements/${statementId}`)}`
                                                    : `/procurement/orders?q=${encodeURIComponent(it.purchaseNo)}`
                                            }
                                            className="num font-medium text-primary underline-offset-2 hover:underline"
                                        >
                                            {it.purchaseNo}
                                        </Link>
                                    ) : (
                                        <span className="text-xs text-muted-foreground">
                                            —
                                        </span>
                                    )}
                                </TableCell>
                                <TableCell className="num text-muted-foreground">
                                    {it.externalOrderNo}
                                </TableCell>
                                <TableCell className="whitespace-normal">
                                    {it.productName}
                                </TableCell>
                                <TableCell data-align="end">
                                    {it.quantity}
                                </TableCell>
                                <TableCell className="text-xs">
                                    {it.factLabel}
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={it.orderAmountGross}
                                        taxBasis="gross"
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={it.freightGross}
                                        taxBasis="gross"
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={it.serviceFeeGross}
                                        taxBasis="gross"
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={it.refundGross}
                                        taxBasis="gross"
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={it.erpAmountGross}
                                        taxBasis="gross"
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    {it.supplierBillLineGross != null ? (
                                        <MoneyValue
                                            value={it.supplierBillLineGross}
                                            taxBasis="gross"
                                        />
                                    ) : (
                                        "—"
                                    )}
                                </TableCell>
                            </TableRow>
                        ))}
                        {items.length === 0 ? (
                            <TableRow>
                                <TableCell
                                    colSpan={12}
                                    className="py-6 text-center text-muted-foreground"
                                >
                                    暂无明细；可在草稿状态下点击刷新试算，自动带入已确认的业务记录
                                </TableCell>
                            </TableRow>
                        ) : null}
                    </TableBody>
                </Table>
                <p className="mt-2 text-xs text-muted-foreground">
                    为保障账实一致，明细金额来自采购单与供应商账单原值，不支持在此处手动修改。
                </p>
            </CardContent>
        </Card>
    )
}

export { SettlementCenterItems }
