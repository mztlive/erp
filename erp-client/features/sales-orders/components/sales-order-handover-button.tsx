"use client"

import * as React from "react"
import { useQuery } from "@tanstack/react-query"
import { LoaderCircleIcon } from "lucide-react"

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
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Textarea } from "@/components/ui/textarea"
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

/**
 * 销售单详情「责任交接」。
 * 显式目标 + 原因 + 幂等键；业务组织省略表示保留原组织。
 * 开放验收任务随交接转交，审批任务与历史归属保持不变。
 */
export function SalesOrderHandoverButton({
    order,
    onResult,
}: {
    order: SalesOrderDetailView
    onResult?: (result: SalesOrderDetailActionResult) => void
}) {
    const [open, setOpen] = React.useState(false)
    const [targetUserId, setTargetUserId] = React.useState("")
    const [orgUnitId, setOrgUnitId] = React.useState("")
    const [reason, setReason] = React.useState("")
    const [idempotencyKey, setIdempotencyKey] = React.useState("")
    const [confirmError, setConfirmError] = React.useState<string | null>(null)
    const candidatesQuery = useSalesHandoverCandidatesQuery(order.id, open)
    const handoverMutation = useSubmitSalesHandoverMutation()
    const orderSegment = toAutomationIdSegment(order.id)
    /** 随转预览复用详情验收面同一 QueryKey；已加载时命中缓存，不新增请求。 */
    const acceptancePreview = useQuery({
        queryKey: salesHandoverPreviewKey(order.id),
        staleTime: 0,
        queryFn: () =>
            fetchCustomerAcceptanceWorkspace({ salesOrderId: order.id }),
        enabled: open,
    })
    const acceptanceSummary = React.useMemo(() => {
        const lines = acceptancePreview.data?.salesLines
        if (!lines) return null
        const progress = buildOrderProgress(lines)
        const openLines = progress.lines.filter((line) =>
            isPositiveQty(line.pendingQuantity),
        ).length
        return { openLines, ...progress }
    }, [acceptancePreview.data])

    const candidates = React.useMemo(
        () =>
            (candidatesQuery.data ?? []).map((item) => ({
                value: item.userId,
                label: `${item.displayName}（${item.account}）`,
            })),
        [candidatesQuery.data],
    )

    return (
        <>
            <Button
                id="sales-orders-detail-handover-trigger"
                type="button"
                size="sm"
                variant="outline"
                onClick={() => {
                    setTargetUserId("")
                    setOrgUnitId("")
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
            <AlertDialog open={open} onOpenChange={setOpen}>
                <AlertDialogContent className="sm:max-w-md">
                    <AlertDialogHeader>
                        <AlertDialogTitle>交接销售责任</AlertDialogTitle>
                        <AlertDialogDescription>
                            开放验收任务随交接转交；审批任务、已完成验收与历史归属保持不变。业务组织不随接收人部门变化。
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <div
                        id={`sales-orders-detail-handover-summary-${orderSegment}`}
                        className="rounded-md border p-3 text-sm"
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
                                正在加载开放验收…
                            </p>
                        ) : acceptancePreview.isError || !acceptanceSummary ? (
                            <p className="mt-1 text-muted-foreground">
                                开放验收清单加载失败，仍可继续交接，以服务端原子转交结果为准。
                            </p>
                        ) : acceptanceSummary.openLines === 0 ? (
                            <p className="mt-1 text-muted-foreground">
                                当前无开放验收数量；提交后新验收任务归新负责人。
                            </p>
                        ) : (
                            <p className="mt-1 text-muted-foreground">
                                开放验收 {acceptanceSummary.openLines} 行
                                {acceptanceSummary.unitCode
                                    ? `，待验收 ${qtyWithUnit(acceptanceSummary.pendingQuantity, acceptanceSummary.unitCode)}`
                                    : ""}
                                ，随交接原子转交；审批任务保持不变。
                            </p>
                        )}
                    </div>
                    <div className="space-y-3">
                        <div className="space-y-2">
                            <label
                                htmlFor={`sales-orders-detail-handover-target-${orderSegment}`}
                                className="text-sm font-medium"
                            >
                                接收人（只列出合格有效人员）
                            </label>
                            <NativeSelect
                                id={`sales-orders-detail-handover-target-${orderSegment}`}
                                value={targetUserId}
                                onChange={(event) =>
                                    setTargetUserId(event.target.value)
                                }
                                disabled={
                                    handoverMutation.isPending ||
                                    candidatesQuery.isPending
                                }
                                className="w-full"
                            >
                                <NativeSelectOption value="">
                                    {candidatesQuery.isPending
                                        ? "正在加载候选人…"
                                        : "选择接收人"}
                                </NativeSelectOption>
                                {candidates.map((item) => (
                                    <NativeSelectOption
                                        key={item.value}
                                        value={item.value}
                                    >
                                        {item.label}
                                    </NativeSelectOption>
                                ))}
                            </NativeSelect>
                            {candidatesQuery.isError ? (
                                <p
                                    className="text-sm text-destructive"
                                    role="alert"
                                >
                                    候选人加载失败，请关闭后重试。
                                </p>
                            ) : null}
                        </div>
                        <div className="space-y-2">
                            <label
                                htmlFor={`sales-orders-detail-handover-org-${orderSegment}`}
                                className="text-sm font-medium"
                            >
                                目标业务组织（留空保留原组织）
                            </label>
                            <Input
                                id={`sales-orders-detail-handover-org-${orderSegment}`}
                                value={orgUnitId}
                                onChange={(event) =>
                                    setOrgUnitId(event.target.value)
                                }
                                placeholder="组织 ID，留空保留原组织"
                                disabled={handoverMutation.isPending}
                            />
                        </div>
                        <div className="space-y-2">
                            <label
                                htmlFor={`sales-orders-detail-handover-reason-${orderSegment}`}
                                className="text-sm font-medium"
                            >
                                交接原因
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
                                className="text-sm text-destructive"
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
                                        targetBusinessOrgUnitId:
                                            orgUnitId.trim() || undefined,
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
                                        onResult?.({
                                            status: "blocked",
                                            title: failure.title,
                                            description: failure.description,
                                            reference: order.documentNumber,
                                        })
                                    })
                            }}
                        >
                            {handoverMutation.isPending ? (
                                <LoaderCircleIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                    className="animate-spin"
                                />
                            ) : null}
                            {handoverMutation.isPending ? "交接中" : "确认交接"}
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>
        </>
    )
}
