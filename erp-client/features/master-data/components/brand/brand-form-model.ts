import { z } from "zod"

export const brandFormSchema = z.object({
    name: z.string().trim().min(2, "请填写名称"),
    code: z.string().trim().min(1, "请填写品牌代码"),
    logo: z.string(),
    changeReason: z.string().trim().min(2, "请填写变更原因"),
})

export type BrandFormValues = {
    name: string
    code: string
    logo: string
    changeReason: string
}

export function emptyBrandForm(): BrandFormValues {
    return { name: "", code: "", logo: "", changeReason: "" }
}
