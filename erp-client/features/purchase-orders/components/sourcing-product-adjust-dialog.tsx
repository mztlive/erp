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
import type { SourcingEditorProps } from "./create-sourcing/types"
import { SourcingProductEditor } from "./sourcing-product-editor"

/** 直接编辑共享草稿；关闭保留输入，完成时检查当前商品，最终写入仍由整单预览确认。 */
export function SourcingProductAdjustDialog({
    productId,
    onClose,
    ...props
}: SourcingEditorProps & {
    productId: string | null
    onClose: () => void
}) {
    const [showErrors, setShowErrors] = useState(false)
    const lastProductId = useRef(productId)
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
        await props.form.validate("change")
        const current = buildSourcingEditorRows(
            props.order,
            props.form.state.values.lines,
        ).find((item) => item.product.salesOrderLineId === productId)
        if (current?.issues.length) {
            setShowErrors(true)
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
                className="flex max-h-[calc(100dvh-2rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-xl"
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
                        </DialogHeader>
                        <div className="min-h-0 overflow-auto">
                            <SourcingProductEditor {...props} row={row} />
                            {showErrors && row.issues.length ? (
                                <p
                                    role="alert"
                                    className="px-4 pb-4 text-sm text-destructive"
                                >
                                    请先补齐或修正上方标红字段。
                                </p>
                            ) : null}
                        </div>
                        <div className="shrink-0 space-y-3 border-t border-border p-4">
                            <p className="text-xs text-muted-foreground">
                                修改暂存于本次分配，预览后统一确认。
                            </p>
                            <DialogFooter>
                                <Button
                                    id="sourcing-product-adjust-back"
                                    type="button"
                                    variant="outline"
                                    onClick={close}
                                >
                                    返回清单
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
                    </>
                ) : null}
            </DialogContent>
        </Dialog>
    )
}
