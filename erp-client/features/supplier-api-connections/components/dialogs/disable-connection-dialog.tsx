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
import type { ConnectionCenterView } from "@/features/supplier-api-connections/types"

/** 停用确认与影响预览；停用改变治理状态，不删除任何数据。 */
export function DisableConnectionDialog({
    open,
    onOpenChange,
    conn,
    canDisable,
    pending,
    onSubmit,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    conn: ConnectionCenterView
    canDisable: boolean
    pending: boolean
    onSubmit: () => Promise<void>
}) {
    const isProd = conn.environment === "PRODUCTION"
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId="supplier-api-connections-disable-close"
                className="sm:max-w-lg"
            >
                <DialogHeader>
                    <DialogTitle>
                        {isProd ? "停用生产环境连接" : "停用连接"}
                    </DialogTitle>
                    <DialogDescription>
                        停用后暂停此连接的接口请求，历史记录保留。
                    </DialogDescription>
                </DialogHeader>
                <p className="text-sm font-medium">
                    {conn.supplier.name} · {conn.connectionCode} ·{" "}
                    {conn.environmentLabel}
                </p>
                <dl className="grid gap-2 text-sm sm:grid-cols-3">
                    <div className="rounded-lg border p-3">
                        <dt className="text-xs text-muted-foreground">
                            生效发布
                        </dt>
                        <dd className="num font-medium">
                            {conn.relatedImpact.activePublications}
                        </dd>
                    </div>
                    <div className="rounded-lg border p-3">
                        <dt className="text-xs text-muted-foreground">
                            待处理订单
                        </dt>
                        <dd className="num font-medium">
                            {conn.relatedImpact.openSupplierOrders}
                        </dd>
                    </div>
                    <div className="rounded-lg border p-3">
                        <dt className="text-xs text-muted-foreground">
                            同步任务
                        </dt>
                        <dd className="num font-medium">
                            {conn.relatedImpact.activeSyncJobs}
                        </dd>
                    </div>
                </dl>
                <DialogFooter>
                    <Button
                        id="supplier-api-connections-disable-cancel"
                        type="button"
                        variant="outline"
                        disabled={pending}
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <Button
                        id="supplier-api-connections-disable-confirm"
                        type="button"
                        variant="destructive"
                        disabled={!canDisable || pending}
                        onClick={() => void onSubmit()}
                    >
                        {pending ? (
                            <Spinner
                                className="size-4 animate-spin"
                                aria-hidden="true"
                            />
                        ) : null}
                        {pending ? "停用中…" : "确认停用"}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
