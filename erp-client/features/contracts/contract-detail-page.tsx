"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import {
  HistoryIcon,
  PrinterIcon,
} from "lucide-react"

import {
  BusinessFailureState,
  BusinessStatusBadge,
  DocumentAttachmentList,
  DocumentHeader,
  DocumentSummary,
  MoneyValue,
  PageActions,
  PageHeader,
} from "@/components/business"
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
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { ContractPaperDialog } from "@/features/contracts/contract-paper-dialog"
import { useContractCenterQuery } from "@/features/contracts/queries"
import {
  CONTRACT_AUDIT_ACTION_LABEL,
  contractOwnerLabel,
} from "@/features/contracts/types"
import type { ContractCenterView } from "@/features/contracts/types"

type SectionId =
  | "overview"
  | "settlement"
  | "attachments"
  | "sales-orders"
  | "versions"

function resolveSection(section?: string): SectionId {
  if (
    section === "settlement" ||
    section === "attachments" ||
    section === "sales-orders" ||
    section === "versions"
  ) {
    return section
  }
  return "overview"
}

function formatAsOf(iso: string): string {
  try {
    return new Intl.DateTimeFormat("zh-CN", {
      month: "long",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
      timeZone: "Asia/Shanghai",
    }).format(new Date(iso))
  } catch {
    return iso
  }
}

/** 30 日内将到期：与列表页同口径（仍生效 + 有效期止在 30 天内）。 */
function isExpiringWithin30Days(contract: ContractCenterView): boolean {
  if (contract.status !== "EFFECTIVE") return false
  const validTo = new Date(contract.currentRevision.validTo + "T00:00:00")
  if (Number.isNaN(validTo.getTime())) return false
  const now = new Date()
  const dayMs = 24 * 60 * 60 * 1000
  const diff = Math.ceil((validTo.getTime() - now.getTime()) / dayMs)
  return diff >= 0 && diff <= 30
}

