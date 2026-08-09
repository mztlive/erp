"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter, useSearchParams } from "next/navigation"
import { z } from "zod"
import {
  ArrowLeftIcon,
  ExternalLinkIcon,
  EyeIcon,
  EyeOffIcon,
  TriangleAlertIcon,
} from "lucide-react"

import {
  BusinessFailureState,
  BusinessStatusBadge,
  DocumentHeader,
  DocumentSection,
  FormalActionConfirmDialog,
  FormalActionResult,
  GuardedBusinessAction,
  MoneyValue,
  OptionCombobox,
  PageHeader,
  PageScaffold,
  StatusTrackSummary,
  surfaceInsetClassName,
  surfacePanelClassName,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  DescriptionDetails,
  DescriptionItem,
  DescriptionList,
  DescriptionTerm,
} from "@/components/ui/description-list"
import { Label } from "@/components/ui/label"
import { Separator } from "@/components/ui/separator"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Textarea } from "@/components/ui/textarea"
import { clearAddressReveal } from "@/features/supplier-orders/api"
import { formatDateTime } from "@/lib/datetime"
import { getErrorMessage } from "@/lib/api/errors"
import { cn } from "@/lib/utils"
import {
  useAddNoteMutation,
  useAfterSalesActionMutation,
  useDeferOrderTaskMutation,
  useQueryResultMutation,
  useReplayOrderMutation,
  useRevealAddressMutation,
  useSupplierOrderDetailQuery,
} from "@/features/supplier-orders/queries"
import type {
  OrderSection,
} from "@/features/supplier-orders/types"
import {
  CANCEL_STATUS_LABEL,
  DEFER_REASON_OPTIONS,
  FULFILLMENT_STATUS_LABEL,
  LEASE_DISPOSITION_LABEL,
  REFUND_STATUS_LABEL,
  SECTION_LABEL,
  SECTIONS,
  WORK_ITEM_STATUS_LABEL,
  WORK_ITEM_TYPE_LABEL,
  codeVersion,
} from "@/features/supplier-orders/types"

function resolveSection(raw?: string | null): OrderSection {
  if (raw && (SECTIONS as string[]).includes(raw)) return raw as OrderSection
  return "overview"
}

const noteSchema = z.object({
  comment: z.string().trim().min(2, "请填写协同说明"),
})

const deferSchema = z.object({
  reasonCode: z.string().min(1, "请选择原因"),
  comment: z.string(),
})

