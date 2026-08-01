"use client"

import * as React from "react"
import Link from "next/link"
import {
  ArrowLeftIcon,
  FilePlus2Icon,
  PencilIcon,
  ShoppingCartIcon,
} from "lucide-react"

import {
  AsyncSectionState,
  BusinessEmptyState,
  BusinessFailureState,
  BusinessStatusBadge,
  DataFreshness,
  DocumentHeader,
  DocumentSection,
  DocumentSummary,
  GuardedBusinessAction,
  MetricItem,
  MetricStrip,
  MoneyValue,
  PageActions,
  PageHeader,
  SensitiveValue,
} from "@/components/business"
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
import { CustomerReviseSheet } from "@/features/customers/customer-form-sheet"
import { useCustomerCenterQuery } from "@/features/customers/queries"
import { revealCustomerSensitiveField } from "@/features/customers/api"
import type {
  CustomerCenterView,
  CustomerSectionId,
} from "@/features/customers/types"
import { openWorkspaceLabel } from "@/lib/ui-text"

const SECTION_NAV: readonly {
  id: CustomerSectionId
  label: string
  hash: string
}[] = [
  { id: "overview", label: "概览", hash: "overview" },
  { id: "contacts", label: "联系与地址", hash: "contacts" },
  { id: "related", label: "合同与销售", hash: "related" },
  { id: "settlement", label: "票款摘要", hash: "settlement" },
  { id: "quality", label: "经营摘要", hash: "quality" },
  { id: "audit", label: "归属与审计", hash: "audit" },
]

function resolveSection(section?: string | null): CustomerSectionId {
  const found = SECTION_NAV.find(
    (item) => item.id === section || item.hash === section
  )
  return found?.id ?? "overview"
}

function can(customer: CustomerCenterView, action: string): boolean {
  return customer.allowedActions.includes(action)
}

function blocker(customer: CustomerCenterView, action: string): string | undefined {
  return customer.actionBlockers.find((b) => b.action === action)?.message
}

function ownerLabel(customer: CustomerCenterView): string {
  const owner = customer.assignments.find(
    (a) => a.role === "OWNER" && a.isCurrent
  )
  return owner?.userName ?? "—"
}

function collaboratorCount(customer: CustomerCenterView): number {
  return customer.assignments.filter(
    (a) => a.role === "COLLABORATOR" && a.isCurrent
  ).length
}

function collaboratorSummary(customer: CustomerCenterView): string {
  const cols = customer.assignments.filter(
    (a) => a.role === "COLLABORATOR" && a.isCurrent
  )
  if (cols.length === 0) return "无有效协作"
  return cols
    .map((c) => {
      const period = c.effectiveTo
        ? `${c.effectiveFrom} ~ ${c.effectiveTo}`
        : `${c.effectiveFrom} 起`
      return `${c.userName}（${period}）`
    })
    .join("；")
}

function collaboratorShortNames(customer: CustomerCenterView): string {
  const cols = customer.assignments.filter(
    (a) => a.role === "COLLABORATOR" && a.isCurrent
  )
  if (cols.length === 0) return "无"
  return cols.map((c) => c.userName).join("、")
}

