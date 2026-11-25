"use client"

import { useRouter } from "@/lib/navigation"
import {
    DetailWorkspaceSection,
    type InterfaceErrorClass,
} from "@/components/business"
import { cn } from "@/lib/utils"
import { useWorkspaceTaskPane } from "@/components/business/workspace-task-pane"

import { IntegrationQueuePanel } from "../../components/integration-queue-panel"
import { IntegrationEvidencePanel } from "../../components/integration-evidence-panel"
import type { IntegrationUrlState } from "../../lib/url-state"
import type { IntegrationResolutionItemView } from "../../types"
import type { useIntegrationActions } from "../hooks/use-integration-actions"
import { IntegrationActionZone } from "./integration-action-zone"
import { IntegrationDetailWorkflow } from "./integration-detail-workflow"
import {
    IntegrationEmptyQueue,
    IntegrationEmptyScope,
    IntegrationEmptySelection,
} from "./integration-empty-states"
import { IntegrationTerminalConfirmation } from "./integration-terminal-confirmation"

export function IntegrationErrorWorkspace({
    items,
    item,
    focusMode,
    detailWorkspace,
    showEvidenceSection = false,
    urlState,
    currentQueueHref,
    positionIndex,
    positionTotal,
    panelErrorClass,
    actions,
    goToItem,
    neighbor,
    replaceUrl,
    onClearFilters,
    emptyReason,
}: {
    items: IntegrationResolutionItemView[]
    item: IntegrationResolutionItemView | undefined
    focusMode: boolean
    detailWorkspace?: boolean
    showEvidenceSection?: boolean
    urlState: IntegrationUrlState
    currentQueueHref?: string
    positionIndex: number
    positionTotal: number
    panelErrorClass: InterfaceErrorClass | null
    actions: ReturnType<typeof useIntegrationActions>
    goToItem: (next: IntegrationResolutionItemView | null | undefined) => void
    neighbor: (delta: number) => IntegrationResolutionItemView | null
    replaceUrl: (patch: Record<string, string | null | undefined>) => void
    onClearFilters: () => void
    emptyReason?: "no_scope" | null
}) {
    const router = useRouter()
    const embedded = useWorkspaceTaskPane()

    const nextItem = () => {
        const next = neighbor(1)
        if (next) goToItem(next)
    }

    const showNoScope = !focusMode && emptyReason === "no_scope"
    if (showNoScope || items.length === 0) {
        const empty = showNoScope ? (
            <IntegrationEmptyScope />
        ) : (
            <IntegrationEmptyQueue onClearFilters={onClearFilters} />
        )
        return detailWorkspace ? (
            <DetailWorkspaceSection>{empty}</DetailWorkspaceSection>
        ) : (
            empty
        )
    }

    const content = item ? (
        <>
            <IntegrationDetailWorkflow
                item={item}
                positionIndex={positionIndex}
                positionTotal={positionTotal}
                responsibilityStatus={actions.responsibilityStatus}
                formalPending={actions.formalPending}
                focusMode={focusMode}
                detailWorkspace={detailWorkspace}
                panelErrorClass={panelErrorClass}
                headingRef={actions.headingRef}
                onBack={() => {
                    if (focusMode) {
                        router.push(
                            `/governance/integration-errors?view=${urlState.view}&queueContextId=${encodeURIComponent(urlState.queueContextId)}`,
                        )
                    } else {
                        replaceUrl({
                            taskId: null,
                            differenceId: null,
                        })
                    }
                }}
                onProcess={() => {
                    actions.focusFirstAction()
                }}
                onProcessNext={nextItem}
                onRefresh={actions.refresh}
            />

            <IntegrationActionZone
                item={item}
                can={actions.can}
                formalPending={actions.formalPending}
                resolvePending={actions.resolvePending}
                pendingTaskAction={actions.pendingTaskAction}
                pendingDirectAction={actions.pendingDirectAction}
                responsibilityStatus={actions.responsibilityStatus}
                comment={actions.comment}
                onCommentChange={actions.setComment}
                replacementTaskId={actions.replacementTaskId}
                onReplacementTaskIdChange={actions.setReplacementTaskId}
                reconReasonId={actions.reconReasonId}
                onReconReasonIdChange={actions.setReconReasonId}
                reasonMismatches={actions.reasonMismatches}
                onTaskAction={(kind) => void actions.runTaskAction(kind)}
                onDirectAction={(kind) => void actions.handleDirectAction(kind)}
                onRequestTerminal={actions.setTerminalConfirm}
                onResolve={() => void actions.handleResolve()}
                actionZoneRef={actions.actionZoneRef}
                detailWorkspace={detailWorkspace}
            />
            {!detailWorkspace ? (
                <IntegrationEvidencePanel item={item} />
            ) : showEvidenceSection ? (
                <DetailWorkspaceSection width="form">
                    <IntegrationEvidencePanel item={item} flat />
                </DetailWorkspaceSection>
            ) : null}

            {actions.terminalConfirm ? (
                <IntegrationTerminalConfirmation
                    confirm={actions.terminalConfirm}
                    item={item}
                    pending={actions.formalPending}
                    onConfirmKind={async (kind) => {
                        if (kind === "CLOSE_DUPLICATE") {
                            await actions.handleClose("CLOSE_DUPLICATE")
                        } else if (kind === "CLOSE_MISROUTED") {
                            await actions.handleClose("CLOSE_MISROUTED")
                        } else {
                            await actions.handleDirectTerminal(kind)
                        }
                    }}
                    onClose={() => actions.setTerminalConfirm(null)}
                />
            ) : null}
        </>
    ) : (
        <IntegrationEmptySelection />
    )

    if (detailWorkspace) return content

    return (
        <div
            className={cn(
                "grid gap-4",
                focusMode
                    ? "grid-cols-1"
                    : "@[1120px]/workspace:grid-cols-[minmax(0,38fr)_minmax(0,62fr)]",
            )}
        >
            {!focusMode ? (
                <IntegrationQueuePanel
                    items={items}
                    selectedId={item?.identity.id}
                    returnTo={currentQueueHref}
                    onSelect={goToItem}
                />
            ) : null}

            <div
                className={cn(
                    "flex min-w-0 flex-col gap-6 p-5 sm:p-6",
                    !embedded &&
                        "rounded-lg border border-border/60 bg-card shadow-xs",
                )}
            >
                {content}
            </div>
        </div>
    )
}
