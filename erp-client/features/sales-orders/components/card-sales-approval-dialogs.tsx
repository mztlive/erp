"use client"

import { FormalActionConfirmDialog } from "@/components/business"

type ConfirmDialogProps = {
    open: boolean
    onOpenChange: (open: boolean) => void
    onConfirm: () => Promise<void>
}

export function CardApprovalApproveConfirmDialog({
    open,
    onOpenChange,
    isManager,
    onConfirm,
}: ConfirmDialogProps & { isManager: boolean }) {
    return (
        <FormalActionConfirmDialog
            open={open}
            onOpenChange={onOpenChange}
            title={isManager ? "确认销售主管通过" : "确认运营通过并生效"}
            actionLabel="通过"
            confirmLabel="确认通过"
            fromStatus={{
                label: isManager ? "待销售领导审批" : "待运营审批",
                tone: "warning",
            }}
            toStatus={{
                label: isManager ? "待运营审批" : "已生效",
                tone: "success",
            }}
            lockedFields={["待审批内容", "销售单号"]}
            effects={
                isManager
                    ? ["记录领导审批通过", "激活唯一运营审批步骤"]
                    : [
                          "记录运营审批通过",
                          "原子形成销售版本、应收和执行投影",
                      ]
            }
            nextDepartment={isManager ? "运营" : "票款与商城执行"}
            onConfirm={onConfirm}
        />
    )
}

export function CardApprovalRejectConfirmDialog({
    open,
    onOpenChange,
    onConfirm,
}: ConfirmDialogProps) {
    return (
        <FormalActionConfirmDialog
            open={open}
            onOpenChange={onOpenChange}
            title="确认驳回卡券审批"
            actionLabel="驳回"
            confirmLabel="确认驳回"
            fromStatus={{ label: "审批中", tone: "warning" }}
            toStatus={{ label: "退回销售", tone: "destructive" }}
            lockedFields={["待审批内容", "销售单号"]}
            effects={[
                "记录驳回原因与说明",
                "结束当前审批实例",
                "不激活下一审批步骤",
            ]}
            nextDepartment="销售"
            onConfirm={onConfirm}
        />
    )
}

export function CardApprovalTerminateConfirmDialog({
    open,
    onOpenChange,
    onConfirm,
}: ConfirmDialogProps) {
    return (
        <FormalActionConfirmDialog
            open={open}
            onOpenChange={onOpenChange}
            title="确认终止卡券审批"
            actionLabel="终止审批"
            confirmLabel="确认终止"
            fromStatus={{ label: "审批中", tone: "warning" }}
            toStatus={{ label: "审批已终止", tone: "destructive" }}
            lockedFields={["待审批内容", "销售单号"]}
            effects={[
                "记录终止原因与说明",
                "结束当前审批实例且不形成驳回记录",
                "冻结提交置为已失效，销售单恢复为草稿",
            ]}
            nextDepartment="销售"
            onConfirm={onConfirm}
        />
    )
}

export function CardApprovalCancelConfirmDialog({
    open,
    onOpenChange,
    onConfirm,
}: ConfirmDialogProps) {
    return (
        <FormalActionConfirmDialog
            open={open}
            onOpenChange={onOpenChange}
            title="确认撤回卡券审批"
            actionLabel="撤回审批"
            confirmLabel="确认撤回"
            fromStatus={{ label: "审批中", tone: "warning" }}
            toStatus={{ label: "可继续修改", tone: "neutral" }}
            lockedFields={["待审批内容", "销售单号"]}
            effects={[
                "取消当前审批和未执行环节",
                "关闭当前待处理事项",
                "销售单恢复为可修改草稿",
            ]}
            nextDepartment="销售"
            onConfirm={onConfirm}
        />
    )
}
