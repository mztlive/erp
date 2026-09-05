"use client"

import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { toast } from "@/components/ui/toast"
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
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId="customers-create-dialog-close"
                className="max-h-[88vh] overflow-y-auto sm:max-w-3xl"
            >
                <DialogHeader>
                    <DialogTitle>新建客户</DialogTitle>
                    <DialogDescription>
                        创建客户主体与首版资料；名称相似只提示候选，不自动合并。
                    </DialogDescription>
                </DialogHeader>
                <CustomerForm
                    key={open ? "open" : "closed"}
                    mode="create"
                    onCancel={() => onOpenChange(false)}
                    onSucceeded={(customerId, revisionNo) => {
                        toast.add({
                            title: "客户已创建",
                            description: revisionNo
                                ? `客户资料版本 v${revisionNo} 已生效，可在客户列表继续查看。`
                                : "客户资料已生效，可在客户列表继续查看。",
                            type: "success",
                            timeout: 4000,
                        })
                        onSucceeded?.(customerId)
                        onOpenChange(false)
                    }}
                />
            </DialogContent>
        </Dialog>
    )
}
