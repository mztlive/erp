"use client"

import Link from "next/link"

import { BatchImpactPreview } from "@/components/business"
import { Button } from "@/components/ui/button"
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
            <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                    <DialogTitle>
                        {isProd ? "停用生产环境连接" : "停用连接"}
                    </DialogTitle>
                    <DialogDescription>
                        停用改变治理状态，不删除连接、版本和历史业务记录。
                    </DialogDescription>
                </DialogHeader>
                <BatchImpactPreview
                    title="停用影响预览"
                    description="请核对发布、待处理订单与同步任务影响。"
                    filterSummary={`${conn.connectionCode} · ${conn.environmentLabel}`}
                    selectionScope={`${conn.supplier.name} · 单一连接`}
                    estimated={
                        conn.relatedImpact.activePublications +
                        conn.relatedImpact.openSupplierOrders +
                        conn.relatedImpact.activeSyncJobs
                    }
                    estimatedLabel="受影响发布/订单/任务"
                    processable={1}
                    processableLabel="连接"
                    skipped={0}
                    background={false}
                    sensitiveFields={["密钥配置", "签名材料"]}
                    skippedReason={undefined}
                />
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
                <div className="space-y-1 text-xs text-muted-foreground">
                    <p>历史版本与业务记录保留，不会删除任何数据。</p>
                    <p className="flex flex-wrap items-center gap-x-3">
                        替代方案：
                        <Link
                            href="/procurement/supplier-offerings"
                            className="text-primary underline-offset-2 hover:underline"
                        >
                            供应商供给
                        </Link>
                        <Link
                            href="/supplier-api/orders"
                            className="text-primary underline-offset-2 hover:underline"
                        >
                            供应商订单
                        </Link>
                        <Link
                            href="/governance/integration-errors"
                            className="text-primary underline-offset-2 hover:underline"
                        >
                            接口错误中心
                        </Link>
                    </p>
                </div>
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
                        disabled={!canDisable || pending}
                        onClick={() => void onSubmit()}
                    >
                        确认停用
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
