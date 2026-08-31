import { SequentialProcessBar } from "@/components/business"
import type { ResponsibilityStatus } from "@/components/business/workflow-actions"
import type { IntegrationResolutionItemView } from "../../types"

export function IntegrationItemProgress({
    item,
    positionIndex,
    positionTotal,
    responsibilityStatus,
    formalPending,
    focusMode,
    onBack,
    onProcess,
    onProcessNext,
    id,
    idPrefix,
}: {
    item: IntegrationResolutionItemView
    positionIndex: number
    positionTotal: number
    responsibilityStatus: ResponsibilityStatus
    formalPending: boolean
    focusMode: boolean
    onBack: () => void
    onProcess: () => void
    onProcessNext: () => void
    id?: string
    idPrefix?: string
}) {
    return (
        <SequentialProcessBar
            id={id}
            idPrefix={idPrefix ?? "integration-item-progress"}
            current={positionIndex}
            total={positionTotal}
            responsibilityStatus={responsibilityStatus}
            responsibilityStatusLabel={
                !item.hasWorkItem
                    ? item.identity.itemType === "RECONCILIATION_DIFFERENCE"
                        ? "直接对账"
                        : "责任未配置"
                    : item.workItem?.ownerUser
                      ? `当前处理人：${item.workItem.ownerUser.displayName}`
                      : undefined
            }
            processLabel="处理当前"
            processNextLabel="下一项"
            pending={formalPending}
            processDisabled={responsibilityStatus !== "assigned_to_me"}
            processNextDisabled={false}
            showProcessNext={!focusMode}
            onBack={onBack}
            onProcess={onProcess}
            onProcessNext={onProcessNext}
        />
    )
}
