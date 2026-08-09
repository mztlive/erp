"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter, useSearchParams } from "next/navigation"
import {
  ArrowLeftIcon,
  ChevronDownIcon,
  ClipboardCheckIcon,
  FilePenLineIcon,
  HistoryIcon,
  PackageIcon,
  ShieldAlertIcon,
  ShieldCheckIcon,
  StoreIcon,
  WalletIcon,
} from "lucide-react"

import {
  BusinessFailureState,
  DocumentHeader,
  DocumentSection,
  DocumentSummary,
  FormalActionConfirmDialog,
  FormalActionResult,
  MetricItem,
  MetricStrip,
  MoneyValue,
  PageActions,
  PageHeader,
  PageScaffold,
  ResponsibilityPanel,
  StatusTrackSummary,
  surfaceInsetClassName,
  surfacePanelClassName,
  type DisplayTime,
  type ResponsibilityTrack,
} from "@/components/business"
import { welfareScenarioLabel } from "@/lib/business-options"
import { formatDateTime } from "@/lib/datetime"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@/components/ui/tabs"
import { AcceptanceWorkspace } from "@/features/sales-orders/acceptance-workspace"
import { CardSalesApprovalPanel } from "@/features/sales-orders/card-sales-approval-panel"
import { CloseConditionsCard } from "@/features/sales-orders/close-conditions-card"
import { ProcurementRejectionCard } from "@/features/sales-orders/procurement-rejection-card"
import { RevisionHistoryCard } from "@/features/sales-orders/revision-history-card"
import { SalesOrderCollaborationCard } from "@/features/execution-projections/collaboration-card"
import {
  useSalesOrderDetailQuery,
  useStartSalesChangeOrderMutation,
} from "@/features/sales-orders/queries"
import type { SalesOrderDetailView } from "@/features/sales-orders/api"
import type { SalesOrderListItem } from "@/features/sales-orders/types"
import {
  NATURE_LABEL,
  ORIGIN_LABEL,
} from "@/features/sales-orders/labels"
import { sumFixed } from "@/lib/fixed-decimal"
import { getErrorPresentation } from "@/lib/api/errors"
import { cn } from "@/lib/utils"

type SectionId =
  | "overview"
  | "acceptance"
  | "procurement-rejection"
  | "approval"
  | "collaboration"
  | "versions"
  | "close"

type FocusTask = {
  id: SectionId
  title: string
  description: string
  actionLabel: string
  tone: "warning" | "info"
}

function resolveSection(section?: string): SectionId {
  if (
    section === "acceptance" ||
    section === "procurement-rejection" ||
    section === "approval" ||
    section === "collaboration" ||
    section === "versions" ||
    section === "close"
  ) {
    return section
  }
  return "overview"
}

function remainingReceivable(gross: string, received: string) {
  try {
    return sumFixed([gross, `-${received}`], {
      maxScale: 2,
      outputScale: 2,
      allowNegative: true,
    })
  } catch {
    return gross
  }
}

function isOpenProcurementRejection(order: SalesOrderListItem) {
  const rejection = order.procurementRejection
  if (!rejection) return false
  return (
    rejection.reviewStatus !== "RESOLVED" &&
    rejection.reviewStatus !== "VOIDED"
  )
}

const WORK_ITEM_STATUS_ZH: Record<string, string> = {
  UNCLAIMED: "待领取",
  CLAIMED: "处理中",
  COMPLETED: "已完成",
}

/** 阶段责任角色中文映射（后端固定码，见 `sales_order/mod.rs` 提交编排）。 */
const STAGE_OWNER_ROLE_LABEL: Record<string, string> = {
  procurement: "采购",
  sales_leader: "销售领导",
  operations: "运营",
}

/** 审核轨进行中的阶段码（草稿/已生效/履约中/已关闭/已作废不在其中）。 */
const PENDING_REVIEW_STAGE_CODES = [
  "awaiting_confirm",
  "awaiting_sales",
  "awaiting_sales_lead",
  "awaiting_ops",
]

