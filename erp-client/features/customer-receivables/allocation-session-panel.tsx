"use client"

import * as React from "react"
import Link from "next/link"
import { z } from "zod"
import { PlusIcon, SaveIcon, XIcon } from "lucide-react"

import {
  AllocationWorkspace,
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
import { Checkbox } from "@/components/ui/checkbox"
import {
  Field,
  FieldDescription,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import {
  usePostAllocationMutation,
  useResolvePostUnknownMutation,
  useSaveAllocationDraftMutation,
} from "@/features/customer-receivables/queries"
import type {
  AllocationDraftLine,
  AllocationSessionView,
  AllocationTarget,
  PostAllocationResult,
} from "@/features/customer-receivables/types"
import { resultText } from "@/lib/ui-text"

const factFormSchema = z.object({
  receivedAt: z.string(),
  amount: z.string(),
  bankReference: z.string(),
  invoiceCode: z.string(),
  invoiceNo: z.string(),
  invoiceDate: z.string(),
  grossAmount: z.string(),
  netAmount: z.string(),
  taxAmount: z.string(),
})

type ResultState =
  | {
      status: "succeeded" | "unknown" | "failed"
      title: string
      description: string
      reference?: string
      facts?: Array<{ label: string; value: string }>
      pendingKey?: string
      returnTo?: string
    }
  | null

function parseAmt(v: string): number {
  const n = Number(v)
  return Number.isFinite(n) ? n : 0
}

function money(n: number): string {
  return n.toFixed(2)
}

export function AllocationSessionPanel({
  session,
  onClose,
  onPosted,
}: {
  session: AllocationSessionView
  onClose: () => void
  onPosted: () => void
}) {
  const saveMutation = useSaveAllocationDraftMutation()
  const postMutation = usePostAllocationMutation()
  const resolveMutation = useResolvePostUnknownMutation()

  const [allocations, setAllocations] = React.useState<AllocationDraftLine[]>(
    () => session.allocations.map((a) => ({ ...a }))
  )
  const [editVersion, setEditVersion] = React.useState(session.editVersion)
  const [confirmOpen, setConfirmOpen] = React.useState(false)
  const [result, setResult] = React.useState<ResultState>(null)
  const [actionError, setActionError] = React.useState<string | null>(null)
  const [forceUnknown, setForceUnknown] = React.useState(false)
  const [forceCrossParty, setForceCrossParty] = React.useState(false)
  const [draftSavedAt, setDraftSavedAt] = React.useState(session.savedAt)
  const idempotencyRef = React.useRef<string | null>(null)

  React.useEffect(() => {
    setAllocations(session.allocations.map((a) => ({ ...a })))
    setEditVersion(session.editVersion)
    setDraftSavedAt(session.savedAt)
  }, [session.draftSessionId, session.editVersion, session.savedAt])

  const isReceipt = session.mode === "receipt"
  const existing = Boolean(session.existingFactId)

  const form = useAppForm({
    defaultValues: isReceipt
      ? {
          receivedAt: session.fact.receivedAt ?? "",
          amount: session.fact.amount ?? "",
          bankReference: session.fact.bankReference ?? "",
          invoiceCode: "",
          invoiceNo: "",
          invoiceDate: "",
          grossAmount: "",
          netAmount: "",
          taxAmount: "",
        }
      : {
          receivedAt: "",
          amount: "",
          bankReference: "",
          invoiceCode: session.fact.invoiceCode ?? "",
          invoiceNo: session.fact.invoiceNo ?? "",
          invoiceDate: session.fact.invoiceDate ?? "",
          grossAmount: session.fact.grossAmount ?? "",
          netAmount: session.fact.netAmount ?? "",
          taxAmount: session.fact.taxAmount ?? "",
        },
    validators: {
      onChange: factFormSchema,
    },
    onSubmit: async () => {
      setConfirmOpen(true)
    },
  })

  const factAmountStr = isReceipt
    ? String(form.state.values.amount ?? "")
    : String(form.state.values.grossAmount ?? "")
  const proposedAllocated = allocations.reduce(
    (s, a) => s + parseAmt(a.amount),
    0
  )
  const proposedUnallocated = Math.max(
    0,
    parseAmt(factAmountStr) - proposedAllocated
  )

  const issues: ValidationIssue[] = []
  if (!session.leaseValid) {
    issues.push({
      id: "lease",
      label: "处理状态",
      message: "权限已收回或处理已失效，禁止提交",
    })
  }
  for (const line of allocations) {
    if (parseAmt(line.amount) < 0) {
      issues.push({
        id: `neg-${line.lineKey}`,
        label: line.label,
        message: "分配金额不能为负",
      })
    }
    if (parseAmt(line.amount) - parseAmt(line.openAmount) > 1e-9) {
      issues.push({
        id: `over-${line.lineKey}`,
        label: line.label,
        message: `拟分配不可超过开放余额 ${line.openAmount}`,
      })
    }
  }
  if (proposedAllocated - parseAmt(factAmountStr) > 1e-9) {
    issues.push({
      id: "over-fact",
      label: "拟分配合计",
      message: "拟分配合计超过记录金额",
    })
  }

  const canSubmit =
    session.leaseValid &&
    issues.length === 0 &&
    parseAmt(factAmountStr) > 0 &&
    session.status === "draft"

  function addFromPool(target: AllocationTarget) {
    if (allocations.some((a) => a.targetId === target.targetId)) return
    if (target.counterpartyPartyId !== session.counterpartyPartyId) {
      setActionError("跨主体目标不能分配，请选择同主体目标。")
      return
    }
    setAllocations((prev) => [
      ...prev,
      {
        lineKey: `line_${target.targetId}_${Date.now()}`,
        targetId: target.targetId,
        targetKind: target.targetKind,
        label: target.label,
        salesOrderNo: target.salesOrderNo,
        openAmount: target.openAmount,
        amount: "",
        baselineVersion: target.baselineVersion,
      },
    ])
  }

  function updateAmount(lineKey: string, amount: string) {
    setAllocations((prev) =>
      prev.map((a) => (a.lineKey === lineKey ? { ...a, amount } : a))
    )
  }

  function removeLine(lineKey: string) {
    setAllocations((prev) => prev.filter((a) => a.lineKey !== lineKey))
  }

  async function doSaveDraft() {
    setActionError(null)
    try {
      const values = form.state.values
      const fact = isReceipt
        ? {
            receivedAt: values.receivedAt,
            amount: values.amount,
            bankReference: values.bankReference,
          }
        : {
            invoiceCode: values.invoiceCode,
            invoiceNo: values.invoiceNo,
            invoiceDate: values.invoiceDate,
            grossAmount: values.grossAmount,
            netAmount: values.netAmount,
            taxAmount: values.taxAmount,
            invoiceKind: "blue" as const,
          }
      const next = await saveMutation.mutateAsync({
        draftSessionId: session.draftSessionId,
        fact,
        allocations,
        editVersion,
      })
      setEditVersion(next.editVersion)
      setDraftSavedAt(next.savedAt)
    } catch (err) {
      setActionError(err instanceof Error ? err.message : "保存草稿失败")
    }
  }

  async function doPost() {
    setActionError(null)
    if (!idempotencyRef.current) {
      idempotencyRef.current = `w11-post-${session.draftSessionId}-${Date.now()}`
    }
    // 先保存草稿同步服务端
    try {
      const values = form.state.values
      const fact = isReceipt
        ? {
            receivedAt: values.receivedAt,
            amount: values.amount,
            bankReference: values.bankReference,
          }
        : {
            invoiceCode: values.invoiceCode,
            invoiceNo: values.invoiceNo,
            invoiceDate: values.invoiceDate,
            grossAmount: values.grossAmount,
            netAmount: values.netAmount,
            taxAmount: values.taxAmount,
            invoiceKind: "blue" as const,
          }
      const saved = await saveMutation.mutateAsync({
        draftSessionId: session.draftSessionId,
        fact,
        allocations,
        editVersion,
      })
      setEditVersion(saved.editVersion)

      const res = await postMutation.mutateAsync({
        draftSessionId: session.draftSessionId,
        editVersion: saved.editVersion,
        idempotencyKey: idempotencyRef.current,
        forceUnknown,
        forceCrossParty,
      })
      applyPostResult(res)
    } catch (err) {
      setActionError(err instanceof Error ? err.message : "提交失败")
      setConfirmOpen(false)
    }
  }

  function applyPostResult(res: PostAllocationResult) {
    setForceUnknown(false)
    setForceCrossParty(false)
    if (res.status === "succeeded") {
      setResult({
        status: "succeeded",
        title: isReceipt ? "回款已登记并核销" : "销项发票已登记并分配",
        description: `已生效单号 ${res.factNo}。未分配余额 ${res.unallocatedAmount}。`,
        reference: res.operationId,
        facts: [
          { label: isReceipt ? "回款单号" : "发票号码", value: res.factNo },
          { label: "净已分配", value: res.allocatedTotal },
          { label: "未分配余额", value: res.unallocatedAmount },
          { label: "往来主体", value: session.counterpartyPartyName },
        ],
        returnTo: res.returnTo,
      })
      setConfirmOpen(false)
      onPosted()
      return
    }
    if (res.status === "unknown") {
      setResult({
        status: "unknown",
        title: resultText.unknown,
        description: res.message,
        reference: res.operationId,
        pendingKey: res.idempotencyKey,
      })
      setConfirmOpen(false)
      return
    }
    if (res.code === "DUPLICATE_INVOICE") {
      setResult({
        status: "failed",
        title: "发票号码重复",
        description: res.message,
        reference: res.existingInvoiceNo,
        facts: res.existingInvoiceId
          ? [{ label: "已有发票", value: res.existingInvoiceNo ?? "" }]
          : undefined,
      })
      setConfirmOpen(false)
      return
    }
    if (res.code === "BALANCE_CONFLICT" || res.code === "OVER_ALLOCATE") {
      setActionError(res.message)
      if (res.refreshedTargets) {
        setAllocations((prev) =>
          prev.map((line) => {
            const hit = res.refreshedTargets?.find(
              (t) => t.targetId === line.targetId
            )
            return hit
              ? { ...line, openAmount: hit.openAmount }
              : line
          })
        )
      }
      setConfirmOpen(false)
      return
    }
    setActionError(res.message)
    setConfirmOpen(false)
  }

  async function resolveUnknown() {
    if (!result?.pendingKey) return
    const res = await resolveMutation.mutateAsync(result.pendingKey)
    if (res) applyPostResult(res)
  }

  const locked = existing || session.status === "posted"

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="font-heading text-lg font-semibold">
            核销 · {session.counterpartyPartyName}
          </h2>
          <p className="text-sm text-muted-foreground">
            模式：{isReceipt ? "回款核销" : "发票核销"}
            {existing ? ` · 继续单号 ${session.existingFactNo}` : null}
            {draftSavedAt ? ` · 草稿已保存` : " · 未保存草稿"}
          </p>
          <p className="mt-1 text-xs text-muted-foreground">{session.note}</p>
        </div>
        <Button type="button" variant="outline" size="sm" onClick={onClose}>
          <XIcon data-icon="inline-start" aria-hidden="true" />
          {session.returnContext?.returnTo ? "取消并返回" : "返回列表"}
        </Button>
      </div>

      {session.returnContext?.from === "W05" &&
      session.returnContext.returnTo ? (
        <Alert variant="info">
          <AlertTitle>来自销售单票款区</AlertTitle>
          <AlertDescription>
            完成或取消后可回到销售单原入口；筛选与主体在本会话内保留。
            <Button
              type="button"
              size="sm"
              variant="link"
              className="ml-2 h-auto p-0"
              render={<Link href={session.returnContext.returnTo} />}
            >
              直接返回来源
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}

      {result ? (
        <FormalActionResult
          status={result.status === "failed" ? "rejected" : result.status}
          title={result.title}
          description={result.description}
          reference={result.reference}
          facts={result.facts}
          actions={
            <>
              {result.pendingKey ? (
                <Button
                  type="button"
                  size="sm"
                  onClick={() => void resolveUnknown()}
                  disabled={resolveMutation.isPending}
                >
                  查询最终结果
                </Button>
              ) : null}
              {result.returnTo ? (
                <Button
                  type="button"
                  size="sm"
                  render={<Link href={result.returnTo} />}
                >
                  返回销售单
                </Button>
              ) : null}
              <Button type="button" size="sm" variant="outline" onClick={onClose}>
                返回列表
              </Button>
            </>
          }
        />
      ) : null}

      {actionError ? (
        <Alert variant="destructive">
          <AlertTitle>操作未成功</AlertTitle>
          <AlertDescription>{actionError}</AlertDescription>
        </Alert>
      ) : null}

      <div className="grid gap-4 lg:grid-cols-2">
        <section className="space-y-3 rounded-2xl border bg-card p-4">
          <h3 className="text-sm font-semibold">
            {isReceipt ? "回款记录" : "销项发票记录"}
          </h3>
          <p className="text-xs text-muted-foreground">
            已过账记录不可编辑删除；此处仅用于新登记或继续核销。
          </p>
          {isReceipt ? (
            <div className="space-y-3">
              <form.AppField
                name="receivedAt"
                children={(field) => (
                  <field.DateTimeField
                    label="实际到账时间"
                    disabled={locked}
                  />
                )}
              />
              <form.AppField
                name="amount"
                children={(field) => (
                  <field.TextField
                    label={existing ? "未分配余额（可分配上限）" : "到账金额（含税）"}
                    disabled={existing}
                  />
                )}
              />
              <form.AppField
                name="bankReference"
                children={(field) => (
                  <field.TextField
                    label="银行流水/回单引用"
                    disabled={locked}
                  />
                )}
              />
            </div>
          ) : (
            <div className="space-y-3">
              <form.AppField
                name="invoiceCode"
                children={(field) => (
                  <field.TextField label="发票代码" disabled={locked} />
                )}
              />
              <form.AppField
                name="invoiceNo"
                children={(field) => (
                  <field.TextField label="发票号码" disabled={locked} />
                )}
              />
              <form.AppField
                name="invoiceDate"
                children={(field) => (
                  <field.DateField
                    label="开票日期"
                    disabled={locked}
                  />
                )}
              />
              <form.AppField
                name="grossAmount"
                children={(field) => (
                  <field.TextField
                    label={existing ? "未分配含税余额" : "含税金额"}
                    disabled={existing}
                  />
                )}
              />
              <div className="grid grid-cols-2 gap-2">
                <form.AppField
                  name="netAmount"
                  children={(field) => (
                    <field.TextField label="不含税" disabled={locked} />
                  )}
                />
                <form.AppField
                  name="taxAmount"
                  children={(field) => (
                    <field.TextField label="税额" disabled={locked} />
                  )}
                />
              </div>
            </div>
          )}
        </section>

        <section className="space-y-3 rounded-2xl border bg-card p-4">
          <h3 className="text-sm font-semibold">
            同主体待核销池
            <span className="ml-2 text-xs font-normal text-muted-foreground">
              仅 {session.counterpartyPartyName}
            </span>
          </h3>
          <p className="text-xs text-muted-foreground">
            仅同主体的开放应收可分配；跨主体即使同名客户也不返回。
          </p>
          <ul className="max-h-72 space-y-2 overflow-auto">
            {session.pool.length === 0 ? (
              <li className="text-sm text-muted-foreground">
                当前主体无开放目标
              </li>
            ) : (
              session.pool.map((t) => {
                const selected = allocations.some(
                  (a) => a.targetId === t.targetId
                )
                return (
                  <li
                    key={t.targetId}
                    className="flex items-center justify-between gap-2 rounded-xl border px-3 py-2"
                  >
                    <div className="min-w-0">
                      <div className="truncate text-sm font-medium">
                        {t.label}
                      </div>
                      <div className="text-xs text-muted-foreground">
                        开放{" "}
                        <MoneyValue value={t.openAmount} taxBasis="gross" />
                        {t.dueDate ? ` · 到期 ${t.dueDate}` : null}
                      </div>
                    </div>
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      disabled={selected || session.status === "posted"}
                      onClick={() => addFromPool(t)}
                    >
                      <PlusIcon data-icon="inline-start" aria-hidden="true" />
                      加入
                    </Button>
                  </li>
                )
              })
            )}
          </ul>
        </section>
      </div>

      <AllocationWorkspace
        title="本次分配"
        description="拟分配金额仅供参考，以提交后结果为准。"
        summary={{
          totalToAllocate: (
            <MoneyValue value={factAmountStr || "0"} taxBasis="gross" />
          ),
          allocated: (
            <span>
              <MoneyValue
                value={money(proposedAllocated)}
                taxBasis="gross"
              />
              <span className="ml-1 text-xs text-muted-foreground">拟</span>
            </span>
          ),
          difference: (
            <span>
              <MoneyValue
                value={money(proposedUnallocated)}
                taxBasis="gross"
              />
              <span className="ml-1 text-xs text-muted-foreground">
                拟未分配
              </span>
            </span>
          ),
        }}
        allocations={allocations}
        getRowId={(a) => a.lineKey}
        disabled={session.status === "posted"}
        addLabel="从池中选择"
        addDisabledReason="请从左侧同主体池加入目标"
        onRemoveAllocation={(a) => removeLine(a.lineKey)}
        columns={[
          {
            id: "target",
            header: "目标",
            renderValue: ({ item }) => (
              <div>
                <div className="text-sm">{item.label}</div>
                <div className="num text-xs text-muted-foreground">
                  {item.salesOrderNo}
                </div>
              </div>
            ),
          },
          {
            id: "open",
            header: "开放余额",
            align: "end",
            numeric: true,
            renderValue: ({ item }) => (
              <MoneyValue value={item.openAmount} taxBasis="gross" />
            ),
          },
          {
            id: "amount",
            header: "分配金额",
            align: "end",
            numeric: true,
            renderValue: ({ item }) => (
              <MoneyValue value={item.amount || "0"} />
            ),
            renderEditor: ({ item }) => (
              <Input
                className="num text-right"
                value={item.amount}
                inputMode="decimal"
                aria-label={`${item.label} 分配金额`}
                onChange={(e) => updateAmount(item.lineKey, e.target.value)}
              />
            ),
          },
        ]}
        statusNotice={
          issues.length > 0 ? (
            <ValidationSummary issues={issues} title="分配校验" />
          ) : (
            <p className="text-xs text-muted-foreground">
              {session.submitPolicy.label}
            </p>
          )
        }
        actions={
          <>
            <Button
              type="button"
              variant="outline"
              disabled={
                saveMutation.isPending || session.status === "posted"
              }
              onClick={() => void doSaveDraft()}
            >
              <SaveIcon data-icon="inline-start" aria-hidden="true" />
              保存草稿
            </Button>
            <Button
              type="button"
              disabled={!canSubmit || postMutation.isPending}
              onClick={() => {
                void form.handleSubmit()
              }}
            >
              确认登记并核销
            </Button>
          </>
        }
      />

      <div className="flex flex-wrap items-center gap-4 rounded-xl border border-dashed p-3 text-xs text-muted-foreground">
        <span>演示选项：</span>
        <label className="flex items-center gap-2">
          <Checkbox
            checked={forceUnknown}
            onCheckedChange={(v) => setForceUnknown(v === true)}
          />
          模拟结果不确定
        </label>
        <label className="flex items-center gap-2">
          <Checkbox
            checked={forceCrossParty}
            onCheckedChange={(v) => setForceCrossParty(v === true)}
          />
          模拟跨主体提交拒绝
        </label>
        <Field className="max-w-xs">
          <FieldLabel className="text-xs">主体锁定</FieldLabel>
          <FieldDescription>
            会话创建后不可更换往来主体
          </FieldDescription>
        </Field>
      </div>

      <FormalActionConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title={isReceipt ? "确认登记回款并核销" : "确认登记销项发票并分配"}
        actionLabel="提交"
        confirmLabel="确认提交"
        fromStatus={{ label: "本次草稿", tone: "warning" }}
        toStatus={{
          label: isReceipt ? "已过账回款" : "已登记发票",
          tone: "success",
        }}
        lockedFields={["往来主体", "记录编号（提交后）", "既有分配行"]}
        effects={[
          "形成回款/发票记录与追加式分配明细",
          "同步更新应收开放余额与净分配（系统）",
          "未分配余额按系统策略保留并可见",
          "重复提交不会重复生成记录",
        ]}
        nextDepartment="财务"
        onConfirm={() => void doPost()}
      />
    </div>
  )
}
