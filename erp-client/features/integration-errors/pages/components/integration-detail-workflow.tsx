import * as React from "react"
import type { InterfaceErrorClass } from "@/components/business"
import type { ResponsibilityStatus } from "@/components/business/workflow-actions"
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
    panelErrorClass,
    headingRef,
    onBack,
    onProcess,
    onProcessNext,
    onRefresh,
}: {
    item: IntegrationResolutionItemView
    positionIndex: number
    positionTotal: number
    responsibilityStatus: ResponsibilityStatus
    formalPending: boolean
    focusMode: boolean
    panelErrorClass: InterfaceErrorClass | null
    headingRef: React.RefObject<HTMLHeadingElement | null>
    onBack: () => void
    onProcess: () => void
    onProcessNext: () => void
    onRefresh: () => void
}) {
    return (
        <>
            <IntegrationItemProgress
                id="integration-item-progress"
                item={item}
                positionIndex={positionIndex}
                positionTotal={positionTotal}
                responsibilityStatus={responsibilityStatus}
                formalPending={formalPending}
                focusMode={focusMode}
                onBack={onBack}
                onProcess={onProcess}
                onProcessNext={onProcessNext}
            />

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
