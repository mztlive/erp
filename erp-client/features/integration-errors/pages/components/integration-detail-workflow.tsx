import * as React from "react"
import { PauseIcon } from "lucide-react"
import type { InterfaceErrorClass } from "@/components/business"
import type { ResponsibilityStatus } from "@/components/business/workflow-actions"
import { Button } from "@/components/ui/button"
import { IntegrationEvidencePanel } from "../../components/integration-evidence-panel"
import { IntegrationItemSummary } from "../../components/integration-item-summary"
import type { IntegrationResolutionItemView } from "../../types"
import { IntegrationErrorResolutionPanel } from "./integration-error-resolution-panel"
import { IntegrationItemProgress } from "./integration-item-progress"

export function IntegrationDetailWorkflow({
    item,
    positionIndex,
    positionTotal,
    responsibilityStatus,
    formalPending,
    focusMode,
    comment,
    panelErrorClass,
    headingRef,
    onBack,
    onProcess,
    onProcessNext,
    onStartProcessing,
    onReleaseToTeam,
    onRefresh,
}: {
    item: IntegrationResolutionItemView
    positionIndex: number
    positionTotal: number
    responsibilityStatus: ResponsibilityStatus
    formalPending: boolean
    focusMode: boolean
    comment: string
    panelErrorClass: InterfaceErrorClass | null
    headingRef: React.RefObject<HTMLHeadingElement | null>
    onBack: () => void
    onProcess: () => void
    onProcessNext: () => void
    onStartProcessing: (() => void) | undefined
    onReleaseToTeam: () => void
    onRefresh: () => void
}) {
    return (
        <>
            <IntegrationItemProgress
                item={item}
                positionIndex={positionIndex}
                positionTotal={positionTotal}
                responsibilityStatus={responsibilityStatus}
                formalPending={formalPending}
                focusMode={focusMode}
                onBack={onBack}
                onProcess={onProcess}
                onProcessNext={onProcessNext}
                onStartProcessing={onStartProcessing}
            />

            {item.workItem?.allowedActions.includes("RELEASE_TO_TEAM") ? (
                <div className="flex flex-wrap gap-2">
                    <Button
                        type="button"
                        size="sm"
                        variant="secondary"
                        disabled={
                            responsibilityStatus !== "assigned_to_me" ||
                            formalPending ||
                            !comment.trim()
                        }
                        onClick={onReleaseToTeam}
                    >
                        <PauseIcon data-icon="inline-start" aria-hidden />
                        退回团队
                    </Button>
                    <span className="self-center text-xs text-muted-foreground">
                        使用下方处理说明作为退回原因
                    </span>
                </div>
            ) : null}

            <IntegrationItemSummary
                item={item}
                headingRef={headingRef}
                onRefresh={onRefresh}
            />
            <IntegrationEvidencePanel item={item} />
            {panelErrorClass ? (
                <IntegrationErrorResolutionPanel
                    item={item}
                    errorClass={panelErrorClass}
                />
            ) : null}
        </>
    )
}