export function CustomerDetailPage({
  customerId,
  section,
}: {
  customerId: string
  section?: string
}) {
  const query = useCustomerCenterQuery(customerId)
  const activeSection = resolveSection(section)
  const [reviseOpen, setReviseOpen] = React.useState(false)
  const sectionRefs = React.useRef<
    Partial<Record<CustomerSectionId, HTMLElement | null>>
  >({})

  const customer = query.data

  React.useEffect(() => {
    if (!customer) return
    const el = sectionRefs.current[activeSection]
    if (el) {
      el.scrollIntoView({ block: "start", behavior: "smooth" })
    }
  }, [activeSection, customer?.customerId])

  if (query.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="客户中心" description="正在加载客户对象…" />
        <div className="space-y-3" aria-busy="true" aria-label="加载中">
          <div className="h-16 animate-pulse rounded-lg bg-muted" />
          <div className="h-20 animate-pulse rounded-lg bg-muted" />
          <div className="h-40 animate-pulse rounded-lg bg-muted" />
        </div>
      </div>
    )
  }

  if (query.isError) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="客户中心" />
        <BusinessFailureState
          kind="system"
          description="加载客户对象失败。"
          action={
            <Button type="button" onClick={() => void query.refetch()}>
              重试
            </Button>
          }
        />
      </div>
    )
  }

  if (!customer) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title="客户不存在或无权访问"
          description={`未找到客户 ${customerId}。可能是错误 ID、已删除的临时草稿，或当前角色无此客户范围。`}
          actions={
            <Button render={<Link href="/sales/customers" />}>
              返回客户选择
            </Button>
          }
        />
      </div>
    )
  }

  const baseHref = `/sales/customers/${customer.customerId}`
  const displayName =
    customer.currentRevision.shortName || customer.currentRevision.legalName
  const isDisabled = customer.status === "disabled"
  const createContractHref = `/sales/contracts?customerId=${encodeURIComponent(customer.customerId)}`
  const createSalesOrderHref = `/sales/orders?mode=create&customerId=${encodeURIComponent(customer.customerId)}`
  const receivableHref = `/finance/customer-accounts?customerId=${encodeURIComponent(customer.customerId)}`
  const qualityHref = `/analytics/customer-quality?customerId=${encodeURIComponent(customer.customerId)}`

  const editBlocked = !can(customer, "EDIT_CUSTOMER")
  const contractBlocked = !can(customer, "CREATE_CONTRACT")
  const salesBlocked = !can(customer, "CREATE_SALES_ORDER")

  const collabCount = collaboratorCount(customer)
  const identityMeta = (
    <>
      <span>
        负责{" "}
        <span className="font-medium text-foreground">
          {ownerLabel(customer)}
        </span>
      </span>
      <span className="text-border" aria-hidden="true">
        ·
      </span>
      <span title={collaboratorSummary(customer)}>
        协作{" "}
        <span className="font-medium text-foreground">
          {collabCount > 0
            ? `${collabCount} 人（${collaboratorShortNames(customer)}）`
            : "无"}
        </span>
      </span>
    </>
  )

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        variant="object-chrome"
        breadcrumbs={[
          { id: "sales", label: "销售", href: "/sales/orders" },
          { id: "customers", label: "客户中心", href: "/sales/customers" },
          {
            id: "detail",
            label: displayName,
            current: true,
          },
        ]}
        actions={
          <PageActions
            actions={[
              {
                actionKey: "back",
                label: "返回选择",
                icon: ArrowLeftIcon,
                variant: "outline",
                render: <Link href="/sales/customers" />,
              },
            ]}
          />
        }
      />

      {/* First screen: identity + owner + metrics + primary actions */}
      <DocumentHeader
        density="compact"
        title={customer.currentRevision.legalName}
        documentNumber={customer.customerNo}
        version={`v${customer.currentRevision.revisionNo}`}
        primaryStatus={customer.statusLabel}
        meta={
          <span className="inline-flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
            {identityMeta}
          </span>
        }
        primaryAction={
          <div className="flex flex-wrap items-center gap-2">
            <GuardedBusinessAction
              size="sm"
              disabled={contractBlocked}
              reason={blocker(customer, "CREATE_CONTRACT")}
              render={
                contractBlocked ? undefined : (
                  <Link href={createContractHref} />
                )
              }
            >
              <FilePlus2Icon data-icon="inline-start" aria-hidden="true" />
              新建合同
            </GuardedBusinessAction>
            <GuardedBusinessAction
              size="sm"
              variant="secondary"
              disabled={salesBlocked}
              reason={blocker(customer, "CREATE_SALES_ORDER")}
              render={
                salesBlocked ? undefined : (
                  <Link href={createSalesOrderHref} />
                )
              }
            >
              <ShoppingCartIcon data-icon="inline-start" aria-hidden="true" />
              新建销售单
            </GuardedBusinessAction>
            <GuardedBusinessAction
              size="sm"
              variant="outline"
              disabled={editBlocked}
              reason={blocker(customer, "EDIT_CUSTOMER")}
              onClick={() => setReviseOpen(true)}
            >
              <PencilIcon data-icon="inline-start" aria-hidden="true" />
              修订主体
            </GuardedBusinessAction>
          </div>
        }
      />

      {isDisabled ? (
        <Alert variant="warning">
          <AlertTitle>客户已停用</AlertTitle>
          <AlertDescription>
            稳定身份、历史修订与已引用单据保留。新建合同/销售单已禁用；可继续查看历史与票款摘要。
          </AlertDescription>
        </Alert>
      ) : null}

      <MetricStrip
        columns={4}
        density="compact"
        aria-label="关系指标"
        aria-live="polite"
      >
        <MetricItem
          density="compact"
          label="有效合同"
          value={String(customer.metrics.activeContractCount)}
          detail={
            customer.metrics.expiringContractCount
              ? `${customer.metrics.expiringContractCount} 将到期`
              : undefined
          }
          detailMode="inline"
        />
        <MetricItem
          density="compact"
          label="进行中销售单"
          value={String(customer.metrics.inProgressSalesOrderCount)}
          detail="服务端聚合 · 非列表求和"
          detailMode="tooltip"
        />
        <MetricItem
          density="compact"
          label="应收余额"
          value={<MoneyValue value={customer.metrics.receivableBalance} />}
          detail="客户往来汇总"
          detailMode="tooltip"
        />
        <MetricItem
          density="compact"
          label="逾期金额"
          value={<MoneyValue value={customer.metrics.overdueAmount} />}
          detailMode="none"
        />
      </MetricStrip>

      {/* Keyboard-navigable section anchors */}
      <nav
        aria-label="客户信息分区"
        className="sticky top-0 z-10 -mx-1 flex flex-wrap gap-1 border-b border-border bg-background/95 px-1 py-2 backdrop-blur supports-backdrop-filter:bg-background/80"
      >
        {SECTION_NAV.map((item) => {
          const href =
            item.id === "overview"
              ? baseHref
              : `${baseHref}?section=${item.id}`
          const isCurrent = activeSection === item.id
          return (
            <Button
              key={item.id}
              type="button"
              size="sm"
              variant={isCurrent ? "secondary" : "ghost"}
              aria-current={isCurrent ? "true" : undefined}
              render={<Link href={href} scroll={false} />}
            >
              {item.label}
            </Button>
          )
        })}
      </nav>

      {/* Overview / identity — always shown when identity partition ok */}
      <div
        id="overview"
        ref={(el) => {
          sectionRefs.current.overview = el
        }}
        tabIndex={-1}
      >
        <DocumentSection title="主体身份与客户角色" description="当前主数据版本，不覆盖历史单据记录">
          {customer.partitions.identity === "error" ? (
            <BusinessFailureState
              kind="system"
              description="主体分区加载失败。"
              action={
                <Button type="button" size="sm" onClick={() => void query.refetch()}>
                  重试
                </Button>
              }
            />
          ) : (
            <DocumentSummary
              columns="two"
              items={[
                {
                  id: "legalName",
                  label: "法定名称",
                  value: customer.currentRevision.legalName,
                },
                {
                  id: "shortName",
                  label: "客户简称",
                  value: customer.currentRevision.shortName ?? "—",
                },
                {
                  id: "credit",
                  label: "统一社会信用代码",
                  value: customer.currentRevision.unifiedCreditCode ?? "—",
                },
                {
                  id: "payment",
                  label: "默认付款条件",
                  value:
                    customer.currentRevision.defaultPaymentTerm ??
                    "—（仅录单提示）",
                },
                {
                  id: "revision",
                  label: "主数据版本",
                  value: `v${customer.currentRevision.revisionNo} · ${customer.currentRevision.effectiveFrom.slice(0, 10)} 生效`,
                },
                {
                  id: "owner",
                  label: "负责销售（OWNER）",
                  value: ownerLabel(customer),
                },
              ]}
            />
          )}
        </DocumentSection>
      </div>

      {/* Contacts & addresses */}
      <div
        id="contacts"
        ref={(el) => {
          sectionRefs.current.contacts = el
        }}
        tabIndex={-1}
      >
        <DocumentSection
          title="联系与地址"
          description="有效联系人与地址；手机与履约地址按字段权限掩码"
        >
          {customer.partitions.contacts === "error" ? (
            <BusinessFailureState
              kind="system"
              description="联系分区失败；主体身份仍保留。"
              action={
                <Button type="button" size="sm" onClick={() => void query.refetch()}>
                  重试分区
                </Button>
              }
            />
          ) : (
            <div className="grid gap-4 lg:grid-cols-2">
              <Card size="sm">
                <CardHeader>
                  <CardTitle className="text-sm">有效联系人</CardTitle>
                  <CardDescription>默认掩码手机；揭示短时可审计</CardDescription>
                </CardHeader>
                <CardContent className="space-y-3">
                  {customer.contacts.length === 0 ? (
                    <p className="text-sm text-muted-foreground">暂无联系人</p>
                  ) : (
                    customer.contacts.map((c) => (
                      <div
                        key={c.id}
                        className="rounded-lg border border-border p-3 text-sm"
                      >
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="font-medium">{c.name}</span>
                          {c.isDefault ? (
                            <Badge variant="secondary">默认</Badge>
                          ) : null}
                          {c.title ? (
                            <span className="text-muted-foreground">
                              {c.title}
                            </span>
                          ) : null}
                        </div>
                        <div className="mt-2 space-y-1 text-muted-foreground">
                          <div className="flex flex-wrap items-center gap-2">
                            <span>手机</span>
                            {c.fieldVisibility.phone === "masked" ? (
                              <SensitiveValue
                                label={`${c.name}手机`}
                                maskedValue={c.phoneMasked}
                                onReveal={
                                  c.phoneRevealToken
                                    ? () =>
                                        revealCustomerSensitiveField(
                                          c.phoneRevealToken!
                                        )
                                    : undefined
                                }
                              />
                            ) : (
                              <span className="num">{c.phoneMasked}</span>
                            )}
                          </div>
                          {c.email ? <div>邮箱 {c.email}</div> : null}
                          <div className="text-xs">
                            有效期 {c.effectiveFrom}
                            {c.effectiveTo ? ` ~ ${c.effectiveTo}` : " 起"}
                          </div>
                        </div>
                      </div>
                    ))
                  )}
                </CardContent>
              </Card>

              <Card size="sm">
                <CardHeader>
                  <CardTitle className="text-sm">地址</CardTitle>
                  <CardDescription>履约地址按权限掩码</CardDescription>
                </CardHeader>
                <CardContent className="space-y-3">
                  {customer.addresses.length === 0 ? (
                    <p className="text-sm text-muted-foreground">暂无地址</p>
                  ) : (
                    customer.addresses.map((a) => (
                      <div
                        key={a.id}
                        className="rounded-lg border border-border p-3 text-sm"
                      >
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="font-medium">{a.addressType}</span>
                          {a.isDefault ? (
                            <Badge variant="secondary">默认</Badge>
                          ) : null}
                        </div>
                        <div className="mt-2">
                          {a.fieldVisibility.address === "masked" ? (
                            <SensitiveValue
                              label={a.addressType}
                              maskedValue={a.addressMasked}
                              onReveal={
                                a.addressRevealToken
                                  ? () =>
                                      revealCustomerSensitiveField(
                                        a.addressRevealToken!
                                      )
                                  : undefined
                              }
                            />
                          ) : (
                            <span>{a.addressMasked}</span>
                          )}
                        </div>
                      </div>
                    ))
                  )}
                </CardContent>
              </Card>
            </div>
          )}
        </DocumentSection>
      </div>

      {/* Related contracts & sales */}
      <div
        id="related"
        ref={(el) => {
          sectionRefs.current.related = el
        }}
        tabIndex={-1}
      >
        <DocumentSection
          title="合同与销售"
          description="中心内可发现有效合同与进行中销售单；完整列表带客户筛选进入合同/销售单"
          action={
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                size="sm"
                variant="outline"
                render={
                  <Link
                    href={`/sales/contracts?customerId=${encodeURIComponent(customer.customerId)}`}
                  />
                }
              >
                查看全部合同
              </Button>
              <Button
                type="button"
                size="sm"
                variant="outline"
                render={
                  <Link
                    href={`/sales/orders?customerId=${encodeURIComponent(customer.customerId)}`}
                  />
                }
              >
                查看全部销售单
              </Button>
            </div>
          }
        >
          {customer.partitions.related === "error" ? (
            <BusinessFailureState
              kind="system"
              description="关联业务分区失败；主体与其它分区仍保留。"
              action={
                <Button type="button" size="sm" onClick={() => void query.refetch()}>
                  重试
                </Button>
              }
            />
          ) : (
            <div className="grid gap-4 lg:grid-cols-2">
              <RelatedList
                title="合同（最近）"
                empty="暂无合同摘要"
                items={customer.contracts}
              />
              <RelatedList
                title="销售单（最近）"
                empty="暂无销售单摘要"
                items={customer.salesOrders}
              />
            </div>
          )}
        </DocumentSection>
      </div>

      {/* Settlement / receivable — read-only, deep link W11 */}
      <div
        id="settlement"
        ref={(el) => {
          sectionRefs.current.settlement = el
        }}
        tabIndex={-1}
      >
        <DocumentSection
          title="票款摘要"
          description="只读应收汇总；不在此核销或开票。往来详情进入客户往来。"
          action={
            <Button
              type="button"
              size="sm"
              variant="outline"
              render={<Link href={receivableHref} />}
            >
              {openWorkspaceLabel("W11")}
            </Button>
          }
        >
          {customer.partitions.settlement === "error" ? (
            <BusinessFailureState
              kind="system"
              description="票款分区失败；主体身份仍保留。"
            />
          ) : customer.receivableSummary ? (
            <div className="space-y-3">
              <DocumentSummary
                columns="two"
                items={[
                  {
                    id: "ar",
                    label: "应收余额",
                    value: (
                      <MoneyValue
                        value={customer.receivableSummary.receivableBalance}
                      />
                    ),
                    numeric: true,
                  },
                  {
                    id: "od",
                    label: "逾期金额",
                    value: (
                      <MoneyValue
                        value={customer.receivableSummary.overdueAmount}
                      />
                    ),
                    numeric: true,
                  },
                  {
                    id: "earliest",
                    label: "最早逾期日",
                    value:
                      customer.receivableSummary.earliestOverdueDate ?? "—",
                  },
                  {
                    id: "coll",
                    label: "回款进度",
                    value:
                      customer.receivableSummary.collectionProgressLabel ??
                      "—",
                  },
                  {
                    id: "inv",
                    label: "开票进度",
                    value:
                      customer.receivableSummary.invoicingProgressLabel ?? "—",
                  },
                ]}
              />
              {customer.receivableSummary.reliabilityNote ? (
                <p className="text-xs text-muted-foreground">
                  {customer.receivableSummary.reliabilityNote}
                </p>
              ) : null}
              {customer.bankAccounts.length > 0 ? (
                <Card size="sm">
                  <CardHeader>
                    <CardTitle className="text-sm">银行账户（掩码）</CardTitle>
                    <CardDescription>
                      默认末四位；完整揭示需授权且记审计
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="space-y-2">
                    {customer.bankAccounts.map((b) => (
                      <div
                        key={b.id}
                        className="flex flex-wrap items-center gap-2 text-sm"
                      >
                        <span className="num text-muted-foreground">
                          {b.internalNo}
                        </span>
                        <span>{b.accountName}</span>
                        <span className="text-muted-foreground">
                          {b.bankName}
                        </span>
                        <SensitiveValue
                          label="银行账号"
                          maskedValue={b.accountMasked}
                          onReveal={
                            b.accountRevealToken
                              ? () =>
                                  revealCustomerSensitiveField(
                                    b.accountRevealToken!
                                  )
                              : undefined
                          }
                        />
                      </div>
                    ))}
                  </CardContent>
                </Card>
              ) : null}
            </div>
          ) : (
            <BusinessEmptyState
              kind="no-data"
              title="暂无票款摘要"
              description="服务端未返回应收数据。"
            />
          )}
        </DocumentSection>
      </div>

      {/* Quality — read-only, deep link W15 */}
      <div
        id="quality"
        ref={(el) => {
          sectionRefs.current.quality = el
        }}
        tabIndex={-1}
      >
        <DocumentSection
          title="经营摘要"
          description="W15 异步汇总（允许延迟）；标签为服务端字段，前端不计算。"
          action={
            <Button
              type="button"
              size="sm"
              variant="outline"
              render={<Link href={qualityHref} />}
            >
              打开经营质量
            </Button>
          }
        >
          <AsyncSectionState
            status={
              customer.partitions.quality === "error" ? "error" : "success"
            }
            error="经营数据分区暂时不可用。已确认的客户主体与其它分区不受影响。"
            errorKind="projection"
            retryAction={
              <Button type="button" size="sm" onClick={() => void query.refetch()}>
                重试经营分区
              </Button>
            }
          >
            {customer.partitions.quality === "ok" && customer.qualitySummary ? (
              <div className="space-y-3">
                <DocumentSummary
                  columns="two"
                  items={[
                    {
                      id: "scale",
                      label: "规模标签",
                      value: customer.qualitySummary.scaleLabel,
                    },
                    {
                      id: "profit",
                      label: "利润贡献",
                      value: customer.qualitySummary.profitContributionLabel,
                    },
                    {
                      id: "risk",
                      label: "回款风险",
                      value: customer.qualitySummary.collectionRiskLabel,
                    },
                    {
                      id: "lastBiz",
                      label: "最近业务",
                      value: customer.qualitySummary.lastBusinessAt ?? "—",
                    },
                  ]}
                />
                <DataFreshness
                  updatedAt={
                    customer.qualitySummary.isStale ? "数据可能不是最新" : "数据"
                  }
                  dateTime={customer.qualitySummary.projectionAt}
                  state={customer.qualitySummary.isStale ? "stale" : "fresh"}
                  label="经营质量"
                />
              </div>
            ) : customer.partitions.quality === "ok" ? (
              <BusinessEmptyState
                kind="no-data"
                title="暂无经营摘要"
                description="数据尚未生成。"
              />
            ) : null}
          </AsyncSectionState>
        </DocumentSection>
      </div>

      {/* Audit / ownership */}
      <div
        id="audit"
        ref={(el) => {
          sectionRefs.current.audit = el
        }}
        tabIndex={-1}
      >
        <DocumentSection title="归属与审计" description="OWNER 唯一；协作销售显示有效期">
          {customer.partitions.audit === "error" ? (
            <BusinessFailureState
              kind="system"
              description="归属审计分区失败。"
            />
          ) : (
            <div className="grid gap-4 lg:grid-cols-2">
              <Card size="sm">
                <CardHeader>
                  <CardTitle className="text-sm">当前责任关系</CardTitle>
                </CardHeader>
                <CardContent className="space-y-2 text-sm">
                  {customer.assignments
                    .filter((a) => a.isCurrent)
                    .map((a) => (
                      <div
                        key={a.id}
                        className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-border px-3 py-2"
                      >
                        <div>
                          <BusinessStatusBadge
                            context="list"
                            label={
                              a.role === "OWNER" ? "负责销售" : "协作销售"
                            }
                            tone={a.role === "OWNER" ? "info" : "neutral"}
                          />
                          <span className="ml-2 font-medium">{a.userName}</span>
                        </div>
                        <span className="text-xs text-muted-foreground">
                          {a.effectiveFrom}
                          {a.effectiveTo ? ` ~ ${a.effectiveTo}` : " 起"}
                        </span>
                      </div>
                    ))}
                </CardContent>
              </Card>
              <Card size="sm">
                <CardHeader>
                  <CardTitle className="text-sm">修订时间线</CardTitle>
                  <CardDescription>
                    新版本不覆盖历史合同/销售单记录
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-2 text-sm">
                  {customer.revisionTimeline.map((r) => (
                    <div
                      key={r.id}
                      className="rounded-md border border-border px-3 py-2"
                    >
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="num font-medium">v{r.revisionNo}</span>
                        {r.isCurrent ? (
                          <Badge variant="secondary">当前</Badge>
                        ) : null}
                        <span className="text-muted-foreground">{r.actor}</span>
                      </div>
                      <p className="mt-1 text-muted-foreground">{r.reason}</p>
                      <p className="mt-0.5 text-xs text-muted-foreground">
                        {r.effectiveAt}
                      </p>
                    </div>
                  ))}
                </CardContent>
              </Card>
            </div>
          )}
        </DocumentSection>
      </div>

      {!editBlocked ? (
        <CustomerReviseSheet
          open={reviseOpen}
          onOpenChange={setReviseOpen}
          customer={customer}
        />
      ) : null}
    </div>
  )
}

function RelatedList({
  title,
  empty,
  items,
}: {
  title: string
  empty: string
  items: CustomerCenterView["contracts"]
}) {
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle className="text-sm">{title}</CardTitle>
      </CardHeader>
      <CardContent className="space-y-2">
        {items.length === 0 ? (
          <p className="text-sm text-muted-foreground">{empty}</p>
        ) : (
          items.map((item) => (
            <div
              key={item.id}
              className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-border px-3 py-2 text-sm"
            >
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2">
                  <Link
                    href={item.href}
                    className="num font-medium underline-offset-4 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  >
                    {item.number}
                  </Link>
                  <BusinessStatusBadge context="list" {...item.status} />
                </div>
                <p className="text-muted-foreground">{item.title}</p>
                {item.detail ? (
                  <p className="text-xs text-muted-foreground">{item.detail}</p>
                ) : null}
              </div>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                render={<Link href={item.href} />}
              >
                打开
              </Button>
            </div>
          ))
        )}
      </CardContent>
    </Card>
  )
}
