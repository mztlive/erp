"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import type { AdjustmentMeta } from "../hooks/use-adjustment-workflow"

interface AdjustmentConfirmDialogProps {
    open: boolean
    pending: boolean
    meta: AdjustmentMeta | null
    onOpenChange: (open: boolean) => void
    onConfirm: () => void
}

export function AdjustmentConfirmDialog({
    open,
    pending,
    meta,
    onOpenChange,
    onConfirm,
}: AdjustmentConfirmDialogProps) {
    return (
        <FormalActionConfirmDialog
            open={open}
            onOpenChange={onOpenChange}
            actionLabel="提交库存调整"
            confirmLabel="确认提交"
            fromStatus={{ label: "草稿", tone: "neutral" }}
            toStatus={{ label: "待仓储复核", tone: "warning" }}
            description="确认后形成调整单，进入仓储复核队列。余额在确认入账前不会变化。"
            lockedFields={[
                meta
                    ? `${meta.warehouseName} / ${meta.skuCode}`
                    : "当前余额",
                "已按当前数据版本核对",
            ]}
            effects={[
                "创建待仓储复核的库存调整单",
                "不立即修改账面、预占和可用数量",
                "经办人不得自行复核或确认入账",
            ]}
            nextDepartment="仓储复核"
            irreversibleEffects={["形成调整单号并进入连续队列"]}
            pending={pending}
            onConfirm={() => void onConfirm()}
        />
    )
}
