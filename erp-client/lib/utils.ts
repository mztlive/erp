import { clsx, type ClassValue } from "clsx"
import { extendTailwindMerge } from "tailwind-merge"

import { themeSpacingNames, themeTextNames } from "@/lib/theme-tokens"

// 自定义字号必须登记为 font-size，避免与 text-foreground 等颜色类互相删除。
const mergeClasses = extendTailwindMerge({
    extend: {
        theme: {
            text: [...themeTextNames],
            spacing: [...themeSpacingNames],
            shadow: ["footer"],
        },
    },
})

export function cn(...inputs: ClassValue[]) {
    return mergeClasses(clsx(inputs))
}
