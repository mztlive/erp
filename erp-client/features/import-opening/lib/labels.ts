/**
 * W18 导入与期初 · 展示用标签格式化纯函数。
 */

import { OBJECT_CODE_LABEL } from "@/features/import-opening/types"

export function formatObjectSet(codes: readonly string[]): string {
    return codes
        .map((c) => OBJECT_CODE_LABEL[c as keyof typeof OBJECT_CODE_LABEL] ?? c)
        .join("、")
}
