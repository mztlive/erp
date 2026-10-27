"use client"

import * as React from "react"
import Link from "next/link"
import {
    ArrowUpRightIcon,
    CircleCheckIcon,
    ChevronRightIcon,
    TriangleAlertIcon,
} from "lucide-react"
import { Button, buttonVariants } from "@/components/ui/button"
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
            <summary
                id="supplier-catalog-adoption-blocked-toggle"
                className="flex cursor-pointer list-none items-start gap-2 px-6 py-3 [&::-webkit-details-marker]:hidden"
            >
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
        <section className="shrink-0 px-6 pt-6 pb-5">
            <div className="flex items-start gap-3 pr-10">
                <div className="flex size-10 shrink-0 items-center justify-center rounded-full bg-success-soft text-success-soft-foreground">
                    <CircleCheckIcon className="size-5" aria-hidden="true" />
                </div>
                <div className="min-w-0 space-y-1.5">
                    <h3 className="text-lg font-semibold tracking-tight">
                        {complete ? "选品入库完成" : "本批入库完成"}
                    </h3>
                    <p className="text-sm text-muted-foreground">
                        已入库{" "}
                        <span className="font-medium text-foreground">
                            {results.length}
                        </span>{" "}
                        项
                        {blockedCount
                            ? ` · ${blockedCount} 项需处理后重新选品`
                            : ""}
                        {!complete ? " · 可继续配置下一批" : ""}
                    </p>
                </div>
            </div>
            <p className="mt-4 text-sm text-muted-foreground">
                商品保持未上架，维护销售价格并上架后可销售。
            </p>
            <ul className="mt-5 divide-y divide-border overflow-hidden rounded-xl border">
                {results.map((row) => {
                    const item = preview.rows.find(
                        (entry) => entry.item.id === row.item_id,
                    )?.item
                    return (
                        <li
                            key={row.item_id}
                            className="flex flex-wrap items-center justify-between gap-x-4 gap-y-3 px-4 py-3.5"
                        >
                            <div className="min-w-0 flex-1 basis-48 space-y-1">
                                <p className="text-sm font-medium break-words">
                                    {item?.name ?? "已入库商品"}
                                </p>
                                {item?.specification ? (
                                    <p className="text-xs break-words text-muted-foreground">
                                        {item.specification}
                                    </p>
                                ) : null}
                            </div>
                            <div className="flex flex-wrap items-center gap-1.5">
                                {row.offering_id ? (
                                    <Link
                                        id={`supplier-catalog-adoption-offering-${toAutomationIdSegment(row.item_id)}`}
                                        className={buttonVariants({
                                            variant: "ghost",
                                            size: "sm",
                                        })}
                                        href={`/procurement/supplier-offerings/${encodeURIComponent(row.offering_id)}`}
                                    >
                                        查看供给
                                    </Link>
                                ) : null}
                                {row.product_id ? (
                                    <Link
                                        id={`supplier-catalog-adoption-product-${toAutomationIdSegment(row.item_id)}`}
                                        className={buttonVariants({
                                            variant: "outline",
                                            size: "sm",
                                        })}
                                        href={`/master-data/products/${encodeURIComponent(row.product_id)}`}
                                    >
                                        打开公司商品
                                        <ArrowUpRightIcon
                                            data-icon="inline-end"
                                            aria-hidden="true"
                                        />
                                    </Link>
                                ) : null}
                            </div>
                        </li>
                    )
                })}
            </ul>
        </section>
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
    const requestClose = React.useCallback(
        (completed: boolean) => {
            if (!pending) onClose(completed)
        },
        [onClose, pending],
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
                    forceOverlay
                    closeButtonId="supplier-catalog-adoption-close"
                    showCloseButton={!pending}
                    className={cn(
                        "flex w-[calc(100vw-2rem)] flex-col gap-0 overflow-hidden p-0",
                        editing
                            ? "h-[min(92dvh,56rem)] sm:max-w-5xl"
                            : "max-h-[min(85dvh,44rem)] sm:max-w-2xl",
                    )}
                >
                    <DialogHeader
                        className={
                            batchDone
                                ? "sr-only"
                                : "shrink-0 border-b border-grid px-6 py-4 pr-16 sm:py-5"
                        }
                    >
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
                    <div
                        className={cn(
                            "flex min-h-0 flex-col",
                            editing
                                ? "flex-1"
                                : "overflow-y-auto overscroll-contain",
                        )}
                    >
                        {batchDone ? (
                            <ResultPanel
                                results={results}
                                preview={preview}
                                complete={complete}
                                blockedCount={blockedRows.length}
                            />
                        ) : null}
                        {missingMedia.length ? (
                            <details
                                className={cn(
                                    "group shrink-0 text-xs text-warning-soft-foreground",
                                    editing
                                        ? "border-b border-warning-border bg-warning-soft px-6 py-2"
                                        : "mx-6 mb-5 rounded-lg border border-warning-border bg-warning-soft px-3 py-3",
                                )}
                            >
                                <summary
                                    id="supplier-catalog-adoption-missing-media-toggle"
                                    className="flex cursor-pointer list-none items-center gap-2 font-medium [&::-webkit-details-marker]:hidden"
                                >
                                    <TriangleAlertIcon
                                        className="size-4 shrink-0"
                                        aria-hidden="true"
                                    />
                                    <span className="flex-1">
                                        {missingMedia.length}{" "}
                                        项商品图片待补齐，不影响入库
                                    </span>
                                    <ChevronRightIcon
                                        className="size-4 shrink-0 transition-transform group-open:rotate-90"
                                        aria-hidden="true"
                                    />
                                </summary>
                                <ul className="mt-2 max-h-24 space-y-1 overflow-y-auto pl-6 break-words">
                                    {missingMedia.map(({ item, message }) => (
                                        <li key={item.id}>
                                            {item.name} ·{" "}
                                            {item.specification || "未标注规格"}
                                            ：{message}
                                        </li>
                                    ))}
                                </ul>
                            </details>
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
                                onCancel={() => requestClose(allCompleted)}
                                onComplete={(rows) => {
                                    setResults((current) => [
                                        ...current,
                                        ...rows,
                                    ])
                                    setBatchDone(true)
                                    setPending(false)
                                }}
                            />
                        ) : batchDone ? null : (
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
                        <DialogFooter className="shrink-0 border-t border-grid bg-muted/20 px-6 py-4">
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
        </>
    )
}
