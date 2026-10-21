"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { PlusIcon, UploadIcon } from "lucide-react"
import type { DemandFormValues } from "../lib/demand-form"
import { deliveryProgress } from "../lib/delivery-progress"
import { InquiryDemandLines } from "./inquiry-demand-lines"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { DatePicker } from "@/components/ui/date-picker"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { CustomerSearchCombobox } from "@/features/entity-selectors/components/customer-search-combobox"
import { SellableSkuSelectDialog } from "@/features/sales-orders/components/sellable-sku-select-dialog"
import type { SellableSkuPick } from "@/features/sales-orders/lib/sellable-sku-pick"
import { hasPermission } from "@/lib/permissions"
import { getErrorMessage } from "@/lib/api/errors"
import { inquiryPath } from "../api"
import { useInquiryCommand } from "../hooks/use-inquiry-command"
import { inquiryDateInput, inquiryDateSeconds } from "../lib/format"
import {
    inquiryStorageKey,
    readInquiryLocalValue,
    writeInquiryLocalValue,
} from "../lib/local-draft"
import type { InquiryLine, InquiryRequirement, SalesInquiry } from "../types"

function fromPick(pick: SellableSkuPick): InquiryLine {
    return {
        line_id: crypto.randomUUID(),
        sku_id: pick.skuId,
        sku_revision_id: pick.skuRevisionId,
        sku_code: pick.skuNo,
        product_name: pick.name,
        specification: pick.specificationLabel,
        unit_code: pick.baseUnit,
        quantity_scale: 6,
        quantity: "1",
        fulfillment_kind:
            pick.productKind === "OFFLINE_SERVICE"
                ? "service"
                : pick.productKind === "VIRTUAL"
                  ? "virtual"
                  : "physical",
        delivery_due_at: 0,
        delivery_terms: "",
        sales_reference_price_gross: null,
        allocations: [],
    }
}

