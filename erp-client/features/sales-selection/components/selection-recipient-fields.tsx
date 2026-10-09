"use client"

import { useState } from "react"
import { type SelectionRecipient } from "../types"
import { useAppForm } from "@/components/form"
import { z } from "zod"

export const recipientSchema = z.object({
    name: z.string().trim().min(1, "请填写收件人").max(64, "收件人最多 64 字"),
    phone: z
        .string()
        .trim()
        .regex(/^\+?[0-9]{7,20}$/, "联系电话须为 7 到 20 位数字，可带前导加号"),
    province: z.string().trim().min(1, "请填写省份").max(64),
    city: z.string().trim().min(1, "请填写城市").max(64),
    district: z.string().trim().min(1, "请填写区县").max(64),
    address: z
        .string()
        .trim()
        .min(1, "请填写详细地址")
        .max(256, "详细地址最多 256 字"),
})

export function useSelectionRecipientForm(
    onSubmit: (recipient: SelectionRecipient) => void,
    initial?: SelectionRecipient,
) {
    const [defaultValues] = useState(
        () =>
            initial ?? {
                name: "",
                phone: "",
                province: "",
                city: "",
                district: "",
                address: "",
            },
    )
    return useAppForm({
        defaultValues,
        validators: { onSubmit: recipientSchema },
        onSubmit: ({ value }) => {
            onSubmit(recipientSchema.parse(value))
        },
    })
}

export function SelectionRecipientFields({
    form,
    disabled = false,
}: {
    disabled?: boolean
    form: ReturnType<typeof useSelectionRecipientForm>
}) {
    return (
        <section
            className="space-y-3 rounded-2xl border bg-card p-4"
            aria-labelledby="sales-selection-public-recipient-title"
        >
            <h2
                id="sales-selection-public-recipient-title"
                className="font-semibold"
            >
                收件信息
            </h2>
            <p className="text-xs text-muted-foreground">
                提交后将按您确认的商品和收件地址安排后续处理，请核对后提交。
            </p>
            <div className="grid grid-cols-2 gap-3">
                <form.AppField
                    name="name"
                    children={(field) => (
                        <field.TextField
                            id="sales-selection-public-recipient-name"
                            label="收件人"
                            autoComplete="name"
                            required
                            disabled={disabled}
                        />
                    )}
                />
                <form.AppField
                    name="phone"
                    children={(field) => (
                        <field.TextField
                            id="sales-selection-public-recipient-phone"
                            label="联系电话"
                            autoComplete="tel"
                            required
                            disabled={disabled}
                        />
                    )}
                />
                <form.AppField
                    name="province"
                    children={(field) => (
                        <field.TextField
                            id="sales-selection-public-recipient-province"
                            label="省份"
                            autoComplete="address-level1"
                            required
                            disabled={disabled}
                        />
                    )}
                />
                <form.AppField
                    name="city"
                    children={(field) => (
                        <field.TextField
                            id="sales-selection-public-recipient-city"
                            label="城市"
                            autoComplete="address-level2"
                            required
                            disabled={disabled}
                        />
                    )}
                />
                <form.AppField
                    name="district"
                    children={(field) => (
                        <field.TextField
                            id="sales-selection-public-recipient-district"
                            label="区县"
                            autoComplete="address-level3"
                            required
                            disabled={disabled}
                        />
                    )}
                />
                <form.AppField
                    name="address"
                    children={(field) => (
                        <field.TextField
                            id="sales-selection-public-recipient-address"
                            label="详细地址"
                            autoComplete="street-address"
                            required
                            disabled={disabled}
                        />
                    )}
                />
            </div>
        </section>
    )
}