export function ContractDetailPage({
  contractId,
  section,
}: {
  contractId: string
  section?: string
}) {
  const router = useRouter()
  const query = useContractCenterQuery(contractId)

  const activeSection = resolveSection(section)
  const [paperOpen, setPaperOpen] = React.useState(false)

  const contract = query.data

  if (query.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="合同" description="正在加载详情…" />
        <div className="space-y-3" aria-busy="true" aria-label="加载中">
          <div className="h-16 animate-pulse rounded-lg bg-muted" />
          <div className="h-24 animate-pulse rounded-2xl bg-muted" />
          <div className="h-40 animate-pulse rounded-2xl bg-muted" />
        </div>
      </div>
    )
  }

  if (query.isError) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="合同" />
        <BusinessFailureState
          kind="system"
          title="合同加载失败"
          description="暂时拿不到这份合同的数据，请重试；合同记录不受影响。"
          onRetry={() => {
            void query.refetch()
          }}
        />
      </div>
    )
  }

  if (!contract) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title="合同不存在"
          description="未找到这份合同。可能编号有误，或当前角色无权查看。"
          actions={
            <Button render={<Link href="/sales/contracts" />}>返回列表</Button>
          }
        />
      </div>
    )
  }

  const baseHref = `/sales/contracts/${contract.contractId}`
  const canCreateSo = contract.allowedActions.includes("CREATE_SALES_ORDER")
  const canPrint = contract.allowedActions.includes("PRINT")
  const soBlocker = contract.actionBlockers.find(
    (b) => b.action === "CREATE_SALES_ORDER"
  )
  const expiring = isExpiringWithin30Days(contract)
  const archived = contract.attachments.length > 0
  const allAttachmentsSafe =
    archived &&
    contract.attachments.every((file) => file.securityState === "done")

  const navItems: {
    id: SectionId
    label: string
    href: string
  }[] = [
    { id: "overview", label: "概览", href: baseHref },
    {
      id: "settlement",
      label: "结算与开票",
      href: `${baseHref}?section=settlement`,
    },
    {
      id: "attachments",
      label: "附件",
      href: `${baseHref}?section=attachments`,
    },
    {
      id: "sales-orders",
      label: "关联销售单",
      href: `${baseHref}?section=sales-orders`,
    },
    {
      id: "versions",
      label: "版本与审计",
      href: `${baseHref}?section=versions`,
    },
  ]

  const rev = contract.currentRevision

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        variant="object-chrome"
        breadcrumbs={[
          { id: "sales", label: "销售", href: "/sales/orders" },
          { id: "contracts", label: "合同", href: "/sales/contracts" },
          {
            id: "detail",
            label: contract.contractNo,
            current: true,
          },
        ]}
        actions={
          <PageActions
            actions={[
              {
                actionKey: "back",
                label: "返回列表",
                variant: "outline",
                render: <Link href="/sales/contracts" />,
              },
            ]}
          />
        }
      />

      <DocumentHeader
        density="compact"
        title={contract.contractNo}
        documentNumber={contract.contractNo}
        version={`v${rev.revisionNo}`}
        primaryStatus={{
          label: contract.statusLabel,
          tone: contract.statusTone,
        }}
        meta={
          <span className="inline-flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
            <span>
              客户{" "}
              <span className="font-medium text-foreground">
                {contract.customer.displayName}
              </span>
            </span>
            <span aria-hidden="true">·</span>
            <span>
              负责人{" "}
              <span className="font-medium text-foreground">
                {contractOwnerLabel(contract.ownerLabel)}
              </span>
            </span>
          </span>
        }
        primaryAction={
          canCreateSo ? (
            <Button
              type="button"
              size="sm"
              render={
                <Link
                  href={`/sales/orders?mode=create&customerId=${encodeURIComponent(
                    contract.customer.id
                  )}&contractId=${encodeURIComponent(contract.contractId)}`}
                />
              }
            >
              新建销售单
            </Button>
          ) : (
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled
              title={soBlocker?.message}
            >
              新建销售单
            </Button>
          )
        }
        secondaryActions={
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={!canPrint}
            onClick={() => setPaperOpen(true)}
          >
            <PrinterIcon data-icon="inline-start" aria-hidden="true" />
            纸质预览
          </Button>
        }
      />

      <div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
        <span>
          有效期{" "}
          <span className="num text-foreground">
            {rev.validFrom} 至 {rev.validTo}
          </span>
        </span>
        <span>·</span>
        <span>{contractOwnerLabel(contract.ownerLabel)}</span>
        {expiring ? <Badge variant="warning">30 日内将到期</Badge> : null}
        {allAttachmentsSafe ? <Badge variant="info">PDF 电子档已归档</Badge> : null}
      </div>

      {!canCreateSo && soBlocker ? (
        <p className="text-xs text-muted-foreground">
          新建销售单不可用：{soBlocker.message}
        </p>
      ) : null}

      <nav
        aria-label="对象分区"
        className="flex flex-wrap gap-2 border-b border-border pb-2"
      >
        {navItems.map((item) => {
          const active = activeSection === item.id
          return (
            <Button
              key={item.id}
              type="button"
              size="sm"
              variant={active ? "secondary" : "ghost"}
              aria-current={active ? "page" : undefined}
              onClick={(event) => {
                event.preventDefault()
                router.replace(item.href, { scroll: false })
              }}
            >
              {item.id === "versions" ? (
                <HistoryIcon data-icon="inline-start" aria-hidden="true" />
              ) : null}
              {item.label}
            </Button>
          )
        })}
      </nav>

      {activeSection === "overview" ? (
        <div className="grid gap-4 lg:grid-cols-2">
          <Card size="sm">
            <CardHeader className="border-b">
              <CardTitle>概览</CardTitle>
              <CardDescription>
                展示合同身份、客户、状态、版本与有效期；不含合同级金额。
              </CardDescription>
            </CardHeader>
            <CardContent>
              <DocumentSummary
                columns="two"
                items={[
                  {
                    id: "no",
                    label: "合同编号",
                    value: contract.contractNo,
                    numeric: true,
                  },
                  {
                    id: "status",
                    label: "状态",
                    value: (
                      <BusinessStatusBadge
                        context="preview"
                        label={contract.statusLabel}
                        tone={contract.statusTone}
                      />
                    ),
                  },
                  {
                    id: "customer",
                    label: "客户",
                    value: contract.customer.displayName,
                  },
                  {
                    id: "settlement",
                    label: "结算主体",
                    value: rev.settlementParty.displayName,
                  },
                  {
                    id: "signed",
                    label: "签订日",
                    value: rev.signedAt ?? "—",
                    numeric: true,
                  },
                  {
                    id: "valid",
                    label: "有效期",
                    value: `${rev.validFrom} 至 ${rev.validTo}`,
                    numeric: true,
                  },
                  {
                    id: "rev",
                    label: "当前版本",
                    value: `v${rev.revisionNo}`,
                    numeric: true,
                  },
                  {
                    id: "owner",
                    label: "负责人",
                    value: contractOwnerLabel(contract.ownerLabel),
                  },
                ]}
              />
              <p className="mt-3 text-sm text-muted-foreground">
                {rev.termsSummary}
              </p>
            </CardContent>
          </Card>

          <Card size="sm">
            <CardHeader className="border-b">
              <CardTitle>关联销售摘要</CardTitle>
              <CardDescription>
                金额仅为各销售单摘要，不汇总为合同金额。更新于{" "}
                <span className="num">{formatAsOf(contract.relatedSalesOrdersAsOf)}</span>
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-2">
              <p className="text-sm">
                关联 {contract.relatedSalesOrders.length} 张
                {contract.relatedSalesOrders.length > 0
                  ? "（见下方关联销售单分区）"
                  : "。"}
              </p>
              <Button
                type="button"
                size="sm"
                variant="outline"
                render={<Link href={`${baseHref}?section=sales-orders`} />}
              >
                查看关联销售单
              </Button>
            </CardContent>
          </Card>
        </div>
      ) : null}

      {activeSection === "settlement" ? (
        <Card size="sm">
          <CardHeader className="border-b">
            <CardTitle>结算与开票</CardTitle>
            <CardDescription>
              当前合同修订的结构化记录；销售单关联时锁定该版本。
            </CardDescription>
          </CardHeader>
          <CardContent>
            <DocumentSummary
              columns="two"
              items={[
                {
                  id: "party",
                  label: "结算主体",
                  value: rev.settlementParty.displayName,
                },
                {
                  id: "payment",
                  label: "付款条件",
                  value: rev.paymentTermSnapshot.label,
                },
                {
                  id: "payment-desc",
                  label: "付款说明",
                  value: rev.paymentTermSnapshot.description,
                },
                {
                  id: "invoice-type",
                  label: "开票类型",
                  value: rev.invoiceRequirementSnapshot.titleType,
                },
                {
                  id: "invoice-content",
                  label: "开票内容",
                  value: rev.invoiceRequirementSnapshot.contentSummary,
                },
                {
                  id: "tax",
                  label: "税号（打码）",
                  value: rev.invoiceRequirementSnapshot.taxIdMasked ?? "—",
                  numeric: true,
                },
              ]}
            />
          </CardContent>
        </Card>
      ) : null}

      {activeSection === "attachments" ? (
        <div className="space-y-3">
          <DocumentAttachmentList
            title="合同 PDF 电子档"
            openLabel="下载"
            attachments={contract.attachments.map((file) => ({
              id: file.id,
              name: file.name,
              description: `${file.uploadedBy} · ${file.uploadedAt}${
                file.revisionNo != null ? ` · v${file.revisionNo}` : ""
              }`,
              state:
                file.securityState === "done"
                  ? ("done" as const)
                  : file.securityState === "quarantined"
                    ? ("error" as const)
                    : ("processing" as const),
              errorMessage:
                file.securityState === "quarantined"
                  ? "安全检查未通过，已隔离，不可下载或作为生效依据。"
                  : undefined,
              onOpen: undefined,
            }))}
          />
        </div>
      ) : null}

      {activeSection === "sales-orders" ? (
        <Card size="sm">
          <CardHeader className="border-b">
            <CardTitle>关联销售单</CardTitle>
            <CardDescription>
              追溯每张销售单使用的合同版本；金额仅作单据摘要。
              统计截至{" "}
              <span className="num">{formatAsOf(contract.relatedSalesOrdersAsOf)}</span>
            </CardDescription>
          </CardHeader>
          <CardContent>
            {contract.relatedSalesOrders.length === 0 ? (
              <p className="text-sm text-muted-foreground">暂无关联销售单。</p>
            ) : (
              <div className="overflow-hidden rounded-lg border border-border">
                <Table data-density="compact">
                  <TableHeader>
                    <TableRow>
                      <TableHead>销售单号</TableHead>
                      <TableHead>业务性质</TableHead>
                      <TableHead>合同版本</TableHead>
                      <TableHead>主状态</TableHead>
                      <TableHead>履约 / 回款 / 开票</TableHead>
                      <TableHead data-align="end">含税金额</TableHead>
                      <TableHead data-align="end">操作</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {contract.relatedSalesOrders.map((so) => (
                      <TableRow key={so.salesOrderId}>
                        <TableCell className="num font-medium">
                          {so.documentNumber}
                        </TableCell>
                        <TableCell>{so.natureLabel}</TableCell>
                        <TableCell className="num">
                          v{so.contractRevisionNo}
                        </TableCell>
                        <TableCell>
                          <BusinessStatusBadge
                            context="list"
                            {...so.primaryStatus}
                          />
                        </TableCell>
                        <TableCell className="text-xs text-muted-foreground">
                          履约 {so.fulfillmentLabel} · 回款{" "}
                          {so.collectionLabel} · 开票 {so.invoicingLabel}
                        </TableCell>
                        <TableCell data-align="end">
                          <MoneyValue
                            value={so.amountGross}
                            taxBasis="gross"
                          />
                        </TableCell>
                        <TableCell data-align="end">
                          <Button
                            type="button"
                            size="xs"
                            variant="outline"
                            render={
                              <Link href={`/sales/orders/${so.salesOrderId}`} />
                            }
                          >
                            打开
                          </Button>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </div>
            )}
          </CardContent>
        </Card>
      ) : null}

      {activeSection === "versions" ? (
        <div className="grid gap-4 lg:grid-cols-2">
          <Card size="sm">
            <CardHeader className="border-b">
              <div className="flex flex-wrap items-center gap-2">
                <HistoryIcon
                  className="size-4 text-muted-foreground"
                  aria-hidden="true"
                />
              <CardTitle>版本时间线</CardTitle>
              </div>
              <CardDescription>
                每个版本对应已上传的签署 PDF。
              </CardDescription>
            </CardHeader>
            <CardContent>
              {contract.revisionTimeline.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  尚无已确认修订。
                </p>
              ) : (
                <ol className="space-y-3" aria-label="合同修订时间线">
                  {contract.revisionTimeline.map((item) => (
                    <li
                      key={item.revisionId}
                      className="rounded-lg border border-border px-3 py-2.5"
                    >
                      <div className="flex flex-wrap items-center justify-between gap-2">
                        <div className="flex items-center gap-2">
                          <span className="num font-medium">
                            v{item.revisionNo}
                          </span>
                          {item.isCurrent ? (
                            <Badge variant="info">当前</Badge>
                          ) : (
                            <Badge variant="outline">历史</Badge>
                          )}
                        </div>
                        <span className="num text-xs text-muted-foreground">
                          {item.effectiveAt ?? "—"}
                        </span>
                      </div>
                      <p className="mt-1 text-xs text-muted-foreground">
                        {item.validFrom} 至 {item.validTo}
                        {item.changeReason
                          ? ` · ${item.changeReason}`
                          : null}
                      </p>
                      {item.diffSummary && item.diffSummary.length > 0 ? (
                        <ul className="mt-2 space-y-1 text-xs">
                          {item.diffSummary.map((diff) => (
                            <li key={diff.field}>
                              <span className="font-medium">{diff.field}</span>
                              ：{diff.before} → {diff.after}
                            </li>
                          ))}
                        </ul>
                      ) : null}
                    </li>
                  ))}
                </ol>
              )}
            </CardContent>
          </Card>

          <Card size="sm">
            <CardHeader className="border-b">
              <CardTitle>审计时间线</CardTitle>
              <CardDescription>
                PDF 上传、版本归档、终止与下载等处理动作。
              </CardDescription>
            </CardHeader>
            <CardContent>
              <ol className="space-y-3" aria-label="合同审计时间线">
                {contract.auditTimeline.map((event) => (
                  <li
                    key={event.id}
                    className="rounded-lg border border-border px-3 py-2.5"
                  >
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <span className="text-sm font-medium">
                        {CONTRACT_AUDIT_ACTION_LABEL[event.action] ??
                          event.action}
                      </span>
                      <span className="num text-xs text-muted-foreground">
                        {event.at}
                      </span>
                    </div>
                    <p className="mt-1 text-xs text-muted-foreground">
                      {event.actorLabel} · {event.summary}
                    </p>
                  </li>
                ))}
              </ol>
            </CardContent>
          </Card>
        </div>
      ) : null}

      <ContractPaperDialog
        contract={contract}
        open={paperOpen}
        onOpenChange={setPaperOpen}
      />
    </div>
  )
}
