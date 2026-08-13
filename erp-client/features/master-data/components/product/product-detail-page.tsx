"use client"

/**
 * 商品详情页 = 查看 + 编辑（同一页面）。
 * - /master-data/products/new  新建
 * - /master-data/products/:id  查看并直接改，保存即形成新版本
 * 不使用侧边 sheet，也不再有单独的 ?mode=edit。
 */

import * as React from "react"
import Link from "next/link"
import {
    BanIcon,
    CheckCircle2Icon,
    CircleAlertIcon,
    ClipboardCheckIcon,
    SaveIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    DiscardConfirmDialog,
    FormalActionResult,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { ProductDisableDialog } from "@/features/master-data/components/shared/disable-action-dialog"
import {
    applySpecsFromDrafts,
    PRODUCT_EDITOR_SECTIONS,
    scrollToProductSection,
    type ProductEditorFormValues,
    type ProductSpecDraft,
    validateProductEditor,
} from "@/features/master-data/lib/product-editor-model"
import {
    ProductBasicSection,
    ProductEffectiveSection,
    ProductHistorySection,
    ProductMediaSection,
} from "@/features/master-data/components/product/product-editor-sections"
import { ProductSkuSection } from "@/features/master-data/components/product/product-sku-section"
import {
    ProductInventoryPreviewSheet,
    type ProductInventoryPreviewSku,
} from "@/features/master-data/components/product/product-inventory-preview-sheet"
import {
    RegisterSupplyForSkuDialog,
    type FixedSku,
} from "@/features/supplier-offerings/offering-dialogs"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { formatEffectiveRange } from "@/features/master-data/lib/filter"
import { useProductEditor } from "@/features/master-data/hooks/use-product-editor"
import type {
    ProductFields,
    ProductSkuFields,
} from "@/features/master-data/types"
import { cn } from "@/lib/utils"

export function ProductDetailPage({ stableId }: { stableId: string }) {
    const editor = useProductEditor(stableId)
    const {
        isCreate,
        router,
        accountQuery,
        detailQuery,
        categoryListQuery,
        brandListQuery,
        categoryOptions,
        brandOptions,
        unitOptionsQuery,
        data,
        supplierCountsQuery,
        form,
        formError,
        formErrorTitle,
        checkPassed,
        setCheckPassed,
        result,
        disableOpen,
        setDisableOpen,
        discardOpen,
        setDiscardOpen,
        pendingNav,
        setPendingNav,
        supplierDialogSku,
        setSupplierDialogSku,
        inventoryOpen,
        inventoryInitialSkuId,
        activeSection,
        setActiveSection,
        errorRef,
        checkedSnapshotRef,
        stickyHeaderRef,
        rememberPendingFiles,
        rememberSkuFile,
        navigateAway,
        openInventoryPreview,
        handleInventoryOpenChange,
        listHref,
        sectionScrollMarginPx,
        pending,
        canCreate,
        hasUpdatePermission,
        canRevise,
        canDisable,
        reviseBlocker,
        disableBlocker,
        runLocalCheck,
        setFormError,
        setFormErrorTitle,
    } = editor
    const canEdit = canRevise

    if (!isCreate && detailQuery.isPending) {
        return (
            <PageScaffold>
                <PageHeader
                    title="商品详情"
                    description={masterDataCopy.centerLoading}
                />
                <div
                    className="h-40 animate-pulse rounded-lg bg-muted"
                    aria-busy
                />
            </PageScaffold>
        )
    }

    if (!isCreate && (detailQuery.isError || !data)) {
        return (
            <PageScaffold>
                <PageHeader title="商品详情" />
                <BusinessFailureState
                    error={detailQuery.isError ? detailQuery.error : undefined}
                    description={
                        detailQuery.isError
                            ? masterDataCopy.centerLoadFail
                            : masterDataCopy.centerMissingDesc
                    }
                    action={
                        detailQuery.isError ? (
                            <Button
                                type="button"
                                onClick={() => void detailQuery.refetch()}
                            >
                                重试
                            </Button>
                        ) : (
                            <Button render={<Link href={listHref} />}>
                                {masterDataCopy.actionBackList}
                            </Button>
                        )
                    }
                />
            </PageScaffold>
        )
    }

    if (isCreate && accountQuery.isPending) {
        return (
            <PageScaffold>
                <PageHeader title="新建商品" description="正在核对创建权限" />
                <div
                    className="h-40 animate-pulse rounded-lg bg-muted"
                    aria-busy
                />
            </PageScaffold>
        )
    }

    if (isCreate && accountQuery.isError) {
        return (
            <PageScaffold>
                <PageHeader title="新建商品" />
                <BusinessFailureState
                    error={accountQuery.error}
                    onRetry={() => void accountQuery.refetch()}
                />
            </PageScaffold>
        )
    }

    if (isCreate && !canCreate) {
        return (
            <PageScaffold>
                <PageHeader title="新建商品" />
                <BusinessFailureState
                    kind="permission"
                    description="当前账号没有创建商品的权限，请联系管理员或有权限的同事。"
                    action={
                        <Button render={<Link href={listHref} />}>
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    const formId = "product-detail-form"

    return (
        <form.Subscribe selector={(state) => state.values}>
            {(values) => {
                const title = isCreate
                    ? masterDataCopy.productCreateTitle
                    : values.name || data?.name || "商品详情"
                const fields = values.fields
                const inventoryPreviewSkus: ProductInventoryPreviewSku[] =
                    fields.productKind === "PHYSICAL"
                        ? fields.skus.flatMap((sku) =>
                              sku.skuId
                                  ? [
                                        {
                                            skuId: sku.skuId,
                                            skuNo: sku.skuNo,
                                            specLabel: sku.specLabel,
                                            baseUnit:
                                                sku.baseUnit || fields.baseUnit,
                                        },
                                    ]
                                  : [],
                          )
                        : []
                const inventoryActionHint =
                    fields.productKind && fields.productKind !== "PHYSICAL"
                        ? "仅实物商品适用公司自有库存台账"
                        : inventoryPreviewSkus.length === 0
                          ? "选择实物商品类型并保存 SKU 后可查看正式库存"
                          : undefined
                const setName = (next: string) =>
                    form.setFieldValue("name", next)
                const setEffectiveFrom = (next: string) =>
                    form.setFieldValue("effectiveFrom", next)
                const setEffectiveTo = (next: string) =>
                    form.setFieldValue("effectiveTo", next)
                const setChangeReason = (next: string) =>
                    form.setFieldValue("changeReason", next)
                const setFields = (next: React.SetStateAction<ProductFields>) =>
                    form.setFieldValue("fields", (previous) =>
                        typeof next === "function" ? next(previous) : next,
                    )
                const setSpecDrafts = (
                    next: React.SetStateAction<readonly ProductSpecDraft[]>,
                ) =>
                    form.setFieldValue("specDrafts", (previous) =>
                        typeof next === "function" ? next(previous) : next,
                    )
                const syncSpecDrafts = (next: readonly ProductSpecDraft[]) => {
                    setSpecDrafts(next)
                    setFields((previous) =>
                        applySpecsFromDrafts(next, previous, values.name),
                    )
                }
                const updateSku = (
                    index: number,
                    patch: Partial<ProductSkuFields>,
                ) => {
                    setFields((previous) => ({
                        ...previous,
                        skus: previous.skus.map((sku, skuIndex) =>
                            skuIndex === index ? { ...sku, ...patch } : sku,
                        ),
                    }))
                }
                const handleSubmit = (event?: React.FormEvent) => {
                    event?.preventDefault()
                    void form.handleSubmit()
                }
                const name = values.name
                const effectiveFrom = values.effectiveFrom
                const effectiveTo = values.effectiveTo
                const changeReason = values.changeReason
                const specDrafts = values.specDrafts
                const activeSpecs = fields.specs.filter(
                    (spec) =>
                        spec.name.trim() &&
                        spec.values.some((value) => value.trim()),
                )
                const applyBatchReferencePrices = () => {
                    const hasAny =
                        values.batchSalePrice.trim() ||
                        values.batchMarketPrice.trim()
                    if (!hasAny) return
                    const hasFilled = values.fields.skus.some(
                        (sku) =>
                            sku.salePrice?.trim() || sku.marketPrice?.trim(),
                    )
                    const message = hasFilled
                        ? `将把批量价格应用到全部 ${values.fields.skus.length} 个 SKU，并覆盖已填写的销售价/市场价。确定继续？`
                        : `将把批量价格应用到全部 ${values.fields.skus.length} 个 SKU。确定继续？`
                    if (!window.confirm(message)) return
                    setFields((previous) => ({
                        ...previous,
                        skus: previous.skus.map((sku) => ({
                            ...sku,
                            salePrice:
                                values.batchSalePrice.trim() ||
                                sku.salePrice ||
                                undefined,
                            marketPrice:
                                values.batchMarketPrice.trim() ||
                                sku.marketPrice ||
                                undefined,
                        })),
                    }))
                }
                return (
                    <PageScaffold
                        style={
                            {
                                "--product-section-scroll-margin": `${sectionScrollMarginPx}px`,
                            } as React.CSSProperties
                        }
                    >
                        <form
                            id={formId}
                            className="flex flex-col gap-4"
                            onSubmit={handleSubmit}
                        >
                            <header
                                ref={stickyHeaderRef}
                                className={cn(
                                    surfacePanelClassName,
                                    "sticky top-0 z-30 overflow-hidden",
                                )}
                            >
                                <div className="flex flex-col gap-3 p-4 lg:flex-row lg:items-center lg:justify-between">
                                    <div className="min-w-0 space-y-1">
                                        <div className="flex min-w-0 flex-wrap items-center gap-2">
                                            <h1 className="truncate text-lg font-semibold tracking-tight">
                                                {title}
                                            </h1>
                                            {!isCreate && data ? (
                                                <StatusBadge
                                                    tone={data.lifecycleTone}
                                                    label={
                                                        data.lifecycleStatusLabel
                                                    }
                                                />
                                            ) : null}
                                        </div>
                                        {!isCreate && data ? (
                                            <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                                                <span>
                                                    单号{" "}
                                                    <span className="num text-foreground">
                                                        {data.stableNo}
                                                    </span>
                                                </span>
                                                <span className="num rounded-md bg-muted px-1.5 py-0.5 text-tiny text-foreground">
                                                    版本{" "}
                                                    {
                                                        data.currentRevision
                                                            .revisionNo
                                                    }
                                                </span>
                                                <span className="num">
                                                    {formatEffectiveRange(
                                                        data.currentRevision
                                                            .effectiveFrom,
                                                        data.currentRevision
                                                            .effectiveTo,
                                                    )}
                                                </span>
                                                <span className="inline-flex items-center gap-1.5">
                                                    <span>
                                                        {
                                                            masterDataCopy.centerVersionState
                                                        }
                                                    </span>
                                                    <StatusBadge
                                                        tone={
                                                            data.revisionTiming ===
                                                            "FUTURE"
                                                                ? "warning"
                                                                : "info"
                                                        }
                                                        label={
                                                            data.revisionTimingLabel
                                                        }
                                                    />
                                                </span>
                                            </div>
                                        ) : (
                                            <p className="text-sm text-muted-foreground">
                                                {
                                                    masterDataCopy.productCreateDesc
                                                }
                                            </p>
                                        )}
                                    </div>

                                    <div className="flex shrink-0 flex-wrap items-center gap-2">
                                        {!isCreate && data ? (
                                            <Button
                                                type="button"
                                                size="sm"
                                                variant="outline"
                                                disabled={!canDisable}
                                                title={
                                                    !hasUpdatePermission
                                                        ? "当前账号没有维护商品资料的权限。"
                                                        : disableBlocker?.message
                                                }
                                                onClick={() =>
                                                    setDisableOpen(true)
                                                }
                                            >
                                                <BanIcon
                                                    data-icon="inline-start"
                                                    aria-hidden
                                                />
                                                {masterDataCopy.actionDisable}
                                            </Button>
                                        ) : null}
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            onClick={() =>
                                                navigateAway(listHref)
                                            }
                                        >
                                            返回列表
                                        </Button>
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            disabled={!canRevise || pending}
                                            onClick={() =>
                                                runLocalCheck(values)
                                            }
                                        >
                                            <ClipboardCheckIcon
                                                data-icon="inline-start"
                                                aria-hidden
                                            />
                                            填写检查
                                        </Button>
                                        <Button
                                            type="submit"
                                            size="sm"
                                            disabled={!canRevise || pending}
                                        >
                                            <SaveIcon
                                                data-icon="inline-start"
                                                aria-hidden
                                            />
                                            {isCreate
                                                ? masterDataCopy.createSubmit
                                                : masterDataCopy.reviseSubmit}
                                        </Button>
                                    </div>
                                </div>

                                {!isCreate && data?.productConstraints ? (
                                    <div className="border-t border-border/60 bg-muted/40 px-4 py-2.5 text-xs">
                                        <p>
                                            基础单位{" "}
                                            <span className="num">
                                                {
                                                    data.productConstraints
                                                        .baseUnit
                                                }
                                            </span>
                                            {" · "}
                                            SKU{" "}
                                            <span className="num">
                                                {
                                                    data.productConstraints
                                                        .skuCount
                                                }
                                            </span>{" "}
                                            个
                                            {data.productConstraints
                                                .hasFormalReferences
                                                ? " · 已被业务单据引用"
                                                : null}
                                        </p>
                                        <p className="mt-1 text-muted-foreground">
                                            {masterDataCopy.centerSpecNote}
                                        </p>
                                    </div>
                                ) : null}

                                <div className="border-t border-border/60 p-2 sm:px-3 sm:pb-3">
                                    <nav
                                        aria-label="商品编辑分区"
                                        className={cn(
                                            "grid grid-cols-2 gap-0.5 rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10",
                                            isCreate
                                                ? "sm:grid-cols-4"
                                                : "sm:grid-cols-5",
                                        )}
                                    >
                                        {PRODUCT_EDITOR_SECTIONS.filter(
                                            (section) =>
                                                !isCreate ||
                                                section.id !== "history",
                                        ).map((section) => {
                                            const active =
                                                activeSection === section.id
                                            return (
                                                <Button
                                                    key={section.id}
                                                    type="button"
                                                    variant="ghost"
                                                    size="sm"
                                                    className={cn(
                                                        "relative h-7 rounded-md text-sm",
                                                        active
                                                            ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10 hover:bg-card"
                                                            : "text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                                                    )}
                                                    aria-current={
                                                        active
                                                            ? "location"
                                                            : undefined
                                                    }
                                                    onClick={() => {
                                                        setActiveSection(
                                                            section.id,
                                                        )
                                                        scrollToProductSection(
                                                            section.id,
                                                        )
                                                    }}
                                                >
                                                    {section.label}
                                                </Button>
                                            )
                                        })}
                                    </nav>
                                </div>
                            </header>

                            <div className="flex min-w-0 flex-col gap-4">
                                {!isCreate && !canRevise ? (
                                    <Alert variant="info">
                                        <AlertTitle>你只能查看</AlertTitle>
                                        <AlertDescription>
                                            {reviseBlocker
                                                ? masterDataCopy.centerUpdateBlocked(
                                                      reviseBlocker.message,
                                                  )
                                                : "当前账号没有维护商品资料的权限；需要修改请联系有权限的同事。"}
                                        </AlertDescription>
                                    </Alert>
                                ) : null}

                                {result?.outcome === "blocked" ? (
                                    <FormalActionResult
                                        status="blocked"
                                        title={
                                            isCreate
                                                ? masterDataCopy.createBlockedTitle
                                                : masterDataCopy.reviseBlockedTitle
                                        }
                                        description={result.message}
                                        facts={
                                            result.detail
                                                ? [
                                                      {
                                                          label: "说明",
                                                          value: result.detail,
                                                      },
                                                  ]
                                                : undefined
                                        }
                                    />
                                ) : null}

                                {result?.outcome === "conflict" ? (
                                    <FormalActionResult
                                        status="blocked"
                                        title={
                                            masterDataCopy.reviseConflictTitle
                                        }
                                        description={
                                            result.message ||
                                            masterDataCopy.reviseConflictHint
                                        }
                                    />
                                ) : null}

                                {formError ? (
                                    <div ref={errorRef}>
                                        <Alert variant="destructive">
                                            <CircleAlertIcon aria-hidden />
                                            <AlertTitle>
                                                {formErrorTitle}
                                            </AlertTitle>
                                            <AlertDescription>
                                                {formError}
                                            </AlertDescription>
                                        </Alert>
                                    </div>
                                ) : null}

                                {checkPassed &&
                                checkedSnapshotRef.current ===
                                    JSON.stringify({
                                        ...values,
                                        fields,
                                    }) ? (
                                    <Alert variant="success">
                                        <CheckCircle2Icon aria-hidden />
                                        <AlertTitle>填写检查通过</AlertTitle>
                                        <AlertDescription>
                                            必填项完整，保存时仍以系统校验结果为准。
                                        </AlertDescription>
                                    </Alert>
                                ) : null}

                                <div
                                    className={cn(
                                        surfacePanelClassName,
                                        "overflow-hidden",
                                    )}
                                >
                                    <ProductBasicSection
                                        isCreate={isCreate}
                                        canRevise={canRevise}
                                        name={name}
                                        setName={setName}
                                        fields={fields}
                                        setFields={setFields}
                                        unitOptions={unitOptionsQuery.data}
                                        categoryOptions={categoryOptions}
                                        brandOptions={brandOptions}
                                        categoryLoading={
                                            categoryListQuery.isPending
                                        }
                                        brandLoading={brandListQuery.isPending}
                                    />

                                    <ProductMediaSection
                                        canRevise={canRevise}
                                        fields={fields}
                                        setFields={setFields}
                                        rememberPendingFiles={
                                            rememberPendingFiles
                                        }
                                    />
                                    <ProductSkuSection
                                        isCreate={isCreate}
                                        canRevise={canRevise}
                                        name={name}
                                        fields={fields}
                                        specDrafts={specDrafts}
                                        activeSpecs={activeSpecs}
                                        inventoryPreviewSkus={
                                            inventoryPreviewSkus
                                        }
                                        syncSpecDrafts={syncSpecDrafts}
                                        updateSku={updateSku}
                                        batchSalePrice={values.batchSalePrice}
                                        batchMarketPrice={
                                            values.batchMarketPrice
                                        }
                                        setBatchSalePrice={(next) =>
                                            form.setFieldValue(
                                                "batchSalePrice",
                                                next,
                                            )
                                        }
                                        setBatchMarketPrice={(next) =>
                                            form.setFieldValue(
                                                "batchMarketPrice",
                                                next,
                                            )
                                        }
                                        onApplyBatchReferencePrices={
                                            applyBatchReferencePrices
                                        }
                                        inventoryActionHint={
                                            inventoryActionHint
                                        }
                                        onOpenInventory={openInventoryPreview}
                                        rememberSkuFile={rememberSkuFile}
                                        supplierCounts={
                                            supplierCountsQuery.data
                                        }
                                        supplierCountsPending={
                                            supplierCountsQuery.isPending
                                        }
                                        supplierCountsError={
                                            supplierCountsQuery.isError
                                                ? supplierCountsQuery.error
                                                : null
                                        }
                                        onRegisterSupply={setSupplierDialogSku}
                                        stableId={stableId}
                                    />

                                    <ProductEffectiveSection
                                        isCreate={isCreate}
                                        canRevise={canRevise}
                                        effectiveFrom={effectiveFrom}
                                        effectiveTo={effectiveTo}
                                        changeReason={changeReason}
                                        setEffectiveFrom={setEffectiveFrom}
                                        setEffectiveTo={setEffectiveTo}
                                        setChangeReason={setChangeReason}
                                    />

                                    {!isCreate ? (
                                        <ProductHistorySection data={data} />
                                    ) : null}
                                </div>
                            </div>
                        </form>

                        {!isCreate && data ? (
                            <ProductDisableDialog
                                open={disableOpen}
                                onOpenChange={setDisableOpen}
                                target={data}
                            />
                        ) : null}
                        <RegisterSupplyForSkuDialog
                            key={supplierDialogSku?.skuId ?? "register-supply"}
                            open={Boolean(supplierDialogSku)}
                            onOpenChange={(open) => {
                                if (!open) setSupplierDialogSku(undefined)
                            }}
                            fixedSku={supplierDialogSku}
                        />
                        <ProductInventoryPreviewSheet
                            open={inventoryOpen}
                            onOpenChange={handleInventoryOpenChange}
                            productName={title}
                            productKind={fields.productKind}
                            skus={inventoryPreviewSkus}
                            initialSkuId={inventoryInitialSkuId}
                        />
                        <DiscardConfirmDialog
                            open={discardOpen}
                            onOpenChange={setDiscardOpen}
                            title="放弃未保存的更改？"
                            description="本次修改尚未保存，离开后将丢失。"
                            confirmLabel="放弃更改"
                            cancelLabel="继续编辑"
                            onConfirm={() => {
                                setDiscardOpen(false)
                                if (pendingNav) {
                                    setPendingNav(null)
                                    router.push(pendingNav)
                                }
                            }}
                        />
                    </PageScaffold>
                )
            }}
        </form.Subscribe>
    )
}
