"use client"

import * as React from "react"
import { FormalActionConfirmDialog } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { cn } from "@/lib/utils"
import { getErrorMessage } from "@/lib/api/errors"
import { isWorkspaceFrame } from "@/lib/workspace-tabs"
import { FulfillmentQueueList } from "@/features/fulfillment-operations/components/queue/fulfillment-queue-list"
import { confirmDescription } from "@/features/fulfillment-operations/lib/validation"
import {
    OPERATION_ACTION_LABEL,
    OPERATION_CONFIRM_TITLE,
    OPERATION_DONE_LABEL,
    type FulfillmentOperationType,
} from "@/features/fulfillment-operations/types"
import type { FulfillmentOperationsController } from "@/features/fulfillment-operations/pages/hooks/use-fulfillment-operations-controller"
import {
    readOnlyNote,
    responsibilityStatus,
    responsibilityStatusLabel,
} from "@/features/fulfillment-operations/pages/lib/presentation"
import { FulfillmentPageStates } from "./fulfillment-page-states"
import { FulfillmentResultPanel } from "./fulfillment-result-panel"
import { FulfillmentWorkSurface } from "./fulfillment-work-surface"
import { serviceCompletionNoteIssue } from "./fulfillment-task-fields"
import type { FulfillmentTaskContext } from "./fulfillment-task-overview"

type FulfillmentOperationsWorkspaceProps = {
    controller: FulfillmentOperationsController
    headerDescription: string
    operationTypes?: FulfillmentOperationType[]
    roleLabel: string
    embedded?: boolean
    singleOperation?: boolean
    taskContext?: FulfillmentTaskContext
    onBack: () => void
    onOpenAcceptance?: () => void
}

function isEditableTarget(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false
    if (target.isContentEditable) return true
    const tag = target.tagName
    return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT"
}

