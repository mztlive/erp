"use client"

import * as React from "react"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { ProductEffectiveSection } from "./product-effective-section"

type Props = React.ComponentProps<typeof ProductEffectiveSection> & {
    open: boolean
    onOpenChange: (open: boolean) => void
    pending: boolean
    changes: readonly string[]
    onConfirm: () => void
    feedback?: React.ReactNode
    attempted: boolean
}

export function ProductSaveDialog({
    open,
    onOpenChange,
    pending,
    changes,
    onConfirm,
    feedback,
    attempted,
    ...effective
}: Props) {
    const reasonRef = React.useRef<HTMLTextAreaElement>(null)
    const reasonInvalid = attempted && effective.changeReason.trim().length < 2
    React.useEffect(() => {
        if (open && reasonInvalid) reasonRef.current?.focus()
    }, [open, reasonInvalid])
    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!pending) onOpenChange(next)
            }}
        >
            <DialogContent
                className="max-h-[90dvh] overflow-y-auto sm:max-w-xl"
                closeButtonId="master-data-product-save-close"
                showCloseButton={!pending}
            >
                <DialogHeader>
                    <DialogTitle>
                        {effective.isCreate ? "创建商品" : "保存更新"}
                    </DialogTitle>
                    <DialogDescription>
                        确认本次内容及生效时间，保存后形成新的商品资料版本。
                    </DialogDescription>
                </DialogHeader>
                <form
                    id="master-data-product-save-form"
                    className="space-y-5"
                    onSubmit={(event) => {
                        event.preventDefault()
                        if (!pending) onConfirm()
                    }}
                >
                    <section
                        className="rounded-lg bg-muted/40 p-4"
                        aria-label="本次修改摘要"
                    >
                        <h3 className="text-sm font-medium">
                            {effective.isCreate ? "新建商品资料" : "本次修改"}
                        </h3>
                        <ul className="mt-2 max-h-36 space-y-1 overflow-y-auto break-words text-sm text-muted-foreground">
                            {(changes.length
                                ? changes
                                : [
                                      effective.isCreate
                                          ? "基本资料、规格与 SKU"
                                          : "商品资料内容未改变，将按本次原因生成新版本",
                                  ]
                            ).map((change) => (
                                <li key={change}>{change}</li>
                            ))}
                        </ul>
                    </section>
                    {feedback}
                    <ProductEffectiveSection
                        {...effective}
                        idPrefix="master-data-product-detail-effective"
                        reasonInvalid={reasonInvalid}
                        reasonRef={reasonRef}
                    />
                    <DialogFooter>
                        <Button
                            id="master-data-product-save-back"
                            type="button"
                            variant="outline"
                            disabled={pending}
                            onClick={() => onOpenChange(false)}
                        >
                            继续编辑
                        </Button>
                        <Button
                            id="master-data-product-save-confirm"
                            type="submit"
                            disabled={pending || !effective.canRevise}
                        >
                            {pending ? "保存中…" : "确认保存"}
                        </Button>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
