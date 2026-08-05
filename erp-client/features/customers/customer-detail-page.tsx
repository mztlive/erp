"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import {
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
  DiscardConfirmDialog,
  DocumentHeader,
  DocumentSection,
  DocumentSummary,
  GuardedBusinessAction,
  MetricItem,
  MetricStrip,
  MoneyValue,
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { CustomerForm } from "@/features/customers/customer-form"
import { useCustomerCenterQuery } from "@/features/customers/queries"
import { revealCustomerSensitiveField } from "@/features/customers/queries"
import type {
  CustomerCenterView,
  CustomerSectionId,
} from "@/features/customers/types"
import { openWorkspaceLabel } from "@/lib/ui-text"

const SECTION_NAV: readonly {
  id: CustomerSectionId
  label: string
}[] = [
  { id: "overview", label: "概览" },
  { id: "related", label: "合同与销售" },
  { id: "settlement", label: "票款摘要" },
  { id: "quality", label: "经营摘要" },
  { id: "audit", label: "归属与审计" },
]

function resolveSection(section?: string | null): CustomerSectionId {
  const found = SECTION_NAV.find((item) => item.id === section)
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
  const router = useRouter()
  const activeSection = resolveSection(section)
  const [editing, setEditing] = React.useState(false)
  const [formDirty, setFormDirty] = React.useState(false)
  const [pendingSection, setPendingSection] =
    React.useState<CustomerSectionId | null>(null)
  const [savedNotice, setSavedNotice] = React.useState<{
    revisionNo: number
  } | null>(null)

  const customer = query.data

  const selectSection = React.useCallback(
    (next: CustomerSectionId) => {
      router.replace(
        next === "overview"
          ? `/sales/customers/${customerId}`
          : `/sales/customers/${customerId}?section=${next}`,
        { scroll: false }
      )
    },
    [customerId, router]
  )

  /** 编辑中且未保存时，切 Tab 先弹确认，避免静默丢输入。 */
  const handleSectionChange = React.useCallback(
    (next: CustomerSectionId) => {
      if (next === activeSection) return
      if (editing && formDirty) {
        setPendingSection(next)
        return
      }
      selectSection(next)
    },
    [activeSection, editing, formDirty, selectSection]
  )

  if (query.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="客户详情" description="正在加载客户…" />
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
        <PageHeader title="客户详情" />
        <BusinessFailureState
          kind="system"
          description="加载客户失败。"
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
          description="未找到该客户。可能编号有误，或当前角色无权访问该客户。"
          actions={
            <Button render={<Link href="/sales/customers" />}>
              返回客户选择
            </Button>
          }
        />
      </div>
    )
  }

  const displayName =
    customer.currentRevision.shortName || customer.currentRevision.legalName
  const isDisabled = customer.status === "disabled"
  const uploadContractHref = `/sales/contracts?customerId=${encodeURIComponent(customer.customerId)}`
  const createSalesOrderHref = `/sales/orders?mode=create&customerId=${encodeURIComponent(customer.customerId)}`
  const receivableHref = `/finance/customer-accounts?customerId=${encodeURIComponent(customer.customerId)}`
  const qualityHref = `/analytics/customer-quality?customerId=${encodeURIComponent(customer.customerId)}`

  const editBlocked = !can(customer, "EDIT_CUSTOMER")
  const contractBlocked = !can(customer, "UPLOAD_CONTRACT_PDF")
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
              reason={blocker(customer, "UPLOAD_CONTRACT_PDF")}
              render={
                contractBlocked ? undefined : (
                  <Link href={uploadContractHref} />
                )
              }
            >
              <FilePlus2Icon data-icon="inline-start" aria-hidden="true" />
              上传合同 PDF
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
          </div>
        }
      />

      {isDisabled ? (
        <Alert variant="warning">
          <AlertTitle>客户已停用</AlertTitle>
          <AlertDescription>
            稳定身份、历史修订与已引用单据保留。上传合同和新建销售单已禁用
            {blocker(customer, "UPLOAD_CONTRACT_PDF") ||
            blocker(customer, "CREATE_SALES_ORDER")
              ? `（${[
                  blocker(customer, "UPLOAD_CONTRACT_PDF"),
                  blocker(customer, "CREATE_SALES_ORDER"),
                ]
                  .filter(Boolean)
                  .join("；")}）`
              : ""}
            ；可继续查看历史与票款摘要。
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
          detail="系统汇总 · 非列表求和"
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

      <Tabs
        value={activeSection}
        onValueChange={(next) => {
          handleSectionChange((next as CustomerSectionId) ?? "overview")
        }}
      >
        <TabsList
          variant="line"
          className="sticky top-0 z-10 w-full justify-start overflow-x-auto rounded-none border-b border-border bg-background/95 backdrop-blur supports-backdrop-filter:bg-background/80"
        >
          {SECTION_NAV.map((item) => (
            <TabsTrigger key={item.id} value={item.id} className="flex-none">
              {item.label}
            </TabsTrigger>
          ))}
        </TabsList>

        <TabsContent value="overview">
          <div className="space-y-4 pt-4">
            {editing ? (
              <>
                <CustomerForm
                  mode="edit"
                  grouped
                  customer={customer}
                  onDirtyChange={setFormDirty}
                  onCancel={() => {
                    setEditing(false)
                    setFormDirty(false)
                  }}
                  onSucceeded={(_customerId, revisionNo) => {
                    setEditing(false)
                    setFormDirty(false)
                    setSavedNotice({ revisionNo: revisionNo ?? customer.currentRevision.revisionNo })
                  }}
                />
              </>
            ) : (
              <>
                {savedNotice ? (
                  <Alert variant="success">
                    <AlertTitle>客户资料已保存</AlertTitle>
                    <AlertDescription className="flex flex-wrap items-center justify-between gap-2">
                      <span>
                        新版本 v{savedNotice.revisionNo} 已生效，历史单据记录不变。
                      </span>
                      <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={() => setSavedNotice(null)}
                      >
                        知道了
                      </Button>
                    </AlertDescription>
                  </Alert>
                ) : null}
                <DocumentSection
                  title="主体身份与客户角色"
                  description="当前基础资料版本，不覆盖历史单据记录"
                  action={
                    !editBlocked ? (
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={() => setEditing(true)}
                      >
                        <PencilIcon
                          data-icon="inline-start"
                          aria-hidden="true"
                        />
                        编辑资料
                      </Button>
                    ) : null
                  }
                >
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
                          label: "基础资料版本",
                          value: `v${customer.currentRevision.revisionNo} · ${customer.currentRevision.effectiveFrom.slice(0, 10)} 生效`,
                        },
                        {
                          id: "owner",
                          label: "负责销售",
                          value: ownerLabel(customer),
                        },
                      ]}
                    />
                  )}
                </DocumentSection>

                <DocumentSection
                  title="联系与地址"
                  description="有效联系人与地址；手机与履约地址按字段权限打码"
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
                          <CardDescription>默认打码手机；揭示操作会留记录</CardDescription>
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
                          <CardDescription>履约地址按权限打码</CardDescription>
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

                <DocumentSection
                  title="银行账户"
                  description="账号默认只显示末四位；完整显示需授权，操作会留记录"
                >
                  {customer.bankAccounts.length === 0 ? (
                    <p className="text-sm text-muted-foreground">暂无银行账户</p>
                  ) : (
                    <Card size="sm">
                      <CardContent className="space-y-2">
                        {customer.bankAccounts.map((b) => (
                          <div
                            key={b.id}
                            className="flex flex-wrap items-center gap-2 text-sm"
                          >
                            <span>{b.accountName}</span>
                            {b.isDefault ? (
                              <Badge variant="secondary">默认</Badge>
                            ) : null}
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
                  )}
                </DocumentSection>
              </>
            )}
          </div>
        </TabsContent>

        <TabsContent value="related">
          <div className="space-y-4 pt-4">
            <DocumentSection
              title="合同与销售"
              description="以下列出最近合同与进行中销售单。"
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
        </TabsContent>

        <TabsContent value="settlement">
          <div className="space-y-4 pt-4">
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
                  action={
                    <Button type="button" size="sm" onClick={() => void query.refetch()}>
                      重试
                    </Button>
                  }
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
                  <p className="text-xs text-muted-foreground">
                    应收余额与逾期金额与顶部指标一致，并非增量数据。
                  </p>
                  {customer.receivableSummary.reliabilityNote ? (
                    <p className="text-xs text-muted-foreground">
                      {customer.receivableSummary.reliabilityNote}
                    </p>
                  ) : null}
                </div>
              ) : (
                <BusinessEmptyState
                  kind="no-data"
                  title="暂无票款摘要"
                  description="系统暂无应收数据。"
                />
              )}
            </DocumentSection>
          </div>
        </TabsContent>

        <TabsContent value="quality">
          <div className="space-y-4 pt-4">
            <DocumentSection
              title="经营摘要"
              description="数据由系统汇总；标签以系统返回为准。"
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
                        customer.qualitySummary.projectionAt.slice(0, 16).replace("T", " ")
                      }
                      dateTime={customer.qualitySummary.projectionAt}
                      state={customer.qualitySummary.isStale ? "stale" : "fresh"}
                      label="经营质量汇总于"
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
        </TabsContent>

        <TabsContent value="audit">
          <div className="space-y-4 pt-4">
            <DocumentSection title="归属与审计" description="每位客户只有一位负责销售；协作销售显示有效期">
              {customer.partitions.audit === "error" ? (
                <BusinessFailureState
                  kind="system"
                  description="归属审计分区失败。"
                  action={
                    <Button type="button" size="sm" onClick={() => void query.refetch()}>
                      重试
                    </Button>
                  }
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
        </TabsContent>
      </Tabs>

      <DiscardConfirmDialog
        open={pendingSection != null}
        onOpenChange={(open) => {
          if (!open) setPendingSection(null)
        }}
        title="放弃未保存的修改？"
        description="编辑内容尚未保存，切换分区后将丢失。可先保存修订再切换。"
        confirmLabel="放弃并切换"
        cancelLabel="继续编辑"
        onConfirm={() => {
          const next = pendingSection
          setPendingSection(null)
          if (next) {
            setEditing(false)
            setFormDirty(false)
            selectSection(next)
          }
        }}
      />
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