function isPendingReviewStage(code: string) {
  return PENDING_REVIEW_STAGE_CODES.includes(code)
}

/** 当前阶段责任人展示文案：有派发待办时按角色+姓名；驳回/低毛利待处理归销售本人。 */
function stageOwnerDisplay(order: SalesOrderDetailView): string {
  if (order.stageOwnerRole) {
    const roleLabel = STAGE_OWNER_ROLE_LABEL[order.stageOwnerRole] ?? order.stageOwnerRole
    return `${roleLabel} · ${order.stageOwnerUserName ?? "待认领"}`
  }
  if (order.primaryStatus.code === "awaiting_sales") {
    return `销售 · ${order.ownerName}`
  }
  return "待分配"
}

/** 当前阶段预计完成时限；未设置时返回 `undefined`（面板自动显示"未设置"）。 */
function stageDueDisplay(order: SalesOrderDetailView): DisplayTime | undefined {
  if (!order.stageDueAt) return undefined
  const iso = new Date(order.stageDueAt * 1000).toISOString()
  return { dateTime: iso, label: formatDateTime(iso, "full") }
}

function resolveFocusTask(
  order: SalesOrderListItem,
  canAccept: boolean
): FocusTask | null {
  if (isOpenProcurementRejection(order) && order.procurementRejection) {
    return {
      id: "procurement-rejection",
      title: "采购未通过，需要你处理",
      description:
        order.procurementRejection.rejectComment ||
        "可以改价后再报采购，或作废本单。",
      actionLabel: "去处理",
      tone: "warning",
    }
  }
  if (order.activeCardSalesApproval) {
    const st =
      WORK_ITEM_STATUS_ZH[order.activeCardSalesApproval.workItemStatus] ??
      order.activeCardSalesApproval.workItemStatus
    return {
      id: "approval",
      title: "卡券销售等审批",
      description: `当前：${st}。审批通过后本单才会生效。`,
      actionLabel: "去审批",
      tone: "info",
    }
  }
  if (canAccept) {
    return {
      id: "acceptance",
      title: "可以做客户验收",
      description: "客户确认完成后，本单才算交付完毕。",
      actionLabel: "去验收",
      tone: "info",
    }
  }
  if (order.activeChangeOrder) {
    return {
      id: "versions",
      title: "有一笔改单还在走",
      description: `状态：${order.activeChangeOrder.statusLabel}（基于 v${order.activeChangeOrder.baseRevisionNo}）。改单生效前，客户仍按当前版本执行。`,
      actionLabel: "看历史版本",
      tone: "warning",
    }
  }
  return null
}

