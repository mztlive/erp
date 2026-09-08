"use client"

import { RefreshCwIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"

/** 健康检查确认（生产环境二次确认）；检查不会创建真实业务订单。 */
export function RunHealthCheckDialog({
    open,
    onOpenChange,
    isProd,
    canRunHealth,
    pending,
    onSubmit,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    isProd: boolean
    canRunHealth: boolean
    pending: boolean
    onSubmit: () => Promise<void>
}) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId="supplier-api-connections-health-check-close"
                className="sm:max-w-md"
            >
                <DialogHeader>
                    <DialogTitle>执行健康检查</DialogTitle>
                    <DialogDescription>
                        检查全部接口能力并更新连接健康状态；失败时生成异常待办。
                        {isProd
                            ? "生产环境检查不会创建真实业务订单。"
                            : "可在本页查看检查结果。"}
                    </DialogDescription>
                </DialogHeader>
                <DialogFooter>
                    <Button
                        id="supplier-api-connections-health-check-cancel"
                        type="button"
                        variant="outline"
                        disabled={pending}
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <Button
                        id="supplier-api-connections-health-check-confirm"
                        type="button"
                        disabled={!canRunHealth || pending}
                        onClick={() => void onSubmit()}
                    >
                        {pending ? (
                            <Spinner
                                className="size-4 animate-spin"
                                aria-hidden="true"
                            />
                        ) : (
                            <RefreshCwIcon
                                className="size-4"
                                aria-hidden="true"
                            />
                        )}
                        {pending ? "执行中…" : "确认执行"}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
