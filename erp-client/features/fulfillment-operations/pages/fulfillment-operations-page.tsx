"use client"

import * as React from "react"
import { useRouter, useSearchParams } from "next/navigation"

import {
    DataFreshness,
    FormalActionConfirmDialog,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import type { FulfillmentQueueFilters } from "@/features/fulfillment-operations/api"
import { FulfillmentQueueList } from "@/features/fulfillment-operations/components/queue/fulfillment-queue-list"
import {
    laneHeader,
    resolveLane,
} from "@/features/fulfillment-operations/lib/lanes"
import {
    parseDueParam,
    parseGateParam,
    parseTypeParam,
    typeParamValue,
} from "@/features/fulfillment-operations/lib/filters"
import {
    DEFAULT_FULFILLMENT_ROLE,
    FULFILLMENT_ROLES,
} from "@/features/fulfillment-operations/lib/fulfillment-roles"
import { impactPreview } from "@/features/fulfillment-operations/lib/validation"
import {
    CORRECTION_NOTICE,
    OPERATION_ACTION_LABEL,
    OPERATION_CONFIRM_TITLE,
    OPERATION_DONE_LABEL,
} from "@/features/fulfillment-operations/types"
import { freshnessText } from "@/lib/ui-text"
import { FulfillmentFilterBar } from "./components/fulfillment-filter-bar"
import { FulfillmentPageStates } from "./components/fulfillment-page-states"
import { FulfillmentResultPanel } from "./components/fulfillment-result-panel"
import { FulfillmentWorkSurface } from "./components/fulfillment-work-surface"
import { SourceReturnBanner } from "./components/source-return-banner"
import { useFulfillmentOperationsController } from "./hooks/use-fulfillment-operations-controller"
import { useFulfillmentKeyboardShortcuts } from "./hooks/use-fulfillment-keyboard-shortcuts"
import {
    readOnlyNote,
    responsibilityStatus,
    responsibilityStatusLabel,
    sourceReturnHref,
} from "./lib/presentation"

/**
 * 履约工作面。
 * PurchaseReceipt 为 NO_APPROVAL，入库创建结果、详情、提交确认
 * 不展示绑定卡、决定、撤回或审批历史。
 * Delivery 为 NO_APPROVAL，仓发与直发创建结果、详情、提交确认
 * 不展示绑定卡、待办或审批入口。
 * ElectronicDelivery 为 NO_APPROVAL，电子交付创建结果、详情、提交确认
 * 不展示绑定卡、决定、撤回或审批历史。
 * ServiceFulfillment 为 NO_APPROVAL，服务履约创建结果、详情、提交确认
 * 不展示绑定卡、决定、撤回或审批历史。
 * CustomerAcceptance 为 NO_APPROVAL，履约结果交接客户验收时
 * 创建结果、详情、提交确认不展示绑定卡、决定、撤回或审批历史。
 */
export function FulfillmentOperationsPage() {
    const router = useRouter()
    const searchParams = useSearchParams()

    const lane = resolveLane(searchParams.get("lane"))
    const header = laneHeader(lane)
    // 队列按岗位通道取角色（仓储/采购经办）；无岗位深链回落默认角色
    const roleValue = lane ?? DEFAULT_FULFILLMENT_ROLE
    const operationTypes = parseTypeParam(searchParams.get("type"))
    const warehouseId = searchParams.get("warehouseId") ?? undefined
    const q = searchParams.get("q") ?? undefined
    const due = parseDueParam(searchParams.get("due"))
    const gate = parseGateParam(searchParams.get("gate"))
    const salesOrderId = searchParams.get("salesOrderId") ?? undefined
    const purchaseOrderId = searchParams.get("purchaseOrderId") ?? undefined
    const returnTo = searchParams.get("returnTo") ?? undefined
    const fromWorkspace = searchParams.get("from") ?? undefined

    const filters = React.useMemo(
        (): FulfillmentQueueFilters => ({
            role: roleValue,
            operationTypes,
            warehouseId,
            q,
            due,
            gate,
            salesOrderId,
            purchaseOrderId,
            currentOperationId:
                searchParams.get("currentOperationId") ?? undefined,
        }),
        [
            roleValue,
            operationTypes,
            warehouseId,
            q,
            due,
            gate,
            salesOrderId,
            purchaseOrderId,
            searchParams,
        ],
    )

    const controller = useFulfillmentOperationsController({
        roleValue,
        filters,
        lane,
        autoNextExplicit: searchParams.get("autoNext"),
    })

    useFulfillmentKeyboardShortcuts({
        dirty: controller.dirty,
        canPost: controller.canPost,
        formalPending: controller.formalPending,
        canExecute: controller.canExecute,
        supportsSave: controller.supportsSave,
        onSave: () => void controller.handleSave(),
        onConfirm: () => controller.setConfirmOpen(true),
        onNavigate: controller.handleNavigate,
        onToggleShortcuts: () => controller.setShortcutsOpen((v) => !v),
    })

    const context = controller.context
    const activeTypeSlug = typeParamValue(operationTypes)

    // 加载 / 失败时筛选区仍然常驻：错误态用 BusinessFailureState + refetch，
    // 不卸载筛选工具栏（ui-filter-design §11）。
    if (controller.queueQuery.isPending || controller.queueQuery.isError) {
        const queryPending = controller.queueQuery.isPending
        return (
            <PageScaffold>
                <PageHeader
                    title={header.label}
                    description={header.description}
                    metadata={
                        <div className="flex flex-wrap items-center gap-3">
                            <DataFreshness
                                updatedAt="刚刚"
                                dateTime={context?.snapshotUpdatedAt}
                                state="fresh"
                                label={freshnessText.dataUpdatedAt}
                            />
                            <span
                                className="text-xs text-muted-foreground"
                                aria-live="polite"
                            >
                                {context?.filterSummary ?? "全部类型"} · 待处理{" "}
                                {context?.total ?? 0}
                            </span>
                        </div>
                    }
                />

                <SourceReturnBanner
                    fromWorkspace={fromWorkspace}
                    sourceReturnHref={sourceReturnHref(
                        returnTo,
                        fromWorkspace,
                        controller.operation,
                    )}
                    operation={controller.operation}
                />

                <FulfillmentFilterBar
                    activeTypeSlug={activeTypeSlug}
                    visibleTypes={controller.visibleTypes}
                    onTypeChange={controller.setTypeFilter}
                    q={q}
                    warehouseId={warehouseId}
                    due={due}
                    gate={gate}
                    salesOrderId={salesOrderId}
                    purchaseOrderId={purchaseOrderId}
                    operations={controller.operations}
                    autoNext={controller.autoNext}
                    showAutoNext={controller.canExecute}
                    onPatch={controller.handlePatch}
                    onClearAllFilters={controller.clearAllFilters}
                    onAutoNextChange={controller.setAutoNext}
                />

                <FulfillmentPageStates
                    status={queryPending ? "pending" : "error"}
                    standalone
                    headerDescription={header.label}
                    error={controller.queueQuery.error}
                    onRetry={() => void controller.queueQuery.refetch()}
                />
            </PageScaffold>
        )
    }

    const status = responsibilityStatus(
        controller.operation,
        controller.canExecute,
    )

    return (
        <PageScaffold>
            <PageHeader
                title={header.label}
                description={header.description}
                metadata={
                    <div className="flex flex-wrap items-center gap-3">
                        <DataFreshness
                            updatedAt="刚刚"
                            dateTime={context?.snapshotUpdatedAt}
                            state="fresh"
                            label={freshnessText.dataUpdatedAt}
                        />
                        <span
                            className="text-xs text-muted-foreground"
                            aria-live="polite"
                        >
                            {context?.filterSummary ?? "全部类型"} · 待处理{" "}
                            {context?.total ?? 0}
                        </span>
                    </div>
                }
            />

            <SourceReturnBanner
                fromWorkspace={fromWorkspace}
                sourceReturnHref={sourceReturnHref(
                    returnTo,
                    fromWorkspace,
                    controller.operation,
                )}
                operation={controller.operation}
            />

            <FulfillmentFilterBar
                activeTypeSlug={activeTypeSlug}
                visibleTypes={controller.visibleTypes}
                onTypeChange={controller.setTypeFilter}
                q={q}
                warehouseId={warehouseId}
                due={due}
                gate={gate}
                salesOrderId={salesOrderId}
                purchaseOrderId={purchaseOrderId}
                operations={controller.operations}
                autoNext={controller.autoNext}
                showAutoNext={controller.canExecute}
                onPatch={controller.handlePatch}
                onClearAllFilters={controller.clearAllFilters}
                onAutoNextChange={controller.setAutoNext}
            />

            {controller.lastResult ? (
                <div
                    ref={controller.resultRef}
                    tabIndex={-1}
                    className="outline-none"
                >
                    <FulfillmentResultPanel
                        lastResult={controller.lastResult}
                        currentUrl={controller.currentUrl}
                        onResolveUnknown={() =>
                            void controller.handleResolveUnknown()
                        }
                        onNext={() => {
                            const next =
                                controller.operations[
                                    controller.currentIndex + 1
                                ]?.operationId ??
                                controller.operations[0]?.operationId
                            if (next) controller.goToOperation(next)
                        }}
                        onContinueWarehouseShip={controller.goToWarehouseShip}
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
                    headerDescription={header.label}
                    completed={controller.completed}
                    operationTypes={operationTypes}
                    emptyReason={controller.view?.emptyReason}
                    roleLabel={
                        context?.roleLabel ?? FULFILLMENT_ROLES[roleValue].label
                    }
                    visibleTypes={controller.visibleTypes}
                    filterSummary={context?.filterSummary}
                    onClearAllFilters={controller.clearAllFilters}
                />
            ) : (
                <div className="grid min-h-[28rem] min-w-0 gap-4 xl:grid-cols-[minmax(15rem,0.9fr)_minmax(0,2.1fr)]">
                    <FulfillmentQueueList
                        operations={controller.operations}
                        currentIndex={controller.currentIndex}
                        position={
                            context?.position ?? controller.currentIndex + 1
                        }
                        total={context?.total ?? controller.operations.length}
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
                    />

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
                        readOnlyNote={readOnlyNote(controller.operation)}
                        responsibilityStatus={status}
                        responsibilityStatusLabel={responsibilityStatusLabel(
                            controller.operation,
                            controller.canExecute,
                        )}
                        currentUrl={controller.currentUrl}
                        snapshotUpdatedAt={context?.snapshotUpdatedAt ?? ""}
                        position={
                            context?.position ?? controller.currentIndex + 1
                        }
                        total={context?.total ?? controller.operations.length}
                        shortcutsOpen={controller.shortcutsOpen}
                        headingRef={controller.headingRef}
                        resultUnknown={
                            controller.lastResult?.status === "unknown"
                        }
                        onDraftChange={controller.updateDraft}
                        onSkip={controller.handleSkip}
                        onDiscard={controller.handleDiscard}
                        onSave={() => void controller.handleSave()}
                        onConfirm={() => controller.setConfirmOpen(true)}
                        onBack={() => {
                            const href = sourceReturnHref(
                                returnTo,
                                fromWorkspace,
                                controller.operation,
                            )
                            if (href) router.push(href)
                            else router.push("/workspace")
                        }}
                        onToggleShortcuts={() =>
                            controller.setShortcutsOpen((v) => !v)
                        }
                    />
                </div>
            )}

            <FormalActionConfirmDialog
                open={controller.confirmOpen}
                onOpenChange={controller.setConfirmOpen}
                title={
                    controller.operation
                        ? OPERATION_CONFIRM_TITLE[
                              controller.operation.operationType
                          ]
                        : "确认？"
                }
                description="没确认成功之前，库存和留货都不会动。"
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
                lockedFields={["来源单据、版本和留货", "单据类型"]}
                effects={
                    controller.operation && controller.draft
                        ? impactPreview(controller.operation, controller.draft)
                        : []
                }
                irreversibleEffects={[CORRECTION_NOTICE]}
                nextDepartment="做完之后由销售登记客户验收"
                pending={controller.formalPending}
                onConfirm={async () => {
                    await controller.handlePost()
                }}
            />
        </PageScaffold>
    )
}
