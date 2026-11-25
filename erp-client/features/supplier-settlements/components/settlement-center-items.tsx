"use client"

import { useMemo, useState } from "react"
import Link from "next/link"
import { SearchIcon, XIcon } from "lucide-react"

import {
    DetailInlineTable,
    DetailWorkspaceSection,
    MoneyValue,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type { SettlementDetailView } from "@/features/supplier-settlements/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { sumFixed } from "@/lib/fixed-decimal"

const MONEY_SUM_OPTIONS = {
    maxScale: 2,
    outputScale: 2,
    allowNegative: true,
} as const

function safeSumMoney(values: readonly string[]): string | null {
    if (values.length === 0) return null
    try {
        return sumFixed(values, MONEY_SUM_OPTIONS)
    } catch {
        return null
    }
}

function SettlementCenterItems({
    items,
    statementId,
}: {
    items: SettlementDetailView["items"]
    statementId: string
}) {
    const [keyword, setKeyword] = useState("")

    const filteredItems = useMemo(() => {
        const needle = keyword.trim().toLowerCase()
        if (!needle) return items
        return items.filter((item) =>
            [
                item.supplierOrderNo,
                item.purchaseNo,
                item.externalOrderNo,
                item.productName,
            ].some((field) => field?.toLowerCase().includes(needle)),
        )
    }, [items, keyword])

    const totals = useMemo(() => {
        if (filteredItems.length === 0) return null
        const billValues = filteredItems
            .map((it) => it.supplierBillLineGross)
            .filter((val): val is string => val != null)
        return {
            orderAmountGross: safeSumMoney(
                filteredItems.map((it) => it.orderAmountGross),
            ),
            freightGross: safeSumMoney(
                filteredItems.map((it) => it.freightGross),
            ),
            serviceFeeGross: safeSumMoney(
                filteredItems.map((it) => it.serviceFeeGross),
            ),
            refundGross: safeSumMoney(
                filteredItems.map((it) => it.refundGross),
            ),
            erpAmountGross: safeSumMoney(
                filteredItems.map((it) => it.erpAmountGross),
            ),
            supplierBillLineGross:
                billValues.length > 0 ? safeSumMoney(billValues) : null,
        }
    }, [filteredItems])

    const hasSearch = keyword.trim().length > 0

    return (
        <DetailWorkspaceSection
            title="结算明细"
            width="full"
            actions={
                items.length > 0 ? (
                    <div className="flex flex-wrap items-center gap-2">
                        <div className="relative w-64">
                            <SearchIcon
                                aria-hidden="true"
                                className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                            />
                            <Input
                                id="supplier-settlements-items-search"
                                size="sm"
                                value={keyword}
                                onChange={(e) => setKeyword(e.target.value)}
                                placeholder="搜索单号或商品名称"
                                aria-label="搜索结算明细单号或商品名称"
                                className="pr-7 pl-8"
                            />
                            {hasSearch ? (
                                <Button
                                    id="supplier-settlements-items-search-clear"
                                    type="button"
                                    variant="ghost"
                                    size="icon-xs"
                                    aria-label="清空搜索"
                                    className="absolute top-1/2 right-1 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                                    onClick={() => setKeyword("")}
                                >
                                    <XIcon
                                        className="size-3.5"
                                        aria-hidden="true"
                                    />
                                </Button>
                            ) : null}
                        </div>
                        <span className="num text-xs text-muted-foreground">
                            {hasSearch
                                ? `命中 ${filteredItems.length} / ${items.length} 行`
                                : `共 ${items.length} 行`}
                        </span>
                    </div>
                ) : undefined
            }
        >
            <DetailInlineTable frame="scroll">
                <Table className="min-w-[48rem]" data-density="compact">
                    <TableHeader>
                        <TableRow>
                            <TableHead>供应商订单</TableHead>
                            <TableHead>采购单号</TableHead>
                            <TableHead>外部单号</TableHead>
                            <TableHead>商品</TableHead>
                            <TableHead data-align="end">数量</TableHead>
                            <TableHead>记录</TableHead>
                            <TableHead data-align="end">订单（含税）</TableHead>
                            <TableHead data-align="end">运费</TableHead>
                            <TableHead data-align="end">服务费</TableHead>
                            <TableHead data-align="end">退款</TableHead>
                            <TableHead data-align="end">ERP 金额</TableHead>
                            <TableHead data-align="end">账单行</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {filteredItems.map((it) => (
                            <TableRow key={it.itemId}>
                                <TableCell>
                                    <Link
                                        id={`supplier-settlements-items-row-${toAutomationIdSegment(it.itemId)}-supplier-order`}
                                        href={`/supplier-api/orders/${encodeURIComponent(it.supplierOrderId)}`}
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
                                                    ? `/procurement/orders/${encodeURIComponent(it.purchaseOrderId)}?returnTo=${encodeURIComponent(`/supplier-api/settlements/${encodeURIComponent(statementId)}`)}`
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
                                <TableCell data-align="end" className="num">
                                    {it.quantity}
                                </TableCell>
                                <TableCell className="text-xs">
                                    {it.factLabel}
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={it.orderAmountGross}
                                        display="exact"
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={it.freightGross}
                                        display="exact"
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={it.serviceFeeGross}
                                        display="exact"
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={it.refundGross}
                                        display="exact"
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={it.erpAmountGross}
                                        display="exact"
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    {it.supplierBillLineGross != null ? (
                                        <MoneyValue
                                            value={it.supplierBillLineGross}
                                            display="exact"
                                        />
                                    ) : (
                                        "—"
                                    )}
                                </TableCell>
                            </TableRow>
                        ))}
                        {filteredItems.length === 0 ? (
                            <TableRow>
                                <TableCell
                                    colSpan={12}
                                    className="py-6 text-center text-muted-foreground"
                                >
                                    {hasSearch
                                        ? "当前搜索无匹配明细，请调整单号或商品关键词。"
                                        : "暂无明细；可在草稿状态下点击刷新试算，自动带入已确认的业务记录"}
                                </TableCell>
                            </TableRow>
                        ) : null}
                    </TableBody>
                    {totals ? (
                        <TableFooter>
                            <TableRow className="font-semibold">
                                <TableCell colSpan={6}>
                                    {hasSearch ? "筛选明细合计" : "明细合计"}（
                                    <span className="num">
                                        {filteredItems.length}
                                    </span>{" "}
                                    行）
                                </TableCell>
                                <TableCell data-align="end">
                                    {totals.orderAmountGross != null ? (
                                        <MoneyValue
                                            value={totals.orderAmountGross}
                                            display="exact"
                                        />
                                    ) : (
                                        "—"
                                    )}
                                </TableCell>
                                <TableCell data-align="end">
                                    {totals.freightGross != null ? (
                                        <MoneyValue
                                            value={totals.freightGross}
                                            display="exact"
                                        />
                                    ) : (
                                        "—"
                                    )}
                                </TableCell>
                                <TableCell data-align="end">
                                    {totals.serviceFeeGross != null ? (
                                        <MoneyValue
                                            value={totals.serviceFeeGross}
                                            display="exact"
                                        />
                                    ) : (
                                        "—"
                                    )}
                                </TableCell>
                                <TableCell data-align="end">
                                    {totals.refundGross != null ? (
                                        <MoneyValue
                                            value={totals.refundGross}
                                            display="exact"
                                        />
                                    ) : (
                                        "—"
                                    )}
                                </TableCell>
                                <TableCell data-align="end">
                                    {totals.erpAmountGross != null ? (
                                        <MoneyValue
                                            value={totals.erpAmountGross}
                                            display="exact"
                                        />
                                    ) : (
                                        "—"
                                    )}
                                </TableCell>
                                <TableCell data-align="end">
                                    {totals.supplierBillLineGross != null ? (
                                        <MoneyValue
                                            value={totals.supplierBillLineGross}
                                            display="exact"
                                        />
                                    ) : (
                                        "—"
                                    )}
                                </TableCell>
                            </TableRow>
                        </TableFooter>
                    ) : null}
                </Table>
            </DetailInlineTable>
        </DetailWorkspaceSection>
    )
}

export { SettlementCenterItems }