/** 履约队列、处理表单、结果与确认框；独立页和销售单详情共用。 */
export function FulfillmentOperationsWorkspace({
    controller,
    headerDescription,
    operationTypes,
    roleLabel,
    embedded = false,
    singleOperation = false,
    taskContext,
    onBack,
    onOpenAcceptance,
}: FulfillmentOperationsWorkspaceProps) {
    const status = responsibilityStatus(
        controller.operation,
        controller.canExecute,
    )

    const completionNoteIssue = taskContext
        ? serviceCompletionNoteIssue(controller.draft)
        : undefined
    const canPost = controller.canPost && !completionNoteIssue
    const validationIssues = completionNoteIssue
        ? [...controller.validationIssues, completionNoteIssue]
        : controller.validationIssues
    const showWorkSurface =
        !controller.completed &&
        Boolean(controller.operation && controller.draft)
    const showRecoveryInFooter = Boolean(
        taskContext &&
        showWorkSurface &&
        controller.lastResult?.status === "unknown",
    )

    React.useEffect(() => {
        const handleKeyDown = (event: KeyboardEvent) => {
            if (event.defaultPrevented || event.isComposing) return
            if (
                isWorkspaceFrame() &&
                window.frameElement?.getAttribute("data-workspace-active") !==
                    "true"
            ) {
                return
            }
            if (
                document.querySelector(
                    '[role="dialog"]:not([data-closed]):not([data-slot="toast"]):not([data-business-component="task-action-sheet"]), [role="alertdialog"]:not([data-closed])',
                )
            ) {
                return
            }
            const modifier = event.ctrlKey || event.metaKey
            if (
                modifier &&
                !event.altKey &&
                event.key.toLowerCase() === "s" &&
                controller.canExecute &&
                controller.supportsSave &&
                controller.dirty &&
                !controller.formalPending &&
                !controller.savePending
            ) {
                event.preventDefault()
                void controller.handleSave()
                return
            }
            if (
                modifier &&
                !event.altKey &&
                event.key === "Enter" &&
                controller.canExecute &&
                canPost &&
                !controller.formalPending &&
                !controller.savePending &&
                !controller.confirmOpen
            ) {
                event.preventDefault()
                void controller.handleSubmit()
                return
            }
            if (
                !taskContext &&
                !modifier &&
                !event.altKey &&
                event.key === "?" &&
                !isEditableTarget(event.target)
            ) {
                event.preventDefault()
                controller.setShortcutsOpen((value) => !value)
                return
            }
            if (
                !singleOperation &&
                !modifier &&
                !event.altKey &&
                !event.shiftKey &&
                !isEditableTarget(event.target)
            ) {
                const key = event.key.toLowerCase()
                if (key === "j") {
                    event.preventDefault()
                    controller.handleNavigate(1)
                } else if (key === "k") {
                    event.preventDefault()
                    controller.handleNavigate(-1)
                }
            }
        }
        window.addEventListener("keydown", handleKeyDown)
        return () => window.removeEventListener("keydown", handleKeyDown)
    }, [canPost, controller, singleOperation, taskContext])

    async function resolveUnknown() {
        try {
            await controller.handleResolveUnknown()
        } catch (error) {
            controller.setActionError(
                getErrorMessage(error, "本次核对失败，请稍后重试"),
            )
        }
    }

    return (
        <div
            className={cn(
                "flex min-w-0 flex-col",
                taskContext
                    ? "gap-3 bg-card"
                    : embedded || singleOperation
                      ? "gap-0"
                      : "gap-4",
                singleOperation && "min-h-0 flex-1",
            )}
        >
            {controller.lastResult ? (
                <div
                    ref={controller.resultRef}
                    tabIndex={-1}
                    className="outline-none"
                >
                    <FulfillmentResultPanel
                        lastResult={controller.lastResult}
                        currentUrl={controller.currentUrl}
                        resolvePending={controller.resolvePending}
                        onResolveUnknown={() => void resolveUnknown()}
                        onRetryUnknown={() =>
                            void controller.handleRetryUnknown()
                        }
                        retryPending={controller.retryPending}
                        onNext={() => {
                            const completedOperationId =
                                controller.lastResult?.outcome?.operationId
                            const next = controller.operations.find(
                                (operation) =>
                                    operation.operationId !==
                                    completedOperationId,
                            )?.operationId
                            if (next) controller.goToOperation(next)
                        }}
                        onContinueWarehouseShip={controller.goToWarehouseShip}
                        onOpenAcceptance={onOpenAcceptance}
                        showNext={!singleOperation}
                        showUnknownActions={!showRecoveryInFooter}
                    />
                </div>
            ) : null}

            {controller.actionError && (!taskContext || !showWorkSurface) ? (
                <Alert variant="destructive">
                    <AlertTitle>没有生效</AlertTitle>
                    <AlertDescription>
                        {controller.actionError}
                    </AlertDescription>
                </Alert>
            ) : null}

            {controller.completed ||
            !controller.operation ||
            !controller.draft ? (
                <FulfillmentPageStates
                    status="empty"
                    headerDescription={headerDescription}
                    completed={controller.completed}
                    operationTypes={operationTypes}
                    emptyReason={controller.emptyReason}
                    roleLabel={roleLabel}
                    visibleTypes={controller.visibleTypes}
                    filterSummary={controller.context?.filterSummary}
                    onClearAllFilters={controller.clearAllFilters}
                    onRetry={() => void controller.queueQuery.refetch()}
                    embedded={embedded}
                />
            ) : (
                <div
                    className={
                        singleOperation
                            ? "flex min-h-0 min-w-0 flex-1 flex-col"
                            : "grid min-h-[28rem] min-w-0 gap-4 @[1120px]/workspace:grid-cols-[minmax(15rem,0.9fr)_minmax(0,2.1fr)]"
                    }
                >
                    {!singleOperation ? (
                        <FulfillmentQueueList
                            operations={controller.operations}
                            currentIndex={controller.currentIndex}
                            position={
                                controller.context?.position ??
                                controller.currentIndex + 1
                            }
                            total={
                                controller.context?.total ??
                                controller.operations.length
                            }
                            page={controller.context?.page ?? 1}
                            totalPages={controller.context?.totalPages ?? 1}
                            onSelect={(operationId) => {
                                if (
                                    controller.dirty &&
                                    operationId !==
                                        controller.operation?.operationId
                                ) {
                                    controller.setActionError(
                                        "有未保存修改，请先保存或放弃后再切换",
                                    )
                                    return
                                }
                                controller.goToOperation(operationId)
                            }}
                            onPageChange={controller.setPage}
                        />
                    ) : null}

                    <FulfillmentWorkSurface
                        operation={controller.operation}
                        draft={controller.draft}
                        taskContext={taskContext}
                        actionError={controller.actionError}
                        validationIssues={validationIssues}
                        recovery={
                            showRecoveryInFooter
                                ? {
                                      resolving: controller.resolvePending,
                                      retrying: controller.retryPending,
                                      onResolve: () => void resolveUnknown(),
                                      onRetry: controller.canExecute
                                          ? () =>
                                                void controller.handleRetryUnknown()
                                          : undefined,
                                  }
                                : undefined
                        }
                        saveMessage={controller.saveMessage}
                        canExecute={controller.canExecute}
                        canPost={canPost}
                        formalPending={controller.formalPending}
                        savePending={controller.savePending}
                        supportsSave={controller.supportsSave}
                        dirty={controller.dirty}
                        autoNext={controller.autoNext}
                        readOnlyNote={
                            controller.executeBlockedReason ??
                            readOnlyNote(controller.operation)
                        }
                        responsibilityStatus={status}
                        responsibilityStatusLabel={responsibilityStatusLabel(
                            controller.operation,
                            controller.canExecute,
                        )}
                        currentUrl={controller.currentUrl}
                        snapshotUpdatedAt={
                            controller.context?.snapshotUpdatedAt ?? ""
                        }
                        position={
                            controller.context?.position ??
                            controller.currentIndex + 1
                        }
                        total={
                            controller.context?.total ??
                            controller.operations.length
                        }
                        shortcutsOpen={controller.shortcutsOpen}
                        headingRef={controller.headingRef}
                        resultUnknown={
                            controller.lastResult?.status === "unknown"
                        }
                        singleOperation={singleOperation}
                        showBack={!embedded && !singleOperation}
                        showSalesOrderLinks={!embedded && !singleOperation}
                        onDraftChange={controller.updateDraft}
                        onSkip={controller.handleSkip}
                        onDiscard={controller.handleDiscard}
                        onSave={() => void controller.handleSave()}
                        onConfirm={() => void controller.handleSubmit()}
                        onBack={onBack}
                        onToggleShortcuts={() =>
                            controller.setShortcutsOpen((value) => !value)
                        }
                    />
                </div>
            )}

            <FormalActionConfirmDialog
                actionVariant="default"
                id="fulfillment-operations-workspace-confirm"
                open={controller.confirmOpen}
                onOpenChange={controller.setConfirmOpen}
                title={
                    controller.operation
                        ? OPERATION_CONFIRM_TITLE[
                              controller.operation.operationType
                          ]
                        : "确认办理操作"
                }
                description={
                    controller.draft
                        ? confirmDescription(controller.draft)
                        : "提交后不可直接撤回，请先核对本次履约信息。"
                }
                actionLabel={
                    controller.operation
                        ? OPERATION_ACTION_LABEL[
                              controller.operation.operationType
                          ]
                        : "确认"
                }
                confirmLabel={
                    controller.operation
                        ? OPERATION_ACTION_LABEL[
                              controller.operation.operationType
                          ]
                        : "确认"
                }
                fromStatus={{ label: "待确认", tone: "warning" }}
                toStatus={{
                    label: controller.operation
                        ? OPERATION_DONE_LABEL[
                              controller.operation.operationType
                          ]
                        : "已完成",
                    tone: "success",
                }}
                pending={controller.formalPending}
                onConfirm={async () => {
                    if (!completionNoteIssue) await controller.handlePost()
                }}
            />
        </div>
    )
}
