"use client"

import { useEffect, useRef, useState } from "react"
import {
    ArrowLeftIcon,
    ArrowRightIcon,
    PlusIcon,
    Trash2Icon,
} from "lucide-react"
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
import { sourcingFormValidationError } from "../lib/purchase-order-create-validation"
import { buildSourcingEditorRows } from "../lib/sourcing/editor-rows"
import { canSplitSourcingProduct } from "../lib/sourcing-quantity"
import type {
    SourcingParticipationProps,
    SourcingEditorProps,
} from "./create-sourcing/types"
import { SourcingParticipationControl } from "./sourcing-participation-control"
import { SourcingProductEditor } from "./sourcing-product-editor"
import { SourcingSourceList } from "./sourcing-source-list"
import { SourcingAllocationSummary } from "./sourcing-allocation-summary"

/** 左侧比较全部来源，右侧编辑一个来源；全部字段继续使用整单共享草稿。 */
export function SourcingProductAdjustDialog({
    productId,
    onClose,
    onSetParticipation,
    navigationIds,
    onNavigate,
    returnFocusId,
    ...props
}: SourcingEditorProps &
    SourcingParticipationProps & {
        productId: string | null
        onClose: () => void
        navigationIds?: readonly string[]
        onNavigate?: (id: string) => void
        returnFocusId?: string
    }) {
    const [showErrors, setShowErrors] = useState(false)
    const [selectedRowKey, setSelectedRowKey] = useState<string | null>(null)
    const lastProductId = useRef(productId)
    const bodyRef = useRef<HTMLDivElement>(null)
    const listRef = useRef<HTMLDivElement>(null)
    if (productId) lastProductId.current = productId
    const rows = buildSourcingEditorRows(
        props.order,
        props.form.state.values.lines,
    )
    const row = rows.find((item) => item.product.salesOrderLineId === productId)
    const navigation = (navigationIds ?? []).flatMap((id) => {
        const found = rows.find((item) => item.product.salesOrderLineId === id)
        return found ? [found] : []
    })
    const position = navigation.findIndex((item) => item === row)
    const previous = navigation[position - 1]
    const next = navigation[position + 1]
    const nextPending = [
        ...navigation.slice(position + 1),
        ...navigation.slice(0, Math.max(0, position)),
    ].find((item) => item.needsAttention)
    const active =
        row?.allocations.find(({ line }) => line.rowKey === selectedRowKey) ??
        row?.allocations[0]
    const activeRowKey = active?.line.rowKey
    const addDisabledReason =
        !row || (!row.selected && !row.partiallySelected)
            ? "恢复本次分配后可添加来源"
            : row.allocations.length >= row.product.options.length
              ? "可用来源均已添加"
              : !canSplitSourcingProduct(row.product, row.allocations.length)
                ? "剩余需求不足以继续拆分，或商品单位不支持拆分"
                : undefined
    const fields =
        sourcingFormValidationError(props.order, props.form.state.values)
            ?.fields ?? {}
    const errors = new Map(
        (row?.allocations ?? []).map(({ line, index }) => [
            line.rowKey,
            showErrors
                ? [
                      ...new Set(
                          Object.entries(fields)
                              .filter(([path]) =>
                                  path.startsWith(`lines[${index}].`),
                              )
                              .map(([, message]) => message),
                      ),
                  ]
                : [],
        ]),
    )
    const activeErrors = activeRowKey ? (errors.get(activeRowKey) ?? []) : []

    useEffect(() => {
        bodyRef.current?.scrollTo({ top: 0 })
        if (!activeRowKey) return
        const target = document.getElementById(
            `sourcing-source-${toAutomationIdSegment(activeRowKey)}-edit`,
        )
        if (target && listRef.current?.contains(target))
            target.scrollIntoView({ block: "nearest", inline: "nearest" })
    }, [activeRowKey])

    const close = () => {
        setShowErrors(false)
        setSelectedRowKey(null)
        onClose()
    }
    const focusEditor = (rowKey: string, invalid = false) => {
        requestAnimationFrame(() => {
            const target = invalid
                ? bodyRef.current?.querySelector<HTMLElement>(
                      '[aria-invalid="true"], [role="alert"]',
                  )
                : document.getElementById(
                      `procurement-orders-create-row-${toAutomationIdSegment(rowKey)}-quantity`,
                  )
            if (!target || !bodyRef.current?.contains(target)) return
            target.scrollIntoView({ block: "nearest", inline: "nearest" })
            target.focus({ preventScroll: true })
        })
    }
    const addSplit = () => {
        if (!row) return
        const previousKeys = new Set(
            row.allocations.map(({ line }) => line.rowKey),
        )
        props.onAddSplit(row.product.salesOrderLineId)
        const added = props.form.state.values.lines.find(
            (line) =>
                line.salesOrderLineId === row.product.salesOrderLineId &&
                !previousKeys.has(line.rowKey),
        )
        if (!added) return
        setSelectedRowKey(added.rowKey)
        focusEditor(added.rowKey)
    }
    const removeActive = () => {
        if (!row || !active || row.allocations.length <= 1) return
        const position = row.allocations.findIndex(
            ({ line }) => line.rowKey === activeRowKey,
        )
        const next =
            row.allocations[position + 1] ?? row.allocations[position - 1]
        setSelectedRowKey(next?.line.rowKey ?? null)
        props.onRemoveSplit(active.line.rowKey)
    }
    const finish = async (nextId?: string) => {
        if (!row) return
        const prefixes = row.allocations.map(({ index }) => `lines[${index}].`)
        for (const [name, field] of Object.entries(props.form.fieldInfo)) {
            const instance = field?.instance
            if (
                !prefixes.some((prefix) => name.startsWith(prefix)) ||
                !instance ||
                instance.store.state.meta.isTouched
            )
                continue
            instance.setMeta((previous) => ({ ...previous, isTouched: true }))
        }
        await props.form.validate("change")
        // 从整单值校验，不能只检查右侧当前挂载的字段。
        const currentErrors =
            sourcingFormValidationError(props.order, props.form.state.values)
                ?.fields ?? {}
        const invalid = row.allocations.find(({ index }) =>
            Object.keys(currentErrors).some((path) =>
                path.startsWith(`lines[${index}].`),
            ),
        )
        if (invalid) {
            setShowErrors(true)
            setSelectedRowKey(invalid.line.rowKey)
            focusEditor(invalid.line.rowKey, true)
            return
        }
        if (nextId && onNavigate) {
            setShowErrors(false)
            setSelectedRowKey(null)
            onNavigate(nextId)
        } else close()
    }
    return (
        <Dialog
            open={Boolean(row)}
            onOpenChange={(open) => {
                if (!open) close()
            }}
        >
            <DialogContent
                className="flex h-[min(46rem,calc(100dvh-3rem))] flex-col gap-0 overflow-hidden p-0 sm:max-w-[min(76rem,calc(100vw-2rem))]"
                closeButtonId="sourcing-product-adjust-close"
                finalFocus={() =>
                    (returnFocusId
                        ? document.getElementById(returnFocusId)
                        : null) ??
                    (lastProductId.current
                        ? document.getElementById(
                              `sourcing-summary-${toAutomationIdSegment(lastProductId.current)}-adjust`,
                          )
                        : null) ??
                    document.getElementById("sourcing-products-search")
                }
            >
                {row ? (
                    <>
                        <DialogHeader className="shrink-0 border-b border-border px-5 py-4 pr-14">
                            <div className="flex flex-wrap items-center justify-between gap-3">
                                <DialogTitle>
                                    供给分配 · {row.product.itemName}
                                </DialogTitle>
                                <SourcingParticipationControl
                                    idPrefix={`sourcing-product-adjust-${toAutomationIdSegment(row.product.salesOrderLineId)}`}
                                    itemName={row.product.itemName}
                                    included={
                                        row.selected || row.partiallySelected
                                    }
                                    onChange={(included) =>
                                        onSetParticipation(
                                            [row.product.salesOrderLineId],
                                            included,
                                        )
                                    }
                                    showHint={false}
                                />
                            </div>
                            <DialogDescription className="sr-only">
                                左侧比较并选择来源，右侧调整当前来源。修改保留在草稿中，整单预览后统一提交。
                            </DialogDescription>
                        </DialogHeader>
                        <SourcingAllocationSummary row={row} />
                        <div className="grid min-h-0 flex-1 grid-rows-[minmax(8rem,0.8fr)_minmax(0,1.2fr)] lg:grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)] lg:grid-rows-1">
                            <section
                                className="flex min-h-0 min-w-0 flex-col border-b border-border lg:border-r lg:border-b-0"
                                aria-label="供给来源列表"
                            >
                                <h3 className="shrink-0 px-5 pt-4 pb-2 text-sm font-medium">
                                    供给来源 · {row.allocations.length}
                                </h3>
                                <div
                                    ref={listRef}
                                    className="min-h-0 flex-1 overflow-y-auto overscroll-contain"
                                >
                                    <SourcingSourceList
                                        form={props.form}
                                        row={row}
                                        activeRowKey={activeRowKey}
                                        onSelect={setSelectedRowKey}
                                        errors={errors}
                                    />
                                </div>
                                <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-t border-border px-5 py-3">
                                    <Button
                                        id={`sourcing-product-${toAutomationIdSegment(row.product.salesOrderLineId)}-split`}
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        disabled={Boolean(addDisabledReason)}
                                        onClick={addSplit}
                                        title={addDisabledReason}
                                    >
                                        <PlusIcon />
                                        添加来源
                                    </Button>
                                    <span className="text-xs text-muted-foreground">
                                        {addDisabledReason ||
                                            "点击来源行进行调整"}
                                    </span>
                                </div>
                            </section>
                            <section
                                id="sourcing-product-adjust-detail"
                                className="flex min-h-0 min-w-0 flex-col"
                                aria-label="当前来源编辑"
                            >
                                <div className="flex min-h-14 shrink-0 items-center justify-between gap-3 px-4 pt-2">
                                    <h3 className="text-sm font-medium">
                                        编辑当前来源
                                    </h3>
                                    {active && row.allocations.length > 1 ? (
                                        <Button
                                            id={`procurement-orders-create-row-${toAutomationIdSegment(active.line.rowKey)}-split-remove`}
                                            type="button"
                                            variant="ghost"
                                            size="sm"
                                            onClick={removeActive}
                                        >
                                            <Trash2Icon />
                                            删除此来源
                                        </Button>
                                    ) : null}
                                </div>
                                <div
                                    id="sourcing-product-adjust-body"
                                    ref={bodyRef}
                                    className="min-h-0 flex-1 overflow-y-auto overscroll-contain"
                                >
                                    {activeErrors.length ? (
                                        <div
                                            id="sourcing-product-adjust-source-errors"
                                            role="alert"
                                            tabIndex={-1}
                                            className="mx-4 mt-2 rounded-md bg-destructive/5 p-3 text-xs text-destructive"
                                        >
                                            {activeErrors.map((message) => (
                                                <p key={message}>{message}</p>
                                            ))}
                                        </div>
                                    ) : null}
                                    {active ? (
                                        <SourcingProductEditor
                                            key={active.line.rowKey}
                                            {...props}
                                            condensed
                                            activeRowKey={active.line.rowKey}
                                            onSetParticipation={
                                                onSetParticipation
                                            }
                                            showParticipation={false}
                                            row={row}
                                        />
                                    ) : (
                                        <p className="p-4 text-sm text-muted-foreground">
                                            尚未添加供给来源
                                        </p>
                                    )}
                                </div>
                            </section>
                        </div>
                        <DialogFooter className="shrink-0 items-center border-t border-border px-5 py-3 sm:justify-between">
                            <p className="text-xs text-muted-foreground">
                                {showErrors && row.issues.length ? (
                                    <span className="mr-3 text-destructive">
                                        还有 {row.issues.length} 项需调整
                                    </span>
                                ) : null}
                                仅更新草稿，整单预览后统一提交
                            </p>
                            <div className="flex flex-wrap items-center gap-2">
                                {onNavigate ? (
                                    <>
                                        <Button
                                            id="sourcing-product-adjust-previous"
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            disabled={!previous}
                                            onClick={() =>
                                                previous &&
                                                void finish(
                                                    previous.product
                                                        .salesOrderLineId,
                                                )
                                            }
                                        >
                                            <ArrowLeftIcon />
                                            上一商品
                                        </Button>
                                        <span className="num text-xs text-muted-foreground">
                                            {position + 1} / {navigation.length}
                                        </span>
                                        <Button
                                            id="sourcing-product-adjust-next"
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            disabled={!next}
                                            onClick={() =>
                                                next &&
                                                void finish(
                                                    next.product
                                                        .salesOrderLineId,
                                                )
                                            }
                                        >
                                            下一商品
                                            <ArrowRightIcon />
                                        </Button>
                                        <Button
                                            id="sourcing-product-adjust-next-pending"
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            disabled={!nextPending}
                                            onClick={() =>
                                                nextPending &&
                                                void finish(
                                                    nextPending.product
                                                        .salesOrderLineId,
                                                )
                                            }
                                        >
                                            下一待调整
                                        </Button>
                                    </>
                                ) : null}
                                <Button
                                    id="sourcing-product-adjust-done"
                                    type="button"
                                    onClick={() => void finish()}
                                >
                                    完成调整
                                </Button>
                            </div>
                        </DialogFooter>
                    </>
                ) : null}
            </DialogContent>
        </Dialog>
    )
}
