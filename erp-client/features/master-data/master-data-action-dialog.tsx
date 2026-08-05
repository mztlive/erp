"use client"

import * as React from "react"
import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"
import { ImageIcon, XIcon } from "lucide-react"
import { z } from "zod"

import {
  CategoryCombobox,
  DiscardConfirmDialog,
  FormalActionResult,
  OptionCombobox,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { DatePicker } from "@/components/ui/date-picker"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { FileUpload } from "@/components/ui/file-upload"
import { Label } from "@/components/ui/label"
import { masterDataCopy } from "@/features/master-data/copy"
import {
  WAREHOUSE_WRITE_MESSAGE,
  resourceLabel,
} from "@/features/master-data/data"
import {
  RESOURCE_FIELDS,
  buildResourceFields,
  buildResourceSchema,
  currentResourceFieldValues,
  defaultImmediateEffectiveFrom,
  emptyResourceFieldValues,
  joinMediaList,
  parseMediaList,
  usesEffectivePeriod,
  usesWideDialog,
  type ResourceFieldDef,
  type ResourceFormValues,
} from "@/features/master-data/resource-fields"
import {
  collectDescendantIds,
  buildCategoryForest,
  toCategoryComboboxItems,
} from "@/features/master-data/category-tree-model"
import {
  masterDataKeys,
  useCreateMasterDataMutation,
  useCreateRevisionMutation,
  useDisableMasterDataMutation,
  useMasterDataListQuery,
} from "@/features/master-data/queries"
import type {
  MasterDataCenterView,
  MasterDataListItem,
  MasterDataMutationResult,
  MasterDataResource,
} from "@/features/master-data/types"
import { cn } from "@/lib/utils"

function newIdempotencyKey(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

type FieldApi = {
  TextField: React.ComponentType<{ label: string }>
  TextareaField: React.ComponentType<{ label: string }>
  SelectField: React.ComponentType<{
    label: string
    options: readonly { value: string; label: string }[]
    allowClear?: boolean
    placeholder?: string
  }>
  state: {
    value: string
    meta: { errors: readonly unknown[]; isTouched: boolean }
  }
  handleChange: (value: string) => void
  handleBlur: () => void
}

type ResourceFormApp = {
  AppField: React.ComponentType<{
    name: string
    children: (field: FieldApi) => React.ReactNode
  }>
}

/** 生效开始 / 结束统一用 DatePicker：格式由控件保证，避免裸文本框静默接受错误格式。 */
function DateField({
  label,
  field,
  id,
}: {
  label: string
  field: FieldApi
  id: string
}) {
  const error = field.state.meta.errors[0]
  return (
    <div className="space-y-1.5">
      <Label htmlFor={id}>{label}</Label>
      <DatePicker
        value={field.state.value || undefined}
        onValueChange={(next) => field.handleChange(next ?? "")}
        className="w-full"
        aria-invalid={Boolean(error)}
      />
      {error ? (
        <p className="text-xs text-destructive" role="alert">
          {String(error)}
        </p>
      ) : null}
    </div>
  )
}

function MediaSingleField({
  label,
  hint,
  value,
  onChange,
  required,
  selectedHint = "已选择",
  /** 品牌 Logo 等固定为正方形预览与上传区。 */
  aspectRatio,
}: {
  label: string
  hint?: string
  value: string
  onChange: (next: string) => void
  required?: boolean
  selectedHint?: string
  aspectRatio?: "1:1"
}) {
  const isSquare = aspectRatio === "1:1"
  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-2">
        <Label className="text-sm font-medium">
          {label}
          {required ? (
            <span className="ml-1 text-destructive">*</span>
          ) : null}
        </Label>
        {value ? (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => onChange("")}
          >
            {masterDataCopy.mediaRemove}
          </Button>
        ) : null}
      </div>
      {value ? (
        isSquare ? (
          <div className="flex items-start gap-3">
            <div
              className="flex size-24 shrink-0 flex-col items-center justify-center gap-1 rounded-lg border border-border bg-surface-sunken aspect-square"
              aria-label={`${label} 预览 1:1`}
            >
              <ImageIcon className="size-8 text-muted-foreground" aria-hidden />
              <span className="text-[10px] text-muted-foreground">1:1</span>
            </div>
            <div className="min-w-0 flex-1 pt-1">
              <div className="truncate text-sm font-medium">{value}</div>
              <div className="text-xs text-muted-foreground">{selectedHint}</div>
              <div className="mt-1 text-xs text-muted-foreground">比例 1:1</div>
            </div>
          </div>
        ) : (
          <div className="flex items-center gap-3 rounded-md border border-border bg-surface-sunken px-3 py-2">
            <div className="flex size-10 items-center justify-center rounded-md bg-muted">
              <ImageIcon className="size-5 text-muted-foreground" aria-hidden />
            </div>
            <div className="min-w-0 flex-1">
              <div className="truncate text-sm font-medium">{value}</div>
              <div className="text-xs text-muted-foreground">{selectedHint}</div>
            </div>
          </div>
        )
      ) : (
        <FileUpload
          accept="image/jpeg,image/png,image/webp"
          multiple={false}
          label={label}
          description={hint ?? masterDataCopy.mediaUploadHint}
          onFilesSelected={(files) => {
            const file = files[0]
            if (file) onChange(file.name)
          }}
          className={cn(
            "p-4",
            isSquare && "mx-auto aspect-square max-w-[10rem] justify-center"
          )}
        />
      )}
    </div>
  )
}

