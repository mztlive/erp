"use client"

import { useEffect, useRef, useState, type ReactNode } from "react"
import { ArrowLeftIcon, ArrowRightIcon, SearchIcon, XIcon } from "lucide-react"
import { QuantityValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { buildSourcingEditorRows } from "../lib/sourcing/editor-rows"
import type {
    SourcingEditorProps,
    SourcingBatchSelectionProps,
    SourcingParticipationProps,
} from "./create-sourcing/types"
import { SourcingProductEditor } from "./sourcing-product-editor"
import { SourcingProductSummaryList } from "./sourcing-product-summary-list"
import { SourcingProductList } from "./sourcing-product-list"

/** 宽屏并排编辑，窄屏摘要行与弹窗编辑；两种视图共用同一份表单和校验。 */
export function PurchaseOrderCreateSourcingWorkspace({
    toolbar,
    selectedProductIds,
    onToggleProducts,
    onSetParticipation,
    ...props
}: SourcingEditorProps &
    SourcingBatchSelectionProps &
    SourcingParticipationProps & { toolbar: ReactNode }) {
    const { form, order } = props
    const [query, setQuery] = useState("")
    const [onlyPending, setOnlyPending] = useState(false)
    const [activeId, setActiveId] = useState(order.lines[0]?.salesOrderLineId)
    const surface = useRef<HTMLDivElement>(null)
    const [narrow, setNarrow] = useState(true)
    useEffect(() => {
        const element = surface.current
        if (!element) return
        const observer = new ResizeObserver(([entry]) => {
            if (entry) setNarrow(entry.contentRect.width < 960)
        })
        observer.observe(element)
        return () => observer.disconnect()
    }, [])
    const editorBody = useRef<HTMLDivElement>(null)
    useEffect(() => {
        if (editorBody.current) editorBody.current.scrollTop = 0
        if (activeId)
            document
                .getElementById(
                    `sourcing-list-${toAutomationIdSegment(activeId)}-edit`,
                )
                ?.scrollIntoView({ block: "nearest" })
    }, [activeId])
    const rows = buildSourcingEditorRows(order, form.state.values.lines)
    const pending = rows.filter((row) => row.needsAttention)
    const visible = rows.filter(
        (row) =>
            (!onlyPending || row.needsAttention) &&
            `${row.product.itemName} ${row.product.itemSku ?? ""}`
                .toLocaleLowerCase()
                .includes(query.trim().toLocaleLowerCase()),
    )
    // 校验变更不会自动跳走，当前行补齐后仍可继续核对；过滤只影响左侧列表。
    const active =
        rows.find((row) => row.product.salesOrderLineId === activeId) ?? rows[0]
    const index = rows.findIndex((row) => row === active)
    const nextPending = [
        ...rows.slice(index + 1),
        ...rows.slice(0, index),
    ].find((row) => row.needsAttention)
    const select = (id: string) => setActiveId(id)
    return (
        <div
            ref={surface}
            className="@container/sourcing-workspace flex min-h-0 flex-1 flex-col overflow-hidden"
        >
            <div className="grid min-h-0 flex-1 grid-rows-1 @min-[960px]/sourcing-workspace:grid-cols-[minmax(0,1.15fr)_minmax(0,1fr)] @min-[960px]/sourcing-workspace:grid-rows-1">
                <section
                    aria-label="供给商品列表"
                    className="flex min-h-0 min-w-0 flex-col border-b border-border @min-[960px]/sourcing-workspace:border-r @min-[960px]/sourcing-workspace:border-b-0"
                >
                    <div className="flex shrink-0 flex-wrap items-center gap-3 border-b border-border/60 px-4 py-3">
                        <div className="relative min-w-40 flex-1">
                            <SearchIcon
                                aria-hidden="true"
                                className="pointer-events-none absolute top-2.5 left-3 size-4 text-muted-foreground"
                            />
                            <Input
                                id="sourcing-products-search"
                                aria-label="搜索商品名称或规格编号"
                                placeholder="搜索商品名称或规格编号"
                                className="h-9 pr-9 pl-9"
                                value={query}
                                onChange={(event) =>
                                    setQuery(event.target.value)
                                }
                            />
                            {query ? (
                                <Button
                                    id="sourcing-products-search-clear"
                                    type="button"
                                    size="icon-sm"
                                    variant="ghost"
                                    aria-label="清除商品搜索"
                                    className="absolute top-0.5 right-0.5"
                                    onClick={() => setQuery("")}
                                >
                                    <XIcon />
                                </Button>
                            ) : null}
                        </div>
                        <label
                            htmlFor="sourcing-products-pending"
                            className="flex cursor-pointer items-center gap-2 text-xs"
                        >
                            <NativeCheckbox
                                id="sourcing-products-pending"
                                checked={onlyPending}
                                onCheckedChange={setOnlyPending}
                            />
                            仅看待调整 {pending.length}
                        </label>
                    </div>
                    {narrow ? (
                        <SourcingProductSummaryList
                            {...props}
                            rows={visible}
                            selectedProductIds={selectedProductIds}
                            onToggleProducts={onToggleProducts}
                            onSetParticipation={onSetParticipation}
                            toolbar={toolbar}
                        />
                    ) : (
                        <SourcingProductList
                            toolbar={toolbar}
                            rows={visible}
                            activeId={active?.product.salesOrderLineId}
                            onSelect={select}
                            selectedProductIds={selectedProductIds}
                            onToggleProducts={onToggleProducts}
                            onSetParticipation={onSetParticipation}
                        />
                    )}
                    {!narrow ? (
                        <p className="shrink-0 border-t border-border px-4 py-2 text-xs text-muted-foreground">
                            共 {rows.length} 行 · 已就绪{" "}
                            {
                                rows.filter((row) => row.status === "已就绪")
                                    .length
                            }{" "}
                            行 · 待调整 {pending.length} 行
                        </p>
                    ) : null}
                </section>
                {!narrow ? (
                    <section
                        aria-label="当前商品供给编辑"
                        className="flex min-h-0 min-w-0 flex-col"
                    >
                        {active ? (
                            <>
                                <header className="flex shrink-0 flex-wrap items-start justify-between gap-3 border-b border-border px-4 py-3">
                                    <div className="min-w-0 space-y-1">
                                        <h3 className="text-base font-semibold">
                                            {active.product.itemName}
                                        </h3>
                                        <p className="text-xs text-muted-foreground">
                                            第 {index + 1} 行 / 共 {rows.length}{" "}
                                            行 · 需供给{" "}
                                            <QuantityValue
                                                value={
                                                    active.product
                                                        .remainingQuantity
                                                }
                                                unit={active.product.unit}
                                            />
                                        </p>
                                        {active.product.deliveryDeadline ? (
                                            <p className="text-xs text-muted-foreground">
                                                最晚交付{" "}
                                                {
                                                    active.product
                                                        .deliveryDeadline
                                                }
                                            </p>
                                        ) : null}
                                    </div>
                                    <div className="flex gap-1">
                                        <Button
                                            id="sourcing-product-previous"
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            disabled={index <= 0}
                                            onClick={() =>
                                                select(
                                                    rows[index - 1]!.product
                                                        .salesOrderLineId,
                                                )
                                            }
                                        >
                                            <ArrowLeftIcon />
                                            上一行
                                        </Button>
                                        <Button
                                            id="sourcing-product-next"
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            disabled={index >= rows.length - 1}
                                            onClick={() =>
                                                select(
                                                    rows[index + 1]!.product
                                                        .salesOrderLineId,
                                                )
                                            }
                                        >
                                            下一行
                                            <ArrowRightIcon />
                                        </Button>
                                    </div>
                                </header>
                                <div
                                    ref={editorBody}
                                    className="min-h-0 flex-1 overflow-auto"
                                    data-slot="sourcing-editor-body"
                                >
                                    {rows.map((row) => (
                                        <div
                                            key={row.product.salesOrderLineId}
                                            hidden={row !== active}
                                        >
                                            <SourcingProductEditor
                                                {...props}
                                                onSetParticipation={
                                                    onSetParticipation
                                                }
                                                row={row}
                                            />
                                        </div>
                                    ))}
                                </div>
                                <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-t border-border px-4 py-3">
                                    <p className="text-xs text-muted-foreground">
                                        修改暂存于本次分配，预览后统一确认。
                                    </p>
                                    <Button
                                        id="sourcing-product-next-pending"
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        disabled={!nextPending}
                                        onClick={() => {
                                            if (nextPending)
                                                select(
                                                    nextPending.product
                                                        .salesOrderLineId,
                                                )
                                        }}
                                    >
                                        下一条待调整
                                        <ArrowRightIcon />
                                    </Button>
                                </div>
                            </>
                        ) : (
                            <p className="p-4 text-sm text-muted-foreground">
                                暂无可分配商品
                            </p>
                        )}
                    </section>
                ) : null}
            </div>
        </div>
    )
}
