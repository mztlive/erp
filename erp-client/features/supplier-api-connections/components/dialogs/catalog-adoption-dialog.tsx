"use client"

import * as React from "react"
import Link from "next/link"
import {
    CircleCheckIcon,
    ChevronRightIcon,
    TriangleAlertIcon,
} from "lucide-react"
import { DiscardConfirmDialog } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type { AdoptionPreview, AdoptionResultRow } from "../../api/adoption"
import { catalogMediaBlocker } from "../../lib/catalog-media"
import { AdoptionBatchEditor } from "../catalog/adoption-batch-editor"

function DialogState({
    tone = "neutral",
    icon,
    title,
    description,
}: {
    tone?: "success" | "warning" | "neutral"
    icon: React.ReactNode
    title: string
    description: string
}) {
    const toneClasses = {
        success: "bg-success-soft text-success-soft-foreground",
        warning: "bg-warning-soft text-warning-soft-foreground",
        neutral: "bg-muted text-muted-foreground",
    }[tone]
    return (
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 px-6 py-12 text-center">
            <div
                className={cn(
                    "flex size-12 items-center justify-center rounded-full",
                    toneClasses,
                )}
            >
                {icon}
            </div>
            <div className="max-w-md space-y-1">
                <p className="text-base font-semibold">{title}</p>
                <p className="text-sm text-muted-foreground">{description}</p>
            </div>
        </div>
    )
}

function BlockedAlert({
    rows,
    defaultOpen = false,
}: {
    rows: AdoptionPreview["rows"]
    defaultOpen?: boolean
}) {
    const [open, setOpen] = React.useState(defaultOpen)
    return (
        <details
            open={open}
            onToggle={(event) => setOpen(event.currentTarget.open)}
            className="group shrink-0 border-b border-warning-border bg-warning-soft text-sm text-warning-soft-foreground"
        >
            <summary className="flex cursor-pointer list-none items-start gap-2 px-6 py-3 [&::-webkit-details-marker]:hidden">
                <TriangleAlertIcon
                    className="mt-0.5 size-4 shrink-0"
                    aria-hidden="true"
                />
                <span className="min-w-0 flex-1 font-medium">
                    {rows.length} 项暂不可入库，请查看原因
                </span>
                <ChevronRightIcon
                    className="mt-0.5 size-4 shrink-0 transition-transform group-open:rotate-90"
                    aria-hidden="true"
                />
            </summary>
            <ul className="max-h-24 list-disc space-y-0.5 overflow-y-auto pr-6 pb-3 pl-10 text-xs">
                {rows.map((row) => (
                    <li key={row.item.id}>
                        {row.item.name} ·{" "}
                        {row.item.specification || "未标注规格"}：
                        {row.blocking_reasons.join("；") || "请刷新后重试"}
                    </li>
                ))}
            </ul>
        </details>
    )
}

