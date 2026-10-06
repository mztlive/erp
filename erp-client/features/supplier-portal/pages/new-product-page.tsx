"use client"
import { useEffect, useRef, useState } from "react"
import { useRouter } from "next/navigation"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { compareDecimal } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { portalSaveApplication, portalSaveNewProduct } from "../api"
import {
    usePortalApplication,
    usePortalCommand,
    usePortalDictionaries,
} from "../hooks/queries"
import type {
    NewProductInput,
    PortalApplication,
    PortalDictionary,
} from "../types"
import {
    commandKey,
    isRejectedPortalCommand,
    timeLabel,
} from "../lib/presentation"
import {
    decimal,
    quantity,
    termsDefaults,
    termsFields,
    termsFromValues,
    termsSchema,
    valuesFromTerms,
} from "../lib/forms"
import { PortalCommandConflict } from "../components/command-conflict"
import { PortalError, PortalSurface } from "../components/surface"
import { usePortalProfile } from "../components/portal-session"
import { PortalCategorySuggestion } from "../components/category-suggestion"
import { PortalAttachments } from "../components/attachments"
import { PortalBatchDialog } from "../components/batch-dialog"

const newSku = () => ({
    ...termsDefaults,
    rowId: crypto.randomUUID(),
    name: "",
    specs: "",
    unitRaw: "",
    unitId: "",
    unitVersion: 0,
    barcode: "",
    imageAssetId: "",
    reportedAt: Math.floor(Date.now() / 1000),
    packOriginalUnit: "",
    packBaseUnit: "",
    packRatio: "",
    packOriginalPrice: "",
    packConfirmed: false,
    quoteBasis: "",
})
const skuSchema = termsSchema
    .omit({ reason: true })
    .extend({
        name: z.string().trim().min(1, "请输入SKU名称"),
        orderingCode: z.string().trim().min(1, "请输入订货编码"),
        unitRaw: z.string().trim().min(1, "请输入原始单位及包装含义"),
        specs: z.string(),
        rowId: z.string(),
        packOriginalUnit: z.string(),
        packBaseUnit: z.string(),
        packRatio: z.string(),
        packOriginalPrice: z.string(),
        packConfirmed: z.boolean(),
        quoteBasis: z.string(),
    })
    .superRefine((sku, context) => {
        if (
            ![
                sku.packOriginalUnit,
                sku.packBaseUnit,
                sku.packRatio,
                sku.packOriginalPrice,
            ].some(Boolean)
        )
            return
        if (!sku.packOriginalUnit.trim() || !sku.packBaseUnit.trim())
            context.addIssue({
                code: "custom",
                message: "请填写完整的原报价单位和基础单位",
            })
        if (sku.packBaseUnit.trim() !== sku.unitRaw.trim())
            context.addIssue({
                code: "custom",
                message: "包装基础单位必须与报价基础单位一致",
            })
        if (
            !quantity.safeParse(sku.packRatio).success ||
            compareDecimal(sku.packRatio || "0", "0", 6) <= 0
        )
            context.addIssue({
                code: "custom",
                message: "每包装基础数量必须为正数",
            })
        if (!decimal.safeParse(sku.packOriginalPrice).success)
            context.addIssue({
                code: "custom",
                message: "请填写有效的原包装含税单价",
            })
        if (!sku.packConfirmed)
            context.addIssue({
                code: "custom",
                message: "请确认供货条款及可供数量已换算为基础单位",
            })
    })
