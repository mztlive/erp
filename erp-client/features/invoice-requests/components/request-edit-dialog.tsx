"use client"

import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import {
    ReworkRejectionEcho,
    type ReworkRejectionContext,
} from "@/features/approval-workflow/components/rework-rejection-echo"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"

import { requestStatusLabels } from "../api"
import {
    useInvoiceRequest,
    useInvoiceRequestPermissions,
} from "../hooks/queries"
import { InvoiceRequestForm } from "./request-form"

/** 开票申请就地修改弹窗：加载原申请后直接复用 `InvoiceRequestForm`（含实时额度校验、重提说明与幂等恢复）。 */
export function InvoiceRequestEditDialog({
    requestId,
    rejectionContext,
    onClose,
    onSaved,
    onSubmitted,
}: {
    requestId: string
    rejectionContext?: ReworkRejectionContext
    onClose: () => void
    onSaved?: () => void
    onSubmitted?: () => void
}) {
    const permissions = useInvoiceRequestPermissions()
    const canRead = permissions.canReadDetail || permissions.canRead
    const query = useInvoiceRequest(
        canRead && permissions.canSubmit ? requestId : undefined,
    )
    const id = `invoice-request-edit-${toAutomationIdSegment(requestId)}`
    const request = query.data
    const applicantId = request?.created_by || ""
    const editableStatus = Boolean(
        request &&
        (request.status === "draft" || request.status === "rejected"),
    )
    const isApplicant = Boolean(
        request && (!permissions.userId || applicantId === permissions.userId),
    )

    if (
        !permissions.isPending &&
        permissions.canSubmit &&
        canRead &&
        request &&
        editableStatus &&
        isApplicant
    ) {
        return (
            <InvoiceRequestForm
                key={request.id}
                existing={request}
                presentation="dialog"
                rejectionContext={rejectionContext}
                onCancel={onClose}
                onDone={(_, action) => {
                    if (action === "submit") {
                        onSubmitted?.()
                    } else {
                        onSaved?.()
                        onClose()
                    }
                }}
            />
        )
    }

    return (
        <Dialog open onOpenChange={(open) => !open && onClose()}>
            <DialogContent
                id={id}
                size="lg"
                closeButtonId={`${id}-close`}
                showCloseButton
            >
                <DialogHeader>
                    <DialogTitle>修改开票申请</DialogTitle>
                    <DialogDescription>
                        修改开票明细或申请事由后重新提交审批。
                    </DialogDescription>
                </DialogHeader>
                <DialogBody className="space-y-4">
                    <ReworkRejectionEcho rejection={rejectionContext} />
                    {permissions.isPending ? (
                        <p
                            role="status"
                            className="text-xs text-muted-foreground"
                        >
                            正在核对操作权限…
                        </p>
                    ) : !permissions.canSubmit || !canRead ? (
                        <BusinessFailureState
                            id={`${id}-permission`}
                            kind="permission"
                            title="当前账号无权修改此开票申请"
                            description="请确认当前账号具备开票申请查看与提交权限。"
                            action={
                                <Button
                                    id={`${id}-close-action`}
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    onClick={onClose}
                                >
                                    关闭
                                </Button>
                            }
                        />
                    ) : query.isPending ? (
                        <p
                            role="status"
                            className="text-xs text-muted-foreground"
                        >
                            正在读取原开票申请…
                        </p>
                    ) : query.isError ? (
                        <BusinessFailureState
                            id={`${id}-load-error`}
                            title="开票申请读取失败"
                            error={query.error}
                            description={getErrorMessage(
                                query.error,
                                "请重新读取后重试。",
                            )}
                            onRetry={() => void query.refetch()}
                        />
                    ) : request ? (
                        <BusinessFailureState
                            id={`${id}-unavailable`}
                            kind={!isApplicant ? "permission" : "conflict"}
                            title={
                                !isApplicant
                                    ? "仅原申请人可就地修改此开票申请"
                                    : `当前状态（${requestStatusLabels[request.status] ?? request.status}）不支持就地修改`
                            }
                            description="可关闭弹窗核对最新单据状态，或直接放弃本次待办。"
                            action={
                                <Button
                                    id={`${id}-close-unavailable`}
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    onClick={onClose}
                                >
                                    关闭
                                </Button>
                            }
                        />
                    ) : null}
                </DialogBody>
            </DialogContent>
        </Dialog>
    )
}
