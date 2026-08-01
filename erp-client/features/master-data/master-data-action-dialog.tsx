"use client"

import * as React from "react"
import { z } from "zod"

import { FormalActionResult } from "@/components/business"
import { useAppForm } from "@/components/form"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import {
  NativeSelect,
  NativeSelectOption,
} from "@/components/ui/native-select"
import {
  useCreateMasterDataMutation,
  useCreateRevisionMutation,
  useDisableMasterDataMutation,
} from "@/features/master-data/queries"
import type {
  MasterDataCenterView,
  MasterDataListItem,
  MasterDataMutationResult,
  MasterDataResource,
} from "@/features/master-data/types"
import {
  WAREHOUSE_WRITE_CODE,
  WAREHOUSE_WRITE_MESSAGE,
  resourceLabel,
} from "@/features/master-data/data"

function newIdempotencyKey(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

function resultFacts(result: Extract<MasterDataMutationResult, { outcome: "succeeded" }>) {
  return [
    { label: "稳定编号", value: result.stableNo },
    { label: "版本", value: `v${result.revisionNo}` },
    { label: "时序", value: result.revisionState === "FUTURE" ? "待生效" : "当前" },
    {
      label: "生效时间",
      value: result.effectiveFrom,
    },
    {
      label: "操作者",
      value: result.actor,
    },
    {
      label: "时间",
      value: result.recordedAt.slice(0, 19).replace("T", " "),
    },
    { label: "原因", value: result.changeReason },
  ]
}

const createSchema = z.object({
  name: z.string().trim().min(2, "请填写名称"),
  effectiveFrom: z.string().min(1, "请填写生效起"),
  effectiveTo: z.string(),
  changeReason: z.string().trim().min(2, "请填写变更原因"),
})

const reviseSchema = z.object({
  name: z.string().trim().min(2, "请填写名称"),
  effectiveFrom: z.string().min(1, "请填写生效起"),
  effectiveTo: z.string(),
  changeReason: z.string().trim().min(2, "请填写变更原因"),
})

const disableSchema = z.object({
  changeReason: z.string().trim().min(2, "请填写停用原因"),
  effectiveFrom: z.string().min(1, "请填写停用时点"),
})

export function MasterDataCreateDialog({
  open,
  onOpenChange,
  resource,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  resource: MasterDataResource
}) {
  const mutation = useCreateMasterDataMutation()
  const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
    newIdempotencyKey("create")
  )
  const [simulate, setSimulate] = React.useState<
    "ok" | "overlap" | "sku_signature"
  >("ok")
  const [result, setResult] = React.useState<MasterDataMutationResult | null>(
    null
  )

  const isWarehouse = resource === "warehouses"

  const form = useAppForm({
    defaultValues: {
      name: "",
      effectiveFrom: "2026-08-01",
      effectiveTo: "",
      changeReason: "",
    },
    validators: { onChange: createSchema },
    onSubmit: async ({ value }) => {
      const response = await mutation.mutateAsync({
        resource,
        name: value.name.trim(),
        effectiveFrom: value.effectiveFrom,
        effectiveTo: value.effectiveTo.trim() || undefined,
        changeReason: value.changeReason.trim(),
        idempotencyKey,
        simulate: isWarehouse ? "ok" : simulate,
      })
      setResult(response)
    },
  })

  const reset = () => {
    setResult(null)
    setIdempotencyKey(newIdempotencyKey("create"))
    form.reset()
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next && result?.outcome === "succeeded") reset()
        onOpenChange(next)
      }}
    >
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>新建 · {resourceLabel(resource)}</DialogTitle>
          <DialogDescription>
            创建稳定身份与 v1 不可变修订；须填写变更原因。仓库在本期未确认前写操作暂不可用。
          </DialogDescription>
        </DialogHeader>

        {isWarehouse ? (
          <Alert variant="destructive">
            <AlertTitle>{WAREHOUSE_WRITE_CODE}</AlertTitle>
            <AlertDescription>{WAREHOUSE_WRITE_MESSAGE}</AlertDescription>
          </Alert>
        ) : null}

        {result?.outcome === "succeeded" ? (
          <FormalActionResult
            status="succeeded"
            title="主数据已创建"
            description="稳定编号与 v1 已形成；历史单据不会引用本会话新建对象的演示数据。"
            reference={result.reference}
            facts={resultFacts(result)}
          />
        ) : null}

        {result?.outcome === "blocked" ? (
          <FormalActionResult
            status="blocked"
            title="创建被阻断"
            description={result.message}
            reference={result.code}
            facts={
              result.detail
                ? [{ label: "说明", value: result.detail }]
                : undefined
            }
          />
        ) : null}

        {result?.outcome !== "succeeded" ? (
          <form
            className="grid gap-3"
            onSubmit={(e) => {
              e.preventDefault()
              void form.handleSubmit()
            }}
          >
            <form.AppField
              name="name"
              children={(field) => <field.TextField label="名称" />}
            />
            <div className="grid gap-3 sm:grid-cols-2">
              <form.AppField
                name="effectiveFrom"
                children={(field) => <field.TextField label="生效起" />}
              />
              <form.AppField
                name="effectiveTo"
                children={(field) => (
                  <field.TextField label="生效止（空=长期）" />
                )}
              />
            </div>
            <form.AppField
              name="changeReason"
              children={(field) => (
                <field.TextareaField label="变更原因" />
              )}
            />
            {!isWarehouse ? (
              <div className="space-y-2">
                <Label htmlFor="create-sim">演示校验（会话）</Label>
                <NativeSelect
                  id="create-sim"
                  value={simulate}
                  onChange={(e) =>
                    setSimulate(
                      e.target.value as "ok" | "overlap" | "sku_signature"
                    )
                  }
                >
                  <NativeSelectOption value="ok">正常成功</NativeSelectOption>
                  <NativeSelectOption value="overlap">
                    有效期重叠阻断
                  </NativeSelectOption>
                  {resource === "products" ? (
                    <NativeSelectOption value="sku_signature">
                      SKU 规格身份阻断
                    </NativeSelectOption>
                  ) : null}
                </NativeSelect>
              </div>
            ) : null}
            <DialogFooter>
              <DialogClose render={<Button type="button" variant="outline" />}>
                关闭
              </DialogClose>
              <Button
                type="submit"
                disabled={mutation.isPending}
                title={isWarehouse ? WAREHOUSE_WRITE_MESSAGE : undefined}
              >
                {isWarehouse ? "提交（将拒绝）" : "创建"}
              </Button>
            </DialogFooter>
          </form>
        ) : (
          <DialogFooter>
            <Button
              type="button"
              onClick={() => {
                reset()
                onOpenChange(false)
              }}
            >
              完成
            </Button>
          </DialogFooter>
        )}
      </DialogContent>
    </Dialog>
  )
}

