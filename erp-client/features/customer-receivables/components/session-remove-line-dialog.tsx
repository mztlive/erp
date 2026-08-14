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

export function SessionRemoveLineDialog({
    pendingRemove,
    onOpenChange,
    onConfirmRemove,
}: {
    pendingRemove: string | null
    onOpenChange: (open: boolean) => void
    onConfirmRemove: (lineKey: string) => void
}) {
    return (
        <Dialog
            open={pendingRemove != null}
            onOpenChange={onOpenChange}
        >
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>移除该分配行？</DialogTitle>
                    <DialogDescription>
                        该行金额将不再分配，需重新输入或从池中再次加入。
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
                        variant="destructive"
                        onClick={() => {
                            if (pendingRemove) onConfirmRemove(pendingRemove)
                            onOpenChange(false)
                        }}
                    >
                        确认移除
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
