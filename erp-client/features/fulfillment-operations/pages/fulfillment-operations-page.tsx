"use client"

import * as React from "react"
import { useRouter, useSearchParams } from "next/navigation"

import { DataFreshness, PageHeader, PageScaffold } from "@/components/business"
import { useAccountProfileQuery } from "@/features/auth/hooks/queries"
import type { FulfillmentQueueFilters } from "@/features/fulfillment-operations/api"
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
import {
    laneHeader,
    resolveLane,
} from "@/features/fulfillment-operations/lib/lanes"
import { freshnessText } from "@/lib/ui-text"
import { FulfillmentFilterBar } from "./components/fulfillment-filter-bar"
import { FulfillmentOperationsWorkspace } from "./components/fulfillment-operations-workspace"
import { FulfillmentPageStates } from "./components/fulfillment-page-states"
import { SourceReturnBanner } from "./components/source-return-banner"
import { useFulfillmentKeyboardShortcuts } from "./hooks/use-fulfillment-keyboard-shortcuts"
import { useFulfillmentOperationsController } from "./hooks/use-fulfillment-operations-controller"
import { sourceReturnHref } from "./lib/presentation"

type FulfillmentOperationsPageProps = {
    /** 深链锁定销售单时使用；提供后不写履约工作区 URL。 */
    embeddedSalesOrderId?: string
    onSalesOrderChanged?: () => void
    onOpenAcceptance?: () => void
}

function parsePage(value: string | null): number {
    const page = Number(value)
    return Number.isInteger(page) && page > 0 ? page : 1
}

/**
 * 履约处理工作台。独立页面保留岗位筛选与队列连续处理。
 * 销售单详情的履约分区不再嵌入本页，只复用本单范围的队列与处理面。
 * PurchaseReceipt、Delivery、ElectronicDelivery、ServiceFulfillment 均为 NO_APPROVAL。
 */
export function FulfillmentOperationsPage({
    embeddedSalesOrderId,
    onSalesOrderChanged,
    onOpenAcceptance,
}: FulfillmentOperationsPageProps = {}) {
    const router = useRouter()
    const searchParams = useSearchParams()
    const profileQuery = useAccountProfileQuery()
    const embedded = Boolean(embeddedSalesOrderId)

    const lane = embedded ? null : resolveLane(searchParams.get("lane"))
    const header = embedded
        ? {
              label: "本单履约",
              description: "处理本销售单的入库、发货、电子交付与服务记录。",
          }
        : laneHeader(lane)
    const roleValue = embedded
        ? "sales_order"
        : (lane ?? DEFAULT_FULFILLMENT_ROLE)
    const operationTypes = embedded
        ? undefined
        : parseTypeParam(searchParams.get("type"))
    const warehouseId = embedded
        ? undefined
        : (searchParams.get("warehouseId") ?? undefined)
    const q = embedded ? undefined : (searchParams.get("q") ?? undefined)
    const due = embedded ? undefined : parseDueParam(searchParams.get("due"))
    const gate = embedded ? undefined : parseGateParam(searchParams.get("gate"))
    const salesOrderId =
        embeddedSalesOrderId ?? searchParams.get("salesOrderId") ?? undefined
    const purchaseOrderId = embedded
        ? undefined
        : (searchParams.get("purchaseOrderId") ?? undefined)
    const page = embedded ? 1 : parsePage(searchParams.get("page"))
    const returnTo = embedded
        ? undefined
        : (searchParams.get("returnTo") ?? undefined)
    const fromWorkspace = embedded
        ? undefined
        : (searchParams.get("from") ?? undefined)

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
            page,
            pageSize: 20,
            currentOperationId: embedded
                ? undefined
                : (searchParams.get("currentOperationId") ?? undefined),
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
            page,
            embedded,
            searchParams,
        ],
    )

    const controller = useFulfillmentOperationsController({
        roleValue,
        filters,
        lane,
        autoNextExplicit: embedded ? "0" : searchParams.get("autoNext"),
        stateMode: embedded ? "local" : "url",
        grantedPermissions: profileQuery.data?.permissions ?? [],
        permissionsReady: !profileQuery.isPending,
        onPosted: () => onSalesOrderChanged?.(),
    })

    useFulfillmentKeyboardShortcuts({
        dirty: controller.dirty,
        canPost: controller.canPost,
        formalPending: controller.formalPending,
        canExecute: controller.canExecute,
        supportsSave: controller.supportsSave,
        onSave: () => void controller.handleSave(),
        onConfirm: () => void controller.handleSubmit(),
        onNavigate: controller.handleNavigate,
        onToggleShortcuts: () => controller.setShortcutsOpen((value) => !value),
    })

    const context = controller.context
    const activeTypeSlug = typeParamValue(operationTypes)
    const onBack = () => {
        if (embedded) return
        const href = sourceReturnHref(
            returnTo,
            fromWorkspace,
            controller.operation,
        )
        if (href) router.push(href)
        else router.push("/workspace")
    }

    if (controller.queueQuery.isPending || controller.queueQuery.isError) {
        const state = (
            <FulfillmentPageStates
                status={controller.queueQuery.isPending ? "pending" : "error"}
                standalone
                embedded={embedded}
                headerDescription={header.label}
                error={controller.queueQuery.error}
                onRetry={() => void controller.queueQuery.refetch()}
            />
        )
        if (embedded) return state
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

                {state}
            </PageScaffold>
        )
    }

    const workspace = (
        <FulfillmentOperationsWorkspace
            controller={controller}
            headerDescription={header.label}
            operationTypes={operationTypes}
            roleLabel={context?.roleLabel ?? FULFILLMENT_ROLES[roleValue].label}
            embedded={embedded}
            onBack={onBack}
            onOpenAcceptance={onOpenAcceptance}
        />
    )

    if (embedded) return workspace

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

            {workspace}
        </PageScaffold>
    )
}