export function MediaListField({
  label,
  hint,
  value,
  onChange,
  accept = "image/jpeg,image/png,image/webp",
}: {
  label: string
  hint?: string
  value: string
  onChange: (next: string) => void
  /** 允许上传的文件类型；默认图片。 */
  accept?: string
}) {
  const items = parseMediaList(value)
  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-2">
        <Label className="text-sm font-medium">{label}</Label>
        <span className="text-xs text-muted-foreground">
          {masterDataCopy.mediaCount(items.length)}
          {hint ? ` · ${hint}` : null}
        </span>
      </div>
      {items.length > 0 ? (
        <ul className="space-y-1.5">
          {items.map((name, index) => (
            <li
              key={`${name}-${index}`}
              className="flex items-center gap-2 rounded-md border border-border px-2.5 py-1.5"
            >
              <ImageIcon
                className="size-4 shrink-0 text-muted-foreground"
                aria-hidden
              />
              <span className="min-w-0 flex-1 truncate text-sm">{name}</span>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label={`${masterDataCopy.mediaRemove} ${name}`}
                onClick={() => {
                  const next = items.filter((_, i) => i !== index)
                  onChange(joinMediaList(next))
                }}
              >
                <XIcon className="size-3.5" />
              </Button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-xs text-muted-foreground">
          {masterDataCopy.mediaEmpty}（{masterDataCopy.mediaAllowEmpty}）
        </p>
      )}
      <FileUpload
        accept={accept}
        multiple
        label={`添加${label}`}
        description={masterDataCopy.mediaUploadHint}
        onFilesSelected={(files) => {
          const names = files.map((f) => f.name)
          onChange(joinMediaList([...items, ...names]))
        }}
        className="p-3"
      />
    </div>
  )
}

function renderStandardField(
  def: ResourceFieldDef,
  field: FieldApi,
  extras?: {
    categoryParentOptions?: ReturnType<typeof toCategoryComboboxItems>
  }
) {
  if (def.kind === "textarea") {
    return <field.TextareaField label={def.label} />
  }
  if (def.kind === "select") {
    return (
      <field.SelectField
        label={def.label}
        options={(def.options ?? []).map((option) => ({
          value: option,
          label: option,
        }))}
        allowClear={!def.required}
        placeholder={def.required ? `请选择${def.label}` : "未填写"}
      />
    )
  }
  if (def.kind === "checkbox-group") {
    const selected = new Set(
      (field.state.value ?? "")
        .split(/[、,，]/)
        .map((s) => s.trim())
        .filter(Boolean)
    )
    return (
      <div className="space-y-1.5">
        <Label className="text-sm font-medium">{def.label}</Label>
        <div className="grid gap-2 sm:grid-cols-2">
          {(def.options ?? []).map((option) => (
            <label
              key={option}
              className="flex items-center gap-2 text-sm"
            >
              <Checkbox
                checked={selected.has(option)}
                onCheckedChange={(checked) => {
                  const next = new Set(selected)
                  if (checked === true) {
                    next.add(option)
                  } else {
                    next.delete(option)
                  }
                  field.handleChange(Array.from(next).join("、"))
                }}
              />
              {option}
            </label>
          ))}
        </div>
      </div>
    )
  }
  if (def.kind === "category-parent") {
    return (
      <div className="space-y-1.5">
        <Label className="text-sm font-medium">{def.label}</Label>
        <CategoryCombobox
          categories={extras?.categoryParentOptions ?? []}
          value={field.state.value || undefined}
          onValueChange={(id) => field.handleChange(id ?? "")}
          placeholder="可选上级；空为根分类"
          emptyLabel="没有可选上级分类"
          className="w-full"
        />
        <p className="text-xs text-muted-foreground">
          留空表示根分类；不可选择自身或下级。
        </p>
      </div>
    )
  }
  if (def.kind === "media") {
    return (
      <MediaSingleField
        label={def.label}
        hint={def.hint}
        value={field.state.value}
        onChange={(next) => field.handleChange(next)}
        required={def.required}
        selectedHint={
          def.key === "logo" ? "Logo · 1:1 · 已选择" : "主图 · 已选择"
        }
        aspectRatio={def.key === "logo" ? "1:1" : undefined}
      />
    )
  }
  if (def.kind === "media-list") {
    return (
      <MediaListField
        label={def.label}
        hint={def.hint}
        value={field.state.value}
        onChange={(next) => field.handleChange(next)}
      />
    )
  }
  return <field.TextField label={def.label} />
}

/** 资源专属字段区块：窄对话框单列；商品 SKU 在宽对话框中分区双列。 */
function ResourceFieldsSection({
  form,
  resource,
  wide,
  excludeCategoryIds,
}: {
  form: ResourceFormApp
  resource: MasterDataResource
  wide?: boolean
  /** 更新分类时排除自身与子树，避免成环。 */
  excludeCategoryIds?: ReadonlySet<string>
}) {
  const categoryListQuery = useMasterDataListQuery({
    resource: "categories",
    lifecycleStatus: "all",
    revisionTiming: "all",
  })
  const categoryParentOptions = React.useMemo(() => {
    if (resource !== "categories") return []
    return toCategoryComboboxItems(categoryListQuery.data?.rows ?? [], {
      excludeIds: excludeCategoryIds,
      enabledOnly: false,
    })
  }, [categoryListQuery.data?.rows, excludeCategoryIds, resource])

  const defs = RESOURCE_FIELDS[resource]
  if (defs.length === 0) return null

  const fieldExtras = { categoryParentOptions }

  if (!wide || resource !== "products") {
    return (
      <fieldset className="space-y-3 rounded-md border border-border p-3">
        <legend className="px-1 text-xs text-muted-foreground">
          {masterDataCopy.fieldResourceSection}
        </legend>
        {defs.map((def) => (
          <form.AppField
            key={def.key}
            name={def.key}
            children={(field) => renderStandardField(def, field, fieldExtras)}
          />
        ))}
      </fieldset>
    )
  }

  const identity = defs.filter((d) => d.section === "identity")
  const catalog = defs.filter((d) => d.section === "catalog")
  const media = defs.filter((d) => d.section === "media")

  return (
    <div className="space-y-4">
      <div className="grid gap-4 lg:grid-cols-2">
        <fieldset className="space-y-3 rounded-md border border-border p-3">
          <legend className="px-1 text-xs text-muted-foreground">
            {masterDataCopy.fieldIdentitySection}
          </legend>
          {identity.map((def) => (
            <form.AppField
              key={def.key}
              name={def.key}
              children={(field) => renderStandardField(def, field)}
            />
          ))}
        </fieldset>
        <fieldset className="space-y-3 rounded-md border border-border p-3">
          <legend className="px-1 text-xs text-muted-foreground">
            {masterDataCopy.fieldCatalogSection}
          </legend>
          <div className="grid gap-3 sm:grid-cols-2">
            {catalog.map((def) => (
              <form.AppField
                key={def.key}
                name={def.key}
                children={(field) => renderStandardField(def, field)}
              />
            ))}
          </div>
        </fieldset>
      </div>
      <fieldset className="space-y-4 rounded-md border border-border p-3">
        <legend className="px-1 text-xs text-muted-foreground">
          {masterDataCopy.fieldMediaSection}
        </legend>
        <div className="grid gap-4 lg:grid-cols-3">
          {media.map((def) => (
            <form.AppField
              key={def.key}
              name={def.key}
              children={(field) => renderStandardField(def, field)}
            />
          ))}
        </div>
      </fieldset>
    </div>
  )
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

const disableSchema = z.object({
  changeReason: z.string().trim().min(2, "请填写停用原因"),
  effectiveFrom: z
    .string()
    .min(1, "请填写停用时间")
    .refine(
      (value) => /^\d{4}-\d{2}-\d{2}$/.test(value),
      "停用时间格式不正确，请使用 YYYY-MM-DD"
    ),
})

function dialogContentClass(resource: MasterDataResource) {
  if (usesWideDialog(resource)) {
    return cn(
      "flex max-h-[92vh] w-full flex-col gap-4 overflow-hidden sm:max-w-5xl"
    )
  }
  // 非 wide 对话框同样加最大高度 + 内部滚动，保证小屏与长表单下底部按钮可用。
  return cn(
    "flex max-h-[92vh] w-full flex-col gap-4 overflow-hidden sm:max-w-lg"
  )
}

function DialogScrollBody({
  children,
  wide,
}: {
  children: React.ReactNode
  wide?: boolean
}) {
  void wide
  return (
    <div className="min-h-0 flex-1 overflow-y-auto pr-1">{children}</div>
  )
}

export function MasterDataCreateDialog({
  open,
  onOpenChange,
  resource,
  defaultFieldValues,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  resource: MasterDataResource
  /** 预填资源专属字段（如新建子分类时的 parentId）。 */
  defaultFieldValues?: Partial<Record<string, string>>
}) {
  const mutation = useCreateMasterDataMutation()
  const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
    newIdempotencyKey("create")
  )
  const [simulate, setSimulate] = React.useState<"ok" | "overlap">("ok")
  const [result, setResult] = React.useState<MasterDataMutationResult | null>(
    null
  )
  const [discardOpen, setDiscardOpen] = React.useState(false)

  const isWarehouse = resource === "warehouses"
  const wide = usesWideDialog(resource)
  const showEffectivePeriod = usesEffectivePeriod(resource)

  // 演示控件不跨打开会话残留：每次打开回到「正常保存」。
  React.useEffect(() => {
    if (open) setSimulate("ok")
  }, [open])

  const defaults: ResourceFormValues = {
    name: "",
    effectiveFrom: showEffectivePeriod
      ? defaultImmediateEffectiveFrom()
      : defaultImmediateEffectiveFrom(),
    effectiveTo: "",
    changeReason: "",
    ...emptyResourceFieldValues(resource),
    ...defaultFieldValues,
  }

  const form = useAppForm({
    defaultValues: defaults,
    validators: {
      onChange: buildResourceSchema(resource, RESOURCE_FIELDS[resource]),
    },
    onSubmit: async ({ value }) => {
      const response = await mutation.mutateAsync({
        resource,
        name: value.name.trim(),
        effectiveFrom: showEffectivePeriod
          ? value.effectiveFrom
          : defaultImmediateEffectiveFrom(),
        effectiveTo: showEffectivePeriod
          ? value.effectiveTo.trim() || undefined
          : undefined,
        changeReason: value.changeReason.trim(),
        fields: buildResourceFields(resource, value),
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

  const requestClose = (next: boolean) => {
    if (next) {
      onOpenChange(true)
      return
    }
    if (result?.outcome === "succeeded") {
      reset()
      onOpenChange(false)
      return
    }
    if (form.state.isDirty || result) {
      setDiscardOpen(true)
      return
    }
    onOpenChange(false)
  }

  return (
    <Dialog open={open} onOpenChange={requestClose}>
      <DialogContent className={dialogContentClass(resource)}>
        <DialogHeader>
          <DialogTitle>
            {masterDataCopy.createTitle(resourceLabel(resource))}
          </DialogTitle>
          <DialogDescription>{masterDataCopy.createDesc}</DialogDescription>
        </DialogHeader>

        <DialogScrollBody wide={wide}>
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
              <ResourceFieldsSection
                form={form}
                resource={resource}
                wide={wide}
              />
              {showEffectivePeriod ? (
                <div className="grid gap-3 sm:grid-cols-2">
                  <form.AppField
                    name="effectiveFrom"
                    children={(field) => (
                      <DateField
                        id="create-ef-from"
                        label={masterDataCopy.fieldEffectiveFrom}
                        field={field}
                      />
                    )}
                  />
                  <form.AppField
                    name="effectiveTo"
                    children={(field) => (
                      <DateField
                        id="create-ef-to"
                        label={masterDataCopy.fieldEffectiveTo}
                        field={field}
                      />
                    )}
                  />
                </div>
              ) : null}
              <form.AppField
                name="changeReason"
                children={(field) => (
                  <field.TextareaField
                    label={masterDataCopy.fieldChangeReason}
                  />
                )}
              />
              {!isWarehouse && showEffectivePeriod ? (
                <div className="space-y-2">
                  <Label htmlFor="create-sim">
                    {masterDataCopy.demoSimulateLabel}
                  </Label>
                  <OptionCombobox
                    id="create-sim"
                    value={simulate}
                    onValueChange={(v) =>
                      setSimulate((v ?? "ok") as "ok" | "overlap")
                    }
                    options={[
                      { value: "ok", label: masterDataCopy.demoOk },
                      {
                        value: "overlap",
                        label: masterDataCopy.demoOverlap,
                      },
                    ]}
                    className="w-full"
                    allowClear={false}
                    aria-label={masterDataCopy.demoSimulateLabel}
                    placeholder={masterDataCopy.demoSimulateLabel}
                  />
                </div>
              ) : null}
              <DialogFooter>
                <DialogClose
                  render={<Button type="button" variant="outline" />}
                >
                  关闭
                </DialogClose>
                <Button
                  type="submit"
                  disabled={mutation.isPending || isWarehouse}
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
        </DialogScrollBody>
      </DialogContent>

      <DiscardConfirmDialog
        open={discardOpen}
        onOpenChange={setDiscardOpen}
        title="放弃本次填写？"
        description="关闭后本次填写的内容将丢失。"
        confirmLabel="放弃填写"
        cancelLabel="继续编辑"
        onConfirm={() => {
          setDiscardOpen(false)
          reset()
          onOpenChange(false)
        }}
      />
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
  const queryClient = useQueryClient()
  const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
    newIdempotencyKey("revise")
  )
  const [simulate, setSimulate] = React.useState<
    "ok" | "overlap" | "base_unit" | "conflict"
  >("ok")
  const [result, setResult] = React.useState<MasterDataMutationResult | null>(
    null
  )
  const [discardOpen, setDiscardOpen] = React.useState(false)

  const isWarehouse = resource === "warehouses"
  const wide = usesWideDialog(resource)
  const showEffectivePeriod = usesEffectivePeriod(resource)
  const stableId = target && "stableId" in target ? target.stableId : ""
  const baseRevisionId =
    target && "currentRevisionId" in target
      ? target.currentRevisionId
      : target && "currentRevision" in target
        ? target.currentRevision.revisionId
        : ""
  const lockVersion = target?.lockVersion ?? 0
  const nameDefault = target?.name ?? ""
  // 更新默认「当前生效日」，避免不改直接保存把修改排期到未来。
  const effectiveFromDefault =
    target && "currentRevision" in target
      ? target.currentRevision.effectiveFrom
      : target && "effectiveFrom" in target && target.effectiveFrom
        ? target.effectiveFrom
        : defaultImmediateEffectiveFrom()

  const categoryListQuery = useMasterDataListQuery({
    resource: "categories",
    lifecycleStatus: "all",
    revisionTiming: "all",
  })
  const excludeCategoryIds = React.useMemo(() => {
    if (resource !== "categories" || !stableId) return undefined
    const forest = buildCategoryForest(categoryListQuery.data?.rows ?? [])
    return collectDescendantIds(forest, stableId)
  }, [categoryListQuery.data?.rows, resource, stableId])

  const defaults: ResourceFormValues = {
    name: nameDefault,
    effectiveFrom: showEffectivePeriod
      ? effectiveFromDefault
      : defaultImmediateEffectiveFrom(),
    effectiveTo: "",
    changeReason: "",
    ...emptyResourceFieldValues(resource),
  }

  const form = useAppForm({
    defaultValues: defaults,
    validators: {
      onChange: buildResourceSchema(resource, RESOURCE_FIELDS[resource]),
    },
    onSubmit: async ({ value }) => {
      if (!stableId || !baseRevisionId) return
      const response = await mutation.mutateAsync({
        resource,
        stableId,
        baseRevisionId,
        expectedLockVersion: lockVersion,
        name: value.name.trim(),
        effectiveFrom: showEffectivePeriod
          ? value.effectiveFrom
          : defaultImmediateEffectiveFrom(),
        effectiveTo: showEffectivePeriod
          ? value.effectiveTo.trim() || undefined
          : undefined,
        changeReason: value.changeReason.trim(),
        fields: buildResourceFields(resource, value),
        idempotencyKey,
        simulate: isWarehouse ? "ok" : simulate,
      })
      setResult(response)
    },
  })

  React.useEffect(() => {
    if (open && target) {
      form.setFieldValue("name", target.name)
      form.setFieldValue(
        "effectiveFrom",
        target && "currentRevision" in target
          ? target.currentRevision.effectiveFrom
          : target?.effectiveFrom ?? defaultImmediateEffectiveFrom()
      )
      form.setFieldValue(
        "effectiveTo",
        target && "currentRevision" in target
          ? target.currentRevision.effectiveTo ?? ""
          : ""
      )
      for (const [key, value] of Object.entries(
        currentResourceFieldValues(target)
      )) {
        form.setFieldValue(key, value)
      }
      setResult(null)
      setIdempotencyKey(newIdempotencyKey("revise"))
      setSimulate("ok")
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reset only when target opens
  }, [open, stableId, baseRevisionId])

  const reloadLatest = React.useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: masterDataKeys.all })
    setResult(null)
    setDiscardOpen(false)
    setIdempotencyKey(newIdempotencyKey("revise"))
  }, [queryClient])

  const requestClose = (next: boolean) => {
    if (next) {
      onOpenChange(true)
      return
    }
    if (result?.outcome === "succeeded") {
      onOpenChange(false)
      return
    }
    if (form.state.isDirty || result) {
      setDiscardOpen(true)
      return
    }
    onOpenChange(false)
  }

  return (
    <Dialog open={open} onOpenChange={requestClose}>
      <DialogContent className={dialogContentClass(resource)}>
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

        <DialogScrollBody wide={wide}>
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
              actions={
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={reloadLatest}
                >
                  {masterDataCopy.reloadAction}
                </Button>
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
                children={(field) => (
                  <field.TextField label={masterDataCopy.reviseNameLabel} />
                )}
              />
              <ResourceFieldsSection
                form={form}
                resource={resource}
                wide={wide}
                excludeCategoryIds={excludeCategoryIds}
              />
              {showEffectivePeriod ? (
                <div className="grid gap-3 sm:grid-cols-2">
                  <form.AppField
                    name="effectiveFrom"
                    children={(field) => (
                      <DateField
                        id="rev-ef-from"
                        label={masterDataCopy.fieldEffectiveFrom}
                        field={field}
                      />
                    )}
                  />
                  <form.AppField
                    name="effectiveTo"
                    children={(field) => (
                      <DateField
                        id="rev-ef-to"
                        label={masterDataCopy.fieldEffectiveTo}
                        field={field}
                      />
                    )}
                  />
                </div>
              ) : null}
              <form.AppField
                name="changeReason"
                children={(field) => (
                  <field.TextareaField
                    label={masterDataCopy.fieldChangeReason}
                  />
                )}
              />
              {!isWarehouse && showEffectivePeriod ? (
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
                          | "base_unit"
                          | "conflict"
                      )
                    }
                    options={[
                      { value: "ok", label: masterDataCopy.demoOk },
                      {
                        value: "overlap",
                        label: masterDataCopy.demoOverlap,
                      },
                      {
                        value: "conflict",
                        label: masterDataCopy.demoConflict,
                      },
                    ]}
                    className="w-full"
                    allowClear={false}
                    aria-label={masterDataCopy.demoSimulateLabel}
                    placeholder={masterDataCopy.demoSimulateLabel}
                  />
                </div>
              ) : null}
              <DialogFooter>
                <DialogClose
                  render={<Button type="button" variant="outline" />}
                >
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
        </DialogScrollBody>
      </DialogContent>

      <DiscardConfirmDialog
        open={discardOpen}
        onOpenChange={setDiscardOpen}
        title="放弃本次填写？"
        description="关闭后本次填写的内容将丢失。"
        confirmLabel="放弃填写"
        cancelLabel="继续编辑"
        onConfirm={() => {
          setDiscardOpen(false)
          onOpenChange(false)
        }}
      />
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
  const queryClient = useQueryClient()
  const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
    newIdempotencyKey("disable")
  )
  const [simulate, setSimulate] = React.useState<
    "ok" | "warehouse_stock" | "conflict"
  >("ok")
  const [result, setResult] = React.useState<MasterDataMutationResult | null>(
    null
  )
  const [discardOpen, setDiscardOpen] = React.useState(false)

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
      effectiveFrom: defaultImmediateEffectiveFrom(),
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
      setSimulate("ok")
      form.reset()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, stableId])

  const reloadLatest = React.useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: masterDataKeys.all })
    setResult(null)
    setIdempotencyKey(newIdempotencyKey("disable"))
  }, [queryClient])

  const requestClose = (next: boolean) => {
    if (next) {
      onOpenChange(true)
      return
    }
    if (result?.outcome === "succeeded") {
      onOpenChange(false)
      return
    }
    if (form.state.isDirty || result) {
      setDiscardOpen(true)
      return
    }
    onOpenChange(false)
  }

  return (
    <Dialog open={open} onOpenChange={requestClose}>
      <DialogContent className={dialogContentClass(resource)}>
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

        <DialogScrollBody wide={false}>
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
                ? [
                    {
                      label: "库存台账",
                      value: (
                        <Link
                          className="text-primary underline-offset-4 hover:underline"
                          href={result.drillHref}
                        >
                          打开库存台账
                        </Link>
                      ),
                    },
                  ]
                : []),
            ]}
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
            actions={
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={reloadLatest}
              >
                {masterDataCopy.reloadAction}
              </Button>
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
              name="effectiveFrom"
              children={(field) => (
                <DateField
                  id="dis-ef-from"
                  label={masterDataCopy.fieldDisableAt}
                  field={field}
                />
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
        </DialogScrollBody>
      </DialogContent>

      <DiscardConfirmDialog
        open={discardOpen}
        onOpenChange={setDiscardOpen}
        title="放弃本次填写？"
        description="关闭后本次填写的内容将丢失。"
        confirmLabel="放弃填写"
        cancelLabel="继续编辑"
        onConfirm={() => {
          setDiscardOpen(false)
          onOpenChange(false)
        }}
      />
    </Dialog>
  )
}
