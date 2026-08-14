import { z } from "zod"

export const categoryFormSchema = z.object({
    name: z.string().trim().min(2, "请填写名称"),
    code: z.string().trim().min(1, "请填写分类代码"),
    parentId: z.string(),
    productKind: z.string(),
    changeReason: z.string().trim().min(2, "请填写变更原因"),
})

export type CategoryFormValues = {
    name: string
    code: string
    parentId: string
    productKind: string
    changeReason: string
}

export function emptyCategoryForm(parentId = ""): CategoryFormValues {
    return {
        name: "",
        code: "",
        parentId,
        productKind: "",
        changeReason: "",
    }
}
