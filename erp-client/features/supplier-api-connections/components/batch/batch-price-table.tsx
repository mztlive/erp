"use client"

import * as React from "react"
import {
    ChevronLeftIcon,
    ChevronRightIcon,
    CircleAlertIcon,
    TagsIcon,
    LockKeyholeIcon,
    PackageSearchIcon,
} from "lucide-react"
import { MoneyValue } from "@/components/business"
import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { Button } from "@/components/ui/button"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type { BatchView } from "../../api/batch"
import type { BatchWorkspaceDraft } from "../../hooks/use-batch-workspace"
import { batchPriceSample } from "../../lib/batch-price-preview"

const PAGE_SIZE = 8

export function BatchPriceTable({
    view,
    values,
}: {
    view: BatchView
    values: BatchWorkspaceDraft
}) {
    const [page, setPage] = React.useState(0)
    const rows = view.rows.filter((row) => row.status !== "EXCLUDED")
    const pageCount = Math.max(1, Math.ceil(rows.length / PAGE_SIZE))
    const currentPage = Math.min(page, pageCount - 1)
    const start = currentPage * PAGE_SIZE
    const currentRows = rows.slice(start, start + PAGE_SIZE)
    return (
        <section
            className="min-w-0 space-y-3"
            aria-labelledby="supplier-catalog-batch-sample-title"
        >
            <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex items-center gap-2">
                    <TagsIcon
                        className="size-4 text-muted-foreground"
                        aria-hidden
                    />
                    <h2
                        id="supplier-catalog-batch-sample-title"
                        className="text-base font-semibold"
                    >
                        售价预览
                    </h2>
                    <span className="rounded-md bg-muted px-2 py-0.5 text-xs text-muted-foreground">
                        {rows.length} 个 SKU
                    </span>
                </div>
                <span className="text-xs text-muted-foreground">含税</span>
            </div>
            <TableToolbarScope>
                <TableToolbar
                    className="pt-0"
                    actions={
                        pageCount > 1 ? (
                            <div className="flex items-center gap-2">
                                <span className="num text-xs text-muted-foreground">
                                    {currentPage + 1} / {pageCount}
                                </span>
                                <Button
                                    id="supplier-catalog-batch-price-previous"
                                    type="button"
                                    variant="outline"
                                    size="icon-sm"
                                    aria-label="上一页商品售价"
                                    disabled={currentPage === 0}
                                    onClick={() => setPage(currentPage - 1)}
                                >
                                    <ChevronLeftIcon aria-hidden />
                                </Button>
                                <Button
                                    id="supplier-catalog-batch-price-next"
                                    type="button"
                                    variant="outline"
                                    size="icon-sm"
                                    aria-label="下一页商品售价"
                                    disabled={currentPage + 1 >= pageCount}
                                    onClick={() => setPage(currentPage + 1)}
                                >
                                    <ChevronRightIcon aria-hidden />
                                </Button>
                            </div>
                        ) : undefined
                    }
                >
                    {pageCount > 1 ? (
                        <span className="text-xs text-muted-foreground">
                            {start + 1}–
                            {Math.min(start + PAGE_SIZE, rows.length)} /{" "}
                            {rows.length} 项
                        </span>
                    ) : null}
                </TableToolbar>
                <div className="overflow-hidden rounded-lg border">
                    <Table data-density="comfortable">
                        <TableHeader>
                            <TableRow>
                                <TableHead className="min-w-44">
                                    商品与规格
                                </TableHead>
                                <TableHead className="text-right">
                                    接口采购报价
                                </TableHead>
                                <TableHead className="text-right">
                                    公司代发价
                                </TableHead>
                                <TableHead className="text-right">
                                    公司集采价
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {currentRows.map((row) => {
                                const sample = batchPriceSample(row, values)
                                return (
                                    <TableRow key={row.source.id}>
                                        <TableCell className="max-w-72 whitespace-normal">
                                            <p className="font-medium wrap-anywhere">
                                                {row.source.name}
                                            </p>
                                            <p className="mt-1 text-xs text-muted-foreground">
                                                {row.source.specification}
                                            </p>
                                            {sample.preserved ? (
                                                <p className="mt-2 flex items-center gap-1 text-xs text-muted-foreground">
                                                    <LockKeyholeIcon
                                                        className="size-3"
                                                        aria-hidden
                                                    />
                                                    已有 SKU · 保留原价
                                                </p>
                                            ) : sample.error ? (
                                                <p className="mt-2 flex items-start gap-1 text-xs text-muted-foreground">
                                                    <CircleAlertIcon
                                                        className="mt-0.5 size-3 shrink-0"
                                                        aria-hidden
                                                    />
                                                    {sample.error}
                                                </p>
                                            ) : null}
                                        </TableCell>
                                        <TableCell className="text-right text-muted-foreground">
                                            {row.source.purchase_price ==
                                            null ? (
                                                <span className="text-xs text-warning">
                                                    缺少报价
                                                </span>
                                            ) : (
                                                <MoneyValue
                                                    value={
                                                        row.source
                                                            .purchase_price
                                                    }
                                                />
                                            )}
                                        </TableCell>
                                        <TableCell className="text-right font-medium">
                                            {sample.dropship != null ? (
                                                <MoneyValue
                                                    value={sample.dropship}
                                                />
                                            ) : (
                                                <span className="text-xs font-normal text-muted-foreground">
                                                    {row.sales_prices.dropship
                                                        .origin === "CLEARED"
                                                        ? "已清空"
                                                        : sample.preserved
                                                          ? "未设原价"
                                                          : "待设置"}
                                                </span>
                                            )}
                                        </TableCell>
                                        <TableCell className="text-right font-medium">
                                            {sample.bulk != null ? (
                                                <MoneyValue
                                                    value={sample.bulk}
                                                />
                                            ) : (
                                                <span className="text-xs font-normal text-muted-foreground">
                                                    {sample.preserved
                                                        ? "未设原价"
                                                        : values.mode ===
                                                            "DROPSHIP_ONLY"
                                                          ? "本次不设置"
                                                          : row.sales_prices
                                                                  .bulk
                                                                  .origin ===
                                                              "CLEARED"
                                                            ? "已清空"
                                                            : "待设置"}
                                                </span>
                                            )}
                                        </TableCell>
                                    </TableRow>
                                )
                            })}
                            {rows.length === 0 ? (
                                <TableRow>
                                    <TableCell
                                        colSpan={4}
                                        className="py-12 text-center text-muted-foreground"
                                    >
                                        <PackageSearchIcon
                                            className="mx-auto mb-3 size-6"
                                            aria-hidden
                                        />
                                        本批暂无可展示的商品
                                    </TableCell>
                                </TableRow>
                            ) : null}
                        </TableBody>
                    </Table>
                </div>
            </TableToolbarScope>
        </section>
    )
}
