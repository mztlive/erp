"use client"

import { useRouter } from "next/navigation"
import type { InterfaceErrorClass } from "@/components/business"
import { cn } from "@/lib/utils"

import { IntegrationQueuePanel } from "../../components/integration-queue-panel"
import type { IntegrationUrlState } from "../../lib/url-state"
import type { IntegrationResolutionItemView } from "../../types"
import type { useIntegrationActions } from "../hooks/use-integration-actions"
import { IntegrationActionZone } from "./integration-action-zone"
import { IntegrationDetailWorkflow } from "./integration-detail-workflow"
import {
    IntegrationEmptyQueue,
    IntegrationEmptySelection,
} from "./integration-empty-states"
import { IntegrationTerminalConfirmation } from "./integration-terminal-confirmation"

export function IntegrationErrorWorkspace({
    items,
    item,
    focusMode,
    urlState,
    positionIndex,
    positionTotal,
    panelErrorClass,
    actions,
    goToItem,
    neighbor,
    replaceUrl,
    onClearFilters,
}: {
    items: IntegrationResolutionItemView[]
    item: IntegrationResolutionItemView | undefined
    focusMode: boolean
    urlState: IntegrationUrlState
    positionIndex: number
    positionTotal: number
    panelErrorClass: InterfaceErrorClass | null
    actions: ReturnType<typeof useIntegrationActions>
    goToItem: (next: IntegrationResolutionItemView | null | undefined) => void
    neighbor: (delta: number) => IntegrationResolutionItemView | null
    replaceUrl: (patch: Record<string, string | null | undefined>) => void
    onClearFilters: () => void
}) {
    const router = useRouter()

    const nextItem = () => {
        const next = neighbor(1)
        if (next) goToItem(next)
    }

    if (items.length === 0) {
        return <IntegrationEmptyQueue onClearFilters={onClearFilters} />
    }

    return (
        <div
            className={cn(
                "grid gap-4",
                focusMode
                    ? "grid-cols-1"
                    : "xl:grid-cols-[minmax(0,38fr)_minmax(0,62fr)]",
            )}
        >
            {!focusMode ? (
                <IntegrationQueuePanel
                    items={items}
                    selectedId={item?.identity.id}
                    onSelect={goToItem}
                />
            ) : null}

            <div
                className={cn(
                    "flex min-w-0 flex-col",
                    focusMode ? "gap-0" : "gap-3",
                )}
            >
                {item ? (
                    <>
                        <IntegrationDetailWorkflow
                            item={item}
                            positionIndex={positionIndex}
                            positionTotal={positionTotal}
                            responsibilityStatus={actions.responsibilityStatus}
                            formalPending={actions.formalPending}
                            focusMode={focusMode}
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
                            responsibilityStatus={actions.responsibilityStatus}
                            comment={actions.comment}
                            onCommentChange={actions.setComment}
                            replacementTaskId={actions.replacementTaskId}
                            onReplacementTaskIdChange={
                                actions.setReplacementTaskId
                            }
                            reconReasonId={actions.reconReasonId}
                            onReconReasonIdChange={actions.setReconReasonId}
                            reasonMismatches={actions.reasonMismatches}
                            onTaskAction={(kind) =>
                                void actions.runTaskAction(kind)
                            }
                            onDirectAction={(kind) =>
                                void actions.handleDirectAction(kind)
                            }
                            onRequestTerminal={actions.setTerminalConfirm}
                            actionZoneRef={actions.actionZoneRef}
                        />

                        {actions.terminalConfirm ? (
                            <IntegrationTerminalConfirmation
                                confirm={actions.terminalConfirm}
                                item={item}
                                pending={actions.formalPending}
                                onConfirmKind={async (kind) => {
                                    if (kind === "CLOSE_DUPLICATE") {
                                        await actions.handleClose(
                                            "CLOSE_DUPLICATE",
                                        )
                                    } else if (kind === "CLOSE_MISROUTED") {
                                        await actions.handleClose(
                                            "CLOSE_MISROUTED",
                                        )
                                    } else if (kind === "RESOLVE") {
                                        await actions.handleResolve()
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
                )}
            </div>
        </div>
    )
}