export function InquiryDemandEditor({
    inquiry,
    copyFrom,
    userId,
    permissions,
    onSaved,
    onImport,
    onDirtyChange,
    onConfirmRequirement,
    storageScope,
}: {
    inquiry?: SalesInquiry
    copyFrom?: SalesInquiry
    userId: string
    permissions: readonly string[]
    onSaved: () => void
    onImport?: () => void
    onDirtyChange?: (dirty: boolean) => void
    onConfirmRequirement?: (requirement: InquiryRequirement) => void
    storageScope?: string
}) {
    const storageKey = inquiryStorageKey(
        userId,
        storageScope ??
            `${inquiry?.id ?? (copyFrom ? `new-from-${copyFrom.id}` : "new")}:demand`,
    )
    const [initial, setInitial] = React.useState<{
        values: DemandFormValues
        base_version: number
    } | null>(null)
    React.useEffect(() => {
        const stored = readInquiryLocalValue<{
            base_version: number
            values: DemandFormValues
        }>(storageKey)
        onDirtyChange?.(Boolean(stored))
        setInitial(
            stored ?? {
                base_version: inquiry?.version ?? 0,
                values: {
                    customer_id:
                        inquiry?.customer_id ?? copyFrom?.customer_id ?? "",
                    customer_name:
                        inquiry?.customer_name ?? copyFrom?.customer_name ?? "",
                    sales_owner_user_id: inquiry?.sales_owner_user_id ?? userId,
                    business_org_unit_id: inquiry?.business_org_unit_id ?? "",
                    deadline_at: inquiry?.deadline_at ?? 0,
                    copied_from_inquiry_id: copyFrom?.id ?? null,
                    draft:
                        inquiry?.draft ??
                        (copyFrom
                            ? {
                                  ...copyFrom.draft,
                                  lines: copyFrom.draft.lines.map((line) => ({
                                      ...line,
                                      line_id: crypto.randomUUID(),
                                      allocations: line.allocations.map(
                                          (address) => ({
                                              ...address,
                                              allocation_id:
                                                  crypto.randomUUID(),
                                          }),
                                      ),
                                  })),
                              }
                            : { lines: [], note: "" }),
                },
            },
        )
        // 同一编辑对象的后台刷新不重建正在输入的表单。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [storageKey])
    return initial ? (
        <DemandEditorContent
            key={storageKey}
            initial={initial.values}
            baseVersion={initial.base_version}
            inquiry={inquiry}
            userId={userId}
            permissions={permissions}
            storageKey={storageKey}
            onSaved={onSaved}
            onImport={onImport}
            onDirtyChange={onDirtyChange}
            onConfirmRequirement={onConfirmRequirement}
        />
    ) : (
        <p role="status">正在恢复询价输入…</p>
    )
}

function DemandEditorContent({
    initial,
    baseVersion,
    inquiry,
    userId,
    permissions,
    storageKey,
    onSaved,
    onImport,
    onDirtyChange,
    onConfirmRequirement,
}: {
    initial: DemandFormValues
    baseVersion: number
    inquiry?: SalesInquiry
    userId: string
    permissions: readonly string[]
    storageKey: string
    onSaved: () => void
    onImport?: () => void
    onDirtyChange?: (dirty: boolean) => void
    onConfirmRequirement?: (requirement: InquiryRequirement) => void
}) {
    const router = useRouter()
    const command = useInquiryCommand(
        userId,
        inquiry?.id ??
            (initial.copied_from_inquiry_id
                ? `new-from-${initial.copied_from_inquiry_id}`
                : "new"),
    )
    const [pickerOpen, setPickerOpen] = React.useState(false)
    const [failure, setFailure] = React.useState<string | null>(null)
    const [localFailure, setLocalFailure] = React.useState(false)
    const [saved, setSaved] = React.useState(false)
    const canSave =
        hasPermission(permissions, "sellable_sku:list") &&
        hasPermission(
            permissions,
            onConfirmRequirement
                ? "sales_inquiry:reinquire"
                : inquiry
                  ? "sales_inquiry:update"
                  : "sales_inquiry:create",
        )
    const versionRef = React.useRef(baseVersion)
    const form = useAppForm({
        defaultValues: initial,
        listeners: {
            onChange: ({ formApi }) => {
                setSaved(false)
                onDirtyChange?.(true)
                try {
                    writeInquiryLocalValue(storageKey, {
                        base_version: versionRef.current,
                        values: formApi.state.values,
                    })
                    setLocalFailure(false)
                } catch {
                    setLocalFailure(true)
                }
            },
        },
        onSubmit: async ({ value }) => {
            setFailure(null)
            try {
                if (onConfirmRequirement) {
                    onConfirmRequirement(value.draft)
                    return
                }
                if (!value.customer_id) throw new Error("请选择客户后保存。")
                const result = await command.run<{
                    inquiry_id?: string
                    id?: string
                    version?: number
                }>(
                    inquiryPath(false, inquiry?.id) + (inquiry ? "/draft" : ""),
                    inquiry
                        ? {
                              expected_version: versionRef.current,
                              data: value.draft,
                          }
                        : { data: value },
                    inquiry ? "PUT" : "POST",
                )
                localStorage.removeItem(storageKey)
                if (result.version != null) versionRef.current = result.version
                setSaved(true)
                onDirtyChange?.(false)
                const createdId = result.inquiry_id ?? result.id
                if (!inquiry && createdId)
                    router.replace(
                        `/sales/inquiries/${encodeURIComponent(createdId)}`,
                    )
                else onSaved()
            } catch (error) {
                setFailure(
                    getErrorMessage(error, "保存失败，请保留输入并重试。"),
                )
            }
        },
    })
    const busy = command.isPending || !command.ready || command.unresolved
    React.useEffect(() => {
        const result = command.resolution?.result as
            | { inquiry_id?: string; version?: number }
            | undefined
        const expectedPath =
            inquiryPath(false, inquiry?.id) + (inquiry ? "/draft" : "")
        if (
            command.resolution?.path !== expectedPath ||
            result?.version == null
        )
            return
        versionRef.current = result.version
        if (!inquiry && result.inquiry_id && command.resolution?.replayed) {
            writeInquiryLocalValue(
                inquiryStorageKey(userId, `${result.inquiry_id}:demand`),
                { base_version: result.version, values: form.state.values },
            )
            localStorage.removeItem(storageKey)
            router.replace(
                `/sales/inquiries/${encodeURIComponent(result.inquiry_id)}`,
            )
        }
        // 仅自己的成功命令可更新本地编辑基准；外部刷新仍保留原版本并交由服务端冲突校验。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [command.resolution])
    return (
        <form
            className="min-w-0 space-y-6"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <fieldset disabled={command.isPending} className="contents">
                {command.unresolved && (
                    <Alert variant="warning">
                        <AlertDescription>
                            上次提交结果尚待核对，当前输入已保留。
                            <Button
                                id="sales-inquiry-demand-recover"
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() =>
                                    void command
                                        .recover()
                                        .then(() => onSaved())
                                        .catch((error) =>
                                            setFailure(getErrorMessage(error)),
                                        )
                                }
                            >
                                核对上次提交结果
                            </Button>
                        </AlertDescription>
                    </Alert>
                )}
                {failure && (
                    <Alert variant="destructive">
                        <AlertDescription>{failure}</AlertDescription>
                    </Alert>
                )}
                {localFailure && (
                    <Alert variant="warning">
                        <AlertDescription>
                            当前浏览器无法保存本地输入，请先保存询价后再离开页面。
                        </AlertDescription>
                    </Alert>
                )}
                <section
                    className="space-y-4 rounded-lg border p-5"
                    aria-label="基本信息"
                >
                    <h2 className="text-sm font-semibold">基本信息</h2>
                    <div className="grid gap-4 md:grid-cols-[minmax(0,1fr)_260px]">
                        <form.Field
                            name="customer_id"
                            children={(field) => (
                                <div className="space-y-2">
                                    <Label htmlFor="sales-inquiry-customer">
                                        客户{" "}
                                        <span className="text-destructive">
                                            *
                                        </span>
                                    </Label>
                                    <CustomerSearchCombobox
                                        id="sales-inquiry-customer"
                                        value={field.state.value || undefined}
                                        disabled={Boolean(inquiry)}
                                        onValueChange={(id) =>
                                            field.handleChange(id ?? "")
                                        }
                                        onItemChange={(item) =>
                                            form.setFieldValue(
                                                "customer_name",
                                                item?.legalName ?? "",
                                            )
                                        }
                                    />
                                </div>
                            )}
                        />
                        <form.Field
                            name="deadline_at"
                            children={(field) => (
                                <div className="space-y-2">
                                    <Label htmlFor="sales-inquiry-deadline">
                                        询价截止日期
                                    </Label>
                                    <DatePicker
                                        id="sales-inquiry-deadline"
                                        size="default"
                                        value={inquiryDateInput(
                                            field.state.value,
                                        )}
                                        onValueChange={(value) =>
                                            field.handleChange(
                                                inquiryDateSeconds(value ?? ""),
                                            )
                                        }
                                        disabled={Boolean(inquiry)}
                                    />
                                </div>
                            )}
                        />
                        {!inquiry && (
                            <p className="text-xs text-muted-foreground md:col-span-2">
                                销售负责人及业务组织按当前账号确定。
                            </p>
                        )}
                    </div>
                    <form.AppField
                        name="draft.note"
                        children={(field) => (
                            <field.TextareaField
                                id="sales-inquiry-demand-note"
                                label="需求说明（选填）"
                                rows={2}
                                textareaClassName="min-h-16"
                                placeholder="客户交付要求及需要采购确认的事项"
                            />
                        )}
                    />
                </section>
                <section
                    className="min-w-0 space-y-4"
                    aria-label="商品与配送需求"
                >
                    <div className="flex flex-wrap items-center justify-between gap-3">
                        <h2 className="text-base font-medium">
                            商品需求{" "}
                            <form.Subscribe
                                selector={(state) =>
                                    state.values.draft.lines.length
                                }
                            >
                                {(count) => (
                                    <span className="ml-2 text-sm font-normal text-muted-foreground">
                                        {count} 项
                                    </span>
                                )}
                            </form.Subscribe>
                        </h2>
                        <div className="flex flex-wrap gap-2">
                            {onImport && (
                                <Button
                                    id="sales-inquiry-import-open"
                                    variant="outline"
                                    type="button"
                                    onClick={onImport}
                                >
                                    <UploadIcon
                                        aria-hidden="true"
                                        data-icon="inline-start"
                                    />
                                    导入选品文件
                                </Button>
                            )}
                            <Button
                                id="sales-inquiry-add-products"
                                variant="outline"
                                type="button"
                                onClick={() => setPickerOpen(true)}
                            >
                                <PlusIcon
                                    aria-hidden="true"
                                    data-icon="inline-start"
                                />
                                添加商品
                            </Button>
                        </div>
                    </div>
                    <InquiryDemandLines
                        form={form}
                        inquiry={inquiry}
                        disabled={command.isPending}
                    />
                </section>
                <div className="sticky bottom-0 z-20 flex flex-wrap items-center justify-end gap-3 border-t bg-background px-1 py-4">
                    <div className="mr-auto space-y-1">
                        <form.Subscribe
                            selector={(state) => state.values.draft.lines}
                        >
                            {(lines) => {
                                const pending = lines.filter(
                                    (line) => !deliveryProgress(line).complete,
                                ).length
                                return (
                                    <p className="text-sm font-medium">
                                        已选 {lines.length} 项商品
                                        {pending > 0
                                            ? ` · ${pending} 项待完善配送`
                                            : ""}
                                    </p>
                                )
                            }}
                        </form.Subscribe>
                        <p
                            className="text-xs text-muted-foreground"
                            role="status"
                        >
                            {saved
                                ? "需求已保存。"
                                : localFailure
                                  ? "本机保留失败，请保存需求后再离开。"
                                  : "输入已在当前浏览器保留，保存后同步到询价草稿。"}
                        </p>
                    </div>
                    <Button
                        id="sales-inquiry-save-demand"
                        type="submit"
                        disabled={busy || !canSave}
                    >
                        {command.isPending
                            ? "正在保存…"
                            : onConfirmRequirement
                              ? "确认本次需求调整"
                              : inquiry
                                ? "保存需求"
                                : "创建询价草稿"}
                    </Button>
                </div>
                <SellableSkuSelectDialog
                    open={pickerOpen}
                    onOpenChange={setPickerOpen}
                    excludeProductKind="VOUCHER"
                    title="选择询价商品"
                    description="按公司 SKU 添加客户需求；销售参考价由销售确认，不作为采购报价。"
                    onConfirm={(picks) => {
                        for (const pick of picks)
                            form.pushFieldValue("draft.lines", fromPick(pick))
                        setPickerOpen(false)
                    }}
                />
            </fieldset>
        </form>
    )
}
