"use client"

import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"

/** 启用连接确认（生产环境二次确认）。 */
export function EnableConnectionDialog({
    open,
    onOpenChange,
    isProd,
    canEnable,
    pending,
    onSubmit,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    isProd: boolean
    canEnable: boolean
    pending: boolean
    onSubmit: () => Promise<void>
}) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="sm:max-w-md">
                <DialogHeader>
                    <DialogTitle>
                        {isProd ? "启用生产环境连接" : "启用连接"}
                    </DialogTitle>
                    <DialogDescription>
                        启用后连接将恢复对外接口可用，后续下单、查询等业务请求将按能力声明放行。
                        {isProd ? " 生产环境操作需谨慎核对。" : ""}
                    </DialogDescription>
                </DialogHeader>
                <DialogFooter>
                    <Button
                        type="button"
                        variant="outline"
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <Button
                        type="button"
                        disabled={!canEnable || pending}
                        onClick={() => void onSubmit()}
                    >
                        确认启用
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
