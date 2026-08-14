"use client"

import Link from "next/link"
import { useRouter } from "next/navigation"
import { TriangleAlertIcon } from "lucide-react"

import {
    BusinessStatusBadge,
    FormalActionResult,
    SequentialProcessBar,
    StatusTrackSummary,
    surfacePanelClassName,
} from "@/components/business"
import type { ResponsibilityStatus } from "@/components/business/workflow-actions"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"
import { WORK_ITEM_STATUS_LABEL, WORK_ITEM_TYPE_LABEL } from "@/features/supplier-orders/types"
import { formatDateTime } from "@/lib/datetime"
import type { SupplierOrderCenterResult } from "@/features/supplier-orders/hooks/use-supplier-order-center-actions"

export function WorkItemProcessPanel({
    workItem,
    workItemBlocker,
    responsibilityStatus,
    canCompleteTask,
    pending,
    releasePending,
    onStartProcessing,
    onProcess,
    onRelease,
}: {
    workItem?: SupplierOrderDetailView["workItem"]
    workItemBlocker?: SupplierOrderDetailView["workItemBlocker"]
    responsibilityStatus: ResponsibilityStatus
    canCompleteTask: boolean
    pending: boolean
    releasePending: boolean
    onStartProcessing: () => void
    onProcess: () => void
    onRelease: () => void
}) {
    const router = useRouter()

    return (
        <>
            {workItemBlocker ? (
                <Alert variant="warning">
                    <AlertTitle>正式任务入口已阻断</AlertTitle>
                    <AlertDescription>
                        {workItemBlocker.message}
                    </AlertDescription>
                </Alert>
            ) : null}

            {workItem ? (
                <div className="space-y-2">
                    <SequentialProcessBar
                        current={1}
                        total={1}
                        responsibilityStatus={responsibilityStatus}
                        processLabel="确认处理结果并完成任务"
                        processDisabled={!canCompleteTask}
                        showProcessNext={false}
                        pending={pending}
                        onBack={() => router.push("/workspace/tasks")}
                        onStartProcessing={onStartProcessing}
                        onProcess={onProcess}
                        onProcessNext={() => undefined}
                    />
                    {responsibilityStatus === "assigned_to_me" &&
                    workItem.allowedTaskActions.includes("RELEASE_TO_TEAM") ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={releasePending}
                            onClick={onRelease}
                        >
                            退回团队
                        </Button>
                    ) : null}
                </div>
            ) : null}
        </>
    )
}

export function StatusAlertsPanel({
    order,
    lastInvestigation,
    noQueryCapability,
}: {
    order: SupplierOrderDetailView["order"]
    lastInvestigation?: SupplierOrderDetailView["lastInvestigation"]
    noQueryCapability: boolean
}) {
    const isResultUnknown = order.fulfillmentStatus === "RESULT_UNKNOWN"

    return (
        <>
            <StatusTrackSummary
                variant="table"
                className="sm:grid-cols-3"
                aria-label="三轨进度"
                tracks={[
                    {
                        id: "ff",
                        label: "履约",
                        status: {
                            label: order.fulfillmentLabel,
                            tone: order.fulfillmentTone,
                        },
                    },
                    {
                        id: "cancel",
                        label: "取消",
                        status: {
                            label: order.cancelLabel,
                            tone: order.cancelTone,
                        },
                    },
                    {
                        id: "refund",
                        label: "退款",
                        status: {
                            label: order.refundLabel,
                            tone: order.refundTone,
                        },
                    },
                ]}
            />

            <Alert variant="info">
                <AlertTitle>商城支付已发生</AlertTitle>
                <AlertDescription className="text-xs leading-relaxed">
                    {order.paymentOccurredNotice} 支付凭证{" "}
                    <span className="num">{order.paymentFactKey}</span> ·
                    支付时间{" "}
                    <span className="num">
                        {formatDateTime(order.paidAt, "fullIntl", "passthrough")}
                    </span>
                </AlertDescription>
            </Alert>

            {isResultUnknown ? (
                <Alert variant="warning" aria-live="polite">
                    <TriangleAlertIcon />
                    <AlertTitle>结果未知 — 请先查询原结果</AlertTitle>
                    <AlertDescription className="text-xs leading-relaxed">
                        不得把结果未知直接改成成功，也不得在未查询前直接再次下单。
                        {lastInvestigation ? (
                            <span className="mt-1 block">
                                最近查询：
                                {lastInvestigation.outcomeLabel} —{" "}
                                {lastInvestigation.summary}
                                {lastInvestigation.canSafeRetry
                                    ? " · 已开放安全重发"
                                    : " · 重发未开放"}
                            </span>
                        ) : noQueryCapability ? (
                            <span className="mt-1 block">
                                尚未查询。该供应商无查询能力，请前往接口错误与对账中心人工处理。
                            </span>
                        ) : (
                            <span className="mt-1 block">
                                尚未查询。先执行「查询原结果」，确认无结果且系统允许后再重发。
                            </span>
                        )}
                    </AlertDescription>
                </Alert>
            ) : null}

            {order.fulfillmentStatus === "COMPLETED" &&
            order.refundStatus === "PARTIAL" ? (
                <Alert variant="info">
                    <AlertTitle>已完成 + 部分退款</AlertTitle>
                    <AlertDescription className="text-xs">
                        履约与退款状态独立记录，互不覆盖
                    </AlertDescription>
                </Alert>
            ) : null}
        </>
    )
}

