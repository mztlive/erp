"use client"

import { useState } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useSourceSkus } from "../hooks/source-skus"
import type { SourceSku } from "../types"

/** 保留全部既有身份；不可售项显式提示，由用户决定移除，不静默删选。 */
export function SourceSkuPicker({
    value,
    initialRows,
    onChange,
}: {
    value: string[]
    initialRows: SourceSku[]
    onChange: (ids: string[]) => void
}) {
    const [q, setQ] = useState("")
    const [page, setPage] = useState(1)
    const [known, setKnown] = useState(
        () => new Map(initialRows.map((row) => [row.sku_id, row])),
    )
    const query = useSourceSkus(q, page)
    return (
        <div className="grid gap-3">
            <Label htmlFor="selection-reprepare-skus">搜索并添加商品</Label>
            <Input
                id="selection-reprepare-skus"
                value={q}
                placeholder="商品名称或 SKU 编号"
                onChange={(event) => {
                    setQ(event.target.value)
                    setPage(1)
                }}
            />
            {query.isError ? (
                <div
                    role="alert"
                    className="flex items-center gap-2 text-sm text-destructive"
                >
                    商品加载失败，请重试。
                    <Button
                        id="selection-source-retry"
                        type="button"
                        variant="outline"
                        onClick={() => void query.refetch()}
                    >
                        重试
                    </Button>
                </div>
            ) : query.isPending ? (
                <p role="status" className="text-sm text-muted-foreground">
                    正在加载商品…
                </p>
            ) : (
                <>
                    <ul className="max-h-52 divide-y overflow-y-auto rounded-md border">
                        {query.data.items.map((row) => (
                            <li
                                key={row.sku_id}
                                className="flex items-center justify-between gap-3 px-3 py-2"
                            >
                                <div className="min-w-0 text-sm">
                                    <p className="break-words">{row.name}</p>
                                    <p className="text-xs text-muted-foreground">
                                        {row.sku_no}
                                    </p>
                                </div>
                                <Button
                                    id={`selection-source-add-${toAutomationIdSegment(row.sku_id)}`}
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    disabled={
                                        value.includes(row.sku_id) ||
                                        value.length >= 500
                                    }
                                    onClick={() => {
                                        setKnown((old) =>
                                            new Map(old).set(row.sku_id, row),
                                        )
                                        onChange([...value, row.sku_id])
                                    }}
                                >
                                    {value.includes(row.sku_id)
                                        ? "已添加"
                                        : "添加"}
                                </Button>
                            </li>
                        ))}
                        {query.data.items.length === 0 && (
                            <li className="p-3 text-sm text-muted-foreground">
                                没有符合条件的可售商品
                            </li>
                        )}
                    </ul>
                    <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
                        <span>
                            共 {query.data.total} 件，第 {page} 页
                        </span>
                        <div className="flex gap-2">
                            <Button
                                id="selection-source-prev"
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={page === 1}
                                onClick={() => setPage(page - 1)}
                            >
                                上一页
                            </Button>
                            <Button
                                id="selection-source-next"
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={page * 10 >= query.data.total}
                                onClick={() => setPage(page + 1)}
                            >
                                下一页
                            </Button>
                        </div>
                    </div>
                </>
            )}
            <p className="text-sm font-medium">已选 {value.length} / 500 件</p>
            <ul className="max-h-52 divide-y overflow-y-auto rounded-md border">
                {value.map((id) => {
                    const row = known.get(id)
                    return (
                        <li
                            key={id}
                            className="flex items-center justify-between gap-3 px-3 py-2"
                        >
                            <div className="min-w-0 text-sm">
                                <p className="break-words">
                                    {row?.name ??
                                        "原来源商品已不可售，请移除后重新选择"}
                                </p>
                                {row && (
                                    <p className="text-xs text-muted-foreground">
                                        {row.sku_no}
                                    </p>
                                )}
                            </div>
                            <Button
                                id={`selection-source-remove-${toAutomationIdSegment(id)}`}
                                type="button"
                                size="sm"
                                variant="ghost"
                                aria-label={`移除${row?.name ?? "不可售商品"}`}
                                onClick={() =>
                                    onChange(
                                        value.filter(
                                            (selected) => selected !== id,
                                        ),
                                    )
                                }
                            >
                                移除
                            </Button>
                        </li>
                    )
                })}
                {value.length === 0 && (
                    <li className="p-3 text-sm text-muted-foreground">
                        请从上方添加至少一件商品
                    </li>
                )}
            </ul>
        </div>
    )
}