function ResultPanel({
    results,
    preview,
    complete,
    blockedCount,
}: {
    results: AdoptionResultRow[]
    preview: AdoptionPreview
    complete: boolean
    blockedCount: number
}) {
    return (
        <div className="min-h-0 flex-1 overflow-y-auto">
            <div className="mx-auto max-w-2xl space-y-5 px-6 py-8">
                <div className="flex flex-col items-center gap-3 text-center">
                    <div className="flex size-12 items-center justify-center rounded-full bg-success-soft text-success-soft-foreground">
                        <CircleCheckIcon
                            className="size-6"
                            aria-hidden="true"
                        />
                    </div>
                    <div className="space-y-1">
                        <h3 className="text-base font-semibold">
                            {complete
                                ? blockedCount
                                    ? "可入库商品已完成"
                                    : "本次选品已入库"
                                : "本批商品已入库"}
                        </h3>
                        <p className="text-sm text-muted-foreground">
                            {complete
                                ? blockedCount
                                    ? `${results.length} 项已入库，${blockedCount} 项需处理后重新选品。`
                                    : `${results.length} 项已入库，商品保持未上架。`
                                : `已完成 ${results.length} 项，继续处理下一批。`}
                        </p>
                    </div>
                </div>
                <div className="space-y-2">
                    {results.map((row) => {
                        const item = preview.rows.find(
                            (entry) => entry.item.id === row.item_id,
                        )?.item
                        return (
                            <div
                                key={row.item_id}
                                className="flex flex-wrap items-center justify-between gap-2 rounded-lg border px-3 py-2 text-sm"
                            >
                                <span className="min-w-0">
                                    <span className="font-medium">
                                        {item?.name ?? row.item_id}
                                    </span>
                                    {item?.specification ? (
                                        <span className="text-muted-foreground">
                                            {" "}
                                            · {item.specification}
                                        </span>
                                    ) : null}
                                </span>
                                <div className="flex shrink-0 gap-3">
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
                        )
                    })}
                </div>
            </div>
        </div>
    )
}

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
    const [dirty, setDirty] = React.useState(false)
    const [discardOpen, setDiscardOpen] = React.useState(false)
    const [closeCompleted, setCloseCompleted] = React.useState(false)
    const [results, setResults] = React.useState<AdoptionResultRow[]>([])
    const [offset, setOffset] = React.useState(0)
    const [batchDone, setBatchDone] = React.useState(false)
    const adoptedCount = preview.rows.filter((row) => row.adopted).length
    const pendingRows = preview.rows.filter((row) => !row.adopted)
    const blockedRows = pendingRows.filter((row) => row.blocked)
    const missingMedia = pendingRows.flatMap((row) => {
        const message = catalogMediaBlocker(row.item)
        return message ? [{ item: row.item, message }] : []
    })
    const unselected = pendingRows.filter((row) => !row.blocked)
    const limit = Math.max(1, Math.min(preview.max_confirm_rows, 50))
    const batch = unselected.slice(offset, offset + limit)
    const complete = batchDone && offset + batch.length >= unselected.length
    const allCompleted =
        !pendingRows.length || (complete && !blockedRows.length)
    const processedCount = results.filter(
        (row) => row.status === "SUCCEEDED",
    ).length
    const remainingPendingCount = Math.max(
        0,
        pendingRows.length - processedCount,
    )
    const editing =
        pendingRows.length > 0 && unselected.length > 0 && !batchDone
    const handleDirtyChange = React.useCallback((next: boolean) => {
        setDirty(next)
    }, [])

    const requestClose = React.useCallback(
        (completed: boolean) => {
            if (pending) return
            setCloseCompleted(completed)
            if (dirty) {
                setDiscardOpen(true)
                return
            }
            onClose(completed)
        },
        [dirty, onClose, pending],
    )

    return (
        <>
            <Dialog
                open
                onOpenChange={(open) => {
                    if (!open) requestClose(allCompleted)
                }}
            >
                <DialogContent
                    closeButtonId="supplier-catalog-adoption-close"
                    showCloseButton={!pending}
                    className="flex h-[min(92dvh,56rem)] w-[calc(100vw-2rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-5xl"
                >
                    <DialogHeader className="shrink-0 border-b border-grid px-6 py-4 pr-16 sm:py-5">
                        <DialogTitle>选品入库</DialogTitle>
                        <DialogDescription className="space-y-1">
                            <span className="block sm:hidden">
                                {remainingPendingCount} 项待入库
                                {blockedRows.length
                                    ? ` · ${blockedRows.length} 项待处理`
                                    : ""}
                            </span>
                            <span className="hidden sm:block">
                                本次 {preview.total} 项 · 待入库{" "}
                                {remainingPendingCount} 项
                                {adoptedCount + processedCount > 0
                                    ? ` · 已入库 ${adoptedCount + processedCount} 项`
                                    : ""}
                                {blockedRows.length
                                    ? ` · ${blockedRows.length} 项待处理`
                                    : ""}
                            </span>
                            <span className="hidden text-xs sm:block">
                                入库后商品保持未上架，维护销售价格并上架后可销售。
                            </span>
                            <span className="block text-xs sm:hidden">
                                入库后保持未上架。
                            </span>
                        </DialogDescription>
                    </DialogHeader>
                    <div className="flex min-h-0 flex-1 flex-col">
                        {missingMedia.length ? (
                            <div className="shrink-0 border-b border-warning-border bg-warning-soft px-6 py-3 text-xs text-warning-soft-foreground">
                                <p>以下商品有缺少的图片，可以继续选品入库：</p>
                                <ul className="max-h-24 overflow-y-auto">
                                    {missingMedia.map(({ item, message }) => (
                                        <li key={item.id}>
                                            {item.name} ·{" "}
                                            {item.specification || "未标注规格"}
                                            ：{message}
                                        </li>
                                    ))}
                                </ul>
                            </div>
                        ) : null}
                        {blockedRows.length ? (
                            <BlockedAlert
                                rows={blockedRows}
                                defaultOpen={!editing}
                            />
                        ) : null}
                        {!pendingRows.length ? (
                            <DialogState
                                tone="success"
                                icon={
                                    <CircleCheckIcon
                                        className="size-6"
                                        aria-hidden="true"
                                    />
                                }
                                title="本次商品均已入库"
                                description="无需重复创建。可关闭后继续选品。"
                            />
                        ) : editing ? (
                            <AdoptionBatchEditor
                                key={`${preview.preview_id}-${offset}`}
                                connectionId={connectionId}
                                supplierId={supplierId}
                                rows={batch}
                                preview={preview}
                                onPendingChange={setPending}
                                onDirtyChange={handleDirtyChange}
                                onCancel={() => requestClose(allCompleted)}
                                onComplete={(rows) => {
                                    setResults((current) => [
                                        ...current,
                                        ...rows,
                                    ])
                                    setBatchDone(true)
                                    setPending(false)
                                    setDirty(false)
                                }}
                            />
                        ) : batchDone ? (
                            <ResultPanel
                                results={results}
                                preview={preview}
                                complete={complete}
                                blockedCount={blockedRows.length}
                            />
                        ) : (
                            <DialogState
                                tone="warning"
                                icon={
                                    <TriangleAlertIcon
                                        className="size-6"
                                        aria-hidden="true"
                                    />
                                }
                                title="暂无可入库商品"
                                description="请查看商品不可入库的原因，处理后重试。"
                            />
                        )}
                    </div>
                    {!editing ? (
                        <DialogFooter className="shrink-0 flex-col items-stretch gap-3 border-t border-grid px-6 py-4 sm:flex-row sm:items-center sm:justify-between">
                            <p className="text-xs text-muted-foreground">
                                {pendingRows.length === 0
                                    ? "本次选品已处理完成。"
                                    : blockedRows.length
                                      ? `${blockedRows.length} 项待处理。`
                                      : "可入库商品已处理完成。"}
                            </p>
                            <div className="flex w-full flex-wrap items-center justify-end gap-2 sm:w-auto">
                                {batchDone && !complete ? (
                                    <>
                                        <Button
                                            id="supplier-catalog-adoption-cancel"
                                            variant="outline"
                                            onClick={() => requestClose(false)}
                                        >
                                            稍后继续
                                        </Button>
                                        <Button
                                            id="supplier-catalog-adoption-next-batch"
                                            onClick={() => {
                                                setOffset(
                                                    (current) =>
                                                        current + limit,
                                                )
                                                setBatchDone(false)
                                            }}
                                        >
                                            继续配置下一批（
                                            {Math.min(
                                                limit,
                                                Math.max(
                                                    0,
                                                    unselected.length -
                                                        offset -
                                                        limit,
                                                ),
                                            )}{" "}
                                            项）
                                        </Button>
                                    </>
                                ) : (
                                    <Button
                                        id="supplier-catalog-adoption-cancel"
                                        variant={
                                            allCompleted ? "default" : "outline"
                                        }
                                        onClick={() =>
                                            requestClose(allCompleted)
                                        }
                                    >
                                        {allCompleted ? "完成" : "关闭"}
                                    </Button>
                                )}
                            </div>
                        </DialogFooter>
                    ) : null}
                </DialogContent>
            </Dialog>
            <DiscardConfirmDialog
                open={discardOpen}
                onOpenChange={setDiscardOpen}
                onConfirm={() => {
                    setDiscardOpen(false)
                    onClose(closeCompleted)
                }}
                idPrefix="supplier-catalog-adoption-discard"
                title="放弃本次入库填写？"
                description="当前填写尚未提交，关闭后会丢失。已入库的商品不受影响。"
                confirmLabel="放弃并关闭"
                cancelLabel="继续填写"
            />
        </>
    )
}