export function WorkItemCard({
    workItem,
    orderNo,
}: {
    workItem: NonNullable<SupplierOrderDetailView["workItem"]>
    orderNo: string
}) {
    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="rounded-t-lg border-b border-border/30 pb-2">
                <CardTitle className="text-sm">
                    {WORK_ITEM_TYPE_LABEL[workItem.workItemType]}
                </CardTitle>
                <CardDescription className="text-xs">
                    关联订单 {orderNo} · 责任模式{" "}
                    {workItem.assignmentMode === "POOL" ? "团队池" : "直接分派"}
                    {workItem.ownerUser
                        ? ` · 当前处理人 ${workItem.ownerUser.displayName}`
                        : ""}
                </CardDescription>
            </CardHeader>
            <CardContent className="flex flex-wrap gap-3 text-xs text-muted-foreground">
                <span>
                    状态{" "}
                    <BusinessStatusBadge
                        context="detail"
                        label={WORK_ITEM_STATUS_LABEL[workItem.workItemStatus]}
                        tone={
                            workItem.workItemStatus === "COMPLETED"
                                ? "success"
                                : "info"
                        }
                    />
                </span>
                <span>
                    查询原结果和再次提交不会完成任务；只有确认处理结果才会完成
                </span>
            </CardContent>
        </Card>
    )
}

export function ResultPanel({
    result,
    order,
    costs,
    onClose,
}: {
    result: SupplierOrderCenterResult
    order: SupplierOrderDetailView["order"]
    costs: SupplierOrderDetailView["costs"]
    onClose: () => void
}) {
    return (
        <FormalActionResult
            status={result.status}
            title={result.title}
            description={result.description}
            reference={result.reference}
            facts={result.facts}
            actions={
                <div className="flex flex-wrap gap-2">
                    <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={onClose}
                    >
                        关闭结果
                    </Button>
                    {order.fulfillmentStatus === "COMPLETED" ||
                    costs.settlementId ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            render={
                                <Link
                                    href={
                                        costs.settlementId
                                            ? `/supplier-api/settlements?q=${encodeURIComponent(costs.settlementNo ?? "")}`
                                            : "/supplier-api/settlements"
                                    }
                                />
                            }
                        >
                            打开 API 结算
                        </Button>
                    ) : null}
                    <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        render={
                            <Link
                                href={`/commerce/consumption-orders?q=${encodeURIComponent(order.mallOrderNo)}`}
                            />
                        }
                    >
                        返回商城消费订单
                    </Button>
                </div>
            }
        />
    )
}
