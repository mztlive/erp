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
    /** 合同详情查询中；否则视为暂不可读取。 */
    pending: boolean
    contractSnapshot?: string
    customerSnapshot: string
    paymentTermLabel: string
}

/** 合同详情暂不可读时的兜底弹窗：以销售提交中的合同快照为准。 */
export function ContractPreviewDialog({
    open,
    onOpenChange,
    pending,
    contractSnapshot,
    customerSnapshot,
    paymentTermLabel,
}: ContractPreviewDialogProps) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                    <DialogTitle>
                        {pending ? "正在读取合同" : "合同信息暂不可读取"}
                    </DialogTitle>
                    <DialogDescription>
                        {pending
                            ? "合同仍在加载，完成后会在当前页面显示。"
                            : "当前账号未取得合同详情，或合同记录已不存在。采购确认仍以销售提交中的合同快照为准。"}
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
