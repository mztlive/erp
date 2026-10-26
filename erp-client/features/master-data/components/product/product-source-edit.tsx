"use client"

import { ArrowDownIcon, ArrowUpIcon, XIcon } from "lucide-react"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type {
    ProductSourceDecision,
    ProductSourceDifference,
    ProductSourceImage,
    ProductSourceValue,
} from "../../api/types-product-source"
import { sourceImageKey, SourceImagePreview } from "./product-source-values"

export function SourceFieldEdit({
    difference,
    disabled,
    onConfirm,
    onCancel,
    prefix,
}: {
    difference: ProductSourceDifference
    disabled: boolean
    onConfirm: (decision: ProductSourceDecision) => Promise<void>
    onCancel: () => void
    prefix: string
}) {
    const availableImages = new Map<string, ProductSourceImage>()
    for (const value of [difference.company, difference.supplier]) {
        if (value.type !== "IMAGES") continue
        for (const image of value.value) {
            if (image.file_asset_id)
                availableImages.set(sourceImageKey(image), image)
        }
    }
    const form = useAppForm({
        defaultValues: { value: difference.company as ProductSourceValue },
        validators: {
            onSubmit: ({ value }) => {
                if (
                    (difference.field === "NAME" ||
                        difference.field === "SKU_NAME") &&
                    value.value.type === "TEXT" &&
                    !value.value.value?.trim()
                )
                    return "商品名称不能为空"
                if (
                    value.value.type === "IMAGES" &&
                    value.value.value.some((image) => !image.file_asset_id)
                )
                    return "部分图片尚未登记，请移除后重新选择已登记图片"
                if (
                    difference.field === "MAIN_IMAGE" &&
                    value.value.type === "IMAGES" &&
                    value.value.value.length > 1
                )
                    return "SKU 主图只能保留一张"
                return undefined
            },
        },
        onSubmit: async ({ value }) =>
            onConfirm({
                field: difference.field,
                ...(difference.sku_id ? { sku_id: difference.sku_id } : {}),
                action: "EDIT",
                value: value.value,
            }),
    })
    return (
        <div className="space-y-3 rounded-lg border bg-muted/20 p-4">
            <p className="text-sm font-medium">
                编辑后确认 · {difference.label}
            </p>
            <form.Field name="value">
                {(field) => {
                    const value = field.state.value
                    const setImages = (images: readonly ProductSourceImage[]) =>
                        field.handleChange({ type: "IMAGES", value: images })
                    return (
                        <div className="space-y-3">
                            {value.type === "TEXT" ? (
                                difference.field === "NAME" ||
                                difference.field === "SKU_NAME" ? (
                                    <Input
                                        id={`${prefix}-edit-value`}
                                        aria-label={
                                            difference.field === "SKU_NAME"
                                                ? "编辑规格商品名称"
                                                : "编辑商品名称"
                                        }
                                        value={value.value ?? ""}
                                        onChange={(event) =>
                                            field.handleChange({
                                                type: "TEXT",
                                                value: event.target.value,
                                            })
                                        }
                                        disabled={disabled}
                                    />
                                ) : (
                                    <Textarea
                                        id={`${prefix}-edit-value`}
                                        aria-label="编辑商品介绍"
                                        value={value.value ?? ""}
                                        onChange={(event) =>
                                            field.handleChange({
                                                type: "TEXT",
                                                value: event.target.value,
                                            })
                                        }
                                        disabled={disabled}
                                    />
                                )
                            ) : (
                                <>
                                    <p className="text-xs text-muted-foreground">
                                        组合已登记的公司与供应商图片并调整顺序。新增图片请先在商品素材中保存，再重新查看差异。
                                    </p>
                                    <div className="space-y-2">
                                        {value.value.map((image, index) => {
                                            const imagePrefix = `${prefix}-edit-image-${toAutomationIdSegment(sourceImageKey(image))}`
                                            const move = (offset: number) => {
                                                const images = [...value.value]
                                                const target = index + offset
                                                const selected = images.splice(
                                                    index,
                                                    1,
                                                )[0]
                                                images.splice(
                                                    target,
                                                    0,
                                                    selected,
                                                )
                                                setImages(images)
                                            }
                                            return (
                                                <div
                                                    key={sourceImageKey(image)}
                                                    className="flex items-center gap-2 rounded-md border bg-background p-2"
                                                >
                                                    <SourceImagePreview
                                                        image={image}
                                                        alt={`已选第 ${index + 1} 张图片`}
                                                        thumbnail
                                                    />
                                                    <span className="flex-1 text-sm">
                                                        第 {index + 1} 张
                                                        {!image.file_asset_id
                                                            ? " · 尚未登记"
                                                            : ""}
                                                    </span>
                                                    <Button
                                                        id={`${imagePrefix}-up`}
                                                        type="button"
                                                        variant="ghost"
                                                        size="icon-sm"
                                                        aria-label={`第 ${index + 1} 张图片上移`}
                                                        disabled={
                                                            disabled ||
                                                            index === 0
                                                        }
                                                        onClick={() => move(-1)}
                                                    >
                                                        <ArrowUpIcon />
                                                    </Button>
                                                    <Button
                                                        id={`${imagePrefix}-down`}
                                                        type="button"
                                                        variant="ghost"
                                                        size="icon-sm"
                                                        aria-label={`第 ${index + 1} 张图片下移`}
                                                        disabled={
                                                            disabled ||
                                                            index ===
                                                                value.value
                                                                    .length -
                                                                    1
                                                        }
                                                        onClick={() => move(1)}
                                                    >
                                                        <ArrowDownIcon />
                                                    </Button>
                                                    <Button
                                                        id={`${imagePrefix}-remove`}
                                                        type="button"
                                                        variant="ghost"
                                                        size="icon-sm"
                                                        aria-label={`移除第 ${index + 1} 张图片`}
                                                        disabled={disabled}
                                                        onClick={() =>
                                                            setImages(
                                                                value.value.filter(
                                                                    (
                                                                        _,
                                                                        position,
                                                                    ) =>
                                                                        position !==
                                                                        index,
                                                                ),
                                                            )
                                                        }
                                                    >
                                                        <XIcon />
                                                    </Button>
                                                </div>
                                            )
                                        })}
                                    </div>
                                    <div className="flex flex-wrap gap-2">
                                        {[...availableImages]
                                            .filter(
                                                ([key]) =>
                                                    !value.value.some(
                                                        (image) =>
                                                            sourceImageKey(
                                                                image,
                                                            ) === key,
                                                    ),
                                            )
                                            .map(([key, image], index) => (
                                                <Button
                                                    id={`${prefix}-add-image-${toAutomationIdSegment(key)}`}
                                                    key={key}
                                                    type="button"
                                                    variant="outline"
                                                    size="sm"
                                                    className="h-auto gap-2 py-2"
                                                    disabled={disabled}
                                                    onClick={() =>
                                                        setImages([
                                                            ...(difference.field ===
                                                            "MAIN_IMAGE"
                                                                ? []
                                                                : value.value),
                                                            image,
                                                        ])
                                                    }
                                                >
                                                    <SourceImagePreview
                                                        image={image}
                                                        alt={`候选图片 ${index + 1}`}
                                                        thumbnail
                                                    />
                                                    添加候选图片 {index + 1}
                                                </Button>
                                            ))}
                                    </div>
                                </>
                            )}
                            {field.state.meta.errors.map((error, index) => (
                                <p
                                    key={`${prefix}-error-${index}`}
                                    role="alert"
                                    className="text-sm text-destructive"
                                >
                                    {String(error)}
                                </p>
                            ))}
                        </div>
                    )
                }}
            </form.Field>
            <form.Subscribe selector={(state) => state.errors}>
                {(errors) =>
                    errors.length ? (
                        <p role="alert" className="text-sm text-destructive">
                            {errors.map(String).join("；")}
                        </p>
                    ) : null
                }
            </form.Subscribe>
            <div className="flex justify-end gap-2">
                <Button
                    id={`${prefix}-edit-cancel`}
                    type="button"
                    variant="outline"
                    disabled={disabled}
                    onClick={onCancel}
                >
                    取消编辑
                </Button>
                <Button
                    id={`${prefix}-edit-confirm`}
                    type="button"
                    disabled={disabled}
                    onClick={() => void form.handleSubmit()}
                >
                    保存编辑并确认
                </Button>
            </div>
        </div>
    )
}