const completeSchema = z.object({
    name: z.string().trim().min(1, "请输入商品名称"),
    productKind: z.string().min(1, "请选择商品类型"),
    brandRaw: z.string().trim().min(1, "请输入品牌原始资料"),
    categoryRaw: z.string().trim().min(1, "请输入分类原始完整路径"),
    skus: z
        .array(skuSchema)
        .min(1, "至少填写一个SKU")
        .max(100, "每批最多100个SKU"),
})
function dictionaryValue(
    raw: string,
    id: string,
    candidates: PortalDictionary[],
    version?: number,
) {
    const selected = candidates.find((candidate) => candidate.id === id)
    return {
        raw_name: raw,
        selected_id: id || null,
        expected_version: id ? version || selected?.version || null : null,
    }
}
export function PortalNewProductPage() {
    const [batch, setBatch] = useState(false)
    return (
        <PortalSurface
            title="新品提报"
            description="商品资料及多个规格一次提交。审核入库后，新建规格由内部定价并上架。"
            actions={
                <Button
                    id="supplier-portal-new-product-batch"
                    variant="outline"
                    onClick={() => setBatch(true)}
                >
                    批量提报新品
                </Button>
            }
        >
            <PortalNewProductEditor />
            {batch && (
                <PortalBatchDialog
                    mode="new_product"
                    onClose={() => setBatch(false)}
                />
            )}
        </PortalSurface>
    )
}
export function PortalNewProductEditor({
    draft,
    onSaved,
    onUnresolvedChange,
    idPrefix = "supplier-portal-new-product",
}: {
    draft?: PortalApplication
    onSaved?: (application: PortalApplication) => void
    idPrefix?: string
    onUnresolvedChange?: (unresolved: boolean) => void
}) {
    const profile = usePortalProfile()
    const router = useRouter()
    const brand = usePortalDictionaries("brand")
    const category = usePortalDictionaries("category")
    const unit = usePortalDictionaries("unit")
    const application = usePortalApplication(draft?.id ?? "")
    const source = draft?.input as unknown as NewProductInput | undefined
    const [defaults] = useState(() => ({
        name: source?.name ?? "",
        productKind: source?.product_kind ?? "PHYSICAL",
        brandRaw: source?.brand?.raw_name ?? "",
        brandId: source?.brand?.selected_id ?? "",
        brandVersion: source?.brand?.expected_version ?? 0,
        categoryRaw: source?.category?.raw_name ?? "",
        categoryId: source?.category?.selected_id ?? "",
        categoryVersion: source?.category?.expected_version ?? 0,
        model: source?.model ?? "",
        description: source?.description ?? "",
        imageAssetIds: source?.image_asset_ids ?? [],
        fileAssetIds: source?.file_asset_ids ?? [],
        skus: source?.skus?.map((sku) => ({
            ...newSku(),
            ...valuesFromTerms(sku.supply_terms),
            rowId: sku.row_id,
            name: sku.name,
            specs: sku.spec_entries
                .map(
                    (entry) =>
                        `${entry.attribute_code}=${entry.attribute_value_code}`,
                )
                .join("；"),
            unitRaw: sku.unit.raw_name,
            unitId: sku.unit.selected_id ?? "",
            unitVersion: sku.unit.expected_version ?? 0,
            barcode: sku.barcode ?? "",
            imageAssetId: sku.image_asset_id ?? "",
            orderingCode: sku.ordering_code,
            quantity: sku.available_quantity ?? "",
            reportedAt: sku.reported_at,
            packOriginalUnit: sku.packaging?.original_unit ?? "",
            packBaseUnit: sku.packaging?.base_unit ?? "",
            packRatio: sku.packaging?.units_per_package ?? "",
            packOriginalPrice: sku.packaging?.original_unit_price ?? "",
            packConfirmed:
                sku.packaging?.conversion_confirmed_by_supplier ?? false,
            quoteBasis: sku.quote_basis ?? "",
        })) ?? [newSku()],
    }))
    const [error, setError] = useState<unknown>(null)
    const [complete, setComplete] = useState<string | null>(null)
    const [requestVersion, setRequestVersion] = useState(draft?.version ?? 0)
    const [uploadBusy, setUploadBusy] = useState(false)
    const intent = useRef<{
        body: NewProductInput
        key: string
        version: number
    } | null>(null)
    const save = usePortalCommand(
        (input: { body: NewProductInput; key: string; version: number }) =>
            draft
                ? portalSaveApplication(
                      {
                          input: input.body,
                          expected_version: input.version,
                          idempotency_key: input.key,
                      },
                      draft.id,
                  )
                : portalSaveNewProduct(input.body, input.key),
    )
    const form = useAppForm({
        defaultValues: defaults,
        validators: {
            onSubmit: ({ value }) =>
                !value.productKind
                    ? "请选择商品类型"
                    : value.skus.length > 100
                      ? "最多100个SKU"
                      : undefined,
        },
        onSubmit: async ({ value }) => {
            setError(null)
            const body: NewProductInput = {
                name: value.name,
                product_kind: value.productKind,
                brand: dictionaryValue(
                    value.brandRaw,
                    value.brandId,
                    brand.data ?? [],
                    value.brandVersion,
                ),
                category: dictionaryValue(
                    value.categoryRaw,
                    value.categoryId,
                    category.data ?? [],
                    value.categoryVersion,
                ),
                model: value.model || undefined,
                description: value.description || undefined,
                image_asset_ids: value.imageAssetIds,
                file_asset_ids: value.fileAssetIds,
                skus: value.skus.map((sku) => ({
                    row_id: sku.rowId,
                    name: sku.name,
                    spec_entries: sku.specs
                        .split(/[；;\n]/)
                        .filter((item) => item.trim())
                        .map((item) => {
                            const position = item.indexOf("=")
                            if (
                                position < 1 ||
                                !item.slice(position + 1).trim()
                            )
                                throw new Error(
                                    `规格「${item}」请按“规格名=取值”填写`,
                                )
                            return {
                                attribute_code: item.slice(0, position).trim(),
                                attribute_value_code: item
                                    .slice(position + 1)
                                    .trim(),
                            }
                        }),
                    unit: dictionaryValue(
                        sku.unitRaw,
                        sku.unitId,
                        unit.data ?? [],
                        sku.unitVersion,
                    ),
                    barcode: sku.barcode || undefined,
                    image_asset_id: sku.imageAssetId || undefined,
                    ordering_code: sku.orderingCode,
                    supply_terms: {
                        ...termsFromValues({
                            ...sku,
                            taxPercentage: sku.taxPercentage || "0",
                        }),
                        input_tax_rate: sku.taxPercentage
                            ? termsFromValues(sku).input_tax_rate
                            : "",
                    },
                    available_quantity: sku.quantity || null,
                    reported_at: sku.reportedAt,
                    packaging:
                        sku.packOriginalUnit ||
                        sku.packBaseUnit ||
                        sku.packRatio ||
                        sku.packOriginalPrice
                            ? {
                                  original_unit: sku.packOriginalUnit,
                                  base_unit: sku.packBaseUnit,
                                  units_per_package: sku.packRatio,
                                  original_unit_price: sku.packOriginalPrice,
                                  conversion_confirmed_by_supplier:
                                      sku.packConfirmed,
                              }
                            : undefined,
                    quote_basis: sku.quoteBasis || undefined,
                })),
            }
            intent.current ??= {
                body,
                key: commandKey("new-product"),
                version: requestVersion,
            }
            try {
                const result = await save.mutateAsync(intent.current)
                intent.current = null
                if (onSaved) onSaved(result)
                else
                    router.push(
                        `/supplier-portal/applications/${encodeURIComponent(result.id)}`,
                    )
            } catch (cause) {
                if (isRejectedPortalCommand(cause)) intent.current = null
                setError(cause)
            }
        },
    })
    useEffect(() => {
        onUnresolvedChange?.(save.isPending || uploadBusy || !!intent.current)
    }, [onUnresolvedChange, save.isPending, uploadBusy, error])
    const disabled =
        profile?.role !== "maintainer" ||
        save.isPending ||
        uploadBusy ||
        !!intent.current
    const dictionaryOptions = (
        rows: PortalDictionary[],
        selectedKind?: string,
    ) =>
        rows
            .filter(
                (row) =>
                    !row.product_kind ||
                    !selectedKind ||
                    row.product_kind === selectedKind,
            )
            .map((row) => ({ value: row.id, label: row.path ?? row.name }))
    return (
        <form
            className="space-y-6 rounded-xl border bg-card p-5"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit().catch(setError)
            }}
        >
            <PortalError error={error} />
            <PortalCommandConflict
                error={error}
                id={`${idPrefix}-recheck-conflict`}
                disabled={save.isPending || uploadBusy}
                onRecheck={async () => {
                    if (draft) {
                        const result = await application.refetch()
                        if (!result.data || result.isError)
                            throw (
                                result.error ??
                                new Error("申请暂不可用，请重新读取")
                            )
                        if (
                            !["draft", "returned", "withdrawn"].includes(
                                result.data.status,
                            )
                        )
                            throw new Error(
                                "申请已进入采购确认，请返回申请页核对状态",
                            )
                        setRequestVersion(result.data.version)
                    }
                    await Promise.all([
                        brand.refetch(),
                        category.refetch(),
                        unit.refetch(),
                    ])
                    setComplete(
                        "当前资料已读取，原稿继续保留。请核对品牌、分类和单位匹配，重新选择后保存。",
                    )
                }}
                onConfirmed={() => {
                    intent.current = null
                    setError(null)
                }}
            />
            <PortalError
                error={brand.error ?? category.error ?? unit.error}
                id={`${idPrefix}-dictionary-retry`}
                retry={() => {
                    void brand.refetch()
                    void category.refetch()
                    void unit.refetch()
                }}
            />
            <div className="grid gap-4 md:grid-cols-2">
                <form.AppField name="name">
                    {(field) => (
                        <field.TextField
                            id={`${idPrefix}-name`}
                            label="商品名称"
                            disabled={disabled}
                        />
                    )}
                </form.AppField>
                <form.AppField name="productKind">
                    {(field) => (
                        <field.SelectField
                            id={`${idPrefix}-kind`}
                            label="商品类型"
                            allowClear={false}
                            disabled={disabled}
                            options={[
                                { value: "PHYSICAL", label: "实物" },
                                { value: "VIRTUAL", label: "虚拟商品" },
                                { value: "OFFLINE_SERVICE", label: "线下服务" },
                                { value: "VOUCHER", label: "卡券" },
                            ]}
                        />
                    )}
                </form.AppField>
                <form.AppField name="brandRaw">
                    {(field) => (
                        <field.TextField
                            id={`${idPrefix}-brand-raw`}
                            label="品牌原始资料"
                            disabled={disabled}
                            description="未匹配时保留真实名称；无品牌请明确填写，不以未知替代。"
                        />
                    )}
                </form.AppField>
                <form.AppField name="brandId">
                    {(field) => (
                        <field.SelectField
                            id={`${idPrefix}-brand`}
                            label="匹配已有品牌（可空）"
                            options={dictionaryOptions(brand.data ?? [])}
                            disabled={disabled}
                            onValueChange={(id) => {
                                const selected = brand.data?.find(
                                    (item) => item.id === id,
                                )
                                if (selected) {
                                    form.setFieldValue(
                                        "brandVersion",
                                        selected.version,
                                    )
                                } else form.setFieldValue("brandVersion", 0)
                            }}
                        />
                    )}
                </form.AppField>
                <form.AppField name="categoryRaw">
                    {(field) => (
                        <field.TextField
                            id={`${idPrefix}-category-raw`}
                            label="分类原始完整路径"
                            disabled={disabled}
                            description="例如：食品 / 饮料 / 果汁；尚未匹配可保存并送审。"
                        />
                    )}
                </form.AppField>
                <form.Subscribe selector={(state) => state.values.productKind}>
                    {(kind) => (
                        <form.AppField name="categoryId">
                            {(field) => (
                                <field.SelectField
                                    id={`${idPrefix}-category`}
                                    label="匹配已有分类（可空）"
                                    options={dictionaryOptions(
                                        category.data ?? [],
                                        kind,
                                    )}
                                    disabled={disabled}
                                    onValueChange={(id) => {
                                        const selected = category.data?.find(
                                            (item) => item.id === id,
                                        )
                                        if (selected) {
                                            form.setFieldValue(
                                                "categoryVersion",
                                                selected.version,
                                            )
                                        } else
                                            form.setFieldValue(
                                                "categoryVersion",
                                                0,
                                            )
                                    }}
                                />
                            )}
                        </form.AppField>
                    )}
                </form.Subscribe>
                <form.AppField name="model">
                    {(field) => (
                        <field.TextField
                            id={`${idPrefix}-model`}
                            label="型号"
                            disabled={disabled}
                        />
                    )}
                </form.AppField>
            </div>
            <form.Subscribe
                selector={(state) => ({
                    path: state.values.categoryRaw,
                    kind: state.values.productKind,
                })}
            >
                {(value) => (
                    <PortalCategorySuggestion
                        originalPath={value.path}
                        productKind={value.kind}
                        candidates={category.data ?? []}
                        disabled={disabled}
                        onApply={(candidate) => {
                            form.setFieldValue("categoryId", candidate.id)
                            form.setFieldValue(
                                "categoryVersion",
                                candidate.version,
                            )
                        }}
                    />
                )}
            </form.Subscribe>
            <form.AppField name="description">
                {(field) => (
                    <field.TextareaField
                        id={`${idPrefix}-description`}
                        label="商品描述及资料说明"
                        rows={4}
                        disabled={disabled}
                    />
                )}
            </form.AppField>
            <section className="space-y-3">
                <h2 className="font-semibold">商品图片与资料</h2>
                {draft ? (
                    <>
                        <form.AppField name="imageAssetIds">
                            {(field) => (
                                <PortalAttachments
                                    applicationId={draft.id}
                                    prefix={`${idPrefix}-images`}
                                    expectedVersion={requestVersion}
                                    onVersionChange={setRequestVersion}
                                    onBusyChange={setUploadBusy}
                                    assetIds={field.state.value}
                                    onChange={field.handleChange}
                                    disabled={disabled}
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="fileAssetIds">
                            {(field) => (
                                <PortalAttachments
                                    applicationId={draft.id}
                                    expectedVersion={requestVersion}
                                    onVersionChange={setRequestVersion}
                                    onBusyChange={setUploadBusy}
                                    assetIds={field.state.value}
                                    onChange={field.handleChange}
                                    prefix={`${idPrefix}-documents`}
                                    imageOnly={false}
                                    disabled={disabled}
                                />
                            )}
                        </form.AppField>
                    </>
                ) : (
                    <p className="text-sm text-muted-foreground">
                        先保存草稿，再上传商品及规格图片、PDF资料。上传完成后保存并提交审核。
                    </p>
                )}
            </section>
            <form.AppField name="skus" mode="array">
                {(array) => (
                    <section className="space-y-4">
                        <div className="flex flex-wrap items-center justify-between gap-2">
                            <h2 className="font-semibold">
                                规格及首次报价 · {array.state.value.length} 行
                            </h2>
                            <Button
                                id={`${idPrefix}-add-sku`}
                                type="button"
                                variant="outline"
                                disabled={
                                    disabled || array.state.value.length >= 100
                                }
                                onClick={() => array.pushValue(newSku())}
                            >
                                添加一个规格
                            </Button>
                        </div>
                        {array.state.value.map((sku, index) => {
                            const prefix = `${idPrefix}-sku-${toAutomationIdSegment(sku.rowId)}`
                            return (
                                <section
                                    key={sku.rowId}
                                    className="space-y-4 rounded-lg border p-4"
                                >
                                    <div className="flex items-center justify-between">
                                        <h3 className="font-medium">
                                            规格 {index + 1}
                                        </h3>
                                        <Button
                                            id={`${prefix}-remove`}
                                            type="button"
                                            variant="ghost"
                                            disabled={
                                                disabled ||
                                                array.state.value.length <= 1
                                            }
                                            onClick={() =>
                                                array.removeValue(index)
                                            }
                                        >
                                            移除此规格
                                        </Button>
                                    </div>
                                    <div className="grid gap-4 md:grid-cols-2">
                                        {(
                                            [
                                                ["name", "SKU名称"],
                                                [
                                                    "orderingCode",
                                                    "供应商订货编码",
                                                ],
                                                [
                                                    "specs",
                                                    "规格名=取值（多个用分号分隔）",
                                                ],
                                                [
                                                    "unitRaw",
                                                    "原始单位及包装含义",
                                                ],
                                                ["barcode", "条码"],
                                            ] as const
                                        ).map(([name, label]) => (
                                            <form.AppField
                                                key={name}
                                                name={`skus[${index}].${name}`}
                                            >
                                                {(field) => (
                                                    <field.TextField
                                                        id={`${prefix}-${name}`}
                                                        label={label}
                                                        disabled={disabled}
                                                    />
                                                )}
                                            </form.AppField>
                                        ))}
                                        <form.AppField
                                            name={`skus[${index}].unitId`}
                                        >
                                            {(field) => (
                                                <field.SelectField
                                                    id={`${prefix}-unit`}
                                                    label="匹配已有单位（可空）"
                                                    options={dictionaryOptions(
                                                        unit.data ?? [],
                                                    )}
                                                    disabled={disabled}
                                                    onValueChange={(id) => {
                                                        const selected =
                                                            unit.data?.find(
                                                                (item) =>
                                                                    item.id ===
                                                                    id,
                                                            )
                                                        if (selected) {
                                                            form.setFieldValue(
                                                                `skus[${index}].unitVersion`,
                                                                selected.version,
                                                            )
                                                        } else
                                                            form.setFieldValue(
                                                                `skus[${index}].unitVersion`,
                                                                0,
                                                            )
                                                    }}
                                                />
                                            )}
                                        </form.AppField>
                                        {termsFields.map(([name, label]) => (
                                            <form.AppField
                                                key={name}
                                                name={`skus[${index}].${name}`}
                                            >
                                                {(field) => (
                                                    <field.TextField
                                                        id={`${prefix}-${name}`}
                                                        label={label}
                                                        disabled={disabled}
                                                    />
                                                )}
                                            </form.AppField>
                                        ))}
                                        <form.AppField
                                            name={`skus[${index}].validFrom`}
                                        >
                                            {(field) => (
                                                <field.DateField
                                                    id={`${prefix}-valid-from`}
                                                    label="生效日期"
                                                    disabled={disabled}
                                                />
                                            )}
                                        </form.AppField>
                                        <form.AppField
                                            name={`skus[${index}].validTo`}
                                        >
                                            {(field) => (
                                                <field.DateField
                                                    id={`${prefix}-valid-to`}
                                                    label="失效日期（可空）"
                                                    disabled={disabled}
                                                />
                                            )}
                                        </form.AppField>
                                        <form.AppField
                                            name={`skus[${index}].quantity`}
                                        >
                                            {(field) => (
                                                <field.TextField
                                                    id={`${prefix}-quantity`}
                                                    label="可供数量"
                                                    description="空白表示未提供；0表示明确无货。价格与数量按上述基础单位填写。"
                                                    disabled={disabled}
                                                />
                                            )}
                                        </form.AppField>
                                    </div>
                                    <div className="flex flex-wrap items-center gap-3 text-sm">
                                        <span>
                                            本规格实际报送时间：
                                            {timeLabel(sku.reportedAt)}
                                        </span>
                                        <Button
                                            id={`${prefix}-report-now`}
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            disabled={disabled}
                                            onClick={() =>
                                                form.setFieldValue(
                                                    `skus[${index}].reportedAt`,
                                                    Math.floor(
                                                        Date.now() / 1000,
                                                    ),
                                                )
                                            }
                                        >
                                            已核对，按当前时间重新报送可供
                                        </Button>
                                    </div>
                                    <section className="space-y-3 rounded-lg bg-muted/30 p-4">
                                        <h4 className="text-sm font-medium">
                                            存在包装换算时填写
                                        </h4>
                                        <p className="text-xs text-muted-foreground">
                                            首次报价和可供数量必须按基础单位填写。系统保存原报价及换算依据，请自行核对，不自动转换价格。
                                        </p>
                                        <div className="grid gap-3 md:grid-cols-2">
                                            {(
                                                [
                                                    [
                                                        "packOriginalUnit",
                                                        "原报价单位（如箱）",
                                                    ],
                                                    [
                                                        "packBaseUnit",
                                                        "基础单位（如瓶，须与原始单位一致）",
                                                    ],
                                                    [
                                                        "packRatio",
                                                        "每包装基础单位数量",
                                                    ],
                                                    [
                                                        "packOriginalPrice",
                                                        "原包装含税单价",
                                                    ],
                                                    [
                                                        "quoteBasis",
                                                        "报价及换算依据",
                                                    ],
                                                ] as const
                                            ).map(([name, label]) => (
                                                <form.AppField
                                                    key={name}
                                                    name={`skus[${index}].${name}`}
                                                >
                                                    {(field) => (
                                                        <field.TextField
                                                            id={`${prefix}-${name}`}
                                                            label={label}
                                                            disabled={disabled}
                                                        />
                                                    )}
                                                </form.AppField>
                                            ))}
                                        </div>
                                        <form.AppField
                                            name={`skus[${index}].packConfirmed`}
                                        >
                                            {(field) => (
                                                <label
                                                    htmlFor={`${prefix}-pack-confirmed`}
                                                    className="flex items-start gap-2 text-sm"
                                                >
                                                    <NativeCheckbox
                                                        id={`${prefix}-pack-confirmed`}
                                                        checked={
                                                            field.state.value
                                                        }
                                                        disabled={disabled}
                                                        onBlur={
                                                            field.handleBlur
                                                        }
                                                        onChange={(event) =>
                                                            field.handleChange(
                                                                event.target
                                                                    .checked,
                                                            )
                                                        }
                                                    />
                                                    我已核对包装数量，并确认上方供货条款和数量已按基础单位填写。
                                                </label>
                                            )}
                                        </form.AppField>
                                    </section>
                                    {draft && (
                                        <form.AppField
                                            name={`skus[${index}].imageAssetId`}
                                        >
                                            {(field) => (
                                                <PortalAttachments
                                                    applicationId={draft.id}
                                                    expectedVersion={
                                                        requestVersion
                                                    }
                                                    onVersionChange={
                                                        setRequestVersion
                                                    }
                                                    onBusyChange={setUploadBusy}
                                                    assetIds={
                                                        field.state.value
                                                            ? [
                                                                  field.state
                                                                      .value,
                                                              ]
                                                            : []
                                                    }
                                                    onChange={(ids) =>
                                                        field.handleChange(
                                                            ids.at(-1) ?? "",
                                                        )
                                                    }
                                                    prefix={`${prefix}-image`}
                                                    disabled={disabled}
                                                />
                                            )}
                                        </form.AppField>
                                    )}
                                </section>
                            )
                        })}
                    </section>
                )}
            </form.AppField>
            {complete && (
                <p role="status" className="whitespace-pre-line text-sm">
                    {complete}
                </p>
            )}
            <div className="flex flex-wrap gap-2">
                <Button
                    id={`${idPrefix}-check`}
                    type="button"
                    variant="outline"
                    onClick={() => {
                        const result = completeSchema.safeParse(
                            form.state.values,
                        )
                        const rows = form.state.values.skus
                        const codes = rows.map((row) => row.orderingCode.trim())
                        setComplete(
                            !result.success
                                ? result.error.issues
                                      .map(
                                          (issue) =>
                                              `${typeof issue.path[1] === "number" ? `规格 ${issue.path[1] + 1}：` : ""}${issue.message}`,
                                      )
                                      .join("\n")
                                : new Set(codes).size !== codes.length
                                  ? "订货编码重复，请修正对应规格。"
                                  : "资料完整，可保存草稿后提交审核；尚未匹配的字典由内部审核处理。",
                        )
                    }}
                >
                    检查资料完整性
                </Button>
                <form.AppForm>
                    <form.SubmitButton
                        id={`${idPrefix}-save`}
                        label={
                            intent.current ? "重试保存原内容" : "保存新品草稿"
                        }
                        disabled={
                            profile?.role !== "maintainer" ||
                            save.isPending ||
                            uploadBusy
                        }
                    />
                </form.AppForm>
            </div>
        </form>
    )
}
