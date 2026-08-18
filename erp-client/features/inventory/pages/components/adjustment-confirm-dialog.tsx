"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import { AdjustmentApprovalArea } from "@/features/inventory/components/adjustment-approval-area"
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
            toStatus={{ label: "审批中", tone: "warning" }}
            description={
                <div className="space-y-3">
                    <p>确认后启动审批。余额在审批通过前不会变化。</p>
                    <AdjustmentApprovalArea
                        phase="confirm"
                        approval={meta?.approval}
                    />
                </div>
            }
            lockedFields={[
                meta ? `${meta.warehouseName} / ${meta.skuCode}` : "当前余额",
                "已按当前数据版本核对",
            ]}
            effects={[
                "创建审批中的库存调整单",
                "不立即修改账面、预占和可用数量",
                "经办人不得自行审批本单",
            ]}
            irreversibleEffects={["形成调整单号并进入审批"]}
            pending={pending}
            onConfirm={() => void onConfirm()}
        />
    )
}
