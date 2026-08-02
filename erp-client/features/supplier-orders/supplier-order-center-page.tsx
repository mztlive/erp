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
  BusinessStatusBadge,
  DocumentHeader,
  DocumentSection,
  FormalActionConfirmDialog,
  FormalActionResult,
  MoneyValue,
  OptionCombobox,
  PageHeader,
  StatusTrackSummary,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
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
  DemoRole,
  OrderSection,
} from "@/features/supplier-orders/types"
import {
  DEFER_REASON_OPTIONS,
  SECTION_LABEL,
  SECTIONS,
} from "@/features/supplier-orders/types"

function resolveSection(raw?: string | null): OrderSection {
  if (raw && (SECTIONS as string[]).includes(raw)) return raw as OrderSection
  return "overview"
}

function formatTime(iso: string): string {
  try {
    return new Intl.DateTimeFormat("zh-CN", {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(iso))
  } catch {
    return iso
  }
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
  const role = (searchParams.get("role") as DemoRole) || "procurement"
  const maskCost =
    searchParams.get("maskCost") === "1" ||
    searchParams.get("maskCost") === "true"
  const noSensitive =
    searchParams.get("noSensitive") === "1" ||
    searchParams.get("noSensitive") === "true"
  const from = searchParams.get("from")
  const sourceId = searchParams.get("sourceId")
  const workItemId = searchParams.get("workItemId") ?? undefined

  const activeSection = resolveSection(
    sectionProp ?? searchParams.get("section")
  )

  const query = useSupplierOrderDetailQuery({
    orderId: supplierOrderId,
    role,
    maskCost,
    noSensitive,
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
        claimToken: "demo-claim",
        leaseVersion: detail.workItem.leaseVersion ?? 1,
        expectedSubjectHash: detail.workItem.subjectHash,
        reasonCode: value.reasonCode,
        comment: value.comment || undefined,
        queueContextId: "queue-w26-demo",
        idempotencyKey: `defer-${detail.workItem.workItemId}-${Date.now()}`,
      })
      setDeferOpen(false)
      setResult({
        status: res.status === "succeeded" ? "succeeded" : "blocked",
        title: res.status === "succeeded" ? "已暂挂本轮" : "暂挂失败",
        description: res.message,
        reference: res.reference,
        facts: res.data
          ? [
              { label: "任务状态", value: res.data.workItemStatus },
              { label: "处理状态", value: res.data.leaseDisposition },
              { label: "原因", value: res.data.reasonCode },
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
      reference: res.reference ?? res.operationId,
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
              value: res.data.workItemStatus ?? "（非任务入口）",
            },
            {
              label: "说明",
              value: res.data.evidence.summary,
            },
          ]
        : undefined,
    })
  }

  async function handleReplay() {
    if (!detail) return
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
      title: res.status === "succeeded" ? "安全重放已完成" : "重放未执行",
      description: res.message,
      reference: res.reference,
      facts: res.data
        ? [
            { label: "外部单号", value: res.data.externalOrderNo ?? "—" },
            {
              label: "履约状态",
              value: res.data.fulfillmentStatus,
            },
            {
              label: "任务状态",
              value: res.data.workItemStatus ?? "（非任务入口）",
            },
            {
              label: "证据",
              value: res.data.evidence.summary,
            },
          ]
        : undefined,
    })
  }

  async function handleAfterSales(
    action: "CANCEL" | "REFUND",
    requestId: string
  ) {
    if (!detail) return
    const res = await afterSalesMutation.mutateAsync({
      orderId: supplierOrderId,
      expectedLockVersion: detail.order.lockVersion,
      action,
      operationId: `op-as-${action}-${Date.now()}`,
      idempotencyKey: `as-${action}-${requestId}`,
      afterSalesRequestId: requestId,
    })
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
            { label: "取消轨", value: res.data.cancelStatus },
            { label: "退款轨", value: res.data.refundStatus },
            { label: "说明", value: res.data.note },
          ]
        : undefined,
    })
  }

  async function handleReveal() {
    if (!detail) return
    const res = await revealMutation.mutateAsync({
      orderId: supplierOrderId,
      reason: "履约处理需要核对收货信息",
    })
    setResult({
      status: res.status === "succeeded" ? "succeeded" : "blocked",
      title: res.status === "succeeded" ? "已短时揭示地址" : "无法揭示",
      description: res.message,
      reference: res.reference,
    })
  }

  if (query.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
        <div className="h-28 animate-pulse rounded-2xl bg-muted" />
        <div className="h-64 animate-pulse rounded-2xl bg-muted" />
      </div>
    )
  }

  if (!detail) {
    return (
      <div className="mx-auto max-w-shell p-5">
        <Alert variant="warning">
          <AlertTitle>未找到供应商订单</AlertTitle>
          <AlertDescription>
            订单 {supplierOrderId} 不存在或无权访问。
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
      </div>
    )
  }

  const o = detail.order
  const canQuery = detail.allowedActions.includes("QUERY_RESULT")
  const canReplay = detail.allowedActions.includes("REPLAY")
  const canReveal = detail.allowedActions.includes("REVEAL_ADDRESS")
  const isResultUnknown = o.fulfillmentStatus === "RESULT_UNKNOWN"

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
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
              render={<Link href="/supplier-api/orders?view=actionable" />}
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
        version={o.lockVersion}
        meta={
          <span className="text-muted-foreground">
            商城单 {o.mallOrderNo}
          </span>
        }
        statuses={[
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
            <Button
              type="button"
              size="sm"
              disabled={!canQuery || queryResultMutation.isPending}
              onClick={() => void handleQueryResult()}
            >
              查询原结果
            </Button>
          ) : undefined
        }
        secondaryActions={
          <div className="flex flex-wrap gap-2">
            {isResultUnknown ? (
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={!canReplay || replayMutation.isPending}
                onClick={() => {
                  if (canReplay) setReplayOpen(true)
                }}
                title={
                  canReplay
                    ? "已确认无结果，可安全重试"
                    : "需先查询确认无结果后，方可重试"
                }
              >
                安全重放
              </Button>
            ) : null}
            {detail.workItem && detail.allowedActions.includes("DEFER") ? (
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() => setDeferOpen(true)}
              >
                暂挂
              </Button>
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
          <span className="num">{formatTime(o.paidAt)}</span>
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
                  ? " · 已开放安全重放"
                  : " · 重放仍关闭"}
              </span>
            ) : (
              <span className="mt-1 block">
                尚未查询。主按钮仅「查询原结果」；重试按钮在确认无结果前保持禁用。
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
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm">关联任务</CardTitle>
            <CardDescription className="text-xs">
              {detail.workItem.workItemType} · {detail.workItem.workItemId}
              {detail.workItem.held ? " · 已暂挂（任务仍待处理）" : ""}
            </CardDescription>
          </CardHeader>
          <CardContent className="flex flex-wrap gap-3 text-xs text-muted-foreground">
            <span>
              状态{" "}
              <BusinessStatusBadge
                context="detail"
                label={detail.workItem.workItemStatus}
                tone={
                  detail.workItem.workItemStatus === "COMPLETED"
                    ? "success"
                    : "info"
                }
              />
            </span>
            <span>完成动作须另行确认可验证终态</span>
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

      <Tabs
        value={activeSection}
        onValueChange={(v) => setSection(resolveSection(v))}
      >
        <TabsList variant="line" className="h-auto flex-wrap">
          {SECTIONS.map((s) => (
            <TabsTrigger key={s} value={s}>
              {SECTION_LABEL[s]}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      {activeSection === "overview" ? (
        <DocumentSection title="概览" description="来源支付、供应商与下单记录版本">
          <DescriptionList className="gap-y-3">
            <Item label="履约链" value="ERP 自动供应商履约" />
            <Item label="供应商" value={o.supplierName} />
            <Item
              label="连接"
              value={`${o.connectionCode} / ${o.connectionEnvironment}`}
            />
            <Item
              label="固定供给版本"
              value={<span className="num">{o.supplyVersion}</span>}
            />
            <Item
              label="发布版本"
              value={<span className="num">{o.publicationVersion}</span>}
            />
            <Item
              label="支付记录键"
              value={<span className="num">{o.paymentFactKey}</span>}
            />
            <Item label="版本" value={String(o.lockVersion)} />
          </DescriptionList>
          <p className="mt-3 text-xs text-muted-foreground">
            发布版本、固定供给、商品与成本在下单时固定，不受后续基础资料变化影响。
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
                <TableHead>供应商商品</TableHead>
                <TableHead>发布/供给</TableHead>
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
                    <Badge variant="secondary" className="mt-1 text-[10px]">
                      下单记录不可变
                    </Badge>
                  </TableCell>
                  <TableCell className="num">
                    {item.quantity} {item.unit}
                  </TableCell>
                  <TableCell>
                    <div className="text-xs">{item.supplierProductName}</div>
                    <div className="num text-[11px] text-muted-foreground">
                      {item.supplierProductId}
                    </div>
                  </TableCell>
                  <TableCell className="num text-xs">
                    {item.publicationVersion} / {item.supplyVersion}
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
                  ? formatTime(detail.logistics.acceptedAt)
                  : "—"
              }
            />
            <Item
              label="发货时间"
              value={
                detail.logistics.shippedAt
                  ? formatTime(detail.logistics.shippedAt)
                  : "—"
              }
            />
            <Item
              label="完成时间"
              value={
                detail.logistics.completedAt
                  ? formatTime(detail.logistics.completedAt)
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

          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm">收货信息</CardTitle>
              <CardDescription className="text-xs">
                默认掩码；仅履约所需角色可短时揭示，揭示写入审计。
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
          <ul className="space-y-2">
            {detail.statusHistory.map((h) => (
              <li
                key={h.id}
                className="rounded-lg border border-border px-3 py-2 text-xs"
              >
                <div className="flex flex-wrap gap-2">
                  <Badge variant="secondary">{h.track}</Badge>
                  <span>
                    {h.fromLabel} → {h.toLabel}
                  </span>
                  <span className="text-muted-foreground">
                    {formatTime(h.at)} · {h.source}
                  </span>
                </div>
                {h.note ? (
                  <p className="mt-1 text-muted-foreground">{h.note}</p>
                ) : null}
              </li>
            ))}
          </ul>
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
                <Card key={as.requestId}>
                  <CardHeader className="pb-2">
                    <CardTitle className="text-sm">
                      {as.requestNo}{" "}
                      <span className="num font-normal text-muted-foreground">
                        · {as.mallRequestRef}
                      </span>
                    </CardTitle>
                    <CardDescription className="text-xs">
                      {as.scope} · 申请于 {formatTime(as.requestedAt)}
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="space-y-3">
                    <div className="grid gap-2 sm:grid-cols-3">
                      <FactGap
                        title="商城退款"
                        status={as.mallRefund.statusLabel}
                        amount={as.mallRefund.amount}
                        gap={as.mallRefund.gapNote}
                        costMasked={detail.costs.costMasked}
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
                        costMasked={detail.costs.costMasked}
                      />
                    </div>
                    <div className="flex flex-wrap gap-2">
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={
                          !as.allowedActions.includes("CANCEL") ||
                          afterSalesMutation.isPending
                        }
                        onClick={() =>
                          void handleAfterSales("CANCEL", as.requestId)
                        }
                      >
                        提交取消
                      </Button>
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={
                          !as.allowedActions.includes("REFUND") ||
                          afterSalesMutation.isPending
                        }
                        onClick={() =>
                          void handleAfterSales("REFUND", as.requestId)
                        }
                      >
                        提交退款
                      </Button>
                    </div>
                    <p className="text-[11px] text-muted-foreground">
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
          description="金额按含税/不含税分别标注；无字段权限时掩码"
        >
          <DescriptionList className="gap-y-3">
            <Item
              label="累计成本（含税）"
              value={
                detail.costs.costMasked ||
                detail.costs.cumulativeCostGross == null ? (
                  <span className="text-muted-foreground">•••</span>
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
                detail.costs.costMasked ||
                detail.costs.cumulativeCostNet == null ? (
                  <span className="text-muted-foreground">•••</span>
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
                detail.costs.costMasked || detail.costs.costVariance == null
                  ? "•••"
                  : detail.costs.costVariance
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
          description="不展示密钥、完整报文或敏感地址"
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
                    {a.techSummary ? (
                      <div className="text-[11px] text-muted-foreground">
                        摘要：{a.techSummary}
                      </div>
                    ) : null}
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
                    {formatTime(a.at)}
                  </TableCell>
                  <TableCell className="num text-xs">
                    {a.idempotencyKeyTail}
                  </TableCell>
                  <TableCell className="num">{a.attemptCount}</TableCell>
                </TableRow>
              ))}
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

      <FormalActionConfirmDialog
        open={replayOpen}
        onOpenChange={setReplayOpen}
        actionLabel="安全重放"
        title="确认沿用原任务号重新提交"
        description="仅在查询明确无结果且服务端确认可安全重试时允许。重放不新建业务订单。"
        fromStatus={{ label: o.fulfillmentLabel, tone: o.fulfillmentTone }}
        toStatus={{ label: "重放后待确认", tone: "info" }}
        effects={[
          `订单 ${o.orderNo}`,
          `供应商 ${o.supplierName}`,
          "沿用原下单任务号",
          "任务保持待处理，不会自动完成",
        ]}
        irreversibleEffects={["将再次调用供应商下单接口"]}
        pending={replayMutation.isPending}
        onConfirm={() => handleReplay()}
      />

      {deferOpen ? (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
          <Card className="w-full max-w-md">
            <CardHeader>
              <CardTitle className="text-base">暂挂 / 本轮跳过</CardTitle>
              <CardDescription className="text-xs">
                非终结动作：任务不完成、不转交、不写 paused。
              </CardDescription>
            </CardHeader>
            <CardContent>
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
                <div className="flex justify-end gap-2">
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => setDeferOpen(false)}
                  >
                    取消
                  </Button>
                  <deferForm.AppForm>
                    <deferForm.SubmitButton label="确认暂挂" />
                  </deferForm.AppForm>
                </div>
              </form>
            </CardContent>
          </Card>
        </div>
      ) : null}
    </div>
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
  costMasked,
}: {
  title: string
  status: string
  amount?: string | null
  gap?: string
  costMasked?: boolean
}) {
  return (
    <div className="rounded-lg border border-border bg-card p-3 text-xs">
      <div className="font-medium">{title}</div>
      <div className="mt-1">{status}</div>
      {amount != null && amount !== "" ? (
        <div className="mt-1 num text-muted-foreground">
          {costMasked ? "•••" : amount}
        </div>
      ) : null}
      {gap ? (
        <p className="mt-2 text-[11px] text-warning-soft-foreground">
          缺口：{gap}
        </p>
      ) : (
        <p className="mt-2 text-[11px] text-muted-foreground">无可见缺口</p>
      )}
    </div>
  )
}
