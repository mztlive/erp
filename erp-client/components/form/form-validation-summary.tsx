"use client"

import type { AppFormApi } from "@/components/form/form-api"

/** 把整表校验错误收成一条提示。各表单原先都是同一段订阅。 */
export function FormValidationSummary<TValues>({
    form,
}: {
    form: AppFormApi<TValues>
}) {
    return (
        <form.Subscribe selector={(state) => state.errors}>
            {(errors) =>
                errors.length ? (
                    <p
                        role="alert"
                        className="text-body-compact text-destructive"
                    >
                        {errors.map(String).join("；")}
                    </p>
                ) : null
            }
        </form.Subscribe>
    )
}
