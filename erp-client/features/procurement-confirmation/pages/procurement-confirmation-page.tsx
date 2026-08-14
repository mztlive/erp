"use client"

import { TriangleAlertIcon } from "lucide-react"

import {
    DataFreshness,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { formatDateTime } from "@/lib/datetime"
import { freshnessText } from "@/lib/ui-text"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { ContractPaperDialog } from "@/features/contracts/contract-paper-dialog"
import { ProcurementSalesDocument } from "@/features/procurement-confirmation/components/procurement-sales-document"
import { ProcurementPlanConfirmationDialog } from "@/features/procurement-confirmation/components/procurement-plan-confirmation-dialog"
import { LegacyProcurementPlanEditor } from "@/features/procurement-confirmation/components/legacy-procurement-plan-editor"
import { ProcurementConfirmationSidebar } from "@/features/procurement-confirmation/components/procurement-confirmation-sidebar"
import { ProcurementQueueControls } from "@/features/procurement-confirmation/components/procurement-queue-controls"
import { ProcurementOutcomeFeedback } from "@/features/procurement-confirmation/components/procurement-result"
import { w05Href } from "@/features/procurement-confirmation/lib/urls"
import { ContractPreviewDialog } from "./components/contract-preview-dialog"
import {
    ProcurementEmptyStates,
    ProcurementPageError,
    ProcurementPagePending,
} from "./components/procurement-page-states"
import { RejectConfirmationDialog } from "./components/reject-confirmation-dialog"
import { useProcurementConfirmationController } from "./hooks/use-procurement-confirmation-controller"

export function ProcurementConfirmationPage() {
    const controller = useProcurementConfirmationController()
    const { url, drafts, actions } = controller
    const task = controller.task
    const context = controller.context

    if (controller.queueQuery.isPending) {
        return <ProcurementPagePending />
    }

    if (controller.queueQuery.isError) {
        return (
            <ProcurementPageError
                error={controller.queueQuery.error}
                onRetry={() => void controller.queueQuery.refetch()}
            />
        )
    }

    return (
        <PageScaffold>
            <PageHeader
                title="采购二次确认"
                breadcrumbs={[
                    {
                        id: "procurement",
                        label: "采购与履约",
                        href: "/procurement/confirm",
                    },
                    { id: "confirm", label: "二次确认", current: true },
                ]}
                metadata={
                    <div className="flex flex-wrap items-center gap-3">
                        <DataFreshness
                            updatedAt={
                                context?.queueContextUpdatedAt
                                    ? formatDateTime(
                                          context.queueContextUpdatedAt,
                                          "default",
                                      )
                                    : "刚刚"
                            }
                            dateTime={context?.queueContextUpdatedAt}
                            state="fresh"
                            label={freshnessText.queueUpdatedAt}
                        />
                        <span
                            className="text-xs text-muted-foreground"
                            aria-live="polite"
                        >
                            {context?.filterSummary ?? "仅我的"} · 待处理{" "}
                            {context?.total ?? 0}
                        </span>
                    </div>
                }
            />

            <ProcurementQueueControls
                scope={url.scope}
                due={url.due}
                orderNoInputRef={url.orderNoInputRef}
                orderNoDraft={url.orderNoDraft}
                onOrderNoDraftChange={url.setOrderNoDraft}
                onCommitOrderNo={url.commitOrderNo}
                hasActiveFilter={url.hasActiveFilter}
                onClearFilters={url.clearFilters}
                autoNext={url.autoNext}
                onToggleAutoNext={url.toggleAutoNext}
                onScopeChange={url.handleScopeChange}
                onDueChange={url.handleDueChange}
            />

            <ProcurementOutcomeFeedback
                finishedResult={controller.finishedResult}
                lastResult={controller.lastResult}
                fallbackSalesOrderId={task?.salesSubmission.salesOrderId}
                context={context}
                submissionNo={task?.salesSubmission.submissionNo}
                returnTo={url.returnTo}
                resultRef={controller.resultRef}
                onDismissFinished={() => controller.setFinishedResult(null)}
                onNext={() => {
                    const next =
                        context?.nextWorkItemId ??
                        controller.neighborId(1) ??
                        controller.tasks[0]?.workItemId
                    controller.goToWorkItem(next)
                }}
            />

            {controller.actionError ? (
                <Alert variant="destructive" role="alert">
                    <TriangleAlertIcon aria-hidden="true" />
                    <AlertTitle>操作未生效</AlertTitle>
                    <AlertDescription>{controller.actionError}</AlertDescription>
                </Alert>
            ) : null}

            {controller.completed ? (
                <ProcurementEmptyStates
                    emptyReason={controller.view?.emptyReason}
                    onClearFilters={url.clearFilters}
                />
            ) : task ? (
                <>
                    {/* 1440 双栏：左销售提交+分行，右决策摘要 sticky */}
                    <div className="grid min-w-0 gap-3 md:gap-4 xl:grid-cols-[minmax(0,66fr)_minmax(17rem,34fr)]">
                        <div
                            className={`min-w-0 space-y-4 p-3 md:p-4 ${surfacePanelClassName}`}
                        >
                            {task.salesSubmission.resubmissionContext ? (
                                <Alert variant="info">
                                    <TriangleAlertIcon aria-hidden="true" />
                                    <AlertTitle>
                                        改品/改价后新提交 · 须重新确认
                                    </AlertTitle>
                                    <AlertDescription>
                                        第 {task.salesSubmission.submissionNo}{" "}
                                        次提交 ·{" "}
                                        {task.salesSubmission.submittedAt} ·{" "}
                                        {task.salesSubmission.submittedByLabel}
                                        ；上一驳回提交已作废。不得复用旧确认分行。
                                    </AlertDescription>
                                </Alert>
                            ) : null}

                            <ProcurementSalesDocument
                                task={task}
                                onOpenContract={
                                    task.salesSubmission.contractId
                                        ? () =>
                                              controller.setContractOpen(true)
                                        : undefined
                                }
                            />

                            <LegacyProcurementPlanEditor
                                task={task}
                                recommendation={controller.recommendation}
                                recommendationPending={
                                    controller.recommendationQuery.isPending
                                }
                                recommendationFailed={
                                    controller.recommendationQuery.isError
                                }
                                lineDrafts={drafts.lineDrafts}
                                coverage={drafts.coverage}
                                supplyOptions={controller.supplyOptions}
                                supplierOptions={controller.supplierOptions}
                                offeringOptionsForSku={
                                    drafts.offeringOptionsForSku
                                }
                                capabilityOptionsForOffering={
                                    drafts.capabilityOptionsForOffering
                                }
                                fulfillmentOptionsForOffering={
                                    drafts.fulfillmentOptionsForOffering
                                }
                                formalPending={
                                    controller.formalPending ||
                                    !task.allowedActions.includes("SAVE")
                                }
                                onApplyRecommendation={
                                    drafts.applyRecommendation
                                }
                                onUpdateLine={drafts.updateLine}
                                onAddSplitLine={drafts.addSplitLine}
                                onRemoveLine={drafts.removeLine}
                                saveMessage={controller.saveMessage}
                                dirty={drafts.dirty}
                            />
                        </div>

                        {/* 决策摘要：桌面 sticky；top 避开 sticky 处理条 */}
                        <ProcurementConfirmationSidebar
                            task={task}
                            headingRef={controller.headingRef}
                            formalPending={controller.formalPending}
                            onReject={actions.handleOpenReject}
                            onConfirm={actions.handleOpenConfirm}
                            onStartProcessing={actions.handleStartProcessing}
                            onReleaseToTeam={actions.handleReleaseToTeam}
                            coverage={drafts.coverage}
                            estimatedPurchase={controller.estimatedPurchase}
                            lineDrafts={drafts.lineDrafts}
                            recommendation={controller.recommendation}
                            clientBlocking={drafts.clientBlocking}
                            salesOrderHref={w05Href(
                                task.salesSubmission.salesOrderId,
                                url.returnTo,
                                task.workItemId,
                            )}
                        />
                    </div>

                    {controller.contractQuery.data ? (
                        <ContractPaperDialog
                            contract={controller.contractQuery.data}
                            open={controller.contractOpen}
                            onOpenChange={controller.setContractOpen}
                        />
                    ) : (
                        <ContractPreviewDialog
                            open={controller.contractOpen}
                            onOpenChange={controller.setContractOpen}
                            pending={controller.contractQuery.isPending}
                            contractSnapshot={
                                task.salesSubmission.contractSnapshot
                            }
                            customerSnapshot={
                                task.salesSubmission.customerSnapshot
                            }
                            paymentTermLabel={
                                task.salesSubmission.paymentTermLabel
                            }
                        />
                    )}

                    <ProcurementPlanConfirmationDialog
                        open={controller.confirmOpen}
                        onOpenChange={controller.setConfirmOpen}
                        task={task}
                        recommendation={controller.recommendation}
                        recommendationPending={
                            controller.recommendationQuery.isPending
                        }
                        recommendationFailed={
                            controller.recommendationQuery.isError
                        }
                        recommendationError={
                            controller.recommendationQuery.error
                        }
                        onRetryRecommendation={() => {
                            void controller.recommendationQuery.refetch()
                        }}
                        currentPlanSummary={drafts.currentPlanSummary}
                        dirty={drafts.dirty}
                        lineDrafts={drafts.lineDrafts}
                        coverage={drafts.coverage}
                        supplyOptions={controller.supplyOptions}
                        supplierOptions={controller.supplierOptions}
                        offeringOptionsForSku={drafts.offeringOptionsForSku}
                        capabilityOptionsForOffering={
                            drafts.capabilityOptionsForOffering
                        }
                        fulfillmentOptionsForOffering={
                            drafts.fulfillmentOptionsForOffering
                        }
                        updatePlanLine={drafts.updatePlanLine}
                        addSplitLine={drafts.addSplitLine}
                        removeLine={drafts.removeLine}
                        clientBlocking={drafts.clientBlocking}
                        allCovered={drafts.allCovered}
                        formalPending={controller.formalPending}
                        isSubmitting={
                            controller.saveMutation.isPending ||
                            controller.completeMutation.isPending
                        }
                        advanceAfterConfirm={controller.advanceAfterConfirm}
                        onApprove={actions.handleApprove}
                    />

                    <RejectConfirmationDialog
                        open={controller.rejectOpen}
                        onOpenChange={controller.setRejectOpen}
                        onSubmit={actions.handleRejectSubmit}
                    />
                </>
            ) : null}
        </PageScaffold>
    )
}