export function MasterDataReviseDialog({
  open,
  onOpenChange,
  resource,
  target,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  resource: MasterDataResource
  target: MasterDataListItem | MasterDataCenterView | null
}) {
  const mutation = useCreateRevisionMutation()
  const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
    newIdempotencyKey("revise")
  )
  const [simulate, setSimulate] = React.useState<
    "ok" | "overlap" | "sku_signature" | "base_unit" | "conflict"
  >("ok")
  const [result, setResult] = React.useState<MasterDataMutationResult | null>(
    null
  )

  const isWarehouse = resource === "warehouses"
  const stableId =
    target && "stableId" in target ? target.stableId : ""
  const baseRevisionId =
    target && "currentRevisionId" in target
      ? target.currentRevisionId
      : target && "currentRevision" in target
        ? target.currentRevision.revisionId
        : ""
  const lockVersion = target?.lockVersion ?? 0
  const nameDefault = target?.name ?? ""

  const form = useAppForm({
    defaultValues: {
      name: nameDefault,
      effectiveFrom: "2026-08-15",
      effectiveTo: "",
      changeReason: "",
    },
    validators: { onChange: reviseSchema },
    onSubmit: async ({ value }) => {
      if (!stableId || !baseRevisionId) return
      const response = await mutation.mutateAsync({
        resource,
        stableId,
        baseRevisionId,
        expectedLockVersion: lockVersion,
        name: value.name.trim(),
        effectiveFrom: value.effectiveFrom,
        effectiveTo: value.effectiveTo.trim() || undefined,
        changeReason: value.changeReason.trim(),
        idempotencyKey,
        simulate: isWarehouse ? "ok" : simulate,
      })
      setResult(response)
    },
  })

  React.useEffect(() => {
    if (open && target) {
      form.setFieldValue("name", target.name)
      setResult(null)
      setIdempotencyKey(newIdempotencyKey("revise"))
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reset only when target opens
  }, [open, stableId])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>形成新版本</DialogTitle>
          <DialogDescription>
            追加不可变修订并保留原因、操作者与时间。当前名称变化不改写历史记录。
            {target ? (
              <>
                {" "}
                基准 <span className="num">{target.stableNo}</span>
              </>
            ) : null}
          </DialogDescription>
        </DialogHeader>

        {isWarehouse ? (
          <Alert variant="destructive">
            <AlertTitle>{WAREHOUSE_WRITE_CODE}</AlertTitle>
            <AlertDescription>{WAREHOUSE_WRITE_MESSAGE}</AlertDescription>
          </Alert>
        ) : null}

        {result?.outcome === "succeeded" ? (
          <FormalActionResult
            status="succeeded"
            title="新版本已形成"
            description="不可变修订已追加；即时生效时更新当前指针，待生效不提前切换。"
            reference={result.reference}
            facts={resultFacts(result)}
          />
        ) : null}

        {result?.outcome === "blocked" ? (
          <FormalActionResult
            status="blocked"
            title="修订被阻断"
            description={result.message}
            reference={result.code}
            facts={
              result.detail
                ? [{ label: "说明", value: result.detail }]
                : undefined
            }
          />
        ) : null}

        {result?.outcome === "conflict" ? (
          <FormalActionResult
            status="blocked"
            title="版本冲突"
            description={result.message}
            reference={`lock=${result.serverLockVersion}`}
            facts={[
              {
                label: "服务端版本",
                value: `v${result.serverRevisionNo}`,
              },
            ]}
          />
        ) : null}

        {result?.outcome !== "succeeded" ? (
          <form
            className="grid gap-3"
            onSubmit={(e) => {
              e.preventDefault()
              void form.handleSubmit()
            }}
          >
            <form.AppField
              name="name"
              children={(field) => <field.TextField label="名称（新版本）" />}
            />
            <div className="grid gap-3 sm:grid-cols-2">
              <form.AppField
                name="effectiveFrom"
                children={(field) => <field.TextField label="生效起" />}
              />
              <form.AppField
                name="effectiveTo"
                children={(field) => (
                  <field.TextField label="生效止（空=长期）" />
                )}
              />
            </div>
            <form.AppField
              name="changeReason"
              children={(field) => (
                <field.TextareaField label="变更原因" />
              )}
            />
            {!isWarehouse ? (
              <div className="space-y-2">
                <Label htmlFor="rev-sim">演示校验（会话）</Label>
                <NativeSelect
                  id="rev-sim"
                  value={simulate}
                  onChange={(e) =>
                    setSimulate(
                      e.target.value as
                        | "ok"
                        | "overlap"
                        | "sku_signature"
                        | "base_unit"
                        | "conflict"
                    )
                  }
                >
                  <NativeSelectOption value="ok">正常成功</NativeSelectOption>
                  <NativeSelectOption value="overlap">
                    有效期重叠阻断
                  </NativeSelectOption>
                  {resource === "products" ? (
                    <>
                      <NativeSelectOption value="sku_signature">
                        SKU 规格身份阻断
                      </NativeSelectOption>
                      <NativeSelectOption value="base_unit">
                        基础单位变更阻断
                      </NativeSelectOption>
                    </>
                  ) : null}
                  <NativeSelectOption value="conflict">
                    版本冲突
                  </NativeSelectOption>
                </NativeSelect>
              </div>
            ) : null}
            <DialogFooter>
              <DialogClose render={<Button type="button" variant="outline" />}>
                关闭
              </DialogClose>
              <Button type="submit" disabled={mutation.isPending || !target}>
                {isWarehouse ? "提交（将拒绝）" : "形成新版本"}
              </Button>
            </DialogFooter>
          </form>
        ) : (
          <DialogFooter>
            <Button type="button" onClick={() => onOpenChange(false)}>
              完成
            </Button>
          </DialogFooter>
        )}
      </DialogContent>
    </Dialog>
  )
}

