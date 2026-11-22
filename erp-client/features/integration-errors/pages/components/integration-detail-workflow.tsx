import * as React from "react"
import {
    DetailWorkspaceSection,
    type InterfaceErrorClass,
} from "@/components/business"
import type { ResponsibilityStatus } from "@/components/business/workflow-actions"
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
    detailWorkspace,
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
    detailWorkspace?: boolean
    panelErrorClass: InterfaceErrorClass | null
    headingRef: React.RefObject<HTMLHeadingElement | null>
    onBack: () => void
    onProcess: () => void
    onProcessNext: () => void
    onRefresh: () => void
}) {
    const progress = (
        <IntegrationItemProgress
            id="integration-item-progress"
            item={item}
            positionIndex={positionIndex}
            positionTotal={positionTotal}
            responsibilityStatus={responsibilityStatus}
            formalPending={formalPending}
            focusMode={focusMode}
            flat={detailWorkspace}
            onBack={onBack}
            onProcess={onProcess}
            onProcessNext={onProcessNext}
        />
    )
    const summary = (
        <>
            <IntegrationItemSummary
                item={item}
                headingRef={headingRef}
                onRefresh={onRefresh}
                flat={detailWorkspace}
            />
            {panelErrorClass && !item.failureSource ? (
                <IntegrationErrorResolutionPanel
                    item={item}
                    errorClass={panelErrorClass}
                    flat={detailWorkspace}
                />
            ) : null}
        </>
    )
    return detailWorkspace ? (
        <>
            {progress}
            <DetailWorkspaceSection>{summary}</DetailWorkspaceSection>
        </>
    ) : (
        <>
            {progress}
            {summary}
        </>
    )
}
