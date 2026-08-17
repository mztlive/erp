"use client"

import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"

export type ContractPreviewDialogProps = {
    open: boolean
    onOpenChange: (open: boolean) => void
    contractSnapshot?: string
    customerSnapshot: string
    paymentTermLabel: string
}

/** 以销售提交中的合同与客户快照展示合同，不读取合同中心或客户主数据。 */
export function ContractPreviewDialog({
    open,
    onOpenChange,
    contractSnapshot,
    customerSnapshot,
    paymentTermLabel,
}: ContractPreviewDialogProps) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                    <DialogTitle>销售提交中的合同快照</DialogTitle>
                    <DialogDescription>
                        采购确认以本次销售提交中的合同与客户为准，不读取客户主数据。
                    </DialogDescription>
                </DialogHeader>
                <dl className="grid gap-3 rounded-lg border border-border bg-muted/30 p-4 text-sm">
                    <div className="flex justify-between gap-4">
                        <dt className="text-muted-foreground">合同编号</dt>
                        <dd className="font-medium">
                            {contractSnapshot ?? "—"}
                        </dd>
                    </div>
                    <div className="flex justify-between gap-4">
                        <dt className="text-muted-foreground">客户</dt>
                        <dd>{customerSnapshot}</dd>
                    </div>
                    <div className="flex justify-between gap-4">
                        <dt className="text-muted-foreground">付款条件</dt>
                        <dd>{paymentTermLabel}</dd>
                    </div>
                </dl>
                <DialogFooter>
                    <DialogClose render={<Button type="button" />}>
                        关闭
                    </DialogClose>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
