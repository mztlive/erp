"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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
import type { OwnershipStage } from "@/features/mall-sync/types"
import { STAGE_LABEL } from "@/features/mall-sync/types"
import type {
    MallSyncIncrementalFormApi,
    MallSyncPullFormApi,
} from "@/features/mall-sync/pages/hooks/use-mall-sync-page"
import { formatDateTime } from "@/lib/datetime"

type MallSyncIncrementalDialogProps = {
    open: boolean
    onOpenChange: (open: boolean) => void
    firstPhase: boolean
    manualSyncDisabledReason: string | null
    stage: OwnershipStage
    currentWatermark?: string
    form: MallSyncIncrementalFormApi
}

export function MallSyncIncrementalDialog({
    open,
    onOpenChange,
    firstPhase,
    manualSyncDisabledReason,
    stage,
    currentWatermark,
    form,
}: MallSyncIncrementalDialogProps) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>立即执行增量</DialogTitle>
                    <DialogDescription>
                        不修改来源；范围由系统按当前同步进度计算。禁止页面改写同步进度。
                    </DialogDescription>
                </DialogHeader>
                {firstPhase ? (
                    <form
                        className="space-y-3"
                        onSubmit={(e) => {
                            e.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <p className="text-sm text-muted-foreground">
                            同步至{" "}
                            {currentWatermark
                                ? formatDateTime(currentWatermark, "default")
                                : "—"}{" "}
                            · 阶段 {STAGE_LABEL[stage]}
                        </p>
                        <form.AppField
                            name="reason"
                            children={(field) => (
                                <field.TextField label="触发理由" required />
                            )}
                        />
                        <DialogFooter>
                            <DialogClose
                                render={
                                    <Button type="button" variant="outline" />
                                }
                            >
                                取消
                            </DialogClose>
                            <form.AppForm>
                                <form.SubmitButton label="创建增量任务" />
                            </form.AppForm>
                        </DialogFooter>
                    </form>
                ) : (
                    <Alert variant="destructive">
                        <AlertTitle>阶段不可用</AlertTitle>
                        <AlertDescription>
                            {manualSyncDisabledReason}
                        </AlertDescription>
                    </Alert>
                )}
            </DialogContent>
        </Dialog>
    )
}

type MallSyncPullDialogProps = {
    open: boolean
    onOpenChange: (open: boolean) => void
    firstPhase: boolean
    manualSyncDisabledReason: string | null
    form: MallSyncPullFormApi
}

export function MallSyncPullDialog({
    open,
    onOpenChange,
    firstPhase,
    manualSyncDisabledReason,
    form,
}: MallSyncPullDialogProps) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>按单号补拉</DialogTitle>
                    <DialogDescription>
                        使用原来源身份；不创建第二张销售单。仅第一阶段（商城开单）可用。
                    </DialogDescription>
                </DialogHeader>
                {!firstPhase ? (
                    <Alert variant="destructive">
                        <AlertTitle>阶段不可用</AlertTitle>
                        <AlertDescription>
                            {manualSyncDisabledReason}
                        </AlertDescription>
                    </Alert>
                ) : (
                    <form
                        className="space-y-3"
                        onSubmit={(e) => {
                            e.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <form.AppField
                            name="externalOrderNo"
                            children={(field) => (
                                <field.TextField label="商城销售单号" required />
                            )}
                        />
                        <form.AppField
                            name="reason"
                            children={(field) => (
                                <field.TextField label="补拉理由" required />
                            )}
                        />
                        <DialogFooter>
                            <DialogClose
                                render={
                                    <Button type="button" variant="outline" />
                                }
                            >
                                取消
                            </DialogClose>
                            <form.AppForm>
                                <form.SubmitButton label="创建补拉任务" />
                            </form.AppForm>
                        </DialogFooter>
                    </form>
                )}
            </DialogContent>
        </Dialog>
    )
}
