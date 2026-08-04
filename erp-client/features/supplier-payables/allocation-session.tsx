"use client"

import * as React from "react"
import Link from "next/link"
import { z } from "zod"
import {
  ArrowLeftIcon,
  SaveIcon,
  ShieldAlertIcon,
} from "lucide-react"

import {
  BusinessEmptyState,
  DataFreshness,
  FormalActionConfirmDialog,
  FormalActionResult,
  MoneyValue,
  ValidationSummary,
  type ValidationIssue,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Separator } from "@/components/ui/separator"
import {
  useAllocationSessionQuery,
  useResolveUnknownMutation,
  useSaveAllocationDraftMutation,
  useSubmitInvoiceMutation,
  useSubmitPaymentMutation,
} from "@/features/supplier-payables/queries"
import type {
  AllocationTrack,
  FormalSubmitResult,
} from "@/features/supplier-payables/types"
import { SOURCE_TYPE_LABEL } from "@/features/supplier-payables/types"
import { cn } from "@/lib/utils"

const paymentSchema = z.object({
  paidAt: z.string().min(1, "请填写实际付款时间"),
  amount: z
    .string()
    .trim()
    .min(1, "请填写付款金额")
    .refine((v) => Number(v) > 0, "付款金额必须为正数"),
  bankReference: z.string().trim().min(1, "请填写银行流水引用"),
  note: z.string(),
})

const invoiceSchema = z.object({
  invoiceCode: z.string().trim().min(1, "请填写发票代码"),
  invoiceNo: z.string().trim().min(1, "请填写发票号码"),
  invoiceDate: z.string().min(1, "请填写开票日期"),
  grossAmount: z
    .string()
    .trim()
    .min(1, "请填写含税金额")
    .refine((v) => Number(v) > 0, "含税金额必须为正数"),
  netAmount: z.string().trim().min(1, "请填写不含税金额"),
  taxAmount: z.string().trim().min(1, "请填写税额"),
})

function cents(s: string): number {
  const n = Number(s)
  return Number.isFinite(n) ? Math.round(n * 100) : 0
}

function fromCents(c: number): string {
  return (c / 100).toFixed(2)
}

function todayInput(): string {
  const d = new Date()
  const pad = (n: number) => String(n).padStart(2, "0")
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}

export type AllocationSessionProps = {
  track: AllocationTrack
  supplierId: string
  draftSessionId?: string
  purchaseOrderId?: string
  returnTo?: string
  fromWorkspace?: string
  existingPaymentId?: string
  existingInvoiceId?: string
  preselectPayableAccountId?: string
  onClose: () => void
  onCompleted?: (result: FormalSubmitResult) => void
}

