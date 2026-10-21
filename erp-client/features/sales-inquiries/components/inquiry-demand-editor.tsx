"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useAppForm } from "@/components/form"
import { MoneyValue, QuantityValue } from "@/components/business"
import { OptionCombobox } from "@/components/business/option-combobox"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { DatePicker } from "@/components/ui/date-picker"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { CustomerSearchCombobox } from "@/features/entity-selectors/components/customer-search-combobox"
import { RegionCascadeSelect } from "@/features/regions"
import { SellableSkuSelectDialog } from "@/features/sales-orders/components/sellable-sku-select-dialog"
import type { SellableSkuPick } from "@/features/sales-orders/lib/sellable-sku-pick"
import { toAutomationIdSegment } from "@/lib/automation-id"
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
import type {
    InquiryAllocation,
    InquiryLine,
    InquiryRequirement,
    SalesInquiry,
} from "../types"

type DemandFormValues = {
    customer_id: string
    customer_name: string
    sales_owner_user_id: string
    business_org_unit_id: string
    deadline_at: number
    copied_from_inquiry_id?: string | null
    draft: InquiryRequirement
}

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

function newAllocation(): InquiryAllocation {
    return {
        allocation_id: crypto.randomUUID(),
        recipient: "",
        phone: "",
        region_code: "",
        region_name: "",
        address: "",
        quantity: "1",
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
            className="space-y-6"
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
                <div className="grid gap-4 md:grid-cols-2">
                    <form.Field
                        name="customer_id"
                        children={(field) => (
                            <div className="space-y-2">
                                <Label htmlFor="sales-inquiry-customer">
                                    客户
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
                                    value={inquiryDateInput(field.state.value)}
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
                            label="需求说明"
                            placeholder="客户交付要求及需要采购确认的事项"
                        />
                    )}
                />
                <section className="space-y-4" aria-label="商品与配送需求">
                    <div className="flex flex-wrap items-center justify-between gap-3">
                        <h2 className="text-base font-medium">
                            商品与配送需求
                        </h2>
                        <div className="flex gap-2">
                            {onImport && (
                                <Button
                                    id="sales-inquiry-import-open"
                                    variant="outline"
                                    type="button"
                                    onClick={onImport}
                                >
                                    导入客户选品文件
                                </Button>
                            )}
                            <Button
                                id="sales-inquiry-add-products"
                                variant="outline"
                                type="button"
                                onClick={() => setPickerOpen(true)}
                            >
                                添加商品
                            </Button>
                        </div>
                    </div>
                    <form.Field
                        name="draft.lines"
                        mode="array"
                        children={(linesField) => (
                            <div className="space-y-4">
                                {linesField.state.value.length === 0 && (
                                    <p className="rounded-lg border border-dashed p-8 text-center text-sm text-muted-foreground">
                                        先保存客户信息，再导入选品文件；也可从公司商品池添加商品。
                                    </p>
                                )}
                                {linesField.state.value.map((line, index) => {
                                    const prefix = `sales-inquiry-line-${toAutomationIdSegment(line.line_id)}`
                                    return (
                                        <article
                                            key={line.line_id}
                                            className="space-y-4 rounded-lg border p-4"
                                        >
                                            <div className="flex items-start justify-between gap-3">
                                                <div>
                                                    <h3 className="font-medium">
                                                        {line.product_name}
                                                    </h3>
                                                    <p className="mt-1 text-xs text-muted-foreground">
                                                        SKU {line.sku_code} ·{" "}
                                                        {line.specification ||
                                                            "未标注规格"}{" "}
                                                        · {line.unit_code}
                                                    </p>
                                                </div>
                                                <Button
                                                    id={`${prefix}-remove`}
                                                    type="button"
                                                    variant="ghost"
                                                    size="sm"
                                                    onClick={() =>
                                                        linesField.removeValue(
                                                            index,
                                                        )
                                                    }
                                                >
                                                    移除商品
                                                </Button>
                                            </div>
                                            <div className="grid gap-4 md:grid-cols-4">
                                                <form.AppField
                                                    name={`draft.lines[${index}].quantity`}
                                                    children={(field) => (
                                                        <field.TextField
                                                            id={`${prefix}-quantity`}
                                                            label="需求数量"
                                                            inputMode="decimal"
                                                            required
                                                        />
                                                    )}
                                                />
                                                <form.Field
                                                    name={`draft.lines[${index}].sales_reference_price_gross`}
                                                    children={(field) => (
                                                        <div className="space-y-2">
                                                            <Label
                                                                htmlFor={`${prefix}-reference-price`}
                                                            >
                                                                销售参考价（可不填）
                                                            </Label>
                                                            <Input
                                                                id={`${prefix}-reference-price`}
                                                                inputMode="decimal"
                                                                value={
                                                                    field.state
                                                                        .value ??
                                                                    ""
                                                                }
                                                                onChange={(
                                                                    event,
                                                                ) =>
                                                                    field.handleChange(
                                                                        event
                                                                            .target
                                                                            .value ||
                                                                            null,
                                                                    )
                                                                }
                                                            />
                                                        </div>
                                                    )}
                                                />
                                                <form.Field
                                                    name={`draft.lines[${index}].delivery_due_at`}
                                                    children={(field) => (
                                                        <div className="space-y-2">
                                                            <Label
                                                                htmlFor={`${prefix}-due-date`}
                                                            >
                                                                要求交付日期
                                                            </Label>
                                                            <DatePicker
                                                                id={`${prefix}-due-date`}
                                                                value={inquiryDateInput(
                                                                    field.state
                                                                        .value,
                                                                )}
                                                                onValueChange={(
                                                                    value,
                                                                ) =>
                                                                    field.handleChange(
                                                                        inquiryDateSeconds(
                                                                            value ??
                                                                                "",
                                                                        ),
                                                                    )
                                                                }
                                                            />
                                                        </div>
                                                    )}
                                                />
                                                <form.Field
                                                    name={`draft.lines[${index}].fulfillment_kind`}
                                                    children={(field) => (
                                                        <div className="space-y-2">
                                                            <Label
                                                                htmlFor={`${prefix}-fulfillment-kind`}
                                                            >
                                                                交付类型
                                                            </Label>
                                                            <OptionCombobox
                                                                id={`${prefix}-fulfillment-kind`}
                                                                aria-label="交付类型"
                                                                allowClear={
                                                                    false
                                                                }
                                                                options={[
                                                                    {
                                                                        value: "physical",
                                                                        label: "实物配送",
                                                                    },
                                                                    {
                                                                        value: "virtual",
                                                                        label: "虚拟交付",
                                                                    },
                                                                    {
                                                                        value: "service",
                                                                        label: "服务交付",
                                                                    },
                                                                ]}
                                                                value={
                                                                    field.state
                                                                        .value
                                                                }
                                                                onValueChange={(
                                                                    value,
                                                                ) =>
                                                                    field.handleChange(
                                                                        value as InquiryLine["fulfillment_kind"],
                                                                    )
                                                                }
                                                            />
                                                        </div>
                                                    )}
                                                />
                                            </div>
                                            <form.AppField
                                                name={`draft.lines[${index}].delivery_terms`}
                                                children={(field) => (
                                                    <field.TextField
                                                        id={`${prefix}-delivery-terms`}
                                                        label="交付条件"
                                                    />
                                                )}
                                            />
                                            <form.Field
                                                name={`draft.lines[${index}].allocations`}
                                                mode="array"
                                                children={(
                                                    allocationsField,
                                                ) => (
                                                    <section className="space-y-3">
                                                        <div className="flex items-center justify-between gap-3">
                                                            <h4 className="text-sm font-medium">
                                                                配送地址{" "}
                                                                <span className="font-normal text-muted-foreground">
                                                                    （实物地址数量合计须等于需求数量）
                                                                </span>
                                                            </h4>
                                                            <Button
                                                                id={`${prefix}-add-address`}
                                                                type="button"
                                                                size="sm"
                                                                variant="outline"
                                                                disabled={
                                                                    allocationsField
                                                                        .state
                                                                        .value
                                                                        .length >=
                                                                    100
                                                                }
                                                                onClick={() =>
                                                                    allocationsField.pushValue(
                                                                        newAllocation(),
                                                                    )
                                                                }
                                                            >
                                                                添加地址
                                                            </Button>
                                                        </div>
                                                        {allocationsField.state.value.map(
                                                            (
                                                                allocation,
                                                                allocationIndex,
                                                            ) => {
                                                                const frozenAddress =
                                                                    inquiry?.requirements.some(
                                                                        (
                                                                            snapshot,
                                                                        ) =>
                                                                            snapshot.data.lines.some(
                                                                                (
                                                                                    row,
                                                                                ) =>
                                                                                    row.allocations.some(
                                                                                        (
                                                                                            address,
                                                                                        ) =>
                                                                                            address.allocation_id ===
                                                                                            allocation.allocation_id,
                                                                                    ),
                                                                            ),
                                                                    ) ?? false
                                                                const occupiedAddress =
                                                                    inquiry?.quantity_ledger.some(
                                                                        (row) =>
                                                                            row.state !==
                                                                                "released" &&
                                                                            row.selections.some(
                                                                                (
                                                                                    selection,
                                                                                ) =>
                                                                                    selection.allocation_id ===
                                                                                    allocation.allocation_id,
                                                                            ),
                                                                    ) ?? false
                                                                const allocationPrefix = `${prefix}-address-${toAutomationIdSegment(allocation.allocation_id)}`
                                                                return (
                                                                    <div
                                                                        key={
                                                                            allocation.allocation_id
                                                                        }
                                                                        className="grid gap-3 rounded-md bg-muted/40 p-3 md:grid-cols-4"
                                                                    >
                                                                        {frozenAddress && (
                                                                            <div className="md:col-span-4 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                                                                                <span>
                                                                                    {occupiedAddress
                                                                                        ? "该地址已有销售占用；请先处理下游，或添加新地址分配未转数量。"
                                                                                        : "已提交的收件资料保留原记录，修改时请替换地址。"}
                                                                                </span>
                                                                                <Button
                                                                                    id={`${allocationPrefix}-replace`}
                                                                                    type="button"
                                                                                    variant="outline"
                                                                                    size="sm"
                                                                                    disabled={
                                                                                        occupiedAddress
                                                                                    }
                                                                                    onClick={() =>
                                                                                        allocationsField.handleChange(
                                                                                            allocationsField.state.value.map(
                                                                                                (
                                                                                                    item,
                                                                                                    position,
                                                                                                ) =>
                                                                                                    position ===
                                                                                                    allocationIndex
                                                                                                        ? {
                                                                                                              ...item,
                                                                                                              allocation_id:
                                                                                                                  crypto.randomUUID(),
                                                                                                          }
                                                                                                        : item,
                                                                                            ),
                                                                                        )
                                                                                    }
                                                                                >
                                                                                    替换地址
                                                                                </Button>
                                                                            </div>
                                                                        )}
                                                                        <form.AppField
                                                                            name={`draft.lines[${index}].allocations[${allocationIndex}].recipient`}
                                                                            children={(
                                                                                field,
                                                                            ) => (
                                                                                <field.TextField
                                                                                    id={`${allocationPrefix}-recipient`}
                                                                                    disabled={
                                                                                        frozenAddress
                                                                                    }
                                                                                    label="收货人"
                                                                                />
                                                                            )}
                                                                        />
                                                                        <form.AppField
                                                                            name={`draft.lines[${index}].allocations[${allocationIndex}].phone`}
                                                                            children={(
                                                                                field,
                                                                            ) => (
                                                                                <field.TextField
                                                                                    id={`${allocationPrefix}-phone`}
                                                                                    disabled={
                                                                                        frozenAddress
                                                                                    }
                                                                                    label="联系电话"
                                                                                    type="tel"
                                                                                />
                                                                            )}
                                                                        />
                                                                        <form.AppField
                                                                            name={`draft.lines[${index}].allocations[${allocationIndex}].quantity`}
                                                                            children={(
                                                                                field,
                                                                            ) => (
                                                                                <field.TextField
                                                                                    id={`${allocationPrefix}-quantity`}
                                                                                    label="配送数量"
                                                                                    inputMode="decimal"
                                                                                />
                                                                            )}
                                                                        />
                                                                        <div className="flex items-end justify-end">
                                                                            <Button
                                                                                id={`${allocationPrefix}-remove`}
                                                                                disabled={
                                                                                    occupiedAddress
                                                                                }
                                                                                type="button"
                                                                                variant="ghost"
                                                                                size="sm"
                                                                                onClick={() =>
                                                                                    allocationsField.removeValue(
                                                                                        allocationIndex,
                                                                                    )
                                                                                }
                                                                            >
                                                                                移除地址
                                                                            </Button>
                                                                        </div>
                                                                        <form.Field
                                                                            name={`draft.lines[${index}].allocations[${allocationIndex}].region_code`}
                                                                            children={(
                                                                                field,
                                                                            ) => (
                                                                                <div className="space-y-2 md:col-span-2">
                                                                                    <Label>
                                                                                        省
                                                                                        /
                                                                                        市
                                                                                        /
                                                                                        区县
                                                                                    </Label>
                                                                                    <RegionCascadeSelect
                                                                                        idPrefix={`${allocationPrefix}-region`}
                                                                                        disabled={
                                                                                            frozenAddress
                                                                                        }
                                                                                        leafOnly
                                                                                        value={
                                                                                            field
                                                                                                .state
                                                                                                .value
                                                                                        }
                                                                                        onValueChange={
                                                                                            field.handleChange
                                                                                        }
                                                                                    />
                                                                                </div>
                                                                            )}
                                                                        />
                                                                        <form.AppField
                                                                            name={`draft.lines[${index}].allocations[${allocationIndex}].address`}
                                                                            children={(
                                                                                field,
                                                                            ) => (
                                                                                <field.TextField
                                                                                    id={`${allocationPrefix}-detail`}
                                                                                    disabled={
                                                                                        frozenAddress
                                                                                    }
                                                                                    className="md:col-span-2"
                                                                                    label="详细地址"
                                                                                />
                                                                            )}
                                                                        />
                                                                    </div>
                                                                )
                                                            },
                                                        )}
                                                    </section>
                                                )}
                                            />
                                            <div className="text-xs text-muted-foreground">
                                                需求{" "}
                                                <QuantityValue
                                                    value={line.quantity}
                                                    unit={line.unit_code}
                                                />
                                                {line.sales_reference_price_gross ? (
                                                    <>
                                                        {" "}
                                                        · 销售参考价{" "}
                                                        <MoneyValue
                                                            value={
                                                                line.sales_reference_price_gross
                                                            }
                                                        />
                                                    </>
                                                ) : null}
                                            </div>
                                        </article>
                                    )
                                })}
                            </div>
                        )}
                    />
                </section>
                <div className="flex flex-wrap items-center justify-end gap-3">
                    <p className="mr-auto text-xs text-muted-foreground">
                        {saved
                            ? "需求已保存。"
                            : "输入会在当前账号的浏览器中保留；后台刷新不会覆盖。"}
                    </p>
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
