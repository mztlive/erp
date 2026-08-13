import type { Metadata } from "next"

import { CategoryTreePage } from "@/features/master-data/components/category/category-tree-page"

export const metadata: Metadata = {
    title: "商品分类",
}

export default function Page() {
    return <CategoryTreePage />
}
