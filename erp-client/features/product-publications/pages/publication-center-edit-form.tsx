"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { DocumentSection, OptionCombobox } from "@/components/business"
import type { SessionEdit } from "@/features/product-publications/lib/publish-form"
import {
    MEDIA_ROLE_LABEL,
    SALE_STATUS_LABEL,
} from "@/features/product-publications/types"
import type {
    ProductPublicationView,
    SaleStatus,
} from "@/features/product-publications/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

import type { PublicationCenterFormApi } from "./publication-center-session"

export function PublicationCenterEditForm({
    data,
    form,
    sessionEdit,
    publishBlocked,
    publishBlocker,
    onDiscard,
}: {
    data: ProductPublicationView
    form: PublicationCenterFormApi
    sessionEdit: SessionEdit
    publishBlocked: boolean
    publishBlocker: { message: string } | undefined
    onDiscard: () => void
}) {
    return (
        <DocumentSection
            id="pub-section-content"
            title="发布内容"
            description="选中修订的完整商城内容记录"
        >
            <form
                className="space-y-3"
                onSubmit={(e) => {
                    e.preventDefault()
                    void form.handleSubmit()
                }}
            >
                <Alert variant="info">
                    <AlertTitle>基于历史/当前版本的本次编辑</AlertTitle>
                    <AlertDescription>
                        基于 r{data.selectedRevision.revisionNo}{" "}
                        版本开始编辑。最小购买量需运营确认填写，不随供应商起订量带入；销售价与供货价分开填写。
                    </AlertDescription>
                </Alert>
                <form.AppField name="name">
                    {(field) => (
                        <field.TextField
                            id="publication-center-edit-name"
                            label="展示名称"
                            required
                        />
                    )}
                </form.AppField>
                <form.AppField name="specification">
                    {(field) => (
                        <field.TextField
                            id="publication-center-edit-specification"
                            label="规格"
                            required
                        />
                    )}
                </form.AppField>
                <form.AppField name="salesDescription">
                    {(field) => (
                        <field.TextareaField
                            id="publication-center-edit-sales-description"
                            label="商城销售说明"
                            rows={3}
                            required
                        />
                    )}
                </form.AppField>
                <div className="grid gap-3 sm:grid-cols-2">
                    <form.AppField name="salesPriceGross">
                        {(field) => (
                            <field.TextField
                                id="publication-center-edit-sales-price-gross"
                                label="含税销售价"
                                required
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="salesTaxRate">
                        {(field) => (
                            <field.TextField
                                id="publication-center-edit-sales-tax-rate"
                                label="销项税率"
                                required
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="minimumPurchaseQuantity">
                        {(field) => (
                            <field.TextField
                                id="publication-center-edit-minimum-purchase-quantity"
                                label="最小购买量（运营确认）"
                                required
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="saleStatus">
                        {(field) => (
                            <div className="space-y-1.5">
                                <Label htmlFor="publication-center-edit-sale-status">
                                    商城销售状态
                                    <span className="text-destructive">*</span>
                                </Label>
                                <OptionCombobox
                                    id="publication-center-edit-sale-status"
                                    value={field.state.value}
                                    onValueChange={(v) =>
                                        field.handleChange(
                                            (v ??
                                                field.state
                                                    .value) as SaleStatus,
                                        )
                                    }
                                    options={[
                                        {
                                            value: "ON_SALE",
                                            label: SALE_STATUS_LABEL.ON_SALE,
                                        },
                                        {
                                            value: "OFF_SALE",
                                            label: SALE_STATUS_LABEL.OFF_SALE,
                                        },
                                        {
                                            value: "PAUSED",
                                            label: SALE_STATUS_LABEL.PAUSED,
                                        },
                                    ]}
                                    className="w-full"
                                    allowClear={false}
                                    aria-label="商城销售状态"
                                    placeholder="商城销售状态"
                                />
                                {data.status === "SAFETY_PAUSED" &&
                                field.state.value === "ON_SALE" ? (
                                    <p className="text-xs text-destructive">
                                        安全暂停中的对象提交上架会被系统阻断。
                                    </p>
                                ) : null}
                            </div>
                        )}
                    </form.AppField>
                </div>
                <div className="grid gap-3 sm:grid-cols-2">
                    <form.AppField name="skuRevisionId">
                        {(field) => (
                            <field.TextField
                                id="publication-center-edit-sku-revision-id"
                                label="SKU 修订编号"
                                required
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="categoryId">
                        {(field) => (
                            <field.TextField
                                id="publication-center-edit-category-id"
                                label="商城类目编号"
                                required
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="supplierOfferingRevisionId">
                        {(field) => (
                            <field.TextField
                                id="publication-center-edit-supplier-offering-revision-id"
                                label="唯一固定供给修订编号"
                                required
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="baseUnitCode">
                        {(field) => (
                            <field.TextField
                                id="publication-center-edit-base-unit-code"
                                label="基础单位代码"
                                required
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="salesRegionText">
                        {(field) => (
                            <field.TextField
                                id="publication-center-edit-sales-region-text"
                                label="可销售区域（顿号/逗号分隔）"
                                required
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="productCapabilitiesText">
                        {(field) => (
                            <field.TextField
                                id="publication-center-edit-product-capabilities-text"
                                label="商品能力（顿号/逗号分隔）"
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="validFrom">
                        {(field) => (
                            <field.TextField
                                id="publication-center-edit-valid-from"
                                label="生效时间"
                                required
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="validTo">
                        {(field) => (
                            <field.TextField
                                id="publication-center-edit-valid-to"
                                label="失效时间（可空）"
                            />
                        )}
                    </form.AppField>
                </div>
                <div className="space-y-2 rounded-lg bg-muted/40 p-3">
                    <div className="text-sm font-medium">发布媒体资料</div>
                    {sessionEdit.media.map((media, index) => (
                        <div
                            key={media.fileAssetId}
                            className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_2fr]"
                        >
                            <div className="text-xs text-muted-foreground">
                                {MEDIA_ROLE_LABEL[media.mediaRole]} · 顺序{" "}
                                {media.sortNo}
                            </div>
                            <form.AppField name={`media[${index}].altText`}>
                                {(field) => (
                                    <field.TextField
                                        id={`publication-center-edit-media-${toAutomationIdSegment(media.fileAssetId)}-alt-text`}
                                        label="图片说明"
                                        required
                                    />
                                )}
                            </form.AppField>
                        </div>
                    ))}
                </div>
                <p className="text-xs text-muted-foreground">
                    供应商起订{" "}
                    {data.selectedRevision.fixedOffering.supplierMoq ?? "—"}
                    （只读展示，不复制到商城最小购买量）。供给修订、区域、能力和媒体变化都会形成新发布修订。
                </p>
                <div className="flex flex-wrap gap-2">
                    <form.AppForm>
                        <form.SubmitButton
                            id="publication-center-edit-submit"
                            label="核对并提交发布"
                            disabled={publishBlocked}
                        />
                    </form.AppForm>
                    <Button
                        id="publication-center-edit-discard"
                        type="button"
                        variant="outline"
                        onClick={onDiscard}
                    >
                        放弃
                    </Button>
                    {publishBlocked ? (
                        <span className="text-xs text-destructive">
                            {publishBlocker?.message ??
                                "当前状态不允许提交发布。"}
                        </span>
                    ) : null}
                </div>
            </form>
        </DocumentSection>
    )
}
