"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import { Input } from "@/components/ui/input"

export type SalesOrderCreateResubmitDialogProps = {
    open: boolean
    onOpenChange: (open: boolean) => void
    evidence: string
    onEvidenceChange: (value: string) => void
    pending: boolean
    onConfirm: () => Promise<void>
}

/** 驳回改单场景：登记客户重新确认依据后再报给采购。 */
export function SalesOrderCreateResubmitDialog({
    open,
    onOpenChange,
    evidence,
    onEvidenceChange,
    pending,
    onConfirm,
}: SalesOrderCreateResubmitDialogProps) {
    return (
        <FormalActionConfirmDialog
            open={open}
            onOpenChange={onOpenChange}
            title="再报给采购"
            description={
                <div className="space-y-2 text-left">
                    <p>请登记已上传的客户重新确认依据，再核对本次重提影响。</p>
                    <label
                        className="block space-y-1"
                        htmlFor="resubmit-customer-evidence-ids"
                    >
                        <span className="text-xs font-medium text-foreground">
                            客户确认依据 ID
                        </span>
                        <Input
                            id="resubmit-customer-evidence-ids"
                            value={evidence}
                            onChange={(event) =>
                                onEvidenceChange(event.target.value)
                            }
                            placeholder="粘贴已登记的文件 ID；多个以逗号分隔"
                            autoComplete="off"
                        />
                    </label>
                </div>
            }
            actionLabel="重提"
            confirmLabel="确认再报"
            fromStatus={{ label: "采购未通过", tone: "warning" }}
            toStatus={{ label: "待二次确认", tone: "info" }}
            lockedFields={["销售单号", "业务性质", "被驳回的那一版"]}
            effects={[
                "按当前整单内容生成新一版提交",
                "采购会收到新的确认待办",
                "以前被驳回的记录仍保留",
            ]}
            nextDepartment="采购"
            pending={pending}
            onConfirm={onConfirm}
        />
    )
}
