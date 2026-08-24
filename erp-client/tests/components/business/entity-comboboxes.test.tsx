import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { ProductCombobox } from "@/components/business/entity-comboboxes"

afterEach(cleanup)

describe("ProductCombobox", () => {
    it("uses the supplied accessible label", () => {
        render(
            <ProductCombobox
                label="公司 SKU"
                products={[
                    {
                        productId: "sku-1",
                        sku: "SKU-001",
                        name: "测试商品",
                    },
                ]}
                value="sku-1"
                onValueChange={vi.fn()}
            />,
        )

        expect(screen.getByRole("combobox", { name: "公司 SKU" })).toBeTruthy()
    })
})
