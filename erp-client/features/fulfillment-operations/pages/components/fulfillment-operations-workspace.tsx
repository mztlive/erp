"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { cn } from "@/lib/utils"
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

type FulfillmentOperationsWorkspaceProps = {
    controller: FulfillmentOperationsController
    headerDescription: string
    operationTypes?: FulfillmentOperationType[]
    roleLabel: string
    embedded?: boolean
    singleOperation?: boolean
    onBack: () => void
    onOpenAcceptance?: () => void
}

/** 履约队列、处理表单、结果与确认框；独立页和销售单详情共用。 */
export function FulfillmentOperationsWorkspace({
    controller,
    headerDescription,
    operationTypes,
    roleLabel,
    embedded = false,
    singleOperation = false,
    onBack,
    onOpenAcceptance,
}: FulfillmentOperationsWorkspaceProps) {
    const status = responsibilityStatus(
        controller.operation,
        controller.canExecute,
    )

    return (
        <div
            className={cn(
                "flex min-w-0 flex-col",
                embedded || singleOperation ? "gap-0" : "gap-4",
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
                        resolvePending={controller.formalPending}
                        onResolveUnknown={() =>
                            void controller.handleResolveUnknown()
                        }
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
                    />
                </div>
            ) : null}

            {controller.actionError ? (
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
                            ? "min-h-[28rem] min-w-0"
                            : "grid min-h-[28rem] min-w-0 gap-4 xl:grid-cols-[minmax(15rem,0.9fr)_minmax(0,2.1fr)]"
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
                        validationIssues={controller.validationIssues}
                        saveMessage={controller.saveMessage}
                        canExecute={controller.canExecute}
                        canPost={controller.canPost}
                        formalPending={controller.formalPending}
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
                id="fulfillment-operations-workspace-confirm"
                open={controller.confirmOpen}
                onOpenChange={controller.setConfirmOpen}
                title={
                    controller.operation
                        ? OPERATION_CONFIRM_TITLE[
                              controller.operation.operationType
                          ]
                        : "确认？"
                }
                description={
                    controller.draft
                        ? confirmDescription(controller.draft)
                        : "确认后不能改。"
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
                    await controller.handlePost()
                }}
            />
        </div>
    )
}
