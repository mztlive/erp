"use client"

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
            <DialogContent
                closeButtonId="supplier-api-connections-enable-close"
                className="sm:max-w-md"
            >
                <DialogHeader>
                    <DialogTitle>
                        {isProd ? "启用生产环境连接" : "启用连接"}
                    </DialogTitle>
                    <DialogDescription>
                        启用后恢复此连接的接口请求。
                        {isProd ? " 请核对生产环境连接。" : ""}
                    </DialogDescription>
                </DialogHeader>
                <DialogFooter>
                    <Button
                        id="supplier-api-connections-enable-cancel"
                        type="button"
                        variant="outline"
                        disabled={pending}
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <Button
                        id="supplier-api-connections-enable-confirm"
                        type="button"
                        disabled={!canEnable || pending}
                        onClick={() => void onSubmit()}
                    >
                        {pending ? (
                            <Spinner
                                className="size-4 animate-spin"
                                aria-hidden="true"
                            />
                        ) : null}
                        {pending ? "启用中…" : "确认启用"}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