export function MasterDataDisableDialog({
  open,
  onOpenChange,
  resource,
  target,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  resource: MasterDataResource
  target: MasterDataListItem | MasterDataCenterView | null
}) {
  const mutation = useDisableMasterDataMutation()
  const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
    newIdempotencyKey("disable")
  )
  const [simulate, setSimulate] = React.useState<"ok" | "warehouse_stock" | "conflict">(
    "ok"
  )
  const [result, setResult] = React.useState<MasterDataMutationResult | null>(
    null
  )

  const isWarehouse = resource === "warehouses"
  const stableId = target?.stableId ?? ""
  const baseRevisionId =
    target && "currentRevisionId" in target
      ? target.currentRevisionId
      : target && "currentRevision" in target
        ? target.currentRevision.revisionId
        : ""
  const lockVersion = target?.lockVersion ?? 0

  const form = useAppForm({
    defaultValues: {
      changeReason: "",
      effectiveFrom: "2026-08-01",
    },
    validators: { onChange: disableSchema },
    onSubmit: async ({ value }) => {
      if (!stableId || !baseRevisionId) return
      const response = await mutation.mutateAsync({
        resource,
        stableId,
        baseRevisionId,
        expectedLockVersion: lockVersion,
        changeReason: value.changeReason.trim(),
        effectiveFrom: value.effectiveFrom,
        idempotencyKey,
        simulate: isWarehouse ? "warehouse_stock" : simulate,
      })
      setResult(response)
    },
  })

  React.useEffect(() => {
    if (open) {
      setResult(null)
      setIdempotencyKey(newIdempotencyKey("disable"))
      form.reset()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, stableId])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>停用主数据</DialogTitle>
          <DialogDescription>
            停用不是删除：形成停用版本，历史引用与版本记录永久保留。
            {target ? (
              <>
                {" "}
                对象 <span className="num">{target.stableNo}</span>
              </>
            ) : null}
          </DialogDescription>
        </DialogHeader>

        {isWarehouse ? (
          <Alert variant="destructive">
            <AlertTitle>{WAREHOUSE_WRITE_CODE}</AlertTitle>
            <AlertDescription>
              {WAREHOUSE_WRITE_MESSAGE}
              {target &&
              "warehouseStockSummary" in target &&
              target.warehouseStockSummary?.hasBlockingStock
                ? ` 另：在库 ${target.warehouseStockSummary.onHandQty} / 预占 ${target.warehouseStockSummary.reservedQty} 时即使 Q1 确认也不得停用。`
                : null}
            </AlertDescription>
          </Alert>
        ) : null}

        {result?.outcome === "succeeded" ? (
          <FormalActionResult
            status="succeeded"
            title="已停用"
            description="已形成停用版本；身份保留，历史版本可只读打开。"
            reference={result.reference}
            facts={resultFacts(result)}
          />
        ) : null}

        {result?.outcome === "blocked" ? (
          <FormalActionResult
            status="blocked"
            title="停用被阻断"
            description={result.message}
            reference={result.code}
            facts={[
              ...(result.detail
                ? [{ label: "说明", value: result.detail }]
                : []),
              ...(result.drillHref
                ? [{ label: "库存台账", value: result.drillHref }]
                : []),
            ]}
          />
        ) : null}

        {result?.outcome !== "succeeded" ? (
          <form
            className="grid gap-3"
            onSubmit={(e) => {
              e.preventDefault()
              void form.handleSubmit()
            }}
          >
            <form.AppField
              name="effectiveFrom"
              children={(field) => <field.TextField label="停用时点" />}
            />
            <form.AppField
              name="changeReason"
              children={(field) => (
                <field.TextareaField label="停用原因" />
              )}
            />
            {!isWarehouse ? (
              <div className="space-y-2">
                <Label htmlFor="dis-sim">演示结果</Label>
                <NativeSelect
                  id="dis-sim"
                  value={simulate}
                  onChange={(e) =>
                    setSimulate(
                      e.target.value as "ok" | "warehouse_stock" | "conflict"
                    )
                  }
                >
                  <NativeSelectOption value="ok">正常停用</NativeSelectOption>
                  <NativeSelectOption value="conflict">版本冲突</NativeSelectOption>
                </NativeSelect>
              </div>
            ) : null}
            <DialogFooter>
              <DialogClose render={<Button type="button" variant="outline" />}>
                关闭
              </DialogClose>
              <Button type="submit" disabled={mutation.isPending || !target}>
                {isWarehouse ? "提交（将拒绝）" : "确认停用"}
              </Button>
            </DialogFooter>
          </form>
        ) : (
          <DialogFooter>
            <Button type="button" onClick={() => onOpenChange(false)}>
              完成
            </Button>
          </DialogFooter>
        )}
      </DialogContent>
    </Dialog>
  )
}
