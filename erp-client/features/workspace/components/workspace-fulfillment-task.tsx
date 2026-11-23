"use client"

import { WorkspaceProcessingPane } from "./workspace-processing-layout"

import * as React from "react"
import { useRouter } from "next/navigation"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import type { FulfillmentQueueFilters } from "@/features/fulfillment-operations/api"
import { FULFILLMENT_ROLES } from "@/features/fulfillment-operations/lib/fulfillment-roles"
import { OPERATION_TYPE_LABEL } from "@/features/fulfillment-operations/types"
import { FulfillmentOperationsWorkspace } from "@/features/fulfillment-operations/pages/components/fulfillment-operations-workspace"
import { FulfillmentPageStates } from "@/features/fulfillment-operations/pages/components/fulfillment-page-states"
import {
    FulfillmentPaymentBadges,
    FulfillmentPaymentLink,
} from "@/features/fulfillment-operations/pages/components/fulfillment-payment-notice"
import { useFulfillmentOperationsController } from "@/features/fulfillment-operations/pages/hooks/use-fulfillment-operations-controller"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { displayName } from "@/lib/display-name"
import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"
import { navigateWorkspace } from "@/lib/workspace-tabs"

import { fulfillmentTaskTitle } from "../lib/fulfillment-title"
import { workspaceFulfillmentDescriptor } from "../lib/workspace-fulfillment"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceDeliveryGate } from "./workspace-delivery-brief"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

type WorkspaceFulfillmentTaskProps = Readonly<{
    item: WorkspaceWorkItem
    grantedPermissions: readonly string[]
    onTaskCompleted: (workItemId: string) => void
}>

