"use client"

import * as React from "react"

import {
    MasterDataCreateDialog,
    MasterDataDisableDialog,
    MasterDataReviseDialog,
} from "@/features/master-data/master-data-action-dialog"
import { ProductSupplyDialog } from "@/features/master-data/product-supply-dialog"
import { VoucherCategoryFormDialog } from "@/features/master-data/voucher-category-form-dialog"
import type {
    MasterDataListItem,
    MasterDataResource,
    ProductListSkuSummary,
} from "@/features/master-data/types"
import { RegisterSupplyForSkuDialog } from "@/features/supplier-offerings/offering-dialogs"
import type {
    FixedSku,
    SupplierOfferingView,
} from "@/features/supplier-offerings/types"

type MasterDataListDialogsProps = {
    resource: MasterDataResource
    isProductResource: boolean
    isSupplierResource: boolean
    isVoucherCategoryResource: boolean
    isSellableResource: boolean
    createOpen: boolean
    setCreateOpen: (open: boolean) => void
    reviseTarget: MasterDataListItem | null
    setReviseTarget: (target: MasterDataListItem | null) => void
    disableTarget: MasterDataListItem | null
    setDisableTarget: (target: MasterDataListItem | null) => void
    supplyProduct: MasterDataListItem | null
    setSupplyProduct: (product: MasterDataListItem | null) => void
    supplyDialogSku: FixedSku | null
    setSupplyDialogSku: (sku: FixedSku | null) => void
    productSkusByProduct: Map<string, ProductListSkuSummary[]>
    productSkusPending: boolean
    productSkusError: unknown
    onRetrySkus: () => void
    offerings: readonly SupplierOfferingView[]
    offeringLoading: boolean
    offeringError: unknown
    onRetryOfferings: () => void
    productPageSkuIds: readonly string[]
}

/** 列表页新建 / 修订 / 停用 / 供给登记等弹层集合。 */
export function MasterDataListDialogs({
    resource,
    isProductResource,
    isSupplierResource,
    isVoucherCategoryResource,
    isSellableResource,
    createOpen,
    setCreateOpen,
    reviseTarget,
    setReviseTarget,
    disableTarget,
    setDisableTarget,
    supplyProduct,
    setSupplyProduct,
    supplyDialogSku,
    setSupplyDialogSku,
    productSkusByProduct,
    productSkusPending,
    productSkusError,
    onRetrySkus,
    offerings,
    offeringLoading,
    offeringError,
    onRetryOfferings,
    productPageSkuIds,
}: MasterDataListDialogsProps) {
    return (
        <>
            <ProductSupplyDialog
                product={supplyProduct}
                skus={
                    supplyProduct
                        ? (productSkusByProduct.get(supplyProduct.stableId) ??
                          [])
                        : []
                }
                skuLoading={productSkusPending}
                skuError={productSkusError}
                offerings={offerings}
                offeringLoading={
                    productPageSkuIds.length > 0 && offeringLoading
                }
                offeringError={offeringError}
                onRetrySkus={onRetrySkus}
                onRetryOfferings={onRetryOfferings}
                onAddSupply={(sku) => {
                    if (!supplyProduct) return
                    setSupplyDialogSku({
                        skuId: sku.skuId,
                        skuCode: sku.skuNo,
                        skuName: sku.skuName || supplyProduct.name,
                        specification: sku.specification,
                        baseUnit: sku.baseUnit,
                        productKind: supplyProduct.productKind,
                    })
                }}
                onOpenChange={(open) => {
                    if (!open) setSupplyProduct(null)
                }}
            />

            {supplyDialogSku ? (
                <RegisterSupplyForSkuDialog
                    key={supplyDialogSku.skuId}
                    open
                    fixedSku={supplyDialogSku}
                    onOpenChange={(open) => {
                        if (!open) setSupplyDialogSku(null)
                    }}
                />
            ) : null}

            {!isProductResource &&
            !isSupplierResource &&
            !isVoucherCategoryResource &&
            !isSellableResource ? (
                <MasterDataCreateDialog
                    open={createOpen}
                    onOpenChange={setCreateOpen}
                    resource={resource}
                />
            ) : null}
            {isVoucherCategoryResource ? (
                <>
                    <VoucherCategoryFormDialog
                        open={createOpen}
                        onOpenChange={setCreateOpen}
                    />
                    <VoucherCategoryFormDialog
                        open={reviseTarget != null}
                        onOpenChange={(open) => {
                            if (!open) setReviseTarget(null)
                        }}
                        target={reviseTarget}
                    />
                </>
            ) : null}
            {!isProductResource &&
            !isSupplierResource &&
            !isVoucherCategoryResource &&
            !isSellableResource ? (
                <MasterDataReviseDialog
                    open={reviseTarget != null}
                    onOpenChange={(open) => {
                        if (!open) setReviseTarget(null)
                    }}
                    resource={resource}
                    target={reviseTarget}
                />
            ) : null}
            {!isVoucherCategoryResource && !isSellableResource ? (
                <MasterDataDisableDialog
                    open={disableTarget != null}
                    onOpenChange={(open) => {
                        if (!open) setDisableTarget(null)
                    }}
                    resource={resource}
                    target={disableTarget}
                />
            ) : null}
        </>
    )
}
