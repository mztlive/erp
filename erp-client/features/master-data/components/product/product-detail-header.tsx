"use client"

import { BanIcon, ClipboardCheckIcon, SaveIcon } from "lucide-react"

import { surfacePanelClassName } from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    PRODUCT_EDITOR_SECTIONS,
    scrollToProductSection,
} from "@/features/master-data/lib/product-editor-model"
import type {
    ProductEditorFormValues,
    ProductEditorSectionId,
} from "@/features/master-data/lib/product-editor-model"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { formatEffectiveRange } from "@/features/master-data/lib/filter"
import type { MasterDataCenterView } from "@/features/master-data/types"
import { cn } from "@/lib/utils"

type ProductDetailHeaderProps = {
    isCreate: boolean
    data: MasterDataCenterView | null | undefined
    title: string
    hasUpdatePermission: boolean
    canDisable: boolean
    disableBlocker: { message: string } | undefined
    setDisableOpen: (open: boolean) => void
    canRevise: boolean
    pending: boolean
    runLocalCheck: (values: ProductEditorFormValues) => void
    values: ProductEditorFormValues
    onBack: () => void
    activeSection: ProductEditorSectionId
    setActiveSection: (section: ProductEditorSectionId) => void
    stickyHeaderRef: React.Ref<HTMLElement>
}

function ProductDetailHeader({
    isCreate,
    data,
    title,
    hasUpdatePermission,
    canDisable,
    disableBlocker,
    setDisableOpen,
    canRevise,
    pending,
    runLocalCheck,
    values,
    onBack,
    activeSection,
    setActiveSection,
    stickyHeaderRef,
}: ProductDetailHeaderProps) {
    return (
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
                                label={data.lifecycleStatusLabel}
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
                                版本 {data.currentRevision.revisionNo}
                            </span>
                            <span className="num">
                                {formatEffectiveRange(
                                    data.currentRevision.effectiveFrom,
                                    data.currentRevision.effectiveTo,
                                )}
                            </span>
                            <span className="inline-flex items-center gap-1.5">
                                <span>
                                    {masterDataCopy.centerVersionState}
                                </span>
                                <StatusBadge
                                    tone={
                                        data.revisionTiming === "FUTURE"
                                            ? "warning"
                                            : "info"
                                    }
                                    label={data.revisionTimingLabel}
                                />
                            </span>
                        </div>
                    ) : (
                        <p className="text-sm text-muted-foreground">
                            {masterDataCopy.productCreateDesc}
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
                            onClick={() => setDisableOpen(true)}
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
                        onClick={onBack}
                    >
                        返回列表
                    </Button>
                    <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={!canRevise || pending}
                        onClick={() => runLocalCheck(values)}
                    >
                        <ClipboardCheckIcon
                            data-icon="inline-start"
                            aria-hidden
                        />
                        填写检查
                    </Button>
                    <Button type="submit" size="sm" disabled={!canRevise || pending}>
                        <SaveIcon data-icon="inline-start" aria-hidden />
                        {isCreate
                            ? masterDataCopy.createSubmit
                            : masterDataCopy.reviseSubmit}
                    </Button>
                </div>
            </div>

            {!isCreate && data?.productConstraints ? (
                <div className="border-t border-grid bg-muted/40 px-4 py-2.5 text-xs">
                    <p>
                        基础单位{" "}
                        <span className="num">
                            {data.productConstraints.baseUnit}
                        </span>
                        {" · "}
                        SKU{" "}
                        <span className="num">
                            {data.productConstraints.skuCount}
                        </span>{" "}
                        个
                        {data.productConstraints.hasFormalReferences
                            ? " · 已被业务单据引用"
                            : null}
                    </p>
                    <p className="mt-1 text-muted-foreground">
                        {masterDataCopy.centerSpecNote}
                    </p>
                </div>
            ) : null}

            <div className="border-t border-grid p-2 sm:px-3 sm:pb-3">
                <nav
                    aria-label="商品编辑分区"
                    className={cn(
                        "grid grid-cols-2 gap-0.5 rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10",
                        isCreate ? "sm:grid-cols-4" : "sm:grid-cols-5",
                    )}
                >
                    {PRODUCT_EDITOR_SECTIONS.filter(
                        (section) => !isCreate || section.id !== "history",
                    ).map((section) => {
                        const active = activeSection === section.id
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
                                    active ? "location" : undefined
                                }
                                onClick={() => {
                                    setActiveSection(section.id)
                                    scrollToProductSection(section.id)
                                }}
                            >
                                {section.label}
                            </Button>
                        )
                    })}
                </nav>
            </div>
        </header>
    )
}

export { ProductDetailHeader }
