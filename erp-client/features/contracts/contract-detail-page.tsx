"use client"

import * as React from "react"
import Link from "next/link"
import {
  ArrowLeftIcon,
  FilePenLineIcon,
  HistoryIcon,
  LockIcon,
  PlusIcon,
  PrinterIcon,
  ShieldAlertIcon,
} from "lucide-react"

import {
  BusinessStatusBadge,
  DataFreshness,
  DocumentAttachmentList,
  DocumentHeader,
  DocumentSummary,
  FormalActionConfirmDialog,
  FormalActionResult,
  MoneyValue,
  PageActions,
  PageHeader,
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
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { ContractPaperDialog } from "@/features/contracts/contract-paper-dialog"
import {
  useActivateContractMutation,
  useContractCenterQuery,
  useReviseContractMutation,
} from "@/features/contracts/queries"

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

export function ContractDetailPage({
  contractId,
  section,
}: {
  contractId: string
  section?: string
}) {
  const query = useContractCenterQuery(contractId)
  const activateMutation = useActivateContractMutation()
  const reviseMutation = useReviseContractMutation()

  const activeSection = resolveSection(section)
  const [paperOpen, setPaperOpen] = React.useState(false)
  const [activateConfirmOpen, setActivateConfirmOpen] = React.useState(false)
  const [reviseConfirmOpen, setReviseConfirmOpen] = React.useState(false)
  const [result, setResult] = React.useState<{
    status: "succeeded" | "blocked" | "rejected"
    title: string
    description: string
    reference: string
    facts: Array<{ label: string; value: string }>
    nextStep?: string
  } | null>(null)

  const contract = query.data

  if (query.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="合同" description="正在加载对象中心…" />
      </div>
    )
  }

  if (!contract) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title="合同不存在"
          description={`未找到编号为 ${contractId} 的合同。`}
          actions={
            <Button render={<Link href="/sales/contracts" />}>返回列表</Button>
          }
        />
      </div>
    )
  }

  const baseHref = `/sales/contracts/${contract.contractId}`
  const canActivate = contract.allowedActions.includes("ACTIVATE")
  const canRevise = contract.allowedActions.includes("REVISE")
  const canCreateSo = contract.allowedActions.includes("CREATE_SALES_ORDER")
  const canPrint = contract.allowedActions.includes("PRINT")
  const reviseBlocker = contract.actionBlockers.find((b) => b.action === "REVISE")
  const soBlocker = contract.actionBlockers.find(
    (b) => b.action === "CREATE_SALES_ORDER"
  )
  const activateBlocker = contract.actionBlockers.find(
    (b) => b.action === "ACTIVATE"
  )
  const policyMissing =
    contract.status === "EFFECTIVE" && !contract.contractRevisionPolicy

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

  const handleActivate = async () => {
    try {
      const data = await activateMutation.mutateAsync({
        contractId: contract.contractId,
        expectedLockVersion: contract.lockVersion,
        idempotencyKey: `act-${contract.contractId}-${contract.lockVersion}`,
      })
      setActivateConfirmOpen(false)
      setResult({
        status: "succeeded",
        title: "合同已生效",
        description: data.nextStep,
        reference: data.reference,
        facts: [
          { label: "合同号", value: data.contractNo },
          { label: "修订号", value: `v${data.revisionNo}` },
          {
            label: "生效时间",
            value: data.effectiveAt.slice(0, 19).replace("T", " "),
          },
          { label: "下一步", value: data.nextStep },
        ],
        nextStep: data.nextStep,
      })
    } catch (error) {
      setActivateConfirmOpen(false)
      const message =
        error instanceof Error ? error.message : "ACTIVATE_FAILED"
      setResult({
        status: message === "VERSION_CONFLICT" ? "blocked" : "rejected",
        title:
          message === "VERSION_CONFLICT"
            ? "版本冲突，未生效"
            : "生效失败",
        description:
          message === "VERSION_CONFLICT"
            ? "服务端 lockVersion 已变化，请刷新后重做。未乐观改正式状态。"
            : `服务端拒绝生效（${message}）。`,
        reference: `ERR-${message}`,
        facts: [
          { label: "合同号", value: contract.contractNo },
          { label: "期望版本", value: String(contract.lockVersion) },
        ],
      })
    }
  }

  const handleRevise = async () => {
    try {
      const data = await reviseMutation.mutateAsync({
        contractId: contract.contractId,
        expectedLockVersion: contract.lockVersion,
        idempotencyKey: `rev-${contract.contractId}-${contract.lockVersion}`,
      })
      setReviseConfirmOpen(false)
      setResult({
        status: "succeeded",
        title: "修订工作副本已建立",
        description: data.nextStep,
        reference: data.reference,
        facts: [
          { label: "合同号", value: data.contractNo },
          { label: "基线修订", value: `v${data.baseRevisionNo}` },
          { label: "工作副本", value: `v${data.workingRevisionNo}` },
          {
            label: "创建时间",
            value: data.createdAt.slice(0, 19).replace("T", " "),
          },
          { label: "下一步", value: data.nextStep },
        ],
        nextStep: data.nextStep,
      })
    } catch (error) {
      setReviseConfirmOpen(false)
      const message =
        error instanceof Error ? error.message : "REVISE_FAILED"
      setResult({
        status: "blocked",
        title: "无法创建修订",
        description:
          message === "REVISION_POLICY_MISSING"
            ? "修订规则尚未配置：已生效合同保持只读，服务端拒绝创建工作副本。"
            : `服务端拒绝修订（${message}）。`,
        reference: `ERR-${message}`,
        facts: [
          { label: "合同号", value: contract.contractNo },
          {
            label: "策略",
            value: contract.contractRevisionPolicy
              ? contract.contractRevisionPolicy.policyVersion
              : "未配置",
          },
        ],
      })
    }
  }

  const rev = contract.currentRevision

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title={`合同 · ${contract.contractNo}`}
        description="稳定合同对象中心：维护草稿、查看版本 diff、打开关联销售单；修订仅在服务端策略允许时可用。"
        breadcrumbs={[
          { id: "sales", label: "销售", href: "/sales/orders" },
          { id: "contracts", label: "合同", href: "/sales/contracts" },
          {
            id: "detail",
            label: contract.contractNo,
            current: true,
          },
        ]}
        metadata={
          <DataFreshness
            updatedAt="刚刚"
            dateTime={contract.queriedAt}
            state="fresh"
            label="对象数据"
          />
        }
        actions={
          <PageActions
            actions={[
              {
                actionKey: "back",
                label: "返回列表",
                icon: ArrowLeftIcon,
                variant: "outline",
                render: <Link href="/sales/contracts" />,
              },
              {
                actionKey: "print",
                label: "打印预览",
                icon: PrinterIcon,
                variant: "outline",
                disabled: !canPrint,
                onClick: () => setPaperOpen(true),
              },
              {
                actionKey: "create-so",
                label: "新建销售单",
                icon: PlusIcon,
                variant: "outline",
                disabled: !canCreateSo,
                onClick: () => {
                  if (!canCreateSo) return
                  // 深链到 W05，携带稳定 ID 与当前修订
                  window.location.assign(
                    `/sales/orders?customerId=${encodeURIComponent(
                      contract.customer.id
                    )}&contractId=${encodeURIComponent(
                      contract.contractId
                    )}&contractRevisionId=${encodeURIComponent(
                      rev.revisionId
                    )}`
                  )
                },
              },
            ]}
          />
        }
      />

      <DocumentHeader
        title={contract.customer.displayName}
        documentNumber={contract.contractNo}
        version={rev.revisionNo}
        primaryStatus={{
          label: contract.statusLabel,
          tone: contract.statusTone,
        }}
        primaryAction={
          canActivate ? (
            <Button
              type="button"
              size="sm"
              disabled={activateMutation.isPending}
              onClick={() => setActivateConfirmOpen(true)}
            >
              提交生效
            </Button>
          ) : canCreateSo ? (
            <Button
              type="button"
              size="sm"
              render={
                <Link
                  href={`/sales/orders?customerId=${encodeURIComponent(
                    contract.customer.id
                  )}&contractId=${encodeURIComponent(
                    contract.contractId
                  )}&contractRevisionId=${encodeURIComponent(rev.revisionId)}`}
                />
              }
            >
              新建销售单
            </Button>
          ) : (
            <Button type="button" size="sm" variant="outline" disabled>
              {soBlocker?.message ?? "当前无主动作"}
            </Button>
          )
        }
        secondaryActions={
          <>
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={!canRevise || reviseMutation.isPending}
              title={reviseBlocker?.message}
              onClick={() => {
                if (canRevise) setReviseConfirmOpen(true)
              }}
            >
              <FilePenLineIcon data-icon="inline-start" aria-hidden="true" />
              创建新修订
            </Button>
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
          </>
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
        <span>{contract.ownerLabel}</span>
        {policyMissing ? (
          <Badge variant="warning">
            <LockIcon data-icon="inline-start" aria-hidden="true" />
            修订规则尚未配置
          </Badge>
        ) : null}
        {contract.contractRevisionPolicy ? (
          <Badge variant="info">
            修订策略 {contract.contractRevisionPolicy.policyVersion} ·{" "}
            {contract.contractRevisionPolicy.mode === "DIRECT_REVISION"
              ? "直接修订"
              : "变更申请"}
          </Badge>
        ) : null}
      </div>

      {policyMissing ? (
        <Alert variant="warning">
          <ShieldAlertIcon aria-hidden="true" />
          <AlertTitle>修订策略未配置（fail-closed）</AlertTitle>
          <AlertDescription>
            已生效合同保持只读查看。服务端不返回 REVISE，直接请求创建修订工作副本也会被拒绝。配置{" "}
            <span className="num">contractRevisionPolicy</span>{" "}
            并重新返回 REVISE 后才开放。
          </AlertDescription>
        </Alert>
      ) : null}

      {!canCreateSo && soBlocker ? (
        <p className="text-xs text-muted-foreground">
          新建销售单不可用：{soBlocker.message}
        </p>
      ) : null}

      {!canActivate && activateBlocker && contract.status === "DRAFT" ? (
        <p className="text-xs text-muted-foreground">
          生效不可用：{activateBlocker.message}
        </p>
      ) : null}

      {!canRevise && reviseBlocker ? (
        <p className="text-xs text-muted-foreground">
          修订不可用：{reviseBlocker.message}
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
              render={<Link href={item.href} />}
            >
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
          facts={result.facts}
          actions={
            result.status === "succeeded" && canCreateSo ? (
              <Button
                type="button"
                size="sm"
                render={
                  <Link
                    href={`/sales/orders?contractId=${encodeURIComponent(
                      contract.contractId
                    )}`}
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
                onClick={() => void query.refetch()}
              >
                刷新对象
              </Button>
            )
          }
        />
      ) : null}

      {activeSection === "overview" ? (
        <div className="grid gap-4 lg:grid-cols-2">
          <Card size="sm">
            <CardHeader className="border-b">
              <CardTitle>概览</CardTitle>
              <CardDescription>
                合同身份、客户、状态、版本与有效期。不展示未定义的“合同金额”。
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
                    value: `${rev.validFrom} ~ ${rev.validTo}`,
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
                    value: contract.ownerLabel,
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
                金额仅为各销售单摘要，不汇总为合同金额。水位{" "}
                <span className="num">{contract.relatedSalesOrdersAsOf}</span>
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-2">
              <p className="text-sm">
                关联 {contract.relatedSalesOrders.length} 张
                {contract.relatedSalesOrders.length > 0
                  ? "（下列为服务端返回明细）"
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
              当前合同修订的结构化快照；销售单使用时再固定具体版本。
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
                  label: "税号（掩码）",
                  value: rev.invoiceRequirementSnapshot.taxIdMasked ?? "—",
                  numeric: true,
                },
              ]}
            />
          </CardContent>
        </Card>
      ) : null}

      {activeSection === "attachments" ? (
        <DocumentAttachmentList
          title="合同附件"
          uploadDisabled={!contract.allowedActions.includes("UPLOAD_ATTACHMENT")}
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
            onOpen: file.canDownload
              ? () => {
                  // 演示：短时链接下载不真实发起
                }
              : undefined,
          }))}
        />
      ) : null}

      {activeSection === "sales-orders" ? (
        <Card size="sm">
          <CardHeader className="border-b">
            <CardTitle>关联销售单</CardTitle>
            <CardDescription>
              追溯哪张单使用哪个合同版本。金额只作单据摘要。
              as-of{" "}
              <span className="num">{contract.relatedSalesOrdersAsOf}</span>
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
                      <TableHead>三轨进度</TableHead>
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
                历史修订不可编辑；销售单链接的版本可在关联列表中对照。
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
                        {item.validFrom} ~ {item.validTo}
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
                创建、修订、生效、终止、附件下载等正式动作。
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
                        {event.action}
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

      <FormalActionConfirmDialog
        open={activateConfirmOpen}
        onOpenChange={setActivateConfirmOpen}
        title="确认将合同内容生效"
        description="生效后将生成不可变合同修订并固定结果。请核对客户、版本、有效期与条款摘要。"
        actionLabel="生效"
        confirmLabel="确认生效"
        fromStatus={{ label: contract.statusLabel, tone: contract.statusTone }}
        toStatus={{ label: "生效", tone: "success" }}
        lockedFields={[
          `合同号 ${contract.contractNo}`,
          `客户 ${contract.customer.displayName}`,
          `版本 ${rev.revisionNo <= 0 ? "草稿 → v1" : `v${rev.revisionNo}`}`,
          `有效期 ${rev.validFrom} ~ ${rev.validTo}`,
        ]}
        effects={[
          "生成不可变合同修订并固定结果",
          rev.termsSummary,
          "新销售单可引用当前修订快照",
        ]}
        irreversibleEffects={["生效后历史销售引用的旧修订不会被替换"]}
        pending={activateMutation.isPending}
        onConfirm={handleActivate}
      />

      <FormalActionConfirmDialog
        open={reviseConfirmOpen}
        onOpenChange={setReviseConfirmOpen}
        title="确认创建新修订"
        description="将按已配置策略在同一合同页签建立可编辑工作副本；历史销售单快照不受影响。"
        actionLabel="创建修订"
        confirmLabel="创建工作副本"
        fromStatus={{ label: contract.statusLabel, tone: contract.statusTone }}
        toStatus={{ label: "修订工作副本", tone: "info" }}
        lockedFields={[
          `合同号 ${contract.contractNo}`,
          `当前修订 v${rev.revisionNo}`,
          contract.contractRevisionPolicy
            ? `策略 ${contract.contractRevisionPolicy.policyVersion} · ${contract.contractRevisionPolicy.mode}`
            : "策略未配置（应已阻断）",
        ]}
        effects={[
          "在同一合同页签建立可编辑工作副本",
          "历史销售单快照不受影响",
          contract.contractRevisionPolicy
            ? `必需证据：${contract.contractRevisionPolicy.requiredEvidenceCodes.join("、")}`
            : "无策略",
        ]}
        pending={reviseMutation.isPending}
        onConfirm={handleRevise}
      />
    </div>
  )
}
