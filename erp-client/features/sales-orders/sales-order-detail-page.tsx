"use client"

import * as React from "react"
import Link from "next/link"
import { useSearchParams } from "next/navigation"
import {
  ArrowLeftIcon,
  ClipboardCheckIcon,
  FilePenLineIcon,
  HistoryIcon,
  WalletIcon,
  LockIcon,
  ShieldAlertIcon,
  ShieldCheckIcon,
} from "lucide-react"

import {
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
  StatusTrackSummary,
} from "@/components/business"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
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
import {
  NATURE_LABEL,
  ORIGIN_LABEL,
  OWNER_LABEL,
} from "@/mock/sales-orders"

type SectionId =
  | "overview"
  | "acceptance"
  | "procurement-rejection"
  | "approval"
  | "collaboration"
  | "versions"
  | "close"

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

export function SalesOrderDetailPage({
  salesOrderId,
  section,
}: {
  salesOrderId: string
  section?: string
}) {
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

  if (query.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="销售单" description="正在加载对象中心…" />
      </div>
    )
  }

  if (!order) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title="销售单不存在"
          description={`未找到编号为 ${salesOrderId} 的销售单。`}
          actions={
            <Button render={<Link href="/sales/orders" />}>返回列表</Button>
          }
        />
      </div>
    )
  }

  const isCard = order.nature === "card_voucher"
  const baseHref = `/sales/orders/${order.id}`

  const navItems: { id: SectionId; label: string; href: string; show: boolean }[] =
    [
      { id: "overview", label: "概览", href: baseHref, show: true },
      {
        id: "procurement-rejection",
        label: "采购驳回处理",
        href: `${baseHref}?section=procurement-rejection`,
        show: Boolean(order.procurementRejection),
      },
      {
        id: "approval",
        label: "卡券审批",
        href: `${baseHref}?section=approval`,
        show: Boolean(order.activeCardSalesApproval),
      },
      {
        id: "collaboration",
        label: "商城协同",
        href: `${baseHref}?section=collaboration`,
        show: isCard,
      },
      {
        id: "acceptance",
        label: "客户验收",
        href: `${baseHref}?section=acceptance`,
        show: order.nature === "physical_service",
      },
      {
        id: "close",
        label: "关闭条件",
        href: `${baseHref}?section=close`,
        show: true,
      },
      {
        id: "versions",
        label: "版本记录",
        href: `${baseHref}?section=versions`,
        show: true,
      },
    ]

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
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
        actions={
          <PageActions
            actions={[
              {
                actionKey: "back",
                label:
                  returnTo && fromWorkspace === "W07"
                    ? "返回采购确认队列"
                    : returnTo && fromWorkspace === "W09"
                      ? "返回履约作业"
                      : "返回列表",
                icon: ArrowLeftIcon,
                variant: "outline",
                render: (
                  <Link
                    href={
                      returnTo &&
                      (fromWorkspace === "W07" || fromWorkspace === "W09")
                        ? returnTo
                        : "/sales/orders"
                    }
                  />
                ),
              },
              {
                actionKey: "change",
                label: "发起销售变更",
                icon: FilePenLineIcon,
                variant: "outline",
                mobileVisibility: "hide",
                disabled: !canStartChange || changeMutation.isPending,
                onClick: () => setChangeConfirmOpen(true),
              },
              ...(canAccept
                ? [
                    {
                      actionKey: "acceptance",
                      label: "客户验收",
                      icon: ClipboardCheckIcon,
                      mobileVisibility: "hide" as const,
                      variant:
                        activeSection === "acceptance"
                          ? ("default" as const)
                          : ("outline" as const),
                      render: (
                        <Link href={`${baseHref}?section=acceptance`} />
                      ),
                    },
                  ]
                : []),
            ]}
          />
        }
      />

      {returnTo && (fromWorkspace === "W07" || fromWorkspace === "W09") ? (
        <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border bg-card px-3 py-2 text-sm">
          <span className="text-muted-foreground">
            {fromWorkspace === "W09"
              ? "自履约作业打开 · 关闭后可返回原队列位置、类型与筛选"
              : "自采购二次确认队列打开 · 关闭后可返回原队列位置与筛选"}
          </span>
          <Button type="button" size="sm" variant="outline" render={<Link href={returnTo} />}>
            {fromWorkspace === "W09" ? "返回履约作业队列" : "返回二次确认队列"}
          </Button>
        </div>
      ) : null}

      <DocumentHeader
        density="compact"
        title={order.customerName}
        documentNumber={order.documentNumber}
        version={order.version}
        primaryStatus={order.primaryStatus}
        statuses={[
          { id: "fulfillment", label: "履约", status: order.fulfillment },
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
                  href={`/finance/customer-accounts?view=receivable&salesOrderId=${encodeURIComponent(order.id)}&q=${encodeURIComponent(order.documentNumber)}&from=W05&returnTo=${encodeURIComponent(
                    `${baseHref}${activeSection === "overview" ? "" : `?section=${activeSection}`}`
                  )}`}
                />
              }
            >
              <WalletIcon data-icon="inline-start" aria-hidden="true" />
              登记回款
            </Button>
            {order.nature === "physical_service" ? (
              <Button
                type="button"
                size="sm"
                variant="outline"
                render={
                  <Link
                    href={`/fulfillment?scope=mine&salesOrderId=${order.id}&from=W05&returnTo=${encodeURIComponent(
                      `${baseHref}${activeSection === "overview" ? "" : `?section=${activeSection}`}`
                    )}`}
                  />
                }
              >
                去履约
              </Button>
            ) : null}
          </div>
        }
        primaryAction={
          order.procurementRejection &&
          order.procurementRejection.reviewStatus !== "RESOLVED" &&
          order.procurementRejection.reviewStatus !== "VOIDED" ? (
            <Button
              type="button"
              size="sm"
              render={
                <Link href={`${baseHref}?section=procurement-rejection`} />
              }
            >
              处理采购驳回
            </Button>
          ) : order.activeCardSalesApproval ? (
            <Button
              type="button"
              size="sm"
              render={<Link href={`${baseHref}?section=approval`} />}
            >
              处理卡券审批
            </Button>
          ) : canAccept ? (
            <Button
              type="button"
              size="sm"
              render={<Link href={`${baseHref}?section=acceptance`} />}
            >
              进入验收作业
            </Button>
          ) : (
            <Button
              type="button"
              size="sm"
              variant="outline"
              render={<Link href={`${baseHref}?section=close`} />}
            >
              查看关闭条件
            </Button>
          )
        }
      />

      <div className="flex flex-wrap items-center gap-2">
        <Badge variant="secondary">{NATURE_LABEL[order.nature]}</Badge>
        {order.natureLocked ? (
          <Badge variant="outline">
            <LockIcon data-icon="inline-start" aria-hidden="true" />
            业务性质已锁定
          </Badge>
        ) : null}
        <Badge variant="outline">{ORIGIN_LABEL[order.originSystem]}</Badge>
        <Badge variant={order.ownerSystem === "erp" ? "info" : "secondary"}>
          {OWNER_LABEL[order.ownerSystem]}
        </Badge>
        {order.originSystem !== order.ownerSystem ? (
          <Badge variant="warning">主责已迁移 · 身份未变</Badge>
        ) : null}
        {order.commercialReadOnly ? (
          <Badge variant="secondary">商业字段只读</Badge>
        ) : null}
        {order.activeChangeOrder ? (
          <Badge variant="warning">
            变更中 {order.activeChangeOrder.id}
          </Badge>
        ) : null}
      </div>

      {order.commercialReadOnlyReason ? (
        <Alert variant="info">
          <LockIcon aria-hidden="true" />
          <AlertTitle>写入主责与只读边界</AlertTitle>
          <AlertDescription>
            {order.commercialReadOnlyReason}
            {" "}创建来源 {ORIGIN_LABEL[order.originSystem]}
            ，当前唯一写入主责 {OWNER_LABEL[order.ownerSystem]}
            。已生效单无直接编辑入口。
          </AlertDescription>
        </Alert>
      ) : null}

      {!canStartChange && changeBlocker ? (
        <p className="text-xs text-muted-foreground">
          销售变更不可用：{changeBlocker.reason}
        </p>
      ) : null}

      <MetricStrip columns={4} aria-label="销售单摘要指标">
        <MetricItem
          label="成交金额（含税）"
          value={<MoneyValue value={order.amountGross} taxBasis="gross" />}
          detail={NATURE_LABEL[order.nature]}
        />
        <MetricItem
          label="已回款"
          value={<MoneyValue value={order.receivedAmount} taxBasis="gross" />}
        />
        <MetricItem
          label="已开票"
          value={<MoneyValue value={order.invoicedAmount} taxBasis="gross" />}
          detail="不阻塞关闭"
        />
        <MetricItem
          label="当前主责"
          value={OWNER_LABEL[order.ownerSystem]}
          detail={`${ORIGIN_LABEL[order.originSystem]} · ${order.ownerName}`}
        />
      </MetricStrip>

      <nav
        aria-label="对象分区"
        className="flex flex-wrap gap-2 border-b border-border pb-2"
      >
        {navItems
          .filter((item) => item.show)
          .map((item) => {
            const active = activeSection === item.id
            return (
              <Button
                key={item.id}
                type="button"
                size="sm"
                variant={active ? "secondary" : "ghost"}
                aria-current={active ? "page" : undefined}
                render={<Link href={item.href} />}
              >
                {item.id === "procurement-rejection" ? (
                  <ShieldAlertIcon data-icon="inline-start" aria-hidden="true" />
                ) : null}
                {item.id === "approval" ? (
                  <ShieldCheckIcon data-icon="inline-start" aria-hidden="true" />
                ) : null}
                {item.id === "versions" ? (
                  <HistoryIcon data-icon="inline-start" aria-hidden="true" />
                ) : null}
                {item.label}
              </Button>
            )
          })}
      </nav>

      {result ? (
        <FormalActionResult
          status={result.status}
          title={result.title}
          description={result.description}
          reference={result.reference}
          facts={[
            { label: "销售单", value: order.documentNumber },
            { label: "客户", value: order.customerName },
          ]}
        />
      ) : null}

      {activeSection === "procurement-rejection" && order.procurementRejection ? (
        <ProcurementRejectionCard
          order={order}
          rejection={order.procurementRejection}
        />
      ) : null}

      {activeSection === "approval" && order.activeCardSalesApproval ? (
        <CardSalesApprovalPanel
          order={order}
          approval={order.activeCardSalesApproval}
        />
      ) : null}

      {activeSection === "close" ? <CloseConditionsCard order={order} /> : null}

      {activeSection === "collaboration" && isCard ? (
        <SalesOrderCollaborationCard
          salesOrderId={order.id}
          salesOrderNo={order.documentNumber}
        />
      ) : null}

      {activeSection === "versions" ? (
        <RevisionHistoryCard
          revisions={order.revisions}
          currentVersion={order.version}
          contractRevisionLabel={order.contractRevisionLabel}
        />
      ) : null}

      {activeSection === "acceptance" ? (
        isCard ? (
          <Alert variant="warning">
            <AlertTitle>卡券不适用客户验收</AlertTitle>
            <AlertDescription>
              卡券以履约期限到期完成履约，不因已消费完提前完成，也不登记客户验收。
              请查看关闭条件与履约期限 {order.fulfillmentDeadline}。
            </AlertDescription>
          </Alert>
        ) : (
          <AcceptanceWorkspace salesOrderId={order.id} />
        )
      ) : null}

      {activeSection === "overview" ? (
        <>
          {order.procurementRejection &&
          order.procurementRejection.reviewStatus !== "RESOLVED" &&
          order.procurementRejection.reviewStatus !== "VOIDED" ? (
            <Alert variant="warning">
              <ShieldAlertIcon aria-hidden="true" />
              <AlertTitle>采购二次确认已驳回</AlertTitle>
              <AlertDescription className="flex flex-wrap items-center gap-2">
                <span>
                  {order.procurementRejection.rejectComment} 请在固定三路中处理。
                </span>
                <Button
                  type="button"
                  size="xs"
                  render={
                    <Link href={`${baseHref}?section=procurement-rejection`} />
                  }
                >
                  打开处理卡
                </Button>
              </AlertDescription>
            </Alert>
          ) : null}

          {order.activeCardSalesApproval ? (
            <Alert variant="info">
              <ShieldCheckIcon aria-hidden="true" />
              <AlertTitle>存在卡券销售审批任务</AlertTitle>
              <AlertDescription className="flex flex-wrap items-center gap-2">
                <span>
                  {order.activeCardSalesApproval.workItemType} ·{" "}
                  {order.activeCardSalesApproval.workItemStatus}
                </span>
                <Button
                  type="button"
                  size="xs"
                  render={<Link href={`${baseHref}?section=approval`} />}
                >
                  打开审批区
                </Button>
              </AlertDescription>
            </Alert>
          ) : null}

          {order.activeChangeOrder ? (
            <Alert variant="warning">
              <FilePenLineIcon aria-hidden="true" />
              <AlertTitle>进行中的销售变更单</AlertTitle>
              <AlertDescription>
                {order.activeChangeOrder.id} ·{" "}
                {order.activeChangeOrder.statusLabel} · 基准版本 v
                {order.activeChangeOrder.baseRevisionNo}
                。历史版本继续有效；
                {order.activeChangeOrder.impactPath === "operations"
                  ? "卡券须运营执行影响确认后财务复核。"
                  : "非卡券须采购履约影响确认后财务复核。"}
              </AlertDescription>
            </Alert>
          ) : null}

          <DocumentSection title="商业内容与明细">
            <DocumentSummary
              columns="three"
              items={[
                {
                  id: "contract",
                  label: "合同修订",
                  value: order.contractRevisionLabel,
                },
                {
                  id: "nature",
                  label: "业务性质",
                  value: `${NATURE_LABEL[order.nature]}（不可改）`,
                },
                {
                  id: "origin",
                  label: "创建来源",
                  value: ORIGIN_LABEL[order.originSystem],
                },
                {
                  id: "owner",
                  label: "当前主责",
                  value: OWNER_LABEL[order.ownerSystem],
                },
                {
                  id: "scene",
                  label: "福利场景",
                  value: order.welfareScene,
                },
                {
                  id: "payment",
                  label: "付款条件",
                  value: order.paymentTerms,
                },
                {
                  id: "deadline",
                  label: isCard ? "履约期限（到期完成）" : "履约期限",
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
                  label: "商业版本",
                  value: `v${order.version}`,
                  numeric: true,
                },
              ]}
            />
            <div className="mt-4 overflow-hidden rounded-lg border border-border">
              <table className="w-full text-sm">
                <thead className="bg-muted/50 text-left">
                  <tr>
                    <th className="px-3 py-2 font-medium">
                      {isCard ? "卡券明细（每版本恰好一行）" : "明细"}
                    </th>
                    <th className="px-3 py-2 font-medium">数量</th>
                    {isCard ? (
                      <th className="px-3 py-2 font-medium">面额 / 形态</th>
                    ) : null}
                    <th className="px-3 py-2 font-medium text-right">
                      含税金额
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {order.lineItems.map((line) => (
                    <tr key={line.id} className="border-t border-border">
                      <td className="px-3 py-2">
                        <div>{line.name}</div>
                        <div className="num text-xs text-muted-foreground">
                          {line.sku}
                        </div>
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
                      ) : null}
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
                页面不展示玩法、卡号、卡密或完整手机号；联系人已掩码。
              </p>
            ) : null}
          </DocumentSection>

          <div className="grid min-w-0 gap-4 xl:grid-cols-2">
            <DocumentSection title="进度与协同">
              <StatusTrackSummary
                tracks={[
                  {
                    id: "fulfillment",
                    label: "履约",
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
              <p className="mt-3 text-sm text-muted-foreground">
                关联：采购单 {order.related.purchaseOrders} · 履约{" "}
                {order.related.fulfillments} · 回款 {order.related.receipts} ·
                开票 {order.related.invoices}
              </p>
              {isCard ? (
                <p className="mt-2 text-xs text-muted-foreground">
                  卡券不因消费汇总提前完成履约或增加关闭条件；消费订单见商城消费订单。
                </p>
              ) : null}
            </DocumentSection>
            <CloseConditionsCard order={order} />
          </div>

          {isCard ? (
            <SalesOrderCollaborationCard
              salesOrderId={order.id}
              salesOrderNo={order.documentNumber}
            />
          ) : null}

          <RevisionHistoryCard
            revisions={order.revisions}
            currentVersion={order.version}
            contractRevisionLabel={order.contractRevisionLabel}
          />
        </>
      ) : null}

      <FormalActionConfirmDialog
        open={changeConfirmOpen}
        onOpenChange={setChangeConfirmOpen}
        title="发起销售变更单"
        actionLabel="创建变更"
        confirmLabel="确认创建"
        fromStatus={{ label: `v${order.version}`, tone: "success" }}
        toStatus={{ label: "变更工作副本", tone: "warning" }}
        lockedFields={["销售版本", "业务性质", "稳定销售单号"]}
        effects={[
          "创建 sales_change_order 与工作副本",
          "历史版本与既有履约/票款不被覆盖",
          isCard
            ? "卡券：运营执行影响确认 → 财务复核"
            : "非卡券：采购履约影响确认 → 财务复核",
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
              title: "销售变更单已创建",
              description: `${change.id} 已进入「${change.statusLabel}」。原版本继续有效。`,
              reference: change.id,
            })
          } catch {
            setResult({
              status: "blocked",
              title: "无法创建销售变更",
              description: changeBlocker?.reason ?? "请刷新后重试。",
              reference: order.documentNumber,
            })
          }
        }}
      />
    </div>
  )
}
