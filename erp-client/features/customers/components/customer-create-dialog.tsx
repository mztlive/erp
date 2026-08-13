"use client"

import * as React from "react"

import { DiscardConfirmDialog } from "@/components/business"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { CustomerForm } from "@/features/customers/components/customer-form"

/**
 * 新建客户对话框：仅负责容器（居中 Dialog + 页头），
 * 表单本体复用 CustomerForm（与详情页原地编辑同一套）。
 */
export function CustomerCreateDialog({
    open,
    onOpenChange,
    onSucceeded,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    onSucceeded?: (customerId: string) => void
}) {
    const [dirty, setDirty] = React.useState(false)
    const [discardOpen, setDiscardOpen] = React.useState(false)

    return (
        <>
            <Dialog
                open={open}
                onOpenChange={(next) => {
                    if (!next && dirty) {
                        setDiscardOpen(true)
                        return
                    }
                    onOpenChange(next)
                }}
            >
                <DialogContent className="max-h-[88vh] overflow-y-auto sm:max-w-3xl">
                    <DialogHeader>
                        <DialogTitle>新建客户</DialogTitle>
                        <DialogDescription>
                            创建客户主体与首版资料；名称相似只提示候选，不自动合并。
                        </DialogDescription>
                    </DialogHeader>
                    <CustomerForm
                        mode="create"
                        onDirtyChange={setDirty}
                        onCancel={() => onOpenChange(false)}
                        onSucceeded={(customerId) => {
                            // 短暂停留让「客户已创建」结果卡可见，再关闭并进入详情（避免成功态一闪而过）。
                            window.setTimeout(() => {
                                onOpenChange(false)
                                onSucceeded?.(customerId)
                            }, 1400)
                        }}
                    />
                </DialogContent>
            </Dialog>

            <DiscardConfirmDialog
                open={discardOpen}
                onOpenChange={setDiscardOpen}
                onConfirm={() => {
                    setDiscardOpen(false)
                    onOpenChange(false)
                }}
            />
        </>
    )
}
