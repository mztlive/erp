import { FormalActionConfirmDialog } from "@/components/business"
import type { IntegrationResolutionItemView } from "../types"
import { EVIDENCE_KIND_LABEL } from "../types"

export type TerminalConfirm =
    | { kind: "CLOSE_DUPLICATE" }
    | { kind: "CLOSE_MISROUTED" }
    | { kind: "RESOLVE" }
    | { kind: "CONFIRM_NO_ERROR" }
    | { kind: "CONFIRM_VALID_DIFFERENCE" }

export function TerminalActionDialog({
    confirm,
    item,
    pending,
    onConfirm,
    onCancel,
}: {
    confirm: TerminalConfirm
    item: IntegrationResolutionItemView
    pending: boolean
    onConfirm: () => void | Promise<void>
    onCancel: () => void
}) {
    const policy = item.resolutionEvidencePolicy
    const evidenceKinds = policy?.requiredEvidenceKinds ?? []

    if (
        confirm.kind === "CLOSE_DUPLICATE" ||
        confirm.kind === "CLOSE_MISROUTED"
    ) {
        const duplicate = confirm.kind === "CLOSE_DUPLICATE"
        return (
            <FormalActionConfirmDialog
                id={
                    duplicate
                        ? "integration-terminal-action-dialog-close-duplicate"
                        : "integration-terminal-action-dialog-close-misrouted"
                }
                open
                onOpenChange={(open) => {
                    if (!open) onCancel()
                }}
                actionLabel={duplicate ? "关闭重复" : "关闭误派"}
                title={duplicate ? "确认关闭重复任务" : "确认关闭误派任务"}
                description="仅关闭当前处理任务；不写业务解决结论，不影响业务记录。"
                fromStatus={{ label: item.status.label, tone: "warning" }}
                toStatus={{ label: "已关闭", tone: "neutral" }}
                effects={["任务退出待处理队列", "不改变业务记录"]}
                irreversibleEffects={["关闭后不再出现在待处理列表"]}
                pending={pending}
                onConfirm={onConfirm}
            />
        )
    }
    if (confirm.kind === "RESOLVE") {
        return (
            <FormalActionConfirmDialog
                id="integration-terminal-action-dialog-resolve"
                open
                onOpenChange={(open) => {
                    if (!open) onCancel()
                }}
                actionLabel="标记已解决"
                title="确认标记已解决"
                description="处理完成要求证据齐备；系统将按证据策略登记处理凭证。"
                fromStatus={{ label: item.status.label, tone: "warning" }}
                toStatus={{ label: "已完成", tone: "success" }}
                effects={[
                    evidenceKinds.length > 0
                        ? `将校验已关联的 ${evidenceKinds.length} 类处理凭证：${evidenceKinds
                              .map((kind) => EVIDENCE_KIND_LABEL[kind])
                              .join("、")}`
                        : "系统将登记本次处理凭证",
                    "任务完成并退出待处理队列",
                ]}
                irreversibleEffects={["处理结论写入审计，不可自动撤回"]}
                pending={pending}
                onConfirm={onConfirm}
            />
        )
    }
    const isNoError = confirm.kind === "CONFIRM_NO_ERROR"
    return (
        <FormalActionConfirmDialog
            id={
                isNoError
                    ? "integration-terminal-action-dialog-confirm-no-error"
                    : "integration-terminal-action-dialog-confirm-valid-difference"
            }
            open
            onOpenChange={(open) => {
                if (!open) onCancel()
            }}
            actionLabel={isNoError ? "确认无误" : "确认有效差异"}
            title={isNoError ? "确认差异无误" : "确认差异为有效差异"}
            description="按已选注册原因追加对账处理记录；本操作不涉及任务关闭。"
            fromStatus={{ label: item.status.label, tone: "warning" }}
            toStatus={{ label: "已确认", tone: "success" }}
            effects={["按注册原因追加对账处理记录", "不改变两侧业务数据"]}
            irreversibleEffects={["对账结论写入审计，不可自动撤回"]}
            pending={pending}
            onConfirm={onConfirm}
        />
    )
}
