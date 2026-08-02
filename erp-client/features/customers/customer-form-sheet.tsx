"use client"

import * as React from "react"
import { z } from "zod"

import {
  ConflictResolutionDialog,
  FormalActionResult,
  OptionCombobox,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import {
  PAYMENT_TERM_OPTIONS,
  paymentTermLabel,
} from "@/lib/business-options"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"
import {
  useCreateCustomerMutation,
  useQueryCustomerIdempotencyMutation,
  useSaveCustomerRevisionMutation,
} from "@/features/customers/queries"
import type {
  CustomerCenterView,
  CustomerMutationResult,
} from "@/features/customers/types"

const createSchema = z.object({
  legalName: z.string().trim().min(2, "请填写法定名称"),
  shortName: z.string(),
  unifiedCreditCode: z.string(),
  defaultPaymentTerm: z.string(),
  changeReason: z.string(),
})

const reviseSchema = z.object({
  legalName: z.string().trim().min(2, "请填写法定名称"),
  shortName: z.string(),
  unifiedCreditCode: z.string(),
  defaultPaymentTerm: z.string(),
  changeReason: z.string().trim().min(2, "请填写修订原因"),
})

type FormValues = {
  legalName: string
  shortName: string
  unifiedCreditCode: string
  defaultPaymentTerm: string
  changeReason: string
}

function newIdempotencyKey(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

export function CustomerCreateSheet({
  open,
  onOpenChange,
  onSucceeded,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  onSucceeded?: (customerId: string) => void
}) {
  const createMutation = useCreateCustomerMutation()
  const queryIdempotency = useQueryCustomerIdempotencyMutation()
  const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
    newIdempotencyKey("create")
  )
  const [simulate, setSimulate] = React.useState<"ok" | "conflict" | "unknown">(
    "ok"
  )
  const [result, setResult] = React.useState<CustomerMutationResult | null>(
    null
  )
  const [conflictOpen, setConflictOpen] = React.useState(false)

  const form = useAppForm({
    defaultValues: {
      legalName: "",
      shortName: "",
      unifiedCreditCode: "",
      defaultPaymentTerm: "POSTPAY_NET30",
      changeReason: "",
    } satisfies FormValues,
    validators: { onChange: createSchema },
    onSubmit: async ({ value }) => {
      const response = await createMutation.mutateAsync({
        legalName: value.legalName.trim(),
        shortName: value.shortName.trim() || undefined,
        unifiedCreditCode: value.unifiedCreditCode.trim() || undefined,
        defaultPaymentTerm: value.defaultPaymentTerm.trim()
          ? paymentTermLabel(value.defaultPaymentTerm.trim())
          : undefined,
        ownerUserId: "u_current",
        ownerName: "当前用户",
        idempotencyKey,
        simulate,
      })
      setResult(response)
      if (response.outcome === "conflict") {
        setConflictOpen(true)
      }
      if (response.outcome === "succeeded") {
        onSucceeded?.(response.customerId)
      }
    },
  })

  const resetSession = () => {
    setResult(null)
    setConflictOpen(false)
    setIdempotencyKey(newIdempotencyKey("create"))
    form.reset()
  }

  return (
    <Sheet
      open={open}
      onOpenChange={(next) => {
        if (!next) {
          // Keep inputs on close after conflict/unknown so user can recover.
          if (result?.outcome === "succeeded") {
            resetSession()
          }
        }
        onOpenChange(next)
      }}
    >
      <SheetContent side="right" size="detail" className="overflow-y-auto">
        <SheetHeader>
          <SheetTitle>新建客户</SheetTitle>
          <SheetDescription>
            创建客户主体与首版资料；名称相似只提示候选，不自动合并。
          </SheetDescription>
        </SheetHeader>

        <form
          className="flex flex-1 flex-col gap-4 px-6 pb-4"
          onSubmit={(e) => {
            e.preventDefault()
            void form.handleSubmit()
          }}
        >
          <form.AppField
            name="legalName"
            children={(field) => (
              <field.TextField label="法定名称" placeholder="企业全称" />
            )}
          />
          <form.AppField
            name="shortName"
            children={(field) => (
              <field.TextField label="客户简称" placeholder="可选" />
            )}
          />
          <form.AppField
            name="unifiedCreditCode"
            children={(field) => (
              <field.TextField
                label="统一社会信用代码"
                placeholder="可选；不作自动合并依据"
              />
            )}
          />
          <form.AppField
            name="defaultPaymentTerm"
            children={(field) => (
              <field.SelectField
                label="默认付款条件"
                options={PAYMENT_TERM_OPTIONS}
                placeholder="录单提示"
              />
            )}
          />

          <div className="space-y-2">
            <Label htmlFor="create-simulate">演示结果</Label>
            <OptionCombobox
              id="create-simulate"
              value={simulate}
              onValueChange={(v) =>
                setSimulate((v ?? "ok") as "ok" | "conflict" | "unknown")
              }
              options={[
                { value: "ok", label: "正常成功" },
                { value: "conflict", label: "重复候选冲突（保留输入）" },
                { value: "unknown", label: "结果不确定（保留输入）" },
              ]}
              allowClear={false}
              aria-label="演示结果"
              placeholder="请选择演示结果"
            />
          </div>

          {result?.outcome === "succeeded" ? (
            <FormalActionResult
              status="succeeded"
              title="客户已创建"
              description={`客户号 ${result.customerNo} · 基础资料版本 v${result.revisionNo}`}
              reference={result.reference}
              facts={[
                { label: "客户号", value: result.customerNo },
                { label: "版本", value: `v${result.revisionNo}` },
                { label: "时间", value: result.occurredAt },
              ]}
            />
          ) : null}

          {result?.outcome === "unknown" ? (
            <FormalActionResult
              status="unknown"
              title="创建结果不确定"
              description={result.message}
              reference={result.idempotencyKey}
              actions={
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={queryIdempotency.isPending}
                  onClick={async () => {
                    const final = await queryIdempotency.mutateAsync(
                      result.idempotencyKey
                    )
                    if (final) setResult(final)
                  }}
                >
                  查询最终结果
                </Button>
              }
            />
          ) : null}

          <SheetFooter className="px-0">
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
            >
              {result?.outcome === "succeeded" ? "关闭" : "取消"}
            </Button>
            {result?.outcome !== "succeeded" ? (
              <form.AppForm>
                <form.SubmitButton
                  label={createMutation.isPending ? "提交中…" : "创建客户"}
                />
              </form.AppForm>
            ) : (
              <Button
                type="button"
                onClick={() => {
                  resetSession()
                  onOpenChange(false)
                }}
              >
                完成
              </Button>
            )}
          </SheetFooter>
        </form>

        {result?.outcome === "conflict" ? (
          <ConflictResolutionDialog
            open={conflictOpen}
            onOpenChange={setConflictOpen}
            title="存在重复候选（演示）"
            description={result.message}
            currentVersion="既有主体候选"
            localBaseline="本次输入"
            actor={result.actor}
            changedAt={result.changedAt}
            diff={
              <p className="text-sm">
                法定名称：{result.serverLegalName || "（候选）"}。系统不会自动合并主体。
              </p>
            }
            onReload={() => {
              setConflictOpen(false)
              setIdempotencyKey(newIdempotencyKey("create"))
              setSimulate("ok")
            }}
            onSaveCopy={() => setConflictOpen(false)}
            onCompare={() => setConflictOpen(false)}
          />
        ) : null}
      </SheetContent>
    </Sheet>
  )
}

export function CustomerReviseSheet({
  open,
  onOpenChange,
  customer,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  customer: CustomerCenterView
}) {
  const saveMutation = useSaveCustomerRevisionMutation()
  const queryIdempotency = useQueryCustomerIdempotencyMutation()
  const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
    newIdempotencyKey("revise")
  )
  const [simulate, setSimulate] = React.useState<"ok" | "conflict" | "unknown">(
    "ok"
  )
  const [result, setResult] = React.useState<CustomerMutationResult | null>(
    null
  )
  const [conflictOpen, setConflictOpen] = React.useState(false)

  const defaults = React.useMemo(
    () =>
      ({
        legalName: customer.currentRevision.legalName,
        shortName: customer.currentRevision.shortName ?? "",
        unifiedCreditCode: customer.currentRevision.unifiedCreditCode ?? "",
        defaultPaymentTerm: customer.currentRevision.defaultPaymentTerm ?? "",
        changeReason: "",
      }) satisfies FormValues,
    [customer]
  )

  const form = useAppForm({
    defaultValues: defaults,
    validators: { onChange: reviseSchema },
    onSubmit: async ({ value }) => {
      const response = await saveMutation.mutateAsync({
        customerId: customer.customerId,
        expectedLockVersion: customer.lockVersion,
        baseRevisionId: customer.currentRevision.revisionId,
        legalName: value.legalName.trim(),
        shortName: value.shortName.trim() || undefined,
        unifiedCreditCode: value.unifiedCreditCode.trim() || undefined,
        changeReason: value.changeReason.trim(),
        idempotencyKey,
        simulate,
      })
      setResult(response)
      if (response.outcome === "conflict") {
        setConflictOpen(true)
      }
      // On success: version updates via query invalidation; form retains until closed.
    },
  })

  // When customer identity refreshes after success, reset form defaults carefully.
  React.useEffect(() => {
    if (result?.outcome === "succeeded") return
    form.reset(defaults)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only sync when opening against new baseline
  }, [customer.customerId, customer.lockVersion, open])

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" size="detail" className="overflow-y-auto">
        <SheetHeader>
          <SheetTitle>修订客户主体</SheetTitle>
          <SheetDescription>
            将生成新客户版本；历史合同与销售单记录不被覆盖。当前版本 v
            {customer.currentRevision.revisionNo}
          </SheetDescription>
        </SheetHeader>

        <form
          className="flex flex-1 flex-col gap-4 px-6 pb-4"
          onSubmit={(e) => {
            e.preventDefault()
            void form.handleSubmit()
          }}
        >
          <form.AppField
            name="legalName"
            children={(field) => <field.TextField label="法定名称" />}
          />
          <form.AppField
            name="shortName"
            children={(field) => <field.TextField label="客户简称" />}
          />
          <form.AppField
            name="unifiedCreditCode"
            children={(field) => (
              <field.TextField label="统一社会信用代码" />
            )}
          />
          <form.AppField
            name="changeReason"
            children={(field) => (
              <field.TextareaField
                label="修订原因"
                placeholder="必填，写入修订时间线"
              />
            )}
          />

          <div className="space-y-2">
            <Label htmlFor="revise-simulate">演示结果</Label>
            <OptionCombobox
              id="revise-simulate"
              value={simulate}
              onValueChange={(v) =>
                setSimulate((v ?? "ok") as "ok" | "conflict" | "unknown")
              }
              options={[
                { value: "ok", label: "正常成功" },
                { value: "conflict", label: "版本冲突（保留输入）" },
                { value: "unknown", label: "结果不确定（保留输入）" },
              ]}
              allowClear={false}
              aria-label="演示结果"
              placeholder="请选择演示结果"
            />
          </div>

          {result?.outcome === "succeeded" ? (
            <FormalActionResult
              status="succeeded"
              title="客户主体已修订"
              description={`客户号 ${result.customerNo} · 新版本 v${result.revisionNo} · 历史单据记录不变`}
              reference={result.reference}
              facts={[
                { label: "客户号", value: result.customerNo },
                { label: "新版本", value: `v${result.revisionNo}` },
                { label: "版本号", value: String(result.lockVersion) },
                { label: "时间", value: result.occurredAt },
              ]}
            />
          ) : null}

          {result?.outcome === "unknown" ? (
            <FormalActionResult
              status="unknown"
              title="修订结果不确定"
              description={result.message}
              reference={result.idempotencyKey}
              actions={
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={queryIdempotency.isPending}
                  onClick={async () => {
                    const final = await queryIdempotency.mutateAsync(
                      result.idempotencyKey
                    )
                    if (final) setResult(final)
                  }}
                >
                  查询最终结果
                </Button>
              }
            />
          ) : null}

          <SheetFooter className="px-0">
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
            >
              {result?.outcome === "succeeded" ? "关闭" : "取消"}
            </Button>
            {result?.outcome !== "succeeded" ? (
              <form.AppForm>
                <form.SubmitButton
                  label={saveMutation.isPending ? "保存中…" : "保存修订"}
                />
              </form.AppForm>
            ) : null}
          </SheetFooter>
        </form>

        {result?.outcome === "conflict" ? (
          <ConflictResolutionDialog
            open={conflictOpen}
            onOpenChange={setConflictOpen}
            description={result.message}
            currentVersion={`v${result.serverRevisionNo} · lock ${result.serverLockVersion}`}
            localBaseline={`v${customer.currentRevision.revisionNo} · lock ${customer.lockVersion}`}
            actor={result.actor}
            changedAt={result.changedAt}
            diff={
              <ul className="list-inside list-disc space-y-1 text-sm">
                <li>服务端法定名称：{result.serverLegalName}</li>
                <li>服务端简称：{result.serverShortName ?? "—"}</li>
                <li>
                  服务端信用代码：{result.serverUnifiedCreditCode ?? "—"}
                </li>
                <li>本地输入仍保留在表单中，未写入业务记录。</li>
              </ul>
            }
            onReload={() => {
              setConflictOpen(false)
              setIdempotencyKey(newIdempotencyKey("revise"))
              setSimulate("ok")
              setResult(null)
            }}
            onSaveCopy={() => setConflictOpen(false)}
            onCompare={() => setConflictOpen(false)}
          />
        ) : null}
      </SheetContent>
    </Sheet>
  )
}
