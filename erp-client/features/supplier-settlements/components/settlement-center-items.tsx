"use client"

import Link from "next/link"

import { MoneyValue, surfaceInsetClassName } from "@/components/business"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
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
            <CardHeader className="rounded-t-lg border-b border-border/30 py-3">
                <CardTitle className="text-base">结算明细</CardTitle>
                <CardDescription>
                    冻结数据 + 不可变完成/取消/退款记录 · 金额只读，不可修改
                </CardDescription>
            </CardHeader>
            <CardContent className="overflow-x-auto pt-0">
                <table className="w-full min-w-[48rem] text-left text-sm">
                    <thead className="border-b text-xs text-muted-foreground">
                        <tr>
                            <th className="px-2 py-2">供应商订单</th>
                            <th className="px-2 py-2">采购单号</th>
                            <th className="px-2 py-2">外部单号</th>
                            <th className="px-2 py-2">商品</th>
                            <th className="px-2 py-2 text-right">数量</th>
                            <th className="px-2 py-2">记录</th>
                            <th className="px-2 py-2 text-right">订单</th>
                            <th className="px-2 py-2 text-right">运费</th>
                            <th className="px-2 py-2 text-right">服务费</th>
                            <th className="px-2 py-2 text-right">退款</th>
                            <th className="px-2 py-2 text-right">ERP</th>
                            <th className="px-2 py-2 text-right">账单行</th>
                        </tr>
                    </thead>
                    <tbody>
                        {items.map((it) => (
                            <tr
                                key={it.itemId}
                                className="border-b border-border/60"
                            >
                                <td className="px-2 py-2">
                                    <Link
                                        href={`/supplier-api/orders?q=${encodeURIComponent(it.supplierOrderNo)}`}
                                        className="num font-medium text-primary underline-offset-2 hover:underline"
                                    >
                                        {it.supplierOrderNo}
                                    </Link>
                                </td>
                                <td className="px-2 py-2">
                                    {it.purchaseNo ? (
                                        <Link
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
                                </td>
                                <td className="num px-2 py-2 text-muted-foreground">
                                    {it.externalOrderNo}
                                </td>
                                <td className="px-2 py-2">{it.productName}</td>
                                <td className="num px-2 py-2 text-right">
                                    {it.quantity}
                                </td>
                                <td className="px-2 py-2 text-xs">
                                    {it.factLabel}
                                </td>
                                <td className="px-2 py-2 text-right">
                                    <MoneyValue
                                        value={it.orderAmountGross}
                                        taxBasis="gross"
                                    />
                                </td>
                                <td className="px-2 py-2 text-right">
                                    <MoneyValue
                                        value={it.freightGross}
                                        taxBasis="gross"
                                    />
                                </td>
                                <td className="px-2 py-2 text-right">
                                    <MoneyValue
                                        value={it.serviceFeeGross}
                                        taxBasis="gross"
                                    />
                                </td>
                                <td className="px-2 py-2 text-right">
                                    <MoneyValue
                                        value={it.refundGross}
                                        taxBasis="gross"
                                    />
                                </td>
                                <td className="px-2 py-2 text-right">
                                    <MoneyValue
                                        value={it.erpAmountGross}
                                        taxBasis="gross"
                                    />
                                </td>
                                <td className="px-2 py-2 text-right">
                                    {it.supplierBillLineGross != null ? (
                                        <MoneyValue
                                            value={it.supplierBillLineGross}
                                            taxBasis="gross"
                                        />
                                    ) : (
                                        "—"
                                    )}
                                </td>
                            </tr>
                        ))}
                        {items.length === 0 ? (
                            <tr>
                                <td
                                    colSpan={12}
                                    className="px-2 py-6 text-center text-muted-foreground"
                                >
                                    暂无明细；可在草稿态刷新试算纳入不可变记录
                                </td>
                            </tr>
                        ) : null}
                    </tbody>
                </table>
                <p className="mt-2 text-xs text-muted-foreground">
                    输入控件未提供金额编辑路径；账单原值与订单记录不可覆盖。
                </p>
            </CardContent>
        </Card>
    )
}

export { SettlementCenterItems }
