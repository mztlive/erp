"use client"

import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"

/** 启用连接确认。 */
export function EnableConnectionDialog({
    open,
    onOpenChange,
    canEnable,
    pending,
    onSubmit,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
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
                    <DialogTitle>启用连接</DialogTitle>
                    <DialogDescription>
                        启用后恢复此连接的接口请求。
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
                    <LoadingButton
                        id="supplier-api-connections-enable-confirm"
                        loading={pending}
                        type="button"
                        disabled={!canEnable || pending}
                        onClick={() => void onSubmit()}
                    >
                        {pending ? "启用中…" : "确认启用"}
                    </LoadingButton>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
