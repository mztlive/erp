"use client"

import * as React from "react"
import { z } from "zod"

import { FormalActionResult, OptionCombobox } from "@/components/business"
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
import { masterDataCopy } from "@/features/master-data/copy"
import {
  WAREHOUSE_WRITE_MESSAGE,
  resourceLabel,
} from "@/features/master-data/data"
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

function newIdempotencyKey(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

function resultFacts(
  result: Extract<MasterDataMutationResult, { outcome: "succeeded" }>
) {
  return [
    { label: masterDataCopy.resultNo, value: result.stableNo },
    { label: masterDataCopy.resultVersion, value: `v${result.revisionNo}` },
    {
      label: masterDataCopy.resultVersionState,
      value:
        result.revisionState === "FUTURE"
          ? masterDataCopy.versionStateFuture
          : masterDataCopy.versionStateCurrent,
    },
    {
      label: masterDataCopy.resultEffective,
      value: result.effectiveFrom,
    },
    {
      label: masterDataCopy.resultActor,
      value: result.actor,
    },
    {
      label: masterDataCopy.resultAt,
      value: result.recordedAt.slice(0, 19).replace("T", " "),
    },
    { label: masterDataCopy.resultReason, value: result.changeReason },
  ]
}

const createSchema = z.object({
  name: z.string().trim().min(2, "请填写名称"),
  effectiveFrom: z.string().min(1, "请填写生效开始日期"),
  effectiveTo: z.string(),
  changeReason: z.string().trim().min(2, "请填写变更原因"),
})

const reviseSchema = z.object({
  name: z.string().trim().min(2, "请填写名称"),
  effectiveFrom: z.string().min(1, "请填写生效开始日期"),
  effectiveTo: z.string(),
  changeReason: z.string().trim().min(2, "请填写变更原因"),
})

const disableSchema = z.object({
  changeReason: z.string().trim().min(2, "请填写停用原因"),
  effectiveFrom: z.string().min(1, "请填写停用时间"),
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
          <DialogTitle>
            {masterDataCopy.createTitle(resourceLabel(resource))}
          </DialogTitle>
          <DialogDescription>{masterDataCopy.createDesc}</DialogDescription>
        </DialogHeader>

        {isWarehouse ? (
          <Alert variant="destructive">
            <AlertTitle>{masterDataCopy.warehouseWriteTitle}</AlertTitle>
            <AlertDescription>{WAREHOUSE_WRITE_MESSAGE}</AlertDescription>
          </Alert>
        ) : null}

        {result?.outcome === "succeeded" ? (
          <FormalActionResult
            status="succeeded"
            title={masterDataCopy.createSuccessTitle}
            description={masterDataCopy.createSuccessDesc}
            reference={result.reference}
            facts={resultFacts(result)}
          />
        ) : null}

        {result?.outcome === "blocked" ? (
          <FormalActionResult
            status="blocked"
            title={masterDataCopy.createBlockedTitle}
            description={result.message}
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
                children={(field) => (
                  <field.TextField label={masterDataCopy.fieldEffectiveFrom} />
                )}
              />
              <form.AppField
                name="effectiveTo"
                children={(field) => (
                  <field.TextField label={masterDataCopy.fieldEffectiveTo} />
                )}
              />
            </div>
            <form.AppField
              name="changeReason"
              children={(field) => (
                <field.TextareaField label={masterDataCopy.fieldChangeReason} />
              )}
            />
            {!isWarehouse ? (
              <div className="space-y-2">
                <Label htmlFor="create-sim">
                  {masterDataCopy.demoSimulateLabel}
                </Label>
                <OptionCombobox
                  id="create-sim"
                  value={simulate}
                  onValueChange={(v) =>
                    setSimulate(
                      (v ?? "ok") as "ok" | "overlap" | "sku_signature"
                    )
                  }
                  options={[
                    { value: "ok", label: masterDataCopy.demoOk },
                    { value: "overlap", label: masterDataCopy.demoOverlap },
                    ...(resource === "products"
                      ? [
                          {
                            value: "sku_signature",
                            label: masterDataCopy.demoSkuSig,
                          },
                        ]
                      : []),
                  ]}
                  className="w-full"
                  allowClear={false}
                  aria-label={masterDataCopy.demoSimulateLabel}
                  placeholder={masterDataCopy.demoSimulateLabel}
                />
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
                {isWarehouse
                  ? masterDataCopy.createSubmitRejected
                  : masterDataCopy.createSubmit}
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
  const stableId = target && "stableId" in target ? target.stableId : ""
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
          <DialogTitle>{masterDataCopy.reviseTitle}</DialogTitle>
          <DialogDescription>
            {masterDataCopy.reviseDesc}
            {target ? (
              <>
                {" "}
                资料编号 <span className="num">{target.stableNo}</span>
              </>
            ) : null}
          </DialogDescription>
        </DialogHeader>

        {isWarehouse ? (
          <Alert variant="destructive">
            <AlertTitle>{masterDataCopy.warehouseWriteTitle}</AlertTitle>
            <AlertDescription>{WAREHOUSE_WRITE_MESSAGE}</AlertDescription>
          </Alert>
        ) : null}

        {result?.outcome === "succeeded" ? (
          <FormalActionResult
            status="succeeded"
            title={masterDataCopy.reviseSuccessTitle}
            description={masterDataCopy.reviseSuccessDesc}
            reference={result.reference}
            facts={resultFacts(result)}
          />
        ) : null}

        {result?.outcome === "blocked" ? (
          <FormalActionResult
            status="blocked"
            title={masterDataCopy.reviseBlockedTitle}
            description={result.message}
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
            title={masterDataCopy.reviseConflictTitle}
            description={result.message || masterDataCopy.reviseConflictHint}
            facts={[
              {
                label: "当前版本",
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
              children={(field) => (
                <field.TextField label={masterDataCopy.reviseNameLabel} />
              )}
            />
            <div className="grid gap-3 sm:grid-cols-2">
              <form.AppField
                name="effectiveFrom"
                children={(field) => (
                  <field.TextField label={masterDataCopy.fieldEffectiveFrom} />
                )}
              />
              <form.AppField
                name="effectiveTo"
                children={(field) => (
                  <field.TextField label={masterDataCopy.fieldEffectiveTo} />
                )}
              />
            </div>
            <form.AppField
              name="changeReason"
              children={(field) => (
                <field.TextareaField label={masterDataCopy.fieldChangeReason} />
              )}
            />
            {!isWarehouse ? (
              <div className="space-y-2">
                <Label htmlFor="rev-sim">
                  {masterDataCopy.demoSimulateLabel}
                </Label>
                <OptionCombobox
                  id="rev-sim"
                  value={simulate}
                  onValueChange={(v) =>
                    setSimulate(
                      (v ?? "ok") as
                        | "ok"
                        | "overlap"
                        | "sku_signature"
                        | "base_unit"
                        | "conflict"
                    )
                  }
                  options={[
                    { value: "ok", label: masterDataCopy.demoOk },
                    { value: "overlap", label: masterDataCopy.demoOverlap },
                    ...(resource === "products"
                      ? [
                          {
                            value: "sku_signature",
                            label: masterDataCopy.demoSkuSig,
                          },
                          {
                            value: "base_unit",
                            label: masterDataCopy.demoBaseUnit,
                          },
                        ]
                      : []),
                    { value: "conflict", label: masterDataCopy.demoConflict },
                  ]}
                  className="w-full"
                  allowClear={false}
                  aria-label={masterDataCopy.demoSimulateLabel}
                  placeholder={masterDataCopy.demoSimulateLabel}
                />
              </div>
            ) : null}
            <DialogFooter>
              <DialogClose render={<Button type="button" variant="outline" />}>
                关闭
              </DialogClose>
              <Button type="submit" disabled={mutation.isPending || !target}>
                {isWarehouse
                  ? masterDataCopy.createSubmitRejected
                  : masterDataCopy.reviseSubmit}
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
  const [simulate, setSimulate] = React.useState<
    "ok" | "warehouse_stock" | "conflict"
  >("ok")
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
          <DialogTitle>{masterDataCopy.disableTitle}</DialogTitle>
          <DialogDescription>
            {masterDataCopy.disableDesc}
            {target ? (
              <>
                {" "}
                资料编号 <span className="num">{target.stableNo}</span>
              </>
            ) : null}
          </DialogDescription>
        </DialogHeader>

        {isWarehouse ? (
          <Alert variant="destructive">
            <AlertTitle>{masterDataCopy.warehouseWriteTitle}</AlertTitle>
            <AlertDescription>
              {WAREHOUSE_WRITE_MESSAGE}
              {target &&
              "warehouseStockSummary" in target &&
              target.warehouseStockSummary?.hasBlockingStock
                ? ` 另：在库 ${target.warehouseStockSummary.onHandQty} / 预占 ${target.warehouseStockSummary.reservedQty} 时也不可停用。`
                : null}
            </AlertDescription>
          </Alert>
        ) : null}

        {result?.outcome === "succeeded" ? (
          <FormalActionResult
            status="succeeded"
            title={masterDataCopy.disableSuccessTitle}
            description={masterDataCopy.disableSuccessDesc}
            reference={result.reference}
            facts={resultFacts(result)}
          />
        ) : null}

        {result?.outcome === "blocked" ? (
          <FormalActionResult
            status="blocked"
            title={masterDataCopy.disableBlockedTitle}
            description={result.message}
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
              children={(field) => (
                <field.TextField label={masterDataCopy.fieldDisableAt} />
              )}
            />
            <form.AppField
              name="changeReason"
              children={(field) => (
                <field.TextareaField
                  label={masterDataCopy.fieldDisableReason}
                />
              )}
            />
            {!isWarehouse ? (
              <div className="space-y-2">
                <Label htmlFor="dis-sim">
                  {masterDataCopy.demoSimulateLabel}
                </Label>
                <OptionCombobox
                  id="dis-sim"
                  value={simulate}
                  onValueChange={(v) =>
                    setSimulate(
                      (v ?? "ok") as "ok" | "warehouse_stock" | "conflict"
                    )
                  }
                  options={[
                    { value: "ok", label: masterDataCopy.demoDisableOk },
                    { value: "conflict", label: masterDataCopy.demoConflict },
                  ]}
                  className="w-full"
                  allowClear={false}
                  aria-label={masterDataCopy.demoSimulateLabel}
                  placeholder={masterDataCopy.demoSimulateLabel}
                />
              </div>
            ) : null}
            <DialogFooter>
              <DialogClose render={<Button type="button" variant="outline" />}>
                关闭
              </DialogClose>
              <Button type="submit" disabled={mutation.isPending || !target}>
                {isWarehouse
                  ? masterDataCopy.createSubmitRejected
                  : masterDataCopy.disableSubmit}
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
