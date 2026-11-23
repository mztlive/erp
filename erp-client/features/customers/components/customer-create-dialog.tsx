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
                size="xl"
                closeButtonId="customers-create-dialog-close"
            >
                <DialogHeader>
                    <DialogTitle>新建客户</DialogTitle>
                    <DialogDescription>
                        填写客户基础信息、联系人和开票资料，创建后可用于签合同与开销售单。
                    </DialogDescription>
                </DialogHeader>
                <CustomerForm
                    key={open ? "open" : "closed"}
                    mode="create"
                    onCancel={() => onOpenChange(false)}
                    onSucceeded={(customerId) => {
                        toast.add({
                            title: "客户已创建",
                            description: "客户资料已保存，可在客户列表中查看。",
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