export function AllocationSession({
  track,
  supplierId,
  draftSessionId,
  purchaseOrderId,
  returnTo,
  fromWorkspace,
  existingPaymentId,
  existingInvoiceId,
  preselectPayableAccountId,
  onClose,
  onCompleted,
}: AllocationSessionProps) {
  const sessionQuery = useAllocationSessionQuery({
    track,
    supplierId,
    draftSessionId,
    purchaseOrderId,
    returnTo,
    fromWorkspace,
    existingPaymentId,
    existingInvoiceId,
    preselectPayableAccountId,
  })
  const submitPayment = useSubmitPaymentMutation()
  const submitInvoice = useSubmitInvoiceMutation()
  const saveDraft = useSaveAllocationDraftMutation()
  const resolveUnknown = useResolveUnknownMutation()

  const session = sessionQuery.data
  const policy = session?.payablePriorityPolicy

  const [amounts, setAmounts] = React.useState<Record<string, string>>({})
  const [selected, setSelected] = React.useState<Set<string>>(new Set())
  const [confirmOpen, setConfirmOpen] = React.useState(false)
  const [result, setResult] = React.useState<FormalSubmitResult | null>(null)
  const [forceUnknown, setForceUnknown] = React.useState(false)
  const [forceDup, setForceDup] = React.useState(false)
  const [forceConflict, setForceConflict] = React.useState(false)
  const [draftHint, setDraftHint] = React.useState<string | null>(null)
  const idempotencyRef = React.useRef<string | null>(null)

  React.useEffect(() => {
    if (!session) return
    const next = new Set(session.preselectedPayableAccountIds)
    setSelected(next)
    const am: Record<string, string> = {}
    for (const id of next) {
      const item = session.pool.find((p) => p.payableAccountId === id)
      if (!item) continue
      am[id] =
        track === "payment" ? item.openTotal : item.openInvoiceableTotal
    }
    setAmounts((prev) => ({ ...am, ...prev }))
  }, [session?.draftSessionId, session?.preselectedPayableAccountIds.join("|")])

  const paymentForm = useAppForm({
    defaultValues: {
      paidAt: todayInput(),
      amount: session?.existingUnallocated ?? session?.existingAmount ?? "",
      bankReference: "",
      note: "",
    },
    validators: { onChange: paymentSchema },
    onSubmit: async () => {
      setConfirmOpen(true)
    },
  })

  const invoiceForm = useAppForm({
    defaultValues: {
      invoiceCode: "3100251199",
      invoiceNo: "",
      invoiceDate: new Date().toISOString().slice(0, 10),
      grossAmount: session?.existingUnallocated ?? session?.existingAmount ?? "",
      netAmount: "",
      taxAmount: "",
    },
    validators: { onChange: invoiceSchema },
    onSubmit: async () => {
      setConfirmOpen(true)
    },
  })

  React.useEffect(() => {
    if (!session?.existingUnallocated) return
    if (track === "payment") {
      paymentForm.setFieldValue("amount", session.existingUnallocated)
    } else {
      invoiceForm.setFieldValue("grossAmount", session.existingUnallocated)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session?.existingUnallocated, session?.existingPaymentId, session?.existingInvoiceId])

  const factAmount =
    track === "payment"
      ? paymentForm.state.values.amount
      : invoiceForm.state.values.grossAmount

  const allocatedHint = React.useMemo(() => {
    let c = 0
    for (const id of selected) {
      c += cents(amounts[id] ?? "0")
    }
    return fromCents(c)
  }, [selected, amounts])

  const unallocatedHint = React.useMemo(() => {
    return fromCents(Math.max(0, cents(factAmount || "0") - cents(allocatedHint)))
  }, [factAmount, allocatedHint])

  const mixedSources = React.useMemo(() => {
    const types = new Set(
      session?.pool
        .filter((p) => selected.has(p.payableAccountId))
        .map((p) => p.sourceType) ?? []
    )
    return types.size > 1
  }, [session?.pool, selected])

  const policyBlocksAuto =
    mixedSources &&
    (!policy ||
      policy.state !== "AVAILABLE" ||
      !policy.mixedAutoAllocationAllowed)

  const issues: ValidationIssue[] = []
  if (selected.size === 0) {
    issues.push({
      id: "no-target",
      label: "核销目标",
      message: "请至少选择一笔同供应商应付",
      targetId: "alloc-pool",
    })
  }
  const capAmount =
    session?.existingUnallocated ||
    session?.existingAmount ||
    factAmount ||
    "0"
  if (
    cents(factAmount || "0") <= 0 &&
    !session?.existingPaymentId &&
    !session?.existingInvoiceId
  ) {
    issues.push({
      id: "amount",
      label: track === "payment" ? "付款金额" : "发票金额",
      message: "金额必须为正数",
    })
  }
  if (cents(allocatedHint) > cents(capAmount)) {
    issues.push({
      id: "over",
      label: "拟分配",
      message: "拟分配合计超过本次记录金额，最终以系统校验为准",
    })
  }
  for (const id of selected) {
    const item = session?.pool.find((p) => p.payableAccountId === id)
    if (!item) continue
    const open =
      track === "payment" ? item.openTotal : item.openInvoiceableTotal
    if (cents(amounts[id] ?? "0") > cents(open)) {
      issues.push({
        id: `over-${id}`,
        label: item.sourceDocumentNo,
        message: `拟分配超过开放余额 ${open}`,
      })
    }
    if (cents(amounts[id] ?? "0") <= 0) {
      issues.push({
        id: `zero-${id}`,
        label: item.sourceDocumentNo,
        message: "分配金额须为正数",
      })
    }
  }
  if (policyBlocksAuto && selected.size > 0) {
    // explicit selection is OK — only warn
  }

  const canSubmit = issues.length === 0 && !result

  async function handleSaveDraft() {
    if (!session) return
    const saved = await saveDraft.mutateAsync({
      draftSessionId: session.draftSessionId,
      track,
      supplierId,
      formSnapshot: {
        amounts,
        selected: [...selected],
        payment: paymentForm.state.values,
        invoice: invoiceForm.state.values,
      },
    })
    setDraftHint(`草稿已保存 ${new Date(saved.savedAt).toLocaleTimeString("zh-CN")}`)
  }

  async function doSubmit() {
    if (!session) return
    if (!idempotencyRef.current) {
      idempotencyRef.current = `w12_${track}_${session.draftSessionId}_${Date.now()}`
    }
    const targets = [...selected].map((payableAccountId) => {
      const item = session.pool.find((p) => p.payableAccountId === payableAccountId)!
      return {
        payableAccountId,
        payableEntryId: item.primaryEntryId,
        amount: amounts[payableAccountId] || "0",
        entryLockVersion: item.entryLockVersion,
        accountLockVersion: item.accountLockVersion,
      }
    })

    const explicitSelection = true // UI always collects explicit picks

    let res: FormalSubmitResult
    if (track === "payment") {
      const v = paymentForm.state.values
      res = await submitPayment.mutateAsync({
        draftSessionId: session.draftSessionId,
        supplierId,
        paidAt: v.paidAt,
        amount: session.existingPaymentId
          ? session.existingAmount ?? v.amount
          : v.amount,
        bankReference: v.bankReference,
        note: v.note,
        targets,
        payablePriorityPolicyId: policy?.payablePriorityPolicyId,
        payablePriorityPolicyVersion: policy?.payablePriorityPolicyVersion,
        explicitSelection,
        existingPaymentId: session.existingPaymentId,
        idempotencyKey: idempotencyRef.current,
        forceUnknown,
        forceVersionConflict: forceConflict,
      })
    } else {
      const v = invoiceForm.state.values
      res = await submitInvoice.mutateAsync({
        draftSessionId: session.draftSessionId,
        supplierId,
        invoiceCode: v.invoiceCode,
        invoiceNo: v.invoiceNo,
        invoiceDate: v.invoiceDate,
        grossAmount: session.existingInvoiceId
          ? session.existingAmount ?? v.grossAmount
          : v.grossAmount,
        netAmount: v.netAmount,
        taxAmount: v.taxAmount,
        invoiceKind: "BLUE",
        targets,
        payablePriorityPolicyId: policy?.payablePriorityPolicyId,
        payablePriorityPolicyVersion: policy?.payablePriorityPolicyVersion,
        explicitSelection,
        existingInvoiceId: session.existingInvoiceId,
        idempotencyKey: idempotencyRef.current,
        forceUnknown,
        forceDuplicateInvoice: forceDup,
        forceVersionConflict: forceConflict,
      })
    }

    setConfirmOpen(false)
    setResult({ ...res, returnTo: returnTo ?? session.returnTo })
    if (res.status === "succeeded") {
      onCompleted?.(res)
    }
  }

  if (sessionQuery.isPending) {
    return (
      <div className="space-y-3 p-1">
        <div className="h-10 w-64 animate-pulse rounded-lg bg-muted" />
        <div className="grid gap-3 md:grid-cols-2">
          <div className="h-72 animate-pulse rounded-2xl bg-muted" />
          <div className="h-72 animate-pulse rounded-2xl bg-muted" />
        </div>
      </div>
    )
  }

  if (sessionQuery.isError || !session) {
    return (
      <BusinessEmptyState
        kind="no-data"
        title="无法开始本次核销"
        description={
          sessionQuery.error instanceof Error
            ? sessionQuery.error.message
            : "供应商核销池加载失败"
        }
        action={
          <Button type="button" variant="outline" onClick={onClose}>
            返回列表
          </Button>
        }
      />
    )
  }

  return (
    <section className="space-y-4" aria-label="供应商核销工作区">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 space-y-1">
          <div className="flex flex-wrap items-center gap-2">
            <Button type="button" variant="ghost" size="sm" onClick={onClose}>
              <ArrowLeftIcon className="size-4" />
              返回列表
            </Button>
            <h2 className="text-lg font-semibold tracking-tight">
              核销 · {session.supplierName}
            </h2>
            <span className="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground">
              {track === "payment" ? "付款核销" : "进项票核销"}
            </span>
          </div>
          <p className="text-sm text-muted-foreground">
            供应商已锁定；核销池仅含该供应商开放目标。采购单与结算单可混合，不同供应商禁止混入。
            {session.existingDocumentNo
              ? ` · 继续核销 ${session.existingDocumentNo}`
              : null}
          </p>
          <DataFreshness
            updatedAt={new Date(session.queriedAt).toLocaleString("zh-CN")}
            dateTime={session.queriedAt}
            label={`更新于 ${session.dataWatermark} · 查询于`}
            className="text-xs"
          />
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => void handleSaveDraft()}
            disabled={saveDraft.isPending || Boolean(result)}
          >
            <SaveIcon className="size-4" />
            保存草稿
          </Button>
        </div>
      </div>

      {draftHint ? (
        <p className="text-xs text-muted-foreground">{draftHint}（不形成业务记录）</p>
      ) : null}

      {policy && policy.state !== "AVAILABLE" ? (
        <Alert>
          <ShieldAlertIcon />
          <AlertTitle>应付优先级策略不可用</AlertTitle>
          <AlertDescription>
            {policy.blockerMessage}
            混合自动分配已禁用；请显式勾选目标并填写金额。
          </AlertDescription>
        </Alert>
      ) : null}

      {fromWorkspace || purchaseOrderId ? (
        <Alert>
          <AlertTitle>来源上下文</AlertTitle>
          <AlertDescription>
            {fromWorkspace ? `来自 ${fromWorkspace}` : null}
            {purchaseOrderId ? ` · 采购单 ${purchaseOrderId}` : null}
            。完成后请返回来源页，将重新校验付款条件；未核销付款不满足先款要求。
          </AlertDescription>
        </Alert>
      ) : null}

      {result ? (
        <div className="space-y-3">
          <FormalActionResult
            status={
              result.status === "succeeded"
                ? "succeeded"
                : result.status === "unknown"
                  ? "unknown"
                  : result.status === "blocked"
                    ? "blocked"
                    : "rejected"
            }
            title={result.title}
            description={result.description}
            reference={result.reference ?? result.operationId}
            facts={result.facts}
          />
          {result.status === "unknown" && idempotencyRef.current ? (
            <Button
              type="button"
              variant="outline"
              onClick={async () => {
                const r = await resolveUnknown.mutateAsync(idempotencyRef.current!)
                if (r) {
                  setResult({ ...r, returnTo: returnTo ?? session.returnTo })
                  if (r.status === "succeeded") onCompleted?.(r)
                }
              }}
            >
              按操作号查询最终结果
            </Button>
          ) : null}
          {result.status === "blocked" && result.existingDocumentId ? (
            <p className="text-sm text-muted-foreground">
              已定位既有发票，不创建副本。可切换到进项发票视图继续核销。
            </p>
          ) : null}
          <div className="flex flex-wrap gap-2">
            <Button type="button" variant="outline" onClick={onClose}>
              回到列表
            </Button>
            {(result.returnTo || returnTo) && result.status === "succeeded" ? (
              <Button
                type="button"
                render={
                  <Link href={result.returnTo || returnTo || "/"} />
                }
              >
                返回来源并重查门禁
              </Button>
            ) : null}
          </div>
        </div>
      ) : null}

      {!result ? (
        <>
          <div className="grid gap-4 lg:grid-cols-2">
            <Card id="alloc-pool">
              <CardHeader className="border-b border-border">
                <CardTitle className="text-base">同供应商待核销池</CardTitle>
                <CardDescription>
                  仅 {session.supplierName} ·{" "}
                  {track === "payment" ? "开放应付" : "可收票余额"}
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-2 p-3">
                {session.pool.length === 0 ? (
                  <p className="p-3 text-sm text-muted-foreground">
                    当前无开放目标
                  </p>
                ) : (
                  session.pool.map((item) => {
                    const checked = selected.has(item.payableAccountId)
                    const open =
                      track === "payment"
                        ? item.openTotal
                        : item.openInvoiceableTotal
                    return (
                      <div
                        key={item.payableAccountId}
                        className={cn(
                          "flex flex-col gap-2 rounded-xl border p-3",
                          checked ? "border-primary/40 bg-primary/5" : "border-border"
                        )}
                      >
                        <div className="flex items-start gap-2">
                          <Checkbox
                            checked={checked}
                            onCheckedChange={(v) => {
                              setSelected((prev) => {
                                const next = new Set(prev)
                                if (v) next.add(item.payableAccountId)
                                else next.delete(item.payableAccountId)
                                return next
                              })
                              if (v && !amounts[item.payableAccountId]) {
                                setAmounts((m) => ({
                                  ...m,
                                  [item.payableAccountId]: open,
                                }))
                              }
                            }}
                            aria-label={`选择 ${item.sourceDocumentNo}`}
                          />
                          <div className="min-w-0 flex-1">
                            <div className="flex flex-wrap items-center gap-2 text-sm">
                              <span className="font-medium">
                                {SOURCE_TYPE_LABEL[item.sourceType]}
                              </span>
                              <span className="num">{item.sourceDocumentNo}</span>
                              <span className="text-xs text-muted-foreground">
                                {item.dueStateLabel} · {item.dueDate}
                              </span>
                            </div>
                            <div className="mt-1 flex flex-wrap items-center justify-between gap-2 text-sm">
                              <span className="text-muted-foreground">
                                开放余额
                              </span>
                              <MoneyValue value={open} taxBasis="gross" />
                            </div>
                          </div>
                        </div>
                        {checked ? (
                          <div className="flex items-center gap-2 pl-6">
                            <Label
                              htmlFor={`amt-${item.payableAccountId}`}
                              className="text-xs whitespace-nowrap"
                            >
                              本次分配
                            </Label>
                            <Input
                              id={`amt-${item.payableAccountId}`}
                              className="num h-8"
                             
                              value={amounts[item.payableAccountId] ?? ""}
                              onChange={(e) =>
                                setAmounts((m) => ({
                                  ...m,
                                  [item.payableAccountId]: e.target.value,
                                }))
                              }
                            />
                          </div>
                        ) : null}
                      </div>
                    )
                  })
                )}
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="border-b border-border">
                <CardTitle className="text-base">
                  {track === "payment" ? "本次付款记录" : "本次进项发票记录"}
                </CardTitle>
                <CardDescription>
                  未分配余额以提交后的系统结果为准
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-4 pt-4">
                {track === "payment" ? (
                  <form
                    className="space-y-3"
                    onSubmit={(e) => {
                      e.preventDefault()
                      void paymentForm.handleSubmit()
                    }}
                  >
                    {!session.existingPaymentId ? (
                      <>
                        <paymentForm.AppField
                          name="paidAt"
                          children={(field) => (
                            <field.DateTimeField label="实际付款时间" />
                          )}
                        />
                        <paymentForm.AppField
                          name="amount"
                          children={(field) => (
                            <field.TextField label="付款金额（含税）" />
                          )}
                        />
                        <paymentForm.AppField
                          name="bankReference"
                          children={(field) => (
                            <field.TextField label="银行流水引用" />
                          )}
                        />
                        <paymentForm.AppField
                          name="note"
                          children={(field) => (
                            <field.TextareaField label="备注（可选）" />
                          )}
                        />
                      </>
                    ) : (
                      <div className="rounded-lg bg-muted/50 p-3 text-sm">
                        <div>原付款 {session.existingDocumentNo}</div>
                        <div className="mt-1 flex justify-between">
                          <span className="text-muted-foreground">未分配余额</span>
                          <MoneyValue
                            value={session.existingUnallocated}
                            taxBasis="gross"
                          />
                        </div>
                      </div>
                    )}
                  </form>
                ) : (
                  <form
                    className="space-y-3"
                    onSubmit={(e) => {
                      e.preventDefault()
                      void invoiceForm.handleSubmit()
                    }}
                  >
                    {!session.existingInvoiceId ? (
                      <>
                        <invoiceForm.AppField
                          name="invoiceCode"
                          children={(field) => (
                            <field.TextField label="发票代码" />
                          )}
                        />
                        <invoiceForm.AppField
                          name="invoiceNo"
                          children={(field) => (
                            <field.TextField label="发票号码" />
                          )}
                        />
                        <invoiceForm.AppField
                          name="invoiceDate"
                          children={(field) => (
                            <field.DateField label="开票日期" />
                          )}
                        />
                        <invoiceForm.AppField
                          name="grossAmount"
                          children={(field) => (
                            <field.TextField label="含税金额" />
                          )}
                        />
                        <div className="grid grid-cols-2 gap-2">
                          <invoiceForm.AppField
                            name="netAmount"
                            children={(field) => (
                              <field.TextField label="不含税" />
                            )}
                          />
                          <invoiceForm.AppField
                            name="taxAmount"
                            children={(field) => (
                              <field.TextField label="税额" />
                            )}
                          />
                        </div>
                      </>
                    ) : (
                      <div className="rounded-lg bg-muted/50 p-3 text-sm">
                        <div>原发票 {session.existingDocumentNo}</div>
                        <div className="mt-1 flex justify-between">
                          <span className="text-muted-foreground">未分配余额</span>
                          <MoneyValue
                            value={session.existingUnallocated}
                            taxBasis="gross"
                          />
                        </div>
                      </div>
                    )}
                  </form>
                )}

                <Separator />

                <dl className="grid grid-cols-3 gap-2 text-sm">
                  <div>
                    <dt className="text-xs text-muted-foreground">记录金额</dt>
                    <dd>
                      <MoneyValue value={factAmount || "0"} />
                    </dd>
                  </div>
                  <div>
                    <dt className="text-xs text-muted-foreground">拟分配</dt>
                    <dd>
                      <MoneyValue value={allocatedHint} />
                    </dd>
                  </div>
                  <div>
                    <dt className="text-xs text-muted-foreground">拟未分配</dt>
                    <dd>
                      <MoneyValue value={unallocatedHint} />
                    </dd>
                  </div>
                </dl>

                {mixedSources ? (
                  <p className="text-xs text-muted-foreground">
                    已选择混合来源（采购单 + 结算单）。
                    {policyBlocksAuto
                      ? "策略不可用，已强制显式选择。"
                      : `策略 ${policy?.payablePriorityPolicyId}@v${policy?.payablePriorityPolicyVersion}；提交将回传策略版本。`}
                  </p>
                ) : null}

                <ValidationSummary issues={issues} />

                <details className="text-xs text-muted-foreground">
                  <summary className="cursor-pointer">演示：异常路径</summary>
                  <div className="mt-2 flex flex-col gap-1">
                    <label className="flex items-center gap-2">
                      <Checkbox
                        checked={forceUnknown}
                        onCheckedChange={(v) => setForceUnknown(Boolean(v))}
                      />
                      模拟结果不确定
                    </label>
                    {track === "purchase_invoice" ? (
                      <label className="flex items-center gap-2">
                        <Checkbox
                          checked={forceDup}
                          onCheckedChange={(v) => setForceDup(Boolean(v))}
                        />
                        模拟重复发票
                      </label>
                    ) : null}
                    <label className="flex items-center gap-2">
                      <Checkbox
                        checked={forceConflict}
                        onCheckedChange={(v) => setForceConflict(Boolean(v))}
                      />
                      模拟并发版本冲突
                    </label>
                  </div>
                </details>
              </CardContent>
              <CardFooter className="justify-end gap-2 border-t border-border">
                <Button type="button" variant="outline" onClick={onClose}>
                  取消
                </Button>
                <Button
                  type="button"
                  disabled={!canSubmit || submitPayment.isPending || submitInvoice.isPending}
                  onClick={() => {
                    if (session.existingPaymentId || session.existingInvoiceId) {
                      setConfirmOpen(true)
                      return
                    }
                    if (track === "payment") void paymentForm.handleSubmit()
                    else void invoiceForm.handleSubmit()
                  }}
                >
                  确认登记并核销
                </Button>
              </CardFooter>
            </Card>
          </div>
        </>
      ) : null}

      <FormalActionConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        actionLabel={track === "payment" ? "登记付款并核销" : "登记进项发票并核销"}
        title={track === "payment" ? "确认登记付款并核销" : "确认登记进项发票并核销"}
        description="提交后形成不可编辑记录；纠错须追加冲正/红票。提交时系统将校验供应商、余额与策略版本。"
        confirmLabel="确认提交"
        fromStatus={{ label: "本次草稿", tone: "neutral" }}
        toStatus={{ label: "已确认", tone: "success" }}
        lockedFields={[
          `供应商 ${session.supplierName}`,
          `目标 ${selected.size} 笔`,
          `拟分配 ${allocatedHint}`,
        ]}
        effects={[
          track === "payment"
            ? "形成供应商付款单与有效 APPLY 分配"
            : "形成进项发票与有效 APPLY 分配",
          "同步更新应付开放余额",
          "未分配余额保留在待核销视图",
          "来源页须重查付款门禁，未核销付款不满足",
        ]}
        irreversibleEffects={["已确认记录不可编辑删除，纠错追加反向记录"]}
        pending={submitPayment.isPending || submitInvoice.isPending}
        onConfirm={() => void doSubmit()}
      />
    </section>
  )
}