/** W01 履约作业面：任务身份锁定一个强类型收货或发货操作。 */
export function WorkspaceFulfillmentTask({
    item,
    grantedPermissions,
    onTaskCompleted,
}: WorkspaceFulfillmentTaskProps) {
    const descriptor = workspaceFulfillmentDescriptor(item)
    const operationType = descriptor?.operationTypes[0]
    const filters = React.useMemo<FulfillmentQueueFilters>(
        () => ({
            role: descriptor?.role ?? "sales_order",
            operationTypes: operationType ? [operationType] : [],
            operationId: item.businessObjectId,
            taskSourceVersion: item.subjectVersion,
            currentOperationId: item.businessObjectId,
        }),
        [
            descriptor?.role,
            item.businessObjectId,
            item.subjectVersion,
            operationType,
        ],
    )
    const controller = useFulfillmentOperationsController({
        roleValue: filters.role,
        filters,
        lane: null,
        autoNextExplicit: "0",
        stateMode: "local",
        grantedPermissions,
        permissionsReady: true,
        executionAuthorized: item.allowedActions.includes("PROCESS"),
        onOperationCompleted: (operationId) => {
            if (operationId === item.businessObjectId) {
                onTaskCompleted(item.workItemId)
            }
        },
    })
    const supplyFacts = (item.summarySections ?? []).filter(
        (section) =>
            section.label === "供给影响提示" ||
            section.label === "当前采购负责人",
    )
    const subtitleParts = [
        operationType ? OPERATION_TYPE_LABEL[operationType] : null,
        item.stableNumber,
    ].filter(Boolean)

    return (
        <WorkspaceProcessingPane
            header={
                <WorkspaceTaskIdentityHeader
                    item={item}
                    title={
                        displayName(
                            controller.operation?.source.customerLabel,
                        ) || fulfillmentTaskTitle(item, controller.operation)
                    }
                    badges={
                        <>
                            <StatusBadge
                                label={item.statusLabel}
                                tone={item.statusTone}
                            />
                            {controller.operation &&
                                controller.operation.gate.state === "BLOCKED" &&
                                operationType !== "WAREHOUSE_SHIP" && (
                                    <>
                                        <StatusBadge
                                            label="预付待完成"
                                            tone="warning"
                                        />
                                        <FulfillmentPaymentBadges
                                            operation={controller.operation}
                                        />
                                    </>
                                )}
                        </>
                    }
                    subtitle={
                        subtitleParts.length > 0 ? (
                            <span className="num">
                                {subtitleParts.join(" · ")}
                            </span>
                        ) : null
                    }
                >
                    {controller.operation && (
                        <FulfillmentPaymentLink
                            operation={controller.operation}
                        />
                    )}
                </WorkspaceTaskIdentityHeader>
            }
            aria-label="当前履约任务"
        >
            {!!supplyFacts.length && (
                <Alert>
                    <AlertTitle>当前供给与采购责任</AlertTitle>
                    <AlertDescription>
                        <dl className="space-y-2">
                            {supplyFacts.map((section) => (
                                <div key={section.label}>
                                    <dt className="font-medium">
                                        {section.label}
                                    </dt>
                                    <dd className="whitespace-pre-wrap">
                                        {section.value}
                                    </dd>
                                </div>
                            ))}
                        </dl>
                    </AlertDescription>
                </Alert>
            )}
            {controller.operation?.impact && (
                <Alert>
                    <AlertTitle>履约影响与核对事项</AlertTitle>
                    <AlertDescription className="whitespace-pre-wrap">
                        {controller.operation.impact}
                    </AlertDescription>
                </Alert>
            )}
            {!descriptor ? (
                <Alert variant="destructive">
                    <AlertTitle>待办关联的单据信息异常</AlertTitle>
                    <AlertDescription>
                        当前待办与关联业务单据不匹配，请刷新页面后重试，或联系管理员处理。
                    </AlertDescription>
                </Alert>
            ) : controller.queueQuery.isPending ||
              controller.queueQuery.isError ? (
                <FulfillmentPageStates
                    status={
                        controller.queueQuery.isPending ? "pending" : "error"
                    }
                    standalone
                    embedded
                    headerDescription="履约处理"
                    error={controller.queueQuery.error}
                    onRetry={() => void controller.queueQuery.refetch()}
                />
            ) : (
                <div className="flex min-h-0 flex-1 flex-col gap-5">
                    <FulfillmentOperationsWorkspace
                        controller={controller}
                        headerDescription="当前任务"
                        operationTypes={[operationType!]}
                        roleLabel={
                            controller.context?.roleLabel ??
                            FULFILLMENT_ROLES[descriptor.role].label
                        }
                        embedded
                        singleOperation
                        onBack={() => undefined}
                    />
                </div>
            )}
        </WorkspaceProcessingPane>
    )
}

/**
 * 履约直达入口：取代原有 max-w-5xl 弹窗套娃，直接跳转至履约主作业面。
 */
export function WorkspaceFulfillmentProcessAction(
    props: WorkspaceFulfillmentTaskProps,
) {
    const router = useRouter()
    const operationType = workspaceFulfillmentDescriptor(props.item)
        ?.operationTypes[0]
    const isDelivery =
        operationType === "SUPPLIER_DIRECT" ||
        operationType === "WAREHOUSE_SHIP"
    if (!props.item.allowedActions.includes("PROCESS")) return null
    const actionLabel = isDelivery
        ? "登记发货"
        : operationType === "RECEIPT"
          ? "登记入库"
          : operationType === "ELECTRONIC"
            ? "登记电子交付"
            : operationType === "SERVICE"
              ? "登记服务完成"
              : "处理履约"
    const href = buildWorkspaceDetailHref("tasks", props.item.workItemId)
    return (
        <>
            {isDelivery ? (
                <WorkspaceDeliveryGate
                    key={props.item.workItemId}
                    item={props.item}
                />
            ) : null}
            <Button
                id={`workspace-fulfillment-process-${toAutomationIdSegment(props.item.workItemId)}`}
                onClick={() => {
                    if (!navigateWorkspace(href)) {
                        router.push(href)
                    }
                }}
            >
                {actionLabel}
            </Button>
        </>
    )
}
