"use client"

import { useRef, useState } from "react"
import { QuantityValue } from "@/components/business"
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
import { buildSourcingEditorRows } from "../lib/sourcing/editor-rows"
import type {
    SourcingParticipationProps,
    SourcingEditorProps,
} from "./create-sourcing/types"
import { SourcingParticipationControl } from "./sourcing-participation-control"
import { SourcingProductEditor } from "./sourcing-product-editor"

/** 直接编辑共享草稿；关闭保留输入，完成时检查当前商品，最终写入仍由整单预览确认。 */
export function SourcingProductAdjustDialog({
    productId,
    onClose,
    onSetParticipation,
    ...props
}: SourcingEditorProps &
    SourcingParticipationProps & {
        productId: string | null
        onClose: () => void
    }) {
    const [showErrors, setShowErrors] = useState(false)
    const lastProductId = useRef(productId)
    const bodyRef = useRef<HTMLDivElement>(null)
    if (productId) lastProductId.current = productId
    const row = buildSourcingEditorRows(
        props.order,
        props.form.state.values.lines,
    ).find((item) => item.product.salesOrderLineId === productId)
    const close = () => {
        setShowErrors(false)
        onClose()
    }
    const finish = async () => {
        for (const field of Object.values(props.form.fieldInfo)) {
            const instance = field?.instance
            if (!instance || instance.store.state.meta.isTouched) continue
            instance.setMeta((previous) => ({ ...previous, isTouched: true }))
        }
        await props.form.validate("change")
        const current = buildSourcingEditorRows(
            props.order,
            props.form.state.values.lines,
        ).find((item) => item.product.salesOrderLineId === productId)
        if (current?.issues.length) {
            setShowErrors(true)
            requestAnimationFrame(() => {
                const target =
                    bodyRef.current?.querySelector<HTMLElement>(
                        '[aria-invalid="true"]',
                    ) ??
                    bodyRef.current?.querySelector<HTMLElement>(
                        '[role="alert"]',
                    )
                target?.scrollIntoView({ block: "center" })
                target?.focus({ preventScroll: true })
            })
            return
        }
        close()
    }
    return (
        <Dialog
            open={Boolean(row)}
            onOpenChange={(open) => {
                if (!open) close()
            }}
        >
            <DialogContent
                className="flex max-h-[calc(100dvh-2rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-2xl"
                closeButtonId="sourcing-product-adjust-close"
                finalFocus={() =>
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
                        <DialogHeader className="shrink-0 border-b border-border p-4 pr-14">
                            <DialogTitle>
                                调整 {row.product.itemName}
                            </DialogTitle>
                            <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
                                <DialogDescription>
                                    需供给{" "}
                                    <QuantityValue
                                        value={row.product.remainingQuantity}
                                        unit={row.product.unit}
                                    />
                                    {row.product.deliveryDeadline
                                        ? ` · 最晚交付 ${row.product.deliveryDeadline}`
                                        : ""}
                                </DialogDescription>
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
                                    control="checkbox"
                                    showHint={false}
                                />
                            </div>
                        </DialogHeader>
                        <div ref={bodyRef} className="min-h-0 overflow-auto">
                            <SourcingProductEditor
                                {...props}
                                onSetParticipation={onSetParticipation}
                                showParticipation={false}
                                row={row}
                            />
                        </div>
                        <div className="shrink-0 border-t border-border p-4">
                            {showErrors && row.issues.length ? (
                                <p
                                    role="alert"
                                    className="mb-3 text-sm text-destructive"
                                >
                                    请先修正上方标红的 {row.issues.length} 项。
                                </p>
                            ) : null}
                            <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                                <p className="text-xs text-muted-foreground">
                                    修改会带入整单预览，确认后统一提交。
                                </p>
                                <DialogFooter className="sm:flex-row">
                                    <Button
                                        id="sourcing-product-adjust-back"
                                        type="button"
                                        variant="outline"
                                        onClick={close}
                                    >
                                        关闭
                                    </Button>
                                    <Button
                                        id="sourcing-product-adjust-done"
                                        type="button"
                                        onClick={() => void finish()}
                                    >
                                        完成调整
                                    </Button>
                                </DialogFooter>
                            </div>
                        </div>
                    </>
                ) : null}
            </DialogContent>
        </Dialog>
    )
}
