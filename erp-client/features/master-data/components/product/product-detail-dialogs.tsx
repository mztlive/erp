"use client"

import { useRouter } from "next/navigation"

import { DiscardConfirmDialog } from "@/components/business"
import { ProductDisableDialog } from "@/features/master-data/components/shared/disable-action-dialog"
import {
    ProductInventoryPreviewSheet,
    type ProductInventoryPreviewSku,
} from "@/features/master-data/components/product/product-inventory-preview-sheet"
import { RegisterSupplyForSkuDialog } from "@/features/supplier-offerings/offering-dialogs"
import type {
    MasterDataCenterView,
    ProductKind,
} from "@/features/master-data/types"
import type { FixedSku } from "@/features/supplier-offerings/types"

type ProductDetailDialogsProps = {
    isCreate: boolean
    data: MasterDataCenterView | null | undefined
    disableOpen: boolean
    setDisableOpen: (open: boolean) => void
    supplierDialogSku: FixedSku | undefined
    setSupplierDialogSku: (sku: FixedSku | undefined) => void
    inventoryOpen: boolean
    onInventoryOpenChange: (open: boolean) => void
    productName: string
    productKind: ProductKind | ""
    inventoryPreviewSkus: readonly ProductInventoryPreviewSku[]
    inventoryInitialSkuId: string | undefined
    discardOpen: boolean
    setDiscardOpen: (open: boolean) => void
    pendingNav: string | null
    setPendingNav: (href: string | null) => void
    router: ReturnType<typeof useRouter>
}

function ProductDetailDialogs({
    isCreate,
    data,
    disableOpen,
    setDisableOpen,
    supplierDialogSku,
    setSupplierDialogSku,
    inventoryOpen,
    onInventoryOpenChange,
    productName,
    productKind,
    inventoryPreviewSkus,
    inventoryInitialSkuId,
    discardOpen,
    setDiscardOpen,
    pendingNav,
    setPendingNav,
    router,
}: ProductDetailDialogsProps) {
    return (
        <>
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
                onOpenChange={onInventoryOpenChange}
                productName={productName}
                productKind={productKind}
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
        </>
    )
}

export { ProductDetailDialogs }
