"use client"

import * as React from "react"
import { TriangleAlertIcon } from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
    DiscardConfirmDialog,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { displayActorName } from "@/features/approval-workflow/display"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { SalesOrderCreateForm } from "@/features/sales-orders/components/sales-order-create-form"
import { SalesOrderIdentityHeader } from "@/features/sales-orders/components/sales-order-detail-panels"
import { useSalesOrderDraftResumeQuery } from "@/features/sales-orders/hooks/queries"
import type { FormalCommandKeyLedger } from "@/lib/formal-command"
import { useRouter } from "@/lib/navigation"
import { useWorkspaceDirtyState } from "@/lib/workspace-form-guard"

export function SalesOrderEditableCenter({
    order,
    backHref,
    backLabel,
    fromQueue,
    fromWorkspace,
    commandLedger,
}: {
    order: SalesOrderDetailView
    backHref: string
    backLabel: string
    fromQueue: boolean
    fromWorkspace: string | null
    commandLedger: FormalCommandKeyLedger
}) {
    const router = useRouter()
    const resumeQuery = useSalesOrderDraftResumeQuery(order.id)
    const [editorState, setEditorState] = React.useState({
        dirty: false,
        busy: false,
    })
    const [leaveConfirmOpen, setLeaveConfirmOpen] = React.useState(false)

    useWorkspaceDirtyState({
        dirty: editorState.dirty,
        busy: editorState.busy,
    })

    const handleDirtyStateChange = React.useCallback(
        (next: { dirty: boolean; busy: boolean }) => {
            setEditorState((prev) =>
                prev.dirty === next.dirty && prev.busy === next.busy
                    ? prev
                    : next,
            )
        },
        [],
    )

    if (resumeQuery.isPending) {
        return (
            <DetailWorkspaceSkeleton
                title={order.customerName}
                description="正在加载可编辑内容…"
            />
        )
    }

    const isRejected = order.primaryStatus.code === "rejected"
    const rejectionInstance = order.approval?.instance
    const rejectorName = rejectionInstance
        ? displayActorName(
              rejectionInstance.latestRejectionByName,
              rejectionInstance.latestRejectionBy,
          )
        : undefined
    const rejectionReason = rejectionInstance?.latestRejection?.trim()

    return (
        <DetailWorkspacePage>
            <SalesOrderIdentityHeader
                identityOnly
                order={order}
                back={{
                    id: "sales-orders-detail-editable-back",
                    label: backLabel,
                    onClick: () => {
                        if (editorState.dirty) {
                            setLeaveConfirmOpen(true)
                            return
                        }
                        router.push(backHref)
                    },
                }}
                navigationMeta={
                    fromQueue
                        ? fromWorkspace === "W01" || fromWorkspace === "W09"
                            ? "从履约处理打开 · 处理完可点返回，回到列表原位"
                            : fromWorkspace === "W08"
                              ? "从采购单打开 · 处理完可点返回，回到列表原位"
                              : "从工作台打开 · 处理完可点返回，回到列表原位"
                        : undefined
                }
            />

            <DetailWorkspaceBody
                className="flex flex-col"
                alerts={
                    isRejected ? (
                        <Alert variant="destructive">
                            <TriangleAlertIcon aria-hidden="true" />
                            <AlertTitle>
                                上次审批已驳回
                                {rejectorName
                                    ? `（驳回人：${rejectorName}）`
                                    : ""}
                            </AlertTitle>
                            <AlertDescription className="whitespace-pre-wrap break-words">
                                {rejectionReason ||
                                    "请根据审批意见修改销售单内容后重新提交审批。"}
                            </AlertDescription>
                        </Alert>
                    ) : undefined
                }
            >
                {resumeQuery.isError || !resumeQuery.data ? (
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            title="可编辑内容加载失败"
                            error={resumeQuery.error}
                            action={
                                <Button
                                    id="sales-orders-detail-editable-center-resume-retry"
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    onClick={() => {
                                        void resumeQuery.refetch()
                                    }}
                                >
                                    重试
                                </Button>
                            }
                        />
                    </DetailWorkspaceSection>
                ) : (
                    <SalesOrderCreateForm
                        chrome="none"
                        purpose="draft"
                        initialDraft={resumeQuery.data}
                        initialNature={resumeQuery.data.nature}
                        initialContractId={resumeQuery.data.contractId}
                        commandLedger={commandLedger}
                        onDirtyStateChange={handleDirtyStateChange}
                    />
                )}
            </DetailWorkspaceBody>

            <DiscardConfirmDialog
                id="sales-orders-detail-editable-leave"
                open={leaveConfirmOpen}
                onOpenChange={setLeaveConfirmOpen}
                confirmLabel="放弃更改并返回"
                onConfirm={() => {
                    setLeaveConfirmOpen(false)
                    router.push(backHref)
                }}
            />
        </DetailWorkspacePage>
    )
}
