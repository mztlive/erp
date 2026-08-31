"use client"

/**
 * 商品详情页 = 查看 + 编辑（同一页面）。
 * - /master-data/products/new  新建
 * - /master-data/products/:id  查看并直接改，保存即形成新版本
 * 壳层对齐供应商对象中心：PageHeader + DocumentHeader + 摘要条 + line tabs。
 * 中间态在 ProductDetailEntryGate，值绑定在 createProductFormBindings。
 */

import { ArrowLeftIcon } from "lucide-react"

import {
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    ProductBasicSection,
    ProductEffectiveSection,
    ProductHistorySection,
    ProductMediaSection,
} from "@/features/master-data/components/product/product-editor-sections"
import { ProductSkuSection } from "@/features/master-data/components/product/product-sku-section"
import { ProductDetailDialogs } from "@/features/master-data/components/product/product-detail-dialogs"
import { ProductDetailEntryGate } from "@/features/master-data/components/product/product-detail-entry-gate"
import { ProductDetailFeedback } from "@/features/master-data/components/product/product-detail-feedback"
import { ProductDetailHeader } from "@/features/master-data/components/product/product-detail-header"
import {
    ProductSectionTabs,
    ProductSummaryStrip,
} from "@/features/master-data/components/product/product-detail-navigation"
import { createProductFormBindings } from "@/features/master-data/lib/product-form-bindings"
import { useProductEditor } from "@/features/master-data/hooks/use-product-editor"
import { productKindLabel } from "@/features/master-data/api/presentation"
import { masterDataCopy } from "@/features/master-data/lib/copy"
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
        rememberPendingFiles,
        rememberSkuFile,
        navigateAway,
        openInventoryPreview,
        handleInventoryOpenChange,
        listHref,
        pending,
        canCreate,
        hasUpdatePermission,
        canRevise,
        canDisable,
        reviseBlocker,
        disableBlocker,
        runLocalCheck,
    } = editor

    const formId = "product-detail-form"

    return (
        <ProductDetailEntryGate
            isCreate={isCreate}
            hasDetailData={Boolean(data)}
            detailQuery={detailQuery}
            accountQuery={accountQuery}
            canCreate={canCreate}
            listHref={listHref}
        >
            <form.Subscribe selector={(state) => state.values}>
                {(values) => {
                    const bindings = createProductFormBindings(
                        form,
                        values,
                        isCreate,
                        data?.name,
                    )
                    const {
                        title,
                        fields,
                        inventoryPreviewSkus,
                        inventoryActionHint,
                        setName,
                        setEffectiveFrom,
                        setEffectiveTo,
                        setChangeReason,
                        setFields,
                        syncSpecDrafts,
                        updateSku,
                        handleSubmit,
                        name,
                        effectiveFrom,
                        effectiveTo,
                        changeReason,
                        specDrafts,
                        activeSpecs,
                        applyBatchReferencePrices,
                    } = bindings
                    return (
                        <PageScaffold density="compact">
                            <PageHeader
                                variant="object-chrome"
                                actions={
                                    <Button
                                        id="master-data-product-detail-back-list"
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        onClick={() => navigateAway(listHref)}
                                    >
                                        <ArrowLeftIcon
                                            data-icon="inline-start"
                                            aria-hidden
                                        />
                                        返回列表
                                    </Button>
                                }
                            />

                            <form
                                id={formId}
                                className="space-y-4"
                                onSubmit={handleSubmit}
                            >
                                <ProductDetailHeader
                                    idPrefix="master-data-product-detail-header"
                                    isCreate={isCreate}
                                    data={data}
                                    title={title}
                                    hasUpdatePermission={hasUpdatePermission}
                                    canDisable={canDisable}
                                    disableBlocker={disableBlocker}
                                    setDisableOpen={setDisableOpen}
                                    canRevise={canRevise}
                                    pending={pending}
                                    runLocalCheck={runLocalCheck}
                                    values={values}
                                />

                                <div className="space-y-3">
                                    <ProductDetailFeedback
                                        isCreate={isCreate}
                                        canRevise={canRevise}
                                        reviseBlocker={reviseBlocker}
                                        result={result}
                                        formError={formError}
                                        formErrorTitle={formErrorTitle}
                                        checkPassed={checkPassed}
                                        checkedSnapshotRef={checkedSnapshotRef}
                                        values={values}
                                        fields={fields}
                                        errorRef={errorRef}
                                    />

                                    <ProductSummaryStrip
                                        rows={[
                                            {
                                                label: masterDataCopy.fBaseUnit,
                                                value:
                                                    fields.baseUnit || "待选择",
                                            },
                                            {
                                                label: masterDataCopy.fSkuCount,
                                                value: `${fields.skus.length} 个`,
                                            },
                                            {
                                                label: "商品类型",
                                                value:
                                                    productKindLabel(
                                                        fields.productKind,
                                                    ) || "待选择",
                                            },
                                            {
                                                label: "引用",
                                                value: isCreate
                                                    ? "新建未引用"
                                                    : data?.productConstraints
                                                            ?.hasFormalReferences
                                                      ? "已被业务单据引用"
                                                      : "尚未被引用",
                                            },
                                        ]}
                                    />

                                    <div
                                        className={cn(
                                            surfacePanelClassName,
                                            "overflow-hidden",
                                        )}
                                    >
                                        <ProductSectionTabs
                                            value={activeSection}
                                            isCreate={isCreate}
                                            onValueChange={setActiveSection}
                                        />

                                        <div className="p-4 md:p-5">
                                            {activeSection === "basic" ? (
                                                <ProductBasicSection
                                                    idPrefix="master-data-product-detail-basic"
                                                    isCreate={isCreate}
                                                    canRevise={canRevise}
                                                    name={name}
                                                    setName={setName}
                                                    fields={fields}
                                                    setFields={setFields}
                                                    unitOptions={
                                                        unitOptionsQuery.data
                                                    }
                                                    categoryOptions={
                                                        categoryOptions
                                                    }
                                                    brandOptions={brandOptions}
                                                    categoryLoading={
                                                        categoryListQuery.isPending
                                                    }
                                                    brandLoading={
                                                        brandListQuery.isPending
                                                    }
                                                />
                                            ) : null}

                                            {activeSection === "media" ? (
                                                <ProductMediaSection
                                                    idPrefix="master-data-product-detail-media"
                                                    canRevise={canRevise}
                                                    fields={fields}
                                                    setFields={setFields}
                                                    rememberPendingFiles={
                                                        rememberPendingFiles
                                                    }
                                                />
                                            ) : null}

                                            {activeSection === "sku" ? (
                                                <ProductSkuSection
                                                    idPrefix="master-data-product-detail-sku"
                                                    isCreate={isCreate}
                                                    canRevise={canRevise}
                                                    name={name}
                                                    fields={fields}
                                                    specDrafts={specDrafts}
                                                    activeSpecs={activeSpecs}
                                                    inventoryPreviewSkus={
                                                        inventoryPreviewSkus
                                                    }
                                                    syncSpecDrafts={
                                                        syncSpecDrafts
                                                    }
                                                    updateSku={updateSku}
                                                    batchSalePrice={
                                                        values.batchSalePrice
                                                    }
                                                    batchMarketPrice={
                                                        values.batchMarketPrice
                                                    }
                                                    setBatchSalePrice={(next) =>
                                                        form.setFieldValue(
                                                            "batchSalePrice",
                                                            next,
                                                        )
                                                    }
                                                    setBatchMarketPrice={(
                                                        next,
                                                    ) =>
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
                                                    onOpenInventory={
                                                        openInventoryPreview
                                                    }
                                                    rememberSkuFile={
                                                        rememberSkuFile
                                                    }
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
                                                    onRegisterSupply={
                                                        setSupplierDialogSku
                                                    }
                                                    stableId={stableId}
                                                />
                                            ) : null}

                                            {activeSection === "effective" ? (
                                                <ProductEffectiveSection
                                                    idPrefix="master-data-product-detail-effective"
                                                    isCreate={isCreate}
                                                    canRevise={canRevise}
                                                    effectiveFrom={
                                                        effectiveFrom
                                                    }
                                                    effectiveTo={effectiveTo}
                                                    changeReason={changeReason}
                                                    setEffectiveFrom={
                                                        setEffectiveFrom
                                                    }
                                                    setEffectiveTo={
                                                        setEffectiveTo
                                                    }
                                                    setChangeReason={
                                                        setChangeReason
                                                    }
                                                />
                                            ) : null}

                                            {activeSection === "history" &&
                                            !isCreate ? (
                                                <ProductHistorySection
                                                    data={data}
                                                />
                                            ) : null}
                                        </div>
                                    </div>
                                </div>
                            </form>

                            <ProductDetailDialogs
                                isCreate={isCreate}
                                data={data}
                                disableOpen={disableOpen}
                                setDisableOpen={setDisableOpen}
                                supplierDialogSku={supplierDialogSku}
                                setSupplierDialogSku={setSupplierDialogSku}
                                inventoryOpen={inventoryOpen}
                                onInventoryOpenChange={
                                    handleInventoryOpenChange
                                }
                                productName={title}
                                productKind={fields.productKind}
                                inventoryPreviewSkus={inventoryPreviewSkus}
                                inventoryInitialSkuId={inventoryInitialSkuId}
                                discardOpen={discardOpen}
                                setDiscardOpen={setDiscardOpen}
                                pendingNav={pendingNav}
                                setPendingNav={setPendingNav}
                                router={router}
                            />
                        </PageScaffold>
                    )
                }}
            </form.Subscribe>
        </ProductDetailEntryGate>
    )
}
