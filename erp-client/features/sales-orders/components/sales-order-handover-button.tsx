"use client"

import { RequiredMark } from "@/components/ui/required-mark"

import * as React from "react"
import { useQuery } from "@tanstack/react-query"

import {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { OptionCombobox } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { fetchCustomerAcceptanceWorkspace } from "@/features/sales-orders/api/acceptance"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import {
    salesHandoverPreviewKey,
    useSalesHandoverCandidatesQuery,
    useSubmitSalesHandoverMutation,
} from "@/features/sales-orders/hooks/use-sales-handover"
import {
    buildOrderProgress,
    isPositiveQty,
    qtyWithUnit,
} from "@/features/sales-orders/lib/acceptance-model"
import type { SalesOrderDetailActionResult } from "@/features/sales-orders/lib/sales-order-detail-model"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { getErrorPresentation } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"

/** 仅具有销售单更新权限且非终态（已关闭/已作废）的销售单允许发起责任交接。 */
export function canShowSalesOrderHandover(
    order: SalesOrderDetailView,
    permissions: readonly string[] | undefined,
): boolean {
    return (
        hasPermission(permissions, "sales_order:update") &&
        order.primaryStatus.code !== "closed" &&
        order.primaryStatus.code !== "voided"
    )
}

/**
 * 销售单详情「责任交接」。
 * 显式目标 + 原因 + 幂等键；业务组织省略表示保留原组织。
 * 开放验收任务随交接转交，审批任务与历史归属保持不变。
 */
export function SalesOrderHandoverButton({
    order,
    onResult,
    triggerVariant = "outline",
    triggerClassName,
}: {
    order: SalesOrderDetailView
    onResult?: (result: SalesOrderDetailActionResult) => void
    triggerVariant?: React.ComponentProps<typeof Button>["variant"]
    triggerClassName?: string
}) {
    const profileQuery = useAccountProfileQuery()
    const allowed = canShowSalesOrderHandover(
        order,
        profileQuery.data?.permissions,
    )
    const [open, setOpen] = React.useState(false)
    const [targetUserId, setTargetUserId] = React.useState("")
    const [reason, setReason] = React.useState("")
    const [idempotencyKey, setIdempotencyKey] = React.useState("")
    const [confirmError, setConfirmError] = React.useState<string | null>(null)
    const candidatesQuery = useSalesHandoverCandidatesQuery(
        order.id,
        allowed && open,
    )
    const handoverMutation = useSubmitSalesHandoverMutation()
    const orderSegment = toAutomationIdSegment(order.id)
    /** 随转预览复用详情验收面同一 QueryKey；已加载时命中缓存，不新增请求。 */
    const acceptancePreview = useQuery({
        queryKey: salesHandoverPreviewKey(order.id),
        staleTime: 0,
        queryFn: () =>
            fetchCustomerAcceptanceWorkspace({ salesOrderId: order.id }),
        enabled: allowed && open,
    })
    const acceptanceSummary = React.useMemo(() => {
        const lines = acceptancePreview.data?.salesLines
        if (!lines) return null
        const orderLines = new Map(
            order.lineItems.map((line) => [line.id, line]),
        )
        const progress = buildOrderProgress(
            lines.map((line) => {
                const source = orderLines.get(line.salesOrderLineId)
                return {
                    ...line,
                    unitName:
                        line.unitName ??
                        (line.unitCode === source?.unit
                            ? source.unitName
                            : undefined),
                }
            }),
        )
        const openLines = progress.lines.filter((line) =>
            isPositiveQty(line.pendingQuantity),
        ).length
        return { openLines, ...progress }
    }, [acceptancePreview.data, order.lineItems])

    const candidates = React.useMemo(
        () =>
            (candidatesQuery.data ?? []).map((item) => ({
                value: item.userId,
                label: `${item.displayName}（${item.account}）`,
            })),
        [candidatesQuery.data],
    )

    if (!allowed) return null

    return (
        <>
            <Button
                id="sales-orders-detail-handover-trigger"
                type="button"
                size="sm"
                variant={triggerVariant}
                className={triggerClassName}
                onClick={() => {
                    setTargetUserId("")
                    setReason("")
                    setConfirmError(null)
                    setIdempotencyKey(
                        `sales-handover:${order.id}:${crypto.randomUUID()}`,
                    )
                    setOpen(true)
                }}
            >
                责任交接
            </Button>
            <AlertDialog
                open={open}
                onOpenChange={(next) => {
                    if (!next && handoverMutation.isPending) return
                    setOpen(next)
                }}
            >
                <AlertDialogContent className="sm:max-w-md">
                    <AlertDialogHeader>
                        <AlertDialogTitle>交接销售责任</AlertDialogTitle>
                        <AlertDialogDescription>
                            待验收记录随交接转交；审批任务、已完成验收与历史归属保持不变。业务组织不随接收人部门变化。
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <div
                        id={`sales-orders-detail-handover-summary-${orderSegment}`}
                        className="rounded-md border p-3 text-body-compact"
                    >
                        <p className="font-medium">
                            {order.documentNumber} · {order.primaryStatus.label}{" "}
                            · 现负责人 {order.ownerName}
                        </p>
                        {acceptancePreview.isPending ? (
                            <p
                                className="mt-1 text-muted-foreground"
                                role="status"
                            >
                                正在加载待验收记录…
                            </p>
                        ) : acceptancePreview.isError || !acceptanceSummary ? (
                            <p className="mt-1 text-muted-foreground">
                                待验收明细加载失败，仍可继续办理交接，系统会自动将全部待验收任务转交给新负责人。
                            </p>
                        ) : acceptanceSummary.openLines === 0 ? (
                            <p className="mt-1 text-muted-foreground">
                                当前无待验收记录；提交后新验收任务归新负责人。
                            </p>
                        ) : (
                            <p className="mt-1 text-muted-foreground">
                                待验收记录 {acceptanceSummary.openLines} 行
                                {acceptanceSummary.unitCode
                                    ? `，待验收 ${qtyWithUnit(acceptanceSummary.pendingQuantity, acceptanceSummary.unitName)}`
                                    : ""}
                                ，随交接一并转交；审批任务保持不变。
                            </p>
                        )}
                    </div>
                    <div className="space-y-3">
                        <div className="space-y-2">
                            <label
                                htmlFor={`sales-orders-detail-handover-target-${orderSegment}`}
                                className="text-xs font-medium"
                            >
                                接收销售人员
                                <RequiredMark />
                            </label>
                            <OptionCombobox
                                id={`sales-orders-detail-handover-target-${orderSegment}`}
                                options={candidates}
                                value={targetUserId || null}
                                onValueChange={(next) =>
                                    setTargetUserId(next ?? "")
                                }
                                loading={candidatesQuery.isPending}
                                disabled={handoverMutation.isPending}
                                placeholder={
                                    candidatesQuery.isPending
                                        ? "正在加载候选人…"
                                        : "选择接收人"
                                }
                                emptyLabel="暂无可交接候选人"
                                className="w-full"
                                inputClassName="w-full"
                            />
                            {candidatesQuery.isError ? (
                                <p
                                    className="text-body-compact text-destructive"
                                    role="alert"
                                >
                                    候选人加载失败，请关闭后重试。
                                </p>
                            ) : null}
                        </div>
                        <div className="space-y-2">
                            <label
                                htmlFor={`sales-orders-detail-handover-reason-${orderSegment}`}
                                className="text-xs font-medium"
                            >
                                交接原因
                                <RequiredMark />
                            </label>
                            <Textarea
                                id={`sales-orders-detail-handover-reason-${orderSegment}`}
                                value={reason}
                                onChange={(event) =>
                                    setReason(event.target.value)
                                }
                                placeholder="请输入交接原因"
                                rows={3}
                                disabled={handoverMutation.isPending}
                            />
                        </div>
                        {confirmError ? (
                            <p
                                className="text-body-compact text-destructive"
                                role="alert"
                            >
                                {confirmError}
                            </p>
                        ) : null}
                    </div>
                    <AlertDialogFooter>
                        <AlertDialogCancel
                            id="sales-orders-detail-handover-cancel"
                            disabled={handoverMutation.isPending}
                        >
                            取消
                        </AlertDialogCancel>
                        <AlertDialogAction
                            loading={handoverMutation.isPending}
                            id="sales-orders-detail-handover-confirm"
                            disabled={
                                handoverMutation.isPending ||
                                !targetUserId.trim() ||
                                !reason.trim()
                            }
                            onClick={() => {
                                setConfirmError(null)
                                void handoverMutation
                                    .mutateAsync({
                                        salesOrderId: order.id,
                                        expectedVersion:
                                            order.lockVersion || order.version,
                                        targetOwnerUserId: targetUserId.trim(),
                                        reason: reason.trim(),
                                        idempotencyKey,
                                    })
                                    .then((result) => {
                                        setOpen(false)
                                        onResult?.({
                                            status: "succeeded",
                                            title: "销售责任已交接",
                                            description: `已交接给新负责人，随转验收任务 ${result.transferredAcceptanceTaskIds.length} 条，审批任务保持 ${result.keptApprovalTaskCount} 条。`,
                                            reference: order.documentNumber,
                                        })
                                    })
                                    .catch((error: unknown) => {
                                        const failure = getErrorPresentation(
                                            error,
                                            "交接未完成，请刷新后重试。",
                                        )
                                        setConfirmError(failure.description)
                                    })
                            }}
                        >
                            {handoverMutation.isPending ? "交接中" : "确认交接"}
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>
        </>
    )
}
