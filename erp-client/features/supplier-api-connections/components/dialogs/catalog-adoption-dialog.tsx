"use client"

import * as React from "react"
import Link from "next/link"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { AdoptionPreview, AdoptionResultRow } from "../../api/adoption"
import { AdoptionBatchEditor } from "../catalog/adoption-batch-editor"

export function CatalogAdoptionDialog({
    connectionId,
    supplierId,
    preview,
    onClose,
}: {
    connectionId: string
    supplierId: string
    preview: AdoptionPreview
    onClose: (completed: boolean) => void
}) {
    const [pending, setPending] = React.useState(false)
    const [results, setResults] = React.useState<AdoptionResultRow[]>([])
    const [offset, setOffset] = React.useState(0)
    const [batchDone, setBatchDone] = React.useState(false)
    const pendingRows = preview.rows.filter((row) => !row.adopted)
    const blockedRows = pendingRows.filter((row) => row.blocked)
    const unselected = pendingRows.filter((row) => !row.blocked)
    const limit = Math.max(1, Math.min(preview.max_confirm_rows, 50))
    const batch = unselected.slice(offset, offset + limit)
    const complete = batchDone && offset + batch.length >= unselected.length
    const allCompleted =
        !pendingRows.length || (complete && !blockedRows.length)
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !pending) onClose(allCompleted)
            }}
        >
            <DialogContent
                closeButtonId="supplier-catalog-adoption-close"
                className="max-h-[94vh] overflow-y-auto sm:max-w-5xl"
            >
                <DialogHeader>
                    <DialogTitle>选品入库</DialogTitle>
                    <DialogDescription>
                        本次已确定 {preview.total} 项，其中{" "}
                        {preview.rows.filter((row) => row.adopted).length}{" "}
                        项已入库
                        {blockedRows.length
                            ? `，${blockedRows.length} 项图片尚未同步完成，暂不可入库`
                            : ""}
                        。新商品进入现有公司商品列表，维护销售价格并上架后才可销售。
                    </DialogDescription>
                </DialogHeader>
                {!pendingRows.length ? (
                    <p className="py-5 text-sm text-muted-foreground">
                        本次商品均已入库，无需重复创建。
                    </p>
                ) : null}
                {blockedRows.length ? (
                    <div
                        className="space-y-2 rounded-lg border p-3 text-sm"
                        role="status"
                    >
                        <p className="font-medium">
                            以下商品暂不可入库，请先重新同步核验图片
                        </p>
                        {blockedRows.map((row) => (
                            <p
                                key={row.item.id}
                                className="text-muted-foreground"
                            >
                                {row.item.name} ·{" "}
                                {row.item.specification || "未标注规格"}：
                                {row.blocking_reasons.join("；") ||
                                    "商品图片尚未同步完成"}
                            </p>
                        ))}
                    </div>
                ) : null}
                {unselected.length && !batchDone ? (
                    <AdoptionBatchEditor
                        key={`${preview.preview_id}-${offset}`}
                        connectionId={connectionId}
                        supplierId={supplierId}
                        rows={batch}
                        preview={preview}
                        onPendingChange={setPending}
                        onComplete={(rows) => {
                            setResults((current) => [...current, ...rows])
                            setBatchDone(true)
                        }}
                    />
                ) : null}
                {batchDone ? (
                    <div className="space-y-3 py-3" aria-live="polite">
                        <p className="font-medium">
                            {complete
                                ? blockedRows.length
                                    ? "可入库商品已完成，其余商品待核验后重新选品"
                                    : "本次选品已入库"
                                : `已完成 ${results.length} 项，继续处理下一批`}
                        </p>
                        <div className="max-h-64 space-y-2 overflow-auto text-sm">
                            {results.map((row) => (
                                <div
                                    key={row.item_id}
                                    className="flex flex-wrap items-center justify-between gap-2 rounded-md bg-muted/40 px-3 py-2"
                                >
                                    <span>
                                        {
                                            preview.rows.find(
                                                (item) =>
                                                    item.item.id ===
                                                    row.item_id,
                                            )?.item.name
                                        }{" "}
                                        · 已入库
                                    </span>
                                    <div className="flex gap-3">
                                        {row.product_id ? (
                                            <Link
                                                id={`supplier-catalog-adoption-product-${toAutomationIdSegment(row.item_id)}`}
                                                className="text-primary underline"
                                                href={`/master-data/products/${encodeURIComponent(row.product_id)}`}
                                            >
                                                打开公司商品
                                            </Link>
                                        ) : null}
                                        {row.offering_id ? (
                                            <Link
                                                id={`supplier-catalog-adoption-offering-${toAutomationIdSegment(row.item_id)}`}
                                                className="text-primary underline"
                                                href={`/procurement/supplier-offerings/${encodeURIComponent(row.offering_id)}`}
                                            >
                                                查看供给
                                            </Link>
                                        ) : null}
                                    </div>
                                </div>
                            ))}
                        </div>
                    </div>
                ) : null}
                <div className="flex justify-end gap-2">
                    <Button
                        id="supplier-catalog-adoption-cancel"
                        variant="outline"
                        disabled={pending}
                        onClick={() => onClose(allCompleted)}
                    >
                        {allCompleted ? "完成" : "关闭"}
                    </Button>
                    {batchDone && !complete ? (
                        <Button
                            id="supplier-catalog-adoption-next-batch"
                            onClick={() => {
                                setOffset((current) => current + limit)
                                setBatchDone(false)
                            }}
                        >
                            配置下一批{" "}
                            {Math.min(
                                limit,
                                unselected.length - offset - limit,
                            )}{" "}
                            项
                        </Button>
                    ) : null}
                </div>
            </DialogContent>
        </Dialog>
    )
}