export function SupplierOrderCenterPage({
  supplierOrderId,
  section: sectionProp,
}: {
  supplierOrderId: string
  section?: string
}) {
  const router = useRouter()
  const searchParams = useSearchParams()
  const from = searchParams.get("from")
  const sourceId = searchParams.get("sourceId")
  const workItemId = searchParams.get("workItemId") ?? undefined

  const activeSection = resolveSection(
    sectionProp ?? searchParams.get("section")
  )

  const query = useSupplierOrderDetailQuery({
    orderId: supplierOrderId,
  })
  const queryResultMutation = useQueryResultMutation()
  const replayMutation = useReplayOrderMutation()
  const deferMutation = useDeferOrderTaskMutation()
  const afterSalesMutation = useAfterSalesActionMutation()
  const revealMutation = useRevealAddressMutation()
  const noteMutation = useAddNoteMutation()

  const [result, setResult] = React.useState<{
    status: "succeeded" | "rejected" | "blocked" | "unknown"
    title: string
    description: string
    reference?: string
    facts?: { label: string; value: React.ReactNode }[]
  } | null>(null)
  const [replayOpen, setReplayOpen] = React.useState(false)
  const [deferOpen, setDeferOpen] = React.useState(false)
  const [afterSalesConfirm, setAfterSalesConfirm] = React.useState<{
    requestId: string
    requestNo: string
    mallRequestRef: string
    action: "CANCEL" | "REFUND"
  } | null>(null)
  const titleRef = React.useRef<HTMLSpanElement>(null)

  const detail = query.data

  const noteForm = useAppForm({
    defaultValues: { comment: "" },
    validators: { onChange: noteSchema },
    onSubmit: async ({ value }) => {
      if (!detail) return
      const res = await noteMutation.mutateAsync({
        orderId: supplierOrderId,
        expectedLockVersion: detail.order.lockVersion,
        comment: value.comment,
        idempotencyKey: `note-${supplierOrderId}-${Date.now()}`,
      })
      if (res.status === "succeeded") {
        noteForm.reset()
        setResult({
          status: "succeeded",
          title: "协同说明已记录",
          description: res.message,
        })
      } else {
        setResult({
          status: res.status === "blocked" ? "blocked" : "rejected",
          title: "协同说明未写入",
          description: res.message,
        })
      }
    },
  })

  const deferForm = useAppForm({
    defaultValues: { reasonCode: "WAITING_SUPPLIER", comment: "" },
    validators: { onChange: deferSchema },
    onSubmit: async ({ value }) => {
      if (!detail?.workItem) return
      const res = await deferMutation.mutateAsync({
        orderId: supplierOrderId,
        workItemId: detail.workItem.workItemId,
        expectedSubjectHash: detail.workItem.subjectHash,
        reasonCode: value.reasonCode,
        comment: value.comment || undefined,
        queueContextId: "queue-w26",
        idempotencyKey: `defer-${detail.workItem.workItemId}-${Date.now()}`,
      })
      setDeferOpen(false)
      setResult({
        status:
          res.status === "succeeded"
            ? "succeeded"
            : res.status === "blocked"
              ? "blocked"
              : "rejected",
        title:
          res.status === "succeeded"
            ? "本轮已跳过"
            : res.status === "blocked"
              ? "跳过被阻断"
              : "跳过未成功",
        description: res.message,
        reference: res.reference,
        facts: res.data
          ? [
              {
                label: "任务状态",
                value:
                  WORK_ITEM_STATUS_LABEL[res.data.workItemStatus] ??
                  res.data.workItemStatus,
              },
              {
                label: "处理状态",
                value:
                  LEASE_DISPOSITION_LABEL[res.data.leaseDisposition] ??
                  res.data.leaseDisposition,
              },
              {
                label: "原因",
                value:
                  DEFER_REASON_OPTIONS.find(
                    (o) => o.value === res.data?.reasonCode
                  )?.label ?? res.data?.reasonCode ?? "—",
              },
            ]
          : undefined,
      })
    },
  })

  React.useEffect(() => {
    titleRef.current?.focus()
  }, [supplierOrderId, activeSection])

  React.useEffect(() => {
    return () => {
      void clearAddressReveal(supplierOrderId)
    }
  }, [supplierOrderId])

  const setSection = (section: OrderSection) => {
    const params = new URLSearchParams(searchParams.toString())
    if (section === "overview") params.delete("section")
    else params.set("section", section)
    const qs = params.toString()
    router.replace(
      `/supplier-api/orders/${supplierOrderId}${qs ? `?${qs}` : ""}`,
      { scroll: false }
    )
  }

  async function handleQueryResult() {
    if (!detail) return
    if (!detail.allowedActions.includes("QUERY_RESULT")) {
      const blocker = detail.actionBlockers.find(
        (b) => b.action === "QUERY_RESULT"
      )
      setResult({
        status: "blocked",
        title: "无法查询原结果",
        description: blocker?.message ?? "当前不可查询",
        facts: blocker?.destinationWorkspaceId
          ? [
              {
                label: "去向",
                value: (
                  <Link
                    href="/governance/integration-errors"
                    className="text-primary underline-offset-2 hover:underline"
                  >
                    接口错误与对账中心
                  </Link>
                ),
              },
            ]
          : undefined,
      })
      return
    }
    try {
      const res = await queryResultMutation.mutateAsync({
        orderId: supplierOrderId,
        expectedLockVersion: detail.order.lockVersion,
        targetSupplierActionId: detail.placeActionId,
        operationId: `op-query-${Date.now()}`,
        idempotencyKey: `query-center-${supplierOrderId}-${Date.now()}`,
        workItemId: detail.workItem?.workItemId ?? workItemId,
        expectedSubjectHash: detail.workItem?.subjectHash,
        expectedSubjectVersion: detail.workItem?.subjectVersion,
      })
      setResult({
        status:
          res.status === "succeeded"
            ? "succeeded"
            : res.status === "unknown"
              ? "unknown"
              : res.status === "blocked"
                ? "blocked"
                : "rejected",
        title:
          res.status === "succeeded"
            ? "查询原结果已完成"
            : res.status === "unknown"
              ? "查询结果仍未知"
              : "查询未成功",
        description: res.message,
        reference: res.reference,
        facts: res.data
          ? [
              {
                label: "证据结论",
                value: res.data.evidence.outcomeLabel,
              },
              {
                label: "可安全重试",
                value: res.data.evidence.canSafeRetry ? "是" : "否",
              },
              {
                label: "任务状态",
                value: res.data.workItemStatus
                  ? (WORK_ITEM_STATUS_LABEL[res.data.workItemStatus] ??
                    res.data.workItemStatus)
                  : "（非任务入口）",
              },
              {
                label: "说明",
                value: res.data.evidence.summary,
              },
            ]
          : undefined,
      })
    } catch (error) {
      setResult({
        status: "rejected",
        title: "查询未完成",
        description: getErrorMessage(error, "查询失败，请稍后重试"),
      })
    }
  }

  async function handleReplay() {
    if (!detail) return
    try {
      const res = await replayMutation.mutateAsync({
        orderId: supplierOrderId,
        expectedLockVersion: detail.order.lockVersion,
        targetSupplierActionId: detail.placeActionId,
        operationId: `op-replay-${Date.now()}`,
        idempotencyKey: `replay-center-${supplierOrderId}-${Date.now()}`,
        workItemId: detail.workItem?.workItemId ?? workItemId,
        expectedSubjectHash: detail.workItem?.subjectHash,
        expectedSubjectVersion: detail.workItem?.subjectVersion,
      })
      setReplayOpen(false)
      setResult({
        status:
          res.status === "succeeded"
            ? "succeeded"
            : res.status === "blocked"
              ? "blocked"
              : "rejected",
        title: res.status === "succeeded" ? "已安全重发" : "未重发",
        description: res.message,
        reference: res.reference,
        facts: res.data
          ? [
              { label: "外部单号", value: res.data.externalOrderNo ?? "—" },
              {
                label: "履约状态",
                value: FULFILLMENT_STATUS_LABEL[res.data.fulfillmentStatus],
              },
              {
                label: "任务状态",
                value: res.data.workItemStatus
                  ? (WORK_ITEM_STATUS_LABEL[res.data.workItemStatus] ??
                    res.data.workItemStatus)
                  : "（非任务入口）",
              },
              {
                label: "证据",
                value: res.data.evidence.summary,
              },
            ]
          : undefined,
      })
    } catch (error) {
      setResult({
        status: "rejected",
        title: "重发未完成",
        description: getErrorMessage(error, "重发失败，请稍后重试"),
      })
    }
  }

  async function handleAfterSales(
    action: "CANCEL" | "REFUND",
    requestId: string
  ) {
    if (!detail) return
    try {
      const res = await afterSalesMutation.mutateAsync({
        orderId: supplierOrderId,
        expectedLockVersion: detail.order.lockVersion,
        action,
        operationId: `op-as-${action}-${Date.now()}`,
        idempotencyKey: `as-${action}-${requestId}`,
        afterSalesRequestId: requestId,
      })
      setAfterSalesConfirm(null)
      setResult({
        status:
          res.status === "succeeded"
            ? "succeeded"
            : res.status === "blocked"
              ? "blocked"
              : "rejected",
        title:
          res.status === "succeeded"
            ? action === "CANCEL"
              ? "取消已提交"
              : "退款已提交"
            : "售后动作未提交",
        description: res.message,
        reference: res.reference,
        facts: res.data
          ? [
              {
                label: "取消轨",
                value: CANCEL_STATUS_LABEL[res.data.cancelStatus],
              },
              {
                label: "退款轨",
                value: REFUND_STATUS_LABEL[res.data.refundStatus],
              },
              { label: "说明", value: res.data.note },
            ]
          : undefined,
      })
    } catch (error) {
      setResult({
        status: "rejected",
        title: "售后动作未提交",
        description: getErrorMessage(error, "提交失败，请稍后重试"),
      })
    }
  }

  async function handleReveal() {
    if (!detail) return
    try {
      const res = await revealMutation.mutateAsync({
        orderId: supplierOrderId,
        reason: "履约处理需要核对收货信息",
      })
      setResult({
        status: res.status === "succeeded" ? "succeeded" : "blocked",
        title: res.status === "succeeded" ? "已短时揭示地址" : "无法揭示",
        description: res.message,
      })
    } catch (error) {
      setResult({
        status: "rejected",
        title: "地址揭示失败",
        description: getErrorMessage(error, "操作失败，请稍后重试"),
      })
    }
  }

  if (query.isPending) {
    return (
      <PageScaffold>
        <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
        <div className="h-28 animate-pulse rounded-lg bg-muted" />
        <div className="h-64 animate-pulse rounded-lg bg-muted" />
      </PageScaffold>
    )
  }

  if (query.isError) {
    return (
      <PageScaffold>
        <BusinessFailureState
          title="供应商订单加载失败"
          error={query.error}
          action={
            <Button type="button" onClick={() => void query.refetch()}>
              重试
            </Button>
          }
        />
      </PageScaffold>
    )
  }

  if (!detail) {
    return (
      <PageScaffold>
        <Alert variant="warning">
          <AlertTitle>未找到供应商订单</AlertTitle>
          <AlertDescription>
            该订单不存在或当前角色无权访问。
            <Button
              type="button"
              variant="link"
              className="px-1"
              render={<Link href="/supplier-api/orders" />}
            >
              返回列表
            </Button>
          </AlertDescription>
        </Alert>
      </PageScaffold>
    )
  }

  const o = detail.order
  const canQuery = detail.allowedActions.includes("QUERY_RESULT")
  const canReplay = detail.allowedActions.includes("REPLAY")
  const canReveal = detail.allowedActions.includes("REVEAL_ADDRESS")
  const isResultUnknown = o.fulfillmentStatus === "RESULT_UNKNOWN"
  const noQueryCapability = detail.actionBlockers.some(
    (b) => b.action === "QUERY_RESULT" && b.code === "NO_QUERY_CAPABILITY"
  )
  const totalQuantity = detail.items.reduce(
    (acc, item) => acc + Number(item.quantity || 0),
    0
  )
  const totalCostGross =
    detail.items.every((item) => item.unitCostGross == null)
      ? null
      : detail.items
          .reduce(
            (acc, item) =>
              acc + Number(item.quantity || 0) * Number(item.unitCostGross ?? 0),
            0
          )
          .toFixed(2)

  return (
    <PageScaffold>
      <PageHeader
        variant="object-chrome"
        breadcrumbs={[
          { id: "list", label: "供应商订单", href: "/supplier-api/orders" },
          {
            id: "order",
            label: (
              <span ref={titleRef} tabIndex={-1} className="outline-none">
                {o.orderNo}
              </span>
            ),
            current: true,
          },
        ]}
        actions={
          from === "mall-order" && sourceId ? (
            <Button
              type="button"
              size="sm"
              variant="outline"
              render={
                <Link
                  href={`/commerce/consumption-orders?q=${encodeURIComponent(o.mallOrderNo)}`}
                />
              }
            >
              <ArrowLeftIcon className="size-3.5" />
              返回商城订单
            </Button>
          ) : (
            <Button
              type="button"
              size="sm"
              variant="outline"
              render={<Link href="/supplier-api/orders" />}
            >
              <ArrowLeftIcon className="size-3.5" />
              返回列表
            </Button>
          )
        }
      />

      <DocumentHeader
        density="compact"
        title={o.supplierName}
        documentNumber={o.orderNo}
        primaryStatus={{
          label: o.fulfillmentLabel,
          tone: o.fulfillmentTone,
        }}
        meta={
          <span className="text-muted-foreground">
            商城单 {o.mallOrderNo}
          </span>
        }
        statuses={[
          {
            id: "supplier",
            label: "供应商",
            status: { label: o.supplierName, tone: "neutral" },
          },
          {
            id: "external",
            label: "外部单号",
            status: {
              label: o.externalOrderNo ?? "尚未返回",
              tone: o.externalOrderNo ? "info" : "neutral",
            },
          },
        ]}
        primaryAction={
          isResultUnknown ? (
            <GuardedBusinessAction
              type="button"
              size="sm"
              disabled={!canQuery || queryResultMutation.isPending}
              reason={
                !canQuery
                  ? (detail.actionBlockers.find(
                      (b) => b.action === "QUERY_RESULT"
                    )?.message ?? "当前不可查询")
                  : undefined
              }
              onClick={() => void handleQueryResult()}
            >
              查询原结果
            </GuardedBusinessAction>
          ) : undefined
        }
        secondaryActions={
          <div className="flex flex-wrap gap-2">
            {isResultUnknown ? (
              <GuardedBusinessAction
                type="button"
                size="sm"
                variant="outline"
                disabled={!canReplay || replayMutation.isPending}
                reason={
                  canReplay
                    ? "已确认无结果，可安全重发"
                    : (detail.actionBlockers.find((b) => b.action === "REPLAY")
                        ?.message ?? "需先查询确认无结果后，方可重发")
                }
                onClick={() => {
                  if (canReplay) setReplayOpen(true)
                }}
              >
                安全重发
              </GuardedBusinessAction>
            ) : null}
            {detail.workItem && detail.allowedActions.includes("DEFER") ? (
              detail.workItem.held ? (
                <>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled
                  >
                    本轮已跳过
                  </Button>
                  <span className="self-center text-xs text-muted-foreground">
                    任务仍待处理，可稍后继续
                  </span>
                </>
              ) : (
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={() => setDeferOpen(true)}
                >
                  先跳过
                </Button>
              )
            ) : null}
            {detail.allowedActions.includes("ESCALATE_W29") ? (
              <Button
                type="button"
                size="sm"
                variant="outline"
                render={<Link href="/governance/integration-errors" />}
              >
                转接口错误中心
                <ExternalLinkIcon className="size-3.5" />
              </Button>
            ) : null}
            <Button
              type="button"
              size="sm"
              variant="ghost"
              render={
                <Link
                  href={`/commerce/consumption-orders?q=${encodeURIComponent(o.mallOrderNo)}`}
                />
              }
            >
              商城 {o.mallOrderNo}
            </Button>
          </div>
        }
      />

      <StatusTrackSummary
        variant="table"
        className="sm:grid-cols-3"
        aria-label="三轨进度"
        tracks={[
          {
            id: "ff",
            label: "履约",
            status: { label: o.fulfillmentLabel, tone: o.fulfillmentTone },
          },
          {
            id: "cancel",
            label: "取消",
            status: { label: o.cancelLabel, tone: o.cancelTone },
          },
          {
            id: "refund",
            label: "退款",
            status: { label: o.refundLabel, tone: o.refundTone },
          },
        ]}
      />

      <Alert variant="info">
        <AlertTitle>商城支付已发生</AlertTitle>
        <AlertDescription className="text-xs leading-relaxed">
          {o.paymentOccurredNotice} 支付凭证{" "}
          <span className="num">{o.paymentFactKey}</span> · 支付时间{" "}
          <span className="num">{formatDateTime(o.paidAt, "fullIntl", "passthrough")}</span>
        </AlertDescription>
      </Alert>

      {isResultUnknown ? (
        <Alert variant="warning" aria-live="polite">
          <TriangleAlertIcon />
          <AlertTitle>结果未知 — 请先查询原结果</AlertTitle>
          <AlertDescription className="text-xs leading-relaxed">
            不得把结果未知直接改成成功，也不得在未查询前直接再次下单。
            {detail.lastInvestigation ? (
              <span className="mt-1 block">
                最近查询：{detail.lastInvestigation.outcomeLabel} —{" "}
                {detail.lastInvestigation.summary}
                {detail.lastInvestigation.canSafeRetry
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

      {o.fulfillmentStatus === "COMPLETED" &&
      o.refundStatus === "PARTIAL" ? (
        <Alert variant="info">
          <AlertTitle>已完成 + 部分退款</AlertTitle>
          <AlertDescription className="text-xs">
            履约与退款状态独立记录，互不覆盖
          </AlertDescription>
        </Alert>
      ) : null}

      {detail.workItem ? (
        <Card size="sm" className={surfacePanelClassName}>
          <CardHeader className="rounded-t-lg border-b border-border/30 pb-2">
            <CardTitle className="text-sm">
              {WORK_ITEM_TYPE_LABEL[detail.workItem.workItemType]}
            </CardTitle>
            <CardDescription className="text-xs">
              关联订单 {o.orderNo}
              {detail.workItem.held ? " · 本轮已跳过，任务仍待处理" : ""}
            </CardDescription>
          </CardHeader>
          <CardContent className="flex flex-wrap gap-3 text-xs text-muted-foreground">
            <span>
              状态{" "}
              <BusinessStatusBadge
                context="detail"
                label={
                  WORK_ITEM_STATUS_LABEL[detail.workItem.workItemStatus] ??
                  detail.workItem.workItemStatus
                }
                tone={
                  detail.workItem.workItemStatus === "COMPLETED"
                    ? "success"
                    : "info"
                }
              />
            </span>
            <span>完成动作须另行确认处理结果</span>
          </CardContent>
        </Card>
      ) : null}

      {result ? (
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
                onClick={() => setResult(null)}
              >
                关闭结果
              </Button>
              {o.fulfillmentStatus === "COMPLETED" ||
              detail.costs.settlementId ? (
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  render={
                    <Link
                      href={
                        detail.costs.settlementId
                          ? `/supplier-api/settlements?q=${encodeURIComponent(detail.costs.settlementNo ?? "")}`
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
                    href={`/commerce/consumption-orders?q=${encodeURIComponent(o.mallOrderNo)}`}
                  />
                }
              >
                返回商城消费订单
              </Button>
            </div>
          }
        />
      ) : null}

      <div className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}>
      <Tabs
        value={activeSection}
        onValueChange={(v) => setSection(resolveSection(v))}
      >
        <TabsList
          variant="line"
          className="sticky top-0 z-10 h-auto w-full flex-wrap justify-start gap-1 overflow-x-auto rounded-none border-b border-border/30 bg-card/95 px-3 py-1.5 backdrop-blur supports-backdrop-filter:bg-card/80"
        >
          {SECTIONS.map((s) => (
            <TabsTrigger key={s} value={s}>
              {SECTION_LABEL[s]}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      <div className="space-y-4 p-3 md:p-4">
      {activeSection === "overview" ? (
        <DocumentSection title="概览" description="来源支付、供应商与下单记录版本">
          <DescriptionList className="gap-y-3">
            <Item label="履约链" value="ERP 自动供应商履约" />
            <Item label="供应商" value={o.supplierName} />
            <Item
              label="连接"
              value={`${o.connectionCode} · ${o.connectionEnvironment}`}
            />
            <Item
              label="供给数据版本"
              value={<span className="num">{codeVersion(o.supplyVersion)}</span>}
            />
            <Item
              label="发布数据版本"
              value={
                <span className="num">{codeVersion(o.publicationVersion)}</span>
              }
            />
            <Item
              label="支付凭证号"
              value={<span className="num">{o.paymentFactKey}</span>}
            />
          </DescriptionList>
          <p className="mt-3 text-xs text-muted-foreground">
            发布版本、供给、商品与成本在下单时固定，不受后续基础资料变化影响。
          </p>
        </DocumentSection>
      ) : null}

      {activeSection === "items" ? (
        <DocumentSection
          title="商品明细"
          description="一条商城明细只属于一个供应商子订单；下单记录不可改供应商"
        >
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>商品</TableHead>
                <TableHead>数量</TableHead>
                <TableHead>供应商订货编码</TableHead>
                <TableHead>发布/供给版本</TableHead>
                <TableHead className="text-right">下单成本（含税）</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {detail.items.map((item) => (
                <TableRow key={item.itemId}>
                  <TableCell>
                    <div className="font-medium">{item.productName}</div>
                    <div className="num text-xs text-muted-foreground">
                      {item.skuCode}
                    </div>
                    <Badge variant="secondary" className="mt-1 text-2xs">
                      下单记录不可变
                    </Badge>
                  </TableCell>
                  <TableCell className="num">
                    {item.quantity} {item.unit}
                  </TableCell>
                  <TableCell>
                    <div className="text-xs">{item.supplierProductName}</div>
                    <div className="num text-tiny text-muted-foreground">
                      {item.supplierProductId}
                    </div>
                  </TableCell>
                  <TableCell className="num text-xs">
                    {codeVersion(item.publicationVersion)} /{" "}
                    {codeVersion(item.supplyVersion)}
                  </TableCell>
                  <TableCell className="text-right">
                    {item.unitCostGross != null ? (
                      <MoneyValue
                        value={item.unitCostGross}
                        taxBasis="gross"
                      />
                    ) : (
                      <span className="text-muted-foreground">•••</span>
                    )}
                  </TableCell>
                </TableRow>
              ))}
              {detail.items.length > 0 ? (
                <TableRow className="border-t-2 border-border font-medium">
                  <TableCell>合计</TableCell>
                  <TableCell className="num">
                    {totalQuantity} {detail.items[0].unit}
                  </TableCell>
                  <TableCell />
                  <TableCell />
                  <TableCell className="text-right">
                    {totalCostGross != null ? (
                      <MoneyValue value={totalCostGross} taxBasis="gross" />
                    ) : (
                      <span className="text-muted-foreground">•••</span>
                    )}
                  </TableCell>
                </TableRow>
              ) : null}
            </TableBody>
          </Table>
        </DocumentSection>
      ) : null}

      {activeSection === "fulfillment" ? (
        <DocumentSection title="履约与物流" description="接单、发货与地址">
          <DescriptionList className="mb-4 gap-y-3">
            <Item
              label="接单时间"
              value={
                detail.logistics.acceptedAt
                  ? formatDateTime(detail.logistics.acceptedAt, "fullIntl", "passthrough")
                  : "—"
              }
            />
            <Item
              label="发货时间"
              value={
                detail.logistics.shippedAt
                  ? formatDateTime(detail.logistics.shippedAt, "fullIntl", "passthrough")
                  : "—"
              }
            />
            <Item
              label="完成时间"
              value={
                detail.logistics.completedAt
                  ? formatDateTime(detail.logistics.completedAt, "fullIntl", "passthrough")
                  : "—"
              }
            />
            <Item label="承运商" value={detail.logistics.carrier ?? "—"} />
            <Item
              label="物流号"
              value={
                detail.logistics.trackingNo ? (
                  <span className="num">{detail.logistics.trackingNo}</span>
                ) : (
                  "—"
                )
              }
            />
          </DescriptionList>

          <Card size="sm" className={cn(surfaceInsetClassName, "shadow-none ring-0")}>
            <CardHeader className="rounded-t-lg border-b border-border/30 pb-2">
              <CardTitle className="text-sm">收货信息</CardTitle>
              <CardDescription className="text-xs">
                默认打码；仅履约所需角色可短时揭示，揭示写入审计。
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-2 text-sm">
              <div>
                收件人：
                {detail.address.recipientRevealed ??
                  detail.address.recipientMasked}
              </div>
              <div>
                手机：
                <span className="num">
                  {detail.address.phoneRevealed ?? detail.address.phoneMasked}
                </span>
              </div>
              <div>
                地址：
                {detail.address.revealed ?? detail.address.masked}
              </div>
              {detail.address.auditNote ? (
                <p className="text-xs text-muted-foreground">
                  {detail.address.auditNote}
                </p>
              ) : null}
              <div className="flex gap-2 pt-1">
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={!canReveal || revealMutation.isPending}
                  onClick={() => void handleReveal()}
                >
                  <EyeIcon className="size-3.5" />
                  短时揭示
                </Button>
                {detail.address.revealed ? (
                  <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    onClick={() => {
                      void clearAddressReveal(supplierOrderId).then(() =>
                        void query.refetch()
                      )
                    }}
                  >
                    <EyeOffIcon className="size-3.5" />
                    立即隐藏
                  </Button>
                ) : null}
              </div>
            </CardContent>
          </Card>

          <Separator className="my-4" />
          <h4 className="mb-2 text-xs font-semibold">状态历史</h4>
          {detail.statusHistory.length === 0 ? (
            <p className="text-sm text-muted-foreground">暂无状态历史</p>
          ) : (
            <ul className="space-y-2">
              {detail.statusHistory.map((h) => (
                <li
                  key={h.id}
                  className={cn(surfaceInsetClassName, "px-3 py-2 text-xs")}
                >
                  <div className="flex flex-wrap gap-2">
                    <Badge variant="secondary">{h.track}</Badge>
                    <span>
                      {h.fromLabel} → {h.toLabel}
                    </span>
                    <span className="text-muted-foreground">
                      {formatDateTime(h.at, "fullIntl", "passthrough")} · {h.source}
                    </span>
                  </div>
                  {h.note ? (
                    <p className="mt-1 text-muted-foreground">{h.note}</p>
                  ) : null}
                </li>
              ))}
            </ul>
          )}
        </DocumentSection>
      ) : null}

      {activeSection === "aftersales" ? (
        <DocumentSection
          title="售后"
          description="商城售后请求 + 商城退款 / 余额恢复 / 供应商退款三类记录分别展示"
        >
          {detail.afterSales.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              暂无商城售后请求。取消与退款必须引用既有请求，禁止任意创建。
            </p>
          ) : (
            <div className="space-y-4">
              {detail.afterSales.map((as) => (
                <Card
                  key={as.requestId}
                  size="sm"
                  className={cn(surfaceInsetClassName, "shadow-none ring-0")}
                >
                  <CardHeader className="rounded-t-lg border-b border-border/30 pb-2">
                    <CardTitle className="text-sm">
                      {as.requestNo}{" "}
                      <span className="num font-normal text-muted-foreground">
                        · {as.mallRequestRef}
                      </span>
                    </CardTitle>
                    <CardDescription className="text-xs">
                      {as.scope} · 申请于 {formatDateTime(as.requestedAt, "fullIntl", "passthrough")}
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="space-y-3">
                    <div className="grid gap-2 sm:grid-cols-3">
                      <FactGap
                        title="商城退款"
                        status={as.mallRefund.statusLabel}
                        amount={as.mallRefund.amount}
                        gap={as.mallRefund.gapNote}
                      />
                      <FactGap
                        title="余额/卡券恢复"
                        status={as.cardRestore.statusLabel}
                        gap={as.cardRestore.gapNote}
                      />
                      <FactGap
                        title="供应商退款"
                        status={as.supplierRefund.statusLabel}
                        amount={as.supplierRefund.amount}
                        gap={as.supplierRefund.gapNote}
                      />
                    </div>
                    <div className="flex flex-wrap gap-2">
                      <GuardedBusinessAction
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={
                          !as.allowedActions.includes("CANCEL") ||
                          afterSalesMutation.isPending
                        }
                        reason={
                          as.actionBlockers.find(
                            (b) => b.action === "CANCEL"
                          )?.message
                        }
                        onClick={() =>
                          setAfterSalesConfirm({
                            requestId: as.requestId,
                            requestNo: as.requestNo,
                            mallRequestRef: as.mallRequestRef,
                            action: "CANCEL",
                          })
                        }
                      >
                        提交取消
                      </GuardedBusinessAction>
                      <GuardedBusinessAction
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={
                          !as.allowedActions.includes("REFUND") ||
                          afterSalesMutation.isPending
                        }
                        reason={
                          as.actionBlockers.find(
                            (b) => b.action === "REFUND"
                          )?.message
                        }
                        onClick={() =>
                          setAfterSalesConfirm({
                            requestId: as.requestId,
                            requestNo: as.requestNo,
                            mallRequestRef: as.mallRequestRef,
                            action: "REFUND",
                          })
                        }
                      >
                        提交退款
                      </GuardedBusinessAction>
                    </div>
                    <p className="text-tiny text-muted-foreground">
                      领域动作引用售后请求 {as.mallRequestRef}
                      ，重复提交返回原结果；不读写任务。
                    </p>
                  </CardContent>
                </Card>
              ))}
            </div>
          )}
        </DocumentSection>
      ) : null}

      {activeSection === "costs" ? (
        <DocumentSection
          title="成本与结算"
          description="金额按含税/不含税分别标注"
        >
          <DescriptionList className="gap-y-3">
            <Item
              label="累计成本（含税）"
              value={
                detail.costs.cumulativeCostGross == null ? (
                  <span className="text-muted-foreground">—</span>
                ) : (
                  <MoneyValue
                    value={detail.costs.cumulativeCostGross}
                    taxBasis="gross"
                  />
                )
              }
            />
            <Item
              label="累计成本（不含税）"
              value={
                detail.costs.cumulativeCostNet == null ? (
                  <span className="text-muted-foreground">—</span>
                ) : (
                  <MoneyValue
                    value={detail.costs.cumulativeCostNet}
                    taxBasis="net"
                  />
                )
              }
            />
            <Item label="成本来源" value={detail.costs.costSource} />
            <Item
              label="成本差额"
              value={
                detail.costs.costVariance == null ? (
                  "—"
                ) : (
                  <MoneyValue value={detail.costs.costVariance} />
                )
              }
            />
            <Item
              label="差额参照"
              value={
                detail.costs.cumulativeCostGross == null
                  ? "—"
                  : `对比累计成本（含税）${detail.costs.cumulativeCostGross}`
              }
            />
            <Item
              label="所属结算单"
              value={
                detail.costs.settlementNo ? (
                  <Link
                    href={`/supplier-api/settlements?q=${encodeURIComponent(detail.costs.settlementNo)}`}
                    className="num text-primary underline-offset-2 hover:underline"
                  >
                    {detail.costs.settlementNo}
                  </Link>
                ) : (
                  "—"
                )
              }
            />
            <Item
              label="应付入口"
              value={detail.costs.payableEntryLabel ?? "—"}
            />
          </DescriptionList>
          <div className="mt-4 flex flex-wrap gap-2">
            <Button
              type="button"
              size="sm"
              variant="outline"
              render={<Link href="/supplier-api/settlements" />}
            >
              打开 API 结算
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              render={<Link href="/finance/supplier-accounts" />}
            >
              供应商往来
            </Button>
          </div>
        </DocumentSection>
      ) : null}

      {activeSection === "audit" ? (
        <DocumentSection
          title="动作与审计"
          description="不展示密钥、完整消息内容或敏感地址"
        >
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>动作</TableHead>
                <TableHead>结果</TableHead>
                <TableHead>操作人</TableHead>
                <TableHead>时间</TableHead>
                <TableHead>任务号尾号</TableHead>
                <TableHead>尝试</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {detail.actions.map((a) => (
                <TableRow key={a.actionId}>
                  <TableCell>
                    <div>{a.actionLabel}</div>
                  </TableCell>
                  <TableCell>
                    <BusinessStatusBadge
                      context="list"
                      label={a.outcomeLabel}
                      tone={a.outcomeTone}
                    />
                  </TableCell>
                  <TableCell className="text-xs">{a.actor}</TableCell>
                  <TableCell className="num text-xs">
                    {formatDateTime(a.at, "fullIntl", "passthrough")}
                  </TableCell>
                  <TableCell className="num text-xs">
                    {a.idempotencyKeyTail}
                  </TableCell>
                  <TableCell className="num">{a.attemptCount}</TableCell>
                </TableRow>
              ))}
              {detail.actions.length === 0 ? (
                <TableRow>
                  <TableCell
                    colSpan={6}
                    className="py-6 text-center text-sm text-muted-foreground"
                  >
                    暂无动作记录
                  </TableCell>
                </TableRow>
              ) : null}
            </TableBody>
          </Table>

          <Separator className="my-4" />
          <form
            className="space-y-2"
            onSubmit={(e) => {
              e.preventDefault()
              void noteForm.handleSubmit()
            }}
          >
            <Label htmlFor="collab-note">记录协同说明</Label>
            <noteForm.AppField
              name="comment"
              children={(field) => (
                <Textarea
                  id="collab-note"
                  value={field.state.value}
                  onChange={(e) => field.handleChange(e.target.value)}
                  onBlur={field.handleBlur}
                  placeholder="不改变订单状态，仅追加审计说明"
                  rows={3}
                />
              )}
            />
            <noteForm.AppForm>
              <noteForm.SubmitButton label="提交说明" />
            </noteForm.AppForm>
          </form>
        </DocumentSection>
      ) : null}
      </div>
      </div>

      <FormalActionConfirmDialog
        open={replayOpen}
        onOpenChange={setReplayOpen}
        actionLabel="安全重发"
        title="确认沿用原任务号重新提交"
        description="仅在确认无结果且系统判定可安全重试时允许。重发不会新建业务订单。"
        fromStatus={{ label: o.fulfillmentLabel, tone: o.fulfillmentTone }}
        toStatus={{ label: "重发后待确认", tone: "info" }}
        effects={[
          `订单 ${o.orderNo}`,
          `供应商 ${o.supplierName}`,
          "沿用原下单任务号",
          "任务保持待处理，不会自动完成",
        ]}
        irreversibleEffects={["将再次向供应商发起下单"]}
        pending={replayMutation.isPending}
        onConfirm={() => handleReplay()}
      />

      <AlertDialog open={deferOpen} onOpenChange={setDeferOpen}>
        <AlertDialogContent className="sm:max-w-md">
          <AlertDialogHeader>
            <AlertDialogTitle>先跳过本轮</AlertDialogTitle>
            <AlertDialogDescription>
              非终结动作：任务不完成、不转交、不会暂停，可稍后继续处理。
            </AlertDialogDescription>
          </AlertDialogHeader>
          <form
            className="space-y-3"
            onSubmit={(e) => {
              e.preventDefault()
              void deferForm.handleSubmit()
            }}
          >
            <div className="space-y-1">
              <Label>原因</Label>
              <deferForm.AppField
                name="reasonCode"
                children={(field) => (
                  <OptionCombobox
                    value={field.state.value}
                    onValueChange={(v) =>
                      field.handleChange(v ?? field.state.value)
                    }
                    options={DEFER_REASON_OPTIONS.map((opt) => ({
                      value: opt.value,
                      label: opt.label,
                    }))}
                    className="w-full"
                    allowClear={false}
                    aria-label="原因"
                    placeholder="选择原因"
                  />
                )}
              />
            </div>
            <div className="space-y-1">
              <Label>说明（可选）</Label>
              <deferForm.AppField
                name="comment"
                children={(field) => (
                  <Textarea
                    value={field.state.value}
                    onChange={(e) => field.handleChange(e.target.value)}
                    rows={2}
                  />
                )}
              />
            </div>
            <AlertDialogFooter>
              <AlertDialogCancel
                type="button"
                onClick={() => setDeferOpen(false)}
              >
                取消
              </AlertDialogCancel>
              <deferForm.AppForm>
                <deferForm.SubmitButton label="确认跳过" />
              </deferForm.AppForm>
            </AlertDialogFooter>
          </form>
        </AlertDialogContent>
      </AlertDialog>

      <FormalActionConfirmDialog
        open={Boolean(afterSalesConfirm)}
        onOpenChange={(open) => {
          if (!open) setAfterSalesConfirm(null)
        }}
        actionLabel={
          afterSalesConfirm?.action === "CANCEL" ? "提交取消" : "提交退款"
        }
        title={
          afterSalesConfirm?.action === "CANCEL"
            ? "确认向供应商提交取消"
            : "确认向供应商提交退款"
        }
        description={
          afterSalesConfirm
            ? `将向供应商发起${
                afterSalesConfirm.action === "CANCEL" ? "取消" : "退款"
              }请求，引用售后请求 ${afterSalesConfirm.mallRequestRef}；重复提交返回原结果。`
            : undefined
        }
        fromStatus={{
          label: "当前状态",
          tone: "neutral",
        }}
        toStatus={{
          label:
            afterSalesConfirm?.action === "CANCEL" ? "取消处理中" : "退款处理中",
          tone: "info",
        }}
        effects={[
          `引用售后请求 ${
            afterSalesConfirm?.mallRequestRef ?? "—"
          }`,
          "重复提交返回原结果，不会重复发起",
        ]}
        irreversibleEffects={[
          `将向供应商发起${
            afterSalesConfirm?.action === "CANCEL" ? "取消" : "退款"
          }请求`,
        ]}
        pending={afterSalesMutation.isPending}
        onConfirm={() => {
          if (afterSalesConfirm) {
            return handleAfterSales(
              afterSalesConfirm.action,
              afterSalesConfirm.requestId
            )
          }
        }}
      />
    </PageScaffold>
  )
}

function Item({
  label,
  value,
}: {
  label: string
  value: React.ReactNode
}) {
  return (
    <DescriptionItem>
      <DescriptionTerm>{label}</DescriptionTerm>
      <DescriptionDetails>{value}</DescriptionDetails>
    </DescriptionItem>
  )
}

function FactGap({
  title,
  status,
  amount,
  gap,
}: {
  title: string
  status: string
  amount?: string | null
  gap?: string
}) {
  return (
    <div className={cn(surfaceInsetClassName, "p-3 text-xs")}>
      <div className="font-medium">{title}</div>
      <div className="mt-1">{status}</div>
      {amount != null && amount !== "" ? (
        <div className="mt-1 num text-muted-foreground">
          {amount}
        </div>
      ) : null}
      {gap ? (
        <p className="mt-2 text-tiny text-warning-soft-foreground">
          缺口：{gap}
        </p>
      ) : (
        <p className="mt-2 text-tiny text-muted-foreground">无可见缺口</p>
      )}
    </div>
  )
}