export function SalesOrderDetailPage({
  salesOrderId,
  section,
}: {
  salesOrderId: string
  section?: string
}) {
  const router = useRouter()
  const searchParams = useSearchParams()
  const returnTo = searchParams.get("returnTo")
  const fromWorkspace = searchParams.get("from")
  const query = useSalesOrderDetailQuery(salesOrderId)
  const changeMutation = useStartSalesChangeOrderMutation()

  const activeSection = resolveSection(section)
  const [changeConfirmOpen, setChangeConfirmOpen] = React.useState(false)
  const [result, setResult] = React.useState<{
    status: "succeeded" | "blocked"
    title: string
    description: string
    reference: string
    /** 下一责任岗位/人。 */
    nextResponsible?: string
  } | null>(null)

  const order = query.data
  const canAccept =
    order?.nature === "physical_service" &&
    order.allowedActions.includes("REGISTER_ACCEPTANCE")
  const canStartChange =
    order?.allowedActions.includes("START_SALES_CHANGE") ?? false
  const changeBlocker = order?.actionBlockers.find(
    (b) => b.action === "START_SALES_CHANGE"
  )

  const selectSection = React.useCallback(
    (next: SectionId) => {
      // 保留从队列带来的 returnTo / from，避免切 Tab 丢返回上下文
      const params = new URLSearchParams()
      if (next !== "overview") params.set("section", next)
      if (returnTo) params.set("returnTo", returnTo)
      if (fromWorkspace) params.set("from", fromWorkspace)
      const qs = params.toString()
      router.replace(
        qs
          ? `/sales/orders/${salesOrderId}?${qs}`
          : `/sales/orders/${salesOrderId}`,
        { scroll: false }
      )
    },
    [fromWorkspace, returnTo, router, salesOrderId]
  )

  if (query.isPending) {
    return (
      <PageScaffold>
        <PageHeader title="销售单" description="正在加载详情…" />
        <div className="space-y-3" aria-busy="true" aria-label="加载中">
          <div className="h-16 animate-pulse rounded-lg bg-muted" />
          <div className="h-24 animate-pulse rounded-lg bg-muted" />
          <div className="h-40 animate-pulse rounded-lg bg-muted" />
        </div>
      </PageScaffold>
    )
  }

  if (query.isError) {
    return (
      <PageScaffold>
        <PageHeader title="销售单" />
        <BusinessFailureState
          title="销售单加载失败"
          error={query.error}
          onRetry={() => {
            void query.refetch()
          }}
        />
      </PageScaffold>
    )
  }

  if (!order) {
    return (
      <PageScaffold>
        <PageHeader
          title="销售单不存在"
          description="未找到这张销售单。可能编号有误，或当前角色无权查看。"
          actions={
            <Button render={<Link href="/sales/orders" />}>返回列表</Button>
          }
        />
      </PageScaffold>
    )
  }

  const isCard = order.nature === "card_voucher"
  const baseHref = `/sales/orders/${order.id}`
  const sectionQuery =
    activeSection === "overview" ? "" : `?section=${activeSection}`
  const selfReturn = encodeURIComponent(`${baseHref}${sectionQuery}`)
  const fromQueue =
    Boolean(returnTo) &&
    (fromWorkspace === "W07" ||
      fromWorkspace === "W08" ||
      fromWorkspace === "W09")
  const backHref =
    fromQueue && returnTo ? returnTo : "/sales/orders"
  const backLabel =
    fromWorkspace === "W07"
      ? "返回采购确认"
      : fromWorkspace === "W08"
        ? "返回采购单列表"
        : fromWorkspace === "W09"
          ? "返回履约处理"
          : "返回列表"

  const focusTask = resolveFocusTask(order, Boolean(canAccept))
  const actionableFocusTask =
    focusTask &&
    (focusTask.id === "procurement-rejection" ||
      focusTask.id === "approval" ||
      focusTask.id === "acceptance")
      ? focusTask
      : null
  const currentTaskTrack: ResponsibilityTrack = actionableFocusTask
    ? {
        id: "current-stage",
        label: actionableFocusTask.title,
        status: { label: order.primaryStatus.label, tone: order.primaryStatus.tone },
        description: actionableFocusTask.description,
        owner: stageOwnerDisplay(order),
        dueAt: stageDueDisplay(order),
        action: (
          <Button
            type="button"
            size="sm"
            onClick={() => selectSection(actionableFocusTask.id)}
          >
            {actionableFocusTask.actionLabel}
          </Button>
        ),
      }
    : {
        id: "current-stage",
        label: "等待处理",
        status: { label: order.primaryStatus.label, tone: order.primaryStatus.tone },
        description: "当前不需要你操作，可以在这里看到进度。",
        owner: stageOwnerDisplay(order),
        dueAt: stageDueDisplay(order),
        disabledReason: "你只能查看，该事项当前由上面的责任人处理。",
      }
  const openRejection = isOpenProcurementRejection(order)
  const receivableLeft = remainingReceivable(
    order.amountGross,
    order.receivedAmount
  )

  const navItems: {
    id: SectionId
    label: string
    show: boolean
    group: "document" | "work" | "reference"
    icon?: React.ComponentType<{ className?: string; "data-icon"?: string }>
  }[] = [
    {
      id: "overview",
      label: "本单内容",
      show: true,
      group: "document",
    },
    {
      id: "procurement-rejection",
      label: "采购未通过",
      show: Boolean(order.procurementRejection),
      group: "work",
      icon: ShieldAlertIcon,
    },
    {
      id: "approval",
      label: "卡券审批",
      show: Boolean(order.activeCardSalesApproval),
      group: "work",
      icon: ShieldCheckIcon,
    },
    {
      id: "acceptance",
      label: "客户验收",
      show: order.nature === "physical_service",
      group: "work",
      icon: ClipboardCheckIcon,
    },
    {
      id: "close",
      label: "进度与结案",
      show: true,
      group: "reference",
    },
    {
      id: "collaboration",
      label: "商城对接",
      show: isCard,
      group: "reference",
      icon: StoreIcon,
    },
    {
      id: "versions",
      label: "历史版本",
      show: true,
      group: "reference",
      icon: HistoryIcon,
    },
  ]

  const visibleNav = navItems.filter((item) => item.show)

  const primaryTaskAction = actionableFocusTask ? (
    <Button
      type="button"
      size="sm"
      onClick={() => selectSection(actionableFocusTask.id)}
    >
      {actionableFocusTask.actionLabel}
    </Button>
  ) : null

  return (
    <PageScaffold>
      <PageHeader
        variant="object-chrome"
        breadcrumbs={[
          { id: "sales", label: "销售", href: "/sales/orders" },
          { id: "orders", label: "销售单", href: "/sales/orders" },
          {
            id: "detail",
            label: order.documentNumber,
            current: true,
          },
        ]}
        metadata={
          fromQueue ? (
            <span>
              {fromWorkspace === "W09"
                ? "从履约处理打开 · 处理完可点返回，回到列表原位"
                : fromWorkspace === "W08"
                  ? "从采购单打开 · 处理完可点返回，回到列表原位"
                  : "从采购确认打开 · 处理完可点返回，回到列表原位"}
            </span>
          ) : undefined
        }
        actions={
          <PageActions
            actions={[
              {
                actionKey: "back",
                label: backLabel,
                icon: ArrowLeftIcon,
                variant: "outline",
                render: <Link href={backHref} />,
              },
            ]}
          />
        }
      />

      <DocumentHeader
        density="compact"
        title={order.customerName}
        documentNumber={order.documentNumber}
        version={order.version}
        primaryStatus={order.primaryStatus}
        meta={
          <span className="inline-flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
            <Badge variant="secondary" className="font-normal">
              {NATURE_LABEL[order.nature]}
            </Badge>
            <span aria-hidden="true">·</span>
            <span>
              负责人{" "}
              <span className="font-medium text-foreground">
                {order.ownerName}
              </span>
            </span>
            {isCard ? (
              <>
                <span aria-hidden="true">·</span>
                <span>到期算交付完成</span>
              </>
            ) : (
              <>
                <span aria-hidden="true">·</span>
                <span>客户验收后算交付完成</span>
              </>
            )}
          </span>
        }
        statuses={[
          { id: "fulfillment", label: "交付", status: order.fulfillment },
          { id: "collection", label: "回款", status: order.collection },
          { id: "invoicing", label: "开票", status: order.invoicing },
        ]}
        secondaryActions={
          <div className="flex flex-wrap items-center gap-2">
            <Button
              type="button"
              size="sm"
              variant="outline"
              render={
                <Link
                  href={`/finance/customer-accounts?view=receivable&salesOrderId=${encodeURIComponent(order.id)}&q=${encodeURIComponent(order.documentNumber)}&from=W05&returnTo=${selfReturn}`}
                />
              }
            >
              <WalletIcon data-icon="inline-start" aria-hidden="true" />
              记一笔回款
            </Button>
            {!isCard ? (
              <Button
                type="button"
                size="sm"
                variant="outline"
                render={
                  <Link
                    href={`/fulfillment?scope=mine&salesOrderId=${order.id}&from=W05&returnTo=${selfReturn}`}
                  />
                }
              >
                <PackageIcon data-icon="inline-start" aria-hidden="true" />
                去发货/交付
              </Button>
            ) : null}
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={!canStartChange || changeMutation.isPending}
              title={
                !canStartChange
                  ? (changeBlocker?.reason ?? "当前不能改单")
                  : undefined
              }
              onClick={() => setChangeConfirmOpen(true)}
            >
              <FilePenLineIcon data-icon="inline-start" aria-hidden="true" />
              发起改单
            </Button>
          </div>
        }
        primaryAction={primaryTaskAction}
      />

      {/* 改单进行中是独立轨道，不并入"当前要办"（不是新增/驳回类待办） */}
      {focusTask && focusTask.id === "versions" ? (
        <Alert variant="warning">
          <ShieldAlertIcon aria-hidden="true" />
          <AlertTitle>{focusTask.title}</AlertTitle>
          <AlertDescription className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
            <span>{focusTask.description}</span>
            {activeSection !== focusTask.id ? (
              <Button
                type="button"
                size="sm"
                className="shrink-0 self-start"
                onClick={() => selectSection(focusTask.id)}
              >
                {focusTask.actionLabel}
              </Button>
            ) : null}
          </AlertDescription>
        </Alert>
      ) : null}

      {/* 当前要办：阶段+责任人+时限+说明+主动作；非责任人只读展示 */}
      {isPendingReviewStage(order.primaryStatus.code) ? (
        <ResponsibilityPanel title="当前要办" tracks={[currentTaskTrack]} />
      ) : null}

      {order.commercialReadOnly ? (
        <Collapsible className={`${surfaceInsetClassName} px-3 py-2`}>
          <CollapsibleTrigger className="group flex w-full items-center justify-between gap-2 text-left text-sm text-muted-foreground hover:text-foreground">
            <span>
              金额和明细不能直接改
            </span>
            <ChevronDownIcon
              aria-hidden="true"
              className="size-4 shrink-0 transition-transform group-aria-expanded:rotate-180"
            />
          </CollapsibleTrigger>
          <CollapsibleContent className="pt-2 text-sm text-muted-foreground">
            <p>
              {order.commercialReadOnlyReason ??
                "已生效的销售单不能在这里直接改价或改明细；需要改请点「发起改单」。"}
            </p>
            <p className="mt-1 text-xs">
              {ORIGIN_LABEL[order.originSystem]}
              {" · "}
              订单类型建单后不能改
            </p>
            {!canStartChange && changeBlocker ? (
              <p className="mt-1 text-xs">
                暂时不能改单：{changeBlocker.reason}
              </p>
            ) : null}
          </CollapsibleContent>
        </Collapsible>
      ) : !canStartChange && changeBlocker ? (
        <p className="text-xs text-muted-foreground">
          暂时不能改单：{changeBlocker.reason}
        </p>
      ) : null}

      <MetricStrip
        columns={4}
        density="compact"
        aria-label="销售单金额摘要"
      >
        <MetricItem
          density="compact"
          label="成交金额（含税）"
          value={<MoneyValue value={order.amountGross} taxBasis="gross" />}
          detail={NATURE_LABEL[order.nature]}
          detailMode="tooltip"
        />
        <MetricItem
          density="compact"
          label="已回款"
          value={
            <MoneyValue value={order.receivedAmount} taxBasis="gross" />
          }
          detail={order.collection.label}
          detailMode="inline"
        />
        <MetricItem
          density="compact"
          label="待回款"
          value={<MoneyValue value={receivableLeft} taxBasis="gross" />}
          detail={
            order.closeEligibility.receivableSettled ? "已收齐" : undefined
          }
          detailMode="inline"
        />
        <MetricItem
          density="compact"
          label="已开票"
          value={
            <MoneyValue value={order.invoicedAmount} taxBasis="gross" />
          }
          detail="开票不挡结案"
          detailMode="tooltip"
        />
      </MetricStrip>

      {result ? (
        <FormalActionResult
          status={result.status}
          title={result.title}
          description={result.description}
          reference={result.reference}
          facts={[
            { label: "销售单", value: order.documentNumber },
            { label: "客户", value: order.customerName },
            ...(result.nextResponsible
              ? [{ label: "下一步", value: result.nextResponsible }]
              : []),
          ]}
        />
      ) : null}

      <div className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}>
      <Tabs
        value={activeSection}
        onValueChange={(next) => {
          const target = resolveSection(next)
          if (target !== activeSection) selectSection(target)
        }}
      >
        <TabsList
          variant="line"
          className="sticky top-0 z-10 h-auto w-full flex-wrap justify-start gap-1 overflow-x-auto rounded-none border-b border-border/30 bg-card/95 px-3 py-1.5 backdrop-blur supports-backdrop-filter:bg-card/80"
        >
          {visibleNav.map((item) => {
            const Icon = item.icon
            const isActiveWork =
              item.group === "work" &&
              focusTask?.id === item.id &&
              (item.id === "procurement-rejection"
                ? openRejection
                : item.id === "approval"
                  ? Boolean(order.activeCardSalesApproval)
                  : item.id === "acceptance"
                    ? canAccept
                    : false)
            return (
              <TabsTrigger
                key={item.id}
                value={item.id}
                className={cn(
                  "flex-none",
                  isActiveWork && "text-foreground"
                )}
              >
                {Icon ? (
                  <Icon data-icon="inline-start" aria-hidden="true" />
                ) : null}
                {item.label}
                {isActiveWork ? (
                  <Badge
                    variant={
                      item.id === "procurement-rejection"
                        ? "warning"
                        : "info"
                    }
                    className="ml-1 h-5 px-1.5 text-2xs font-normal"
                  >
                    待办
                  </Badge>
                ) : null}
              </TabsTrigger>
            )
          })}
        </TabsList>

        <TabsContent value="overview" className="space-y-4 px-3 pt-4 pb-4 md:px-4">
          <DocumentSection
            title="订单信息"
            description={
              isCard
                ? "以当前版本为准；卡密、玩法等敏感信息不在此展示"
                : "以当前版本为准"
            }
          >
            <DocumentSummary
              columns="three"
              items={[
                {
                  id: "contract",
                  label: "关联合同",
                  value: order.contractRevisionLabel,
                },
                {
                  id: "scene",
                  label: "福利场景",
                  value: welfareScenarioLabel(order.welfareScene),
                },
                {
                  id: "payment",
                  label: "付款条件",
                  value: order.paymentTerms,
                },
                {
                  id: "deadline",
                  label: isCard ? "履约期限（到期交付）" : "履约期限",
                  value: order.fulfillmentDeadline,
                  numeric: true,
                },
                {
                  id: "contact",
                  label: "客户联系人",
                  value: order.customerContact ?? "—",
                },
                {
                  id: "version",
                  label: "当前版本",
                  value: `v${order.version}`,
                  numeric: true,
                },
              ]}
            />
          </DocumentSection>

          <DocumentSection
            title={isCard ? "卡券明细" : "销售明细"}
            description={
              isCard ? "卡券明细一行" : `共 ${order.lineItems.length} 行`
            }
          >
            <div className="overflow-hidden rounded-lg ring-1 ring-foreground/[0.04]">
              <table className="w-full text-sm">
                <thead className="bg-muted/50 text-left">
                  <tr>
                    <th className="px-3 py-2 font-medium">项目</th>
                    <th className="px-3 py-2 font-medium">数量</th>
                    {isCard ? (
                      <th className="px-3 py-2 font-medium">面额 / 形态</th>
                    ) : (
                      <th className="px-3 py-2 font-medium">交付方式</th>
                    )}
                    <th className="px-3 py-2 font-medium text-right">
                      含税金额
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {order.lineItems.map((line) => (
                    <tr key={line.id} className="border-t border-border/30">
                      <td className="px-3 py-2">
                        <div>{line.name}</div>
                        {line.sku ? (
                          <div className="num text-xs text-muted-foreground">
                            {line.sku}
                          </div>
                        ) : null}
                      </td>
                      <td className="num px-3 py-2">
                        {line.quantity} {line.unit}
                      </td>
                      {isCard ? (
                        <td className="px-3 py-2 text-sm">
                          {line.faceValue ? (
                            <MoneyValue value={line.faceValue} />
                          ) : (
                            "—"
                          )}
                          {line.cardForm ? (
                            <span className="mt-0.5 block text-xs text-muted-foreground">
                              {line.cardForm}
                            </span>
                          ) : null}
                        </td>
                      ) : (
                        <td className="px-3 py-2 text-sm text-muted-foreground">
                          <div>{line.fulfillmentMode ?? "—"}</div>
                          {line.dueDate ? (
                            <div className="num mt-0.5 text-xs">
                              {line.dueDate}
                            </div>
                          ) : null}
                        </td>
                      )}
                      <td className="px-3 py-2 text-right">
                        <MoneyValue
                          value={line.amountGross}
                          taxBasis="gross"
                        />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {isCard ? (
              <p className="mt-2 text-xs text-muted-foreground">
                卡号、卡密和完整手机号不在此展示。
              </p>
            ) : null}
          </DocumentSection>

          <DocumentSection
            title="相关业务"
            description="本单关联的采购、交付、回款与发票数量"
          >
            <div className="flex flex-wrap gap-3 text-sm">
              <span className="rounded-md bg-muted px-2.5 py-1.5">
                采购单{" "}
                <span className="num font-medium">
                  {order.related.purchaseOrders}
                </span>
              </span>
              <span className="rounded-md bg-muted px-2.5 py-1.5">
                交付{" "}
                <span className="num font-medium">
                  {order.related.fulfillments}
                </span>
              </span>
              <span className="rounded-md bg-muted px-2.5 py-1.5">
                回款{" "}
                <span className="num font-medium">
                  {order.related.receipts}
                </span>
              </span>
              <span className="rounded-md bg-muted px-2.5 py-1.5">
                开票{" "}
                <span className="num font-medium">
                  {order.related.invoices}
                </span>
              </span>
            </div>
            <p className="mt-3 text-xs text-muted-foreground">
              交付/回款进度与结案说明见「进度与结案」；改过哪些内容见「历史版本」。
              {isCard ? " 与商城的对接情况见「商城对接」。" : null}
            </p>
          </DocumentSection>
        </TabsContent>

        <TabsContent value="procurement-rejection" className="px-3 pt-4 pb-4 md:px-4">
          {order.procurementRejection ? (
            <ProcurementRejectionCard
              order={order}
              rejection={order.procurementRejection}
            />
          ) : (
            <EmptySection message="没有需要处理的采购驳回。" />
          )}
        </TabsContent>

        <TabsContent value="approval" className="px-3 pt-4 pb-4 md:px-4">
          {order.activeCardSalesApproval ? (
            <CardSalesApprovalPanel
              order={order}
              approval={order.activeCardSalesApproval}
            />
          ) : (
            <EmptySection message="当前没有待办的卡券审批。" />
          )}
        </TabsContent>

        <TabsContent value="acceptance" className="px-3 pt-4 pb-4 md:px-4">
          {isCard ? (
            <Alert variant="warning">
              <AlertTitle>卡券单不用做客户验收</AlertTitle>
              <AlertDescription>
                卡券到履约期限即算交付完成。期限{" "}
                {order.fulfillmentDeadline}，结案说明见「进度与结案」。
              </AlertDescription>
            </Alert>
          ) : (
            <AcceptanceWorkspace salesOrderId={order.id} />
          )}
        </TabsContent>

        <TabsContent value="close" className="space-y-4 px-3 pt-4 pb-4 md:px-4">
          <DocumentSection title="当前进度">
            <StatusTrackSummary
              tracks={[
                {
                  id: "fulfillment",
                  label: "交付",
                  status: order.fulfillment,
                },
                {
                  id: "collection",
                  label: "回款",
                  status: order.collection,
                },
                {
                  id: "invoicing",
                  label: "开票",
                  status: order.invoicing,
                },
              ]}
            />
            {isCard ? (
              <p className="mt-3 text-sm text-muted-foreground">
                持卡人消费多少都不影响本单是否算交付完成；到期即可。开票进度单独看，不挡结案。
              </p>
            ) : (
              <p className="mt-3 text-sm text-muted-foreground">
                客户验收完成后才算交付完成。开票进度单独看，不挡结案。
              </p>
            )}
          </DocumentSection>
          <CloseConditionsCard order={order} />
        </TabsContent>

        <TabsContent value="collaboration" className="px-3 pt-4 pb-4 md:px-4">
          {isCard ? (
            <SalesOrderCollaborationCard
              salesOrderId={order.id}
              salesOrderNo={order.documentNumber}
            />
          ) : (
            <EmptySection message="只有卡券销售单会与商城对接。" />
          )}
        </TabsContent>

        <TabsContent value="versions" className="px-3 pt-4 pb-4 md:px-4">
          <RevisionHistoryCard
            revisions={order.revisions}
            currentVersion={order.version}
            contractRevisionLabel={order.contractRevisionLabel}
          />
          {order.activeChangeOrder ? (
            <Alert variant="warning" className="mt-4">
              <FilePenLineIcon aria-hidden="true" />
              <AlertTitle>改单进行中</AlertTitle>
              <AlertDescription>
                {order.activeChangeOrder.statusLabel}（基于 v
                {order.activeChangeOrder.baseRevisionNo}
                ）。
                {order.activeChangeOrder.impactPath === "operations"
                  ? "还需运营确认影响，再由财务复核后生效。"
                  : "还需采购确认交付影响，再由财务复核后生效。"}
              </AlertDescription>
            </Alert>
          ) : null}
        </TabsContent>
      </Tabs>
      </div>

      <FormalActionConfirmDialog
        open={changeConfirmOpen}
        onOpenChange={setChangeConfirmOpen}
        title="发起改单"
        actionLabel="创建改单"
        confirmLabel="确认创建"
        fromStatus={{ label: `当前 v${order.version}`, tone: "success" }}
        toStatus={{ label: "改单草稿", tone: "warning" }}
        lockedFields={["销售单号", "订单类型", "已生效版本"]}
        effects={[
          "生成一笔改单，不改掉当前客户正在执行的版本",
          "已有交付、回款、开票记录都会保留",
          isCard
            ? "卡券：运营确认影响 → 财务复核后新版本生效"
            : "实物/服务：采购确认影响 → 财务复核后新版本生效",
        ]}
        nextDepartment={isCard ? "运营与财务" : "采购与财务"}
        onConfirm={async () => {
          try {
            const change = await changeMutation.mutateAsync({
              salesOrderId: order.id,
              baseRevisionNo: order.version,
              nature: order.nature,
            })
            setResult({
              status: "succeeded",
              title: "改单已创建",
              description: `已进入「${change.statusLabel}」。当前版本对客户仍然有效。`,
              reference: change.id,
              nextResponsible: isCard ? "运营与财务" : "采购与财务",
            })
          } catch (error) {
            const failure = getErrorPresentation(
              error,
              "改单未创建，请刷新后重试。"
            )
            setResult({
              status: "blocked",
              title: failure.title,
              description: failure.description,
              reference: order.documentNumber,
            })
          }
        }}
      />
    </PageScaffold>
  )
}

function EmptySection({ message }: { message: string }) {
  return (
    <div className="rounded-lg border border-dashed border-border px-4 py-8 text-center text-sm text-muted-foreground">
      {message}
    </div>
  )
}
